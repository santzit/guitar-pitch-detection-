//! Pitch modulation detection and the pitch-shift utility.
//!
//! # Bend
//!
//! A string bend causes the pitch to rise or fall steadily from the nominal
//! note frequency.  The [`ModulationAnalyzer`] tracks the per-note cents
//! history and reports a [`PitchModulation::Bend`] when the deviation exceeds
//! [`ModulationConfig::bend_threshold_cents`] and is in a consistent direction.
//!
//! # Vibrato
//!
//! Vibrato is a rapid, periodic oscillation of pitch around the nominal note.
//! The analyser detects sign changes in the cents-deviation history and
//! estimates the rate from zero-crossing intervals.  A [`PitchModulation::Vibrato`]
//! is reported when the peak-to-peak range exceeds
//! [`ModulationConfig::vibrato_depth_cents`] and the rate falls between
//! [`ModulationConfig::vibrato_rate_min_hz`] and
//! [`ModulationConfig::vibrato_rate_max_hz`].
//!
//! # Pitch shift
//!
//! [`shift_result`] is a convenience function that transposes every note in a
//! [`DetectionResult`] by a fixed number of semitones and re-runs chord
//! detection.  This is useful when the guitar has a capo or is tuned to an
//! alternate pitch (e.g. drop-D, open-G).
//!
//! # Usage
//!
//! ```rust
//! use guitar_pitch_detection::{GuitarPitchDetector, modulation::{ModulationAnalyzer, ModulationConfig, shift_result}};
//!
//! let mut detector = GuitarPitchDetector::new(44_100, 512);
//! let mut analyzer = ModulationAnalyzer::new(ModulationConfig::default());
//!
//! let samples = vec![0.0_f32; 512];
//! let mut result = detector.process(&samples);
//!
//! // Classify the modulation technique for each detected note.
//! analyzer.update(&mut result.notes);
//!
//! // Optionally transpose all detected notes by −2 semitones (capo on fret 2).
//! let transposed = shift_result(&result, -2);
//! ```

use std::collections::{HashMap, VecDeque};

use crate::{
    chord::detect_chord,
    notes::{midi_to_freq, note_name, octave, pitch_class},
    types::{DetectedNote, DetectionResult, PitchModulation},
};

// ── Public configuration ──────────────────────────────────────────────────────

/// Configuration for the pitch modulation analyser.
///
/// All thresholds can be left at their defaults for typical guitar playing.
/// Adjust them only when working with non-standard instruments or playing
/// styles.
#[derive(Debug, Clone)]
pub struct ModulationConfig {
    /// Number of detection frames kept in the per-note history (default: 32).
    ///
    /// At 44 100 Hz with 512-sample frames this is about 372 ms of history.
    /// Longer histories give more stable classification but react more slowly
    /// to technique changes.
    pub history_frames: usize,

    /// Minimum absolute cents deviation that triggers a [`PitchModulation::Bend`]
    /// (default: 15.0).
    ///
    /// Guitar string bends are typically 50–200 cents (½–2 semitones).
    /// The default of 15 cents ignores small intonation imperfections while
    /// catching even a quarter-tone bend.
    pub bend_threshold_cents: f32,

    /// Minimum peak-to-peak cents range that triggers a
    /// [`PitchModulation::Vibrato`] (default: 10.0).
    ///
    /// Classical guitar vibrato is typically ±10–30 cents.
    pub vibrato_depth_cents: f32,

    /// Slowest oscillation rate classified as vibrato in Hz (default: 3.0).
    pub vibrato_rate_min_hz: f32,

    /// Fastest oscillation rate classified as vibrato in Hz (default: 10.0).
    pub vibrato_rate_max_hz: f32,

    /// Frame rate (frames per second) used for vibrato rate estimation.
    ///
    /// Set this to `sample_rate / frame_size`.  The default assumes the
    /// common 44 100 Hz / 512-sample configuration (≈ 86.1 fps).
    pub frames_per_second: f32,
}

impl Default for ModulationConfig {
    fn default() -> Self {
        Self {
            history_frames: 32,
            bend_threshold_cents: 15.0,
            vibrato_depth_cents: 10.0,
            vibrato_rate_min_hz: 3.0,
            vibrato_rate_max_hz: 10.0,
            frames_per_second: 86.1,
        }
    }
}

// ── Internal note tracker ─────────────────────────────────────────────────────

/// Rolling window of cents-deviation samples for a single sustained note.
#[derive(Debug)]
struct NoteTracker {
    history: VecDeque<f32>,
    capacity: usize,
}

impl NoteTracker {
    fn new(capacity: usize) -> Self {
        Self {
            history: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    fn push(&mut self, cents: f32) {
        if self.history.len() == self.capacity {
            self.history.pop_front();
        }
        self.history.push_back(cents);
    }

    fn len(&self) -> usize {
        self.history.len()
    }
}

// ── Modulation analyser ───────────────────────────────────────────────────────

/// Tracks the pitch history of each active note and classifies the modulation
/// technique: bend, vibrato, or stable.
///
/// Create one `ModulationAnalyzer` per [`crate::GuitarPitchDetector`] and call
/// [`update`](ModulationAnalyzer::update) on each detection result.
///
/// # Example
/// ```rust
/// use guitar_pitch_detection::{GuitarPitchDetector, modulation::{ModulationAnalyzer, ModulationConfig}};
///
/// let mut detector = GuitarPitchDetector::new(44_100, 512);
/// let mut analyzer = ModulationAnalyzer::new(ModulationConfig::default());
///
/// let samples = vec![0.0_f32; 512];
/// let mut result = detector.process(&samples);
/// analyzer.update(&mut result.notes);
/// ```
#[derive(Debug)]
pub struct ModulationAnalyzer {
    config: ModulationConfig,
    /// Per-MIDI-note history trackers.
    trackers: HashMap<u8, NoteTracker>,
}

impl ModulationAnalyzer {
    /// Create a new analyser with the given configuration.
    pub fn new(config: ModulationConfig) -> Self {
        Self {
            config,
            trackers: HashMap::new(),
        }
    }

    /// Analyse `notes`, setting the `modulation` field of each note in place.
    ///
    /// Call this once per audio frame, immediately after
    /// [`crate::GuitarPitchDetector::process`].  Notes that have been absent
    /// since the last call have their history discarded.
    pub fn update(&mut self, notes: &mut [DetectedNote]) {
        // Drop trackers for notes that are no longer active.
        let active: Vec<u8> = notes.iter().map(|n| n.midi_note).collect();
        self.trackers.retain(|&m, _| active.contains(&m));

        for note in notes.iter_mut() {
            // Cents deviation from the nominal equal-temperament frequency.
            let nominal = midi_to_freq(note.midi_note);
            let cents = if nominal > 0.0 && note.frequency > 0.0 {
                1200.0 * (note.frequency / nominal).log2()
            } else {
                0.0
            };

            let tracker = self
                .trackers
                .entry(note.midi_note)
                .or_insert_with(|| NoteTracker::new(self.config.history_frames));
            tracker.push(cents);

            note.modulation = classify(tracker, &self.config);
        }
    }

    /// Reset all tracked note histories (call after [`crate::GuitarPitchDetector::reset`]).
    pub fn reset(&mut self) {
        self.trackers.clear();
    }
}

// ── Classification helper ─────────────────────────────────────────────────────

fn classify(tracker: &NoteTracker, cfg: &ModulationConfig) -> PitchModulation {
    let hist = &tracker.history;
    if tracker.len() < 4 {
        return PitchModulation::Stable;
    }

    let max_c = hist.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let min_c = hist.iter().cloned().fold(f32::INFINITY, f32::min);
    let range = max_c - min_c;

    // Count zero-crossings in the deviation sequence.
    let crossings: usize = hist
        .iter()
        .zip(hist.iter().skip(1))
        .filter(|(&a, &b)| a * b < 0.0)
        .count();

    // ── Vibrato: oscillating pitch within the vibrato range ──────────────────
    if range >= cfg.vibrato_depth_cents && crossings >= 2 {
        let half_cycles = crossings as f32;
        let duration_s = tracker.len() as f32 / cfg.frames_per_second;
        let rate_hz = (half_cycles / 2.0) / duration_s;
        if rate_hz >= cfg.vibrato_rate_min_hz && rate_hz <= cfg.vibrato_rate_max_hz {
            return PitchModulation::Vibrato {
                depth_cents: range / 2.0,
                rate_hz,
            };
        }
    }

    // ── Bend: sustained deviation in one direction ────────────────────────────
    let latest = *hist.back().unwrap();
    if latest.abs() >= cfg.bend_threshold_cents {
        // Require that more than half the history has the same sign as `latest`.
        let consistent = hist.iter().filter(|&&c| c * latest > 0.0).count();
        if consistent > tracker.len() / 2 {
            return PitchModulation::Bend { cents: latest };
        }
    }

    PitchModulation::Stable
}

// ── Pitch shift utility ───────────────────────────────────────────────────────

/// Transpose every note in `result` by `semitones` semitones and re-run chord
/// detection on the shifted pitch classes.
///
/// Positive `semitones` shifts upward; negative shifts downward.  Notes that
/// would fall outside the MIDI range \[0, 127\] after transposition are
/// silently dropped.  The modulation classification is preserved.
///
/// This is useful when the guitar has a capo or is tuned to an alternate
/// pitch — feed the raw detection result and the capo fret number (negated)
/// to recover the concert-pitch note names.
///
/// # Example
///
/// ```rust
/// use guitar_pitch_detection::{GuitarPitchDetector, modulation::shift_result};
///
/// let mut detector = GuitarPitchDetector::new(44_100, 512);
/// let result = detector.process(&vec![0.0_f32; 512]);
///
/// // Capo on fret 2 — shift detected pitches down by 2 semitones to get the
/// // concert pitch.
/// let concert = shift_result(&result, -2);
/// ```
pub fn shift_result(result: &DetectionResult, semitones: i8) -> DetectionResult {
    let notes: Vec<DetectedNote> = result
        .notes
        .iter()
        .filter_map(|n| {
            let new_midi = (n.midi_note as i16) + (semitones as i16);
            if !(0i16..=127).contains(&new_midi) {
                return None;
            }
            let new_midi = new_midi as u8;
            Some(DetectedNote {
                frequency: midi_to_freq(new_midi),
                midi_note: new_midi,
                semitone: pitch_class(new_midi),
                octave: octave(new_midi),
                name: note_name(new_midi),
                confidence: n.confidence,
                modulation: n.modulation.clone(),
            })
        })
        .collect();

    let pitch_classes: Vec<u8> = notes.iter().map(|n| n.semitone).collect();
    let chord = detect_chord(&pitch_classes);

    DetectionResult { notes, chord }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::DetectionResult;

    fn make_note(midi: u8, freq: f32) -> DetectedNote {
        DetectedNote {
            frequency: freq,
            midi_note: midi,
            semitone: pitch_class(midi),
            octave: octave(midi),
            name: note_name(midi),
            confidence: 1.0,
            modulation: PitchModulation::Stable,
        }
    }

    // ── Stable ────────────────────────────────────────────────────────────────

    #[test]
    fn stable_note_stays_stable() {
        let mut analyzer = ModulationAnalyzer::new(ModulationConfig::default());
        let nominal = midi_to_freq(69); // A4 = 440 Hz
        let mut notes = vec![make_note(69, nominal)];
        for _ in 0..20 {
            analyzer.update(&mut notes);
        }
        assert_eq!(notes[0].modulation, PitchModulation::Stable);
    }

    // ── Bend ──────────────────────────────────────────────────────────────────

    #[test]
    fn upward_bend_detected() {
        let cfg = ModulationConfig {
            bend_threshold_cents: 15.0,
            history_frames: 32,
            ..Default::default()
        };
        let mut analyzer = ModulationAnalyzer::new(cfg);
        let nominal = midi_to_freq(69); // A4

        // Feed 20 frames with steadily rising pitch (0 → ~50 cents).
        for i in 0..=20usize {
            let cents = i as f32 * 2.5;
            let freq = nominal * 2.0_f32.powf(cents / 1200.0);
            let mut notes = vec![make_note(69, freq)];
            analyzer.update(&mut notes);
            // Only check on the last iteration.
            if i == 20 {
                match notes[0].modulation {
                    PitchModulation::Bend { cents: c } => {
                        assert!(c > 15.0, "Expected bend > 15 cents, got {c}");
                    }
                    ref other => panic!("Expected Bend, got {other:?}"),
                }
            }
        }
    }

    #[test]
    fn downward_bend_detected() {
        let cfg = ModulationConfig {
            bend_threshold_cents: 15.0,
            history_frames: 32,
            ..Default::default()
        };
        let mut analyzer = ModulationAnalyzer::new(cfg);
        let nominal = midi_to_freq(69);

        for i in 0..=20usize {
            let cents = -(i as f32 * 2.5);
            let freq = nominal * 2.0_f32.powf(cents / 1200.0);
            let mut notes = vec![make_note(69, freq)];
            analyzer.update(&mut notes);
            if i == 20 {
                match notes[0].modulation {
                    PitchModulation::Bend { cents: c } => {
                        assert!(c < -15.0, "Expected downward bend < −15 cents, got {c}");
                    }
                    ref other => panic!("Expected Bend, got {other:?}"),
                }
            }
        }
    }

    // ── Vibrato ───────────────────────────────────────────────────────────────

    #[test]
    fn vibrato_detected() {
        // Use fps=10 so maths is easy: alternating frames give 5 Hz vibrato.
        let cfg = ModulationConfig {
            history_frames: 20,
            vibrato_depth_cents: 10.0,
            vibrato_rate_min_hz: 3.0,
            vibrato_rate_max_hz: 10.0,
            frames_per_second: 10.0,
            bend_threshold_cents: 15.0,
        };
        let mut analyzer = ModulationAnalyzer::new(cfg);
        let nominal = midi_to_freq(69);

        let mut last_notes = vec![make_note(69, nominal)];
        for i in 0..20usize {
            let cents = if i % 2 == 0 { 20.0_f32 } else { -20.0_f32 };
            let freq = nominal * 2.0_f32.powf(cents / 1200.0);
            let mut notes = vec![make_note(69, freq)];
            analyzer.update(&mut notes);
            last_notes = notes;
        }
        match last_notes[0].modulation {
            PitchModulation::Vibrato { depth_cents, rate_hz } => {
                assert!(depth_cents > 5.0, "depth_cents={depth_cents}");
                assert!(
                    (3.0..=10.0).contains(&rate_hz),
                    "rate_hz={rate_hz}"
                );
            }
            ref other => panic!("Expected Vibrato, got {other:?}"),
        }
    }

    // ── Pitch shift ───────────────────────────────────────────────────────────

    #[test]
    fn shift_up_one_semitone() {
        // Build a DetectionResult with A4 (MIDI 69).
        let result = DetectionResult {
            notes: vec![make_note(69, midi_to_freq(69))],
            chord: None,
        };
        let shifted = shift_result(&result, 1);
        assert_eq!(shifted.notes.len(), 1);
        assert_eq!(shifted.notes[0].midi_note, 70); // A#4
        assert_eq!(shifted.notes[0].name, "A#4");
    }

    #[test]
    fn shift_down_two_semitones() {
        let result = DetectionResult {
            notes: vec![make_note(69, midi_to_freq(69))],
            chord: None,
        };
        let shifted = shift_result(&result, -2);
        assert_eq!(shifted.notes.len(), 1);
        assert_eq!(shifted.notes[0].midi_note, 67); // G4
    }

    #[test]
    fn shift_drops_notes_outside_midi_range() {
        // MIDI 0 shifted down by 1 would be −1 → dropped.
        let result = DetectionResult {
            notes: vec![make_note(0, midi_to_freq(0))],
            chord: None,
        };
        let shifted = shift_result(&result, -1);
        assert!(shifted.notes.is_empty());
    }

    #[test]
    fn shift_reruns_chord_detection() {
        use crate::types::ChordQuality;
        // Am triad (A=9, C=0, E=4) shifted up 3 semitones → Cm (C=0, Eb=3, G=7).
        let result = DetectionResult {
            notes: vec![
                make_note(57, midi_to_freq(57)), // A3
                make_note(60, midi_to_freq(60)), // C4
                make_note(64, midi_to_freq(64)), // E4
            ],
            chord: None,
        };
        let shifted = shift_result(&result, 3);
        // Notes should now be C4 (60), Eb4 (63), G4 (67).
        let midis: Vec<u8> = shifted.notes.iter().map(|n| n.midi_note).collect();
        assert!(midis.contains(&60), "missing C4");
        assert!(midis.contains(&63), "missing Eb4");
        assert!(midis.contains(&67), "missing G4");
        if let Some(chord) = &shifted.chord {
            assert_eq!(chord.quality, ChordQuality::Minor);
        }
    }

    #[test]
    fn shift_zero_is_identity() {
        let result = DetectionResult {
            notes: vec![make_note(69, midi_to_freq(69))],
            chord: None,
        };
        let shifted = shift_result(&result, 0);
        assert_eq!(shifted.notes[0].midi_note, 69);
    }

    // ── Analyser reset ────────────────────────────────────────────────────────

    #[test]
    fn analyzer_reset_clears_history() {
        let mut analyzer = ModulationAnalyzer::new(ModulationConfig::default());
        let nominal = midi_to_freq(69);

        // Build up history.
        for i in 0..16usize {
            let cents = i as f32 * 3.0;
            let freq = nominal * 2.0_f32.powf(cents / 1200.0);
            let mut notes = vec![make_note(69, freq)];
            analyzer.update(&mut notes);
        }

        analyzer.reset();

        // After reset, classification should start fresh (history < 4 → Stable).
        let mut notes = vec![make_note(69, nominal * 2.0_f32.powf(50.0 / 1200.0))];
        analyzer.update(&mut notes);
        assert_eq!(notes[0].modulation, PitchModulation::Stable);
    }
}
