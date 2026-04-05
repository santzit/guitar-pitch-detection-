//! The main `GuitarPitchDetector` — combines the resonator bank, note
//! mapping, harmonic suppression, and chord recognition into a single,
//! easy-to-use API.
//!
//! # Quick start
//!
//! ```rust
//! use guitar_pitch_detection::GuitarPitchDetector;
//!
//! let mut detector = GuitarPitchDetector::new(44_100, 512);
//!
//! // Feed a frame of 32-bit float PCM samples (mono, normalized −1.0 … 1.0).
//! let samples: Vec<f32> = vec![0.0; 512]; // silence
//! let result = detector.process(&samples);
//!
//! for note in &result.notes {
//!     println!("Detected {} ({:.1} Hz, confidence {:.2})", note.name, note.frequency, note.confidence);
//! }
//! if let Some(chord) = &result.chord {
//!     println!("Chord: {}", chord.name);
//! }
//! ```

use crate::{
    chord::detect_chord,
    modulation::{ModulationAnalyzer, ModulationConfig},
    notes::{midi_to_freq, note_name, octave, pitch_class, MAX_MIDI, MIN_MIDI},
    resonator::{ResonatorBank, DEFAULT_ALPHA},
    types::{DetectedNote, DetectionResult, PitchModulation},
};

/// Configuration used to construct a [`GuitarPitchDetector`].
#[derive(Debug, Clone)]
pub struct DetectorConfig {
    /// Audio sample rate in Hz (default: 44 100).
    pub sample_rate: u32,
    /// Lowest MIDI note to track (default: [`MIN_MIDI`] = 36 = C2).
    pub min_midi: u8,
    /// Highest MIDI note to track (default: [`MAX_MIDI`] = 88 = E6).
    pub max_midi: u8,
    /// Resonator forgetting factor α ∈ (0, 1).  Higher values → slower decay
    /// → more stable but more latency.  Default: [`DEFAULT_ALPHA`] (≈ 20 ms).
    pub alpha: f32,
    /// A note is reported when its resonator energy exceeds this fraction of
    /// the maximum energy across all resonators.  Range 0.0–1.0.
    /// Default: 0.15 (15 %).
    pub detection_threshold: f32,
    /// Maximum number of notes returned per frame.  Guitar has 6 strings, so
    /// 6 is a reasonable ceiling.  Default: 6.
    pub max_polyphony: usize,
    /// Configuration for the built-in pitch modulation analyser (bend /
    /// vibrato detection).  The `frames_per_second` field is automatically
    /// overridden to `sample_rate / frame_size` when the detector is
    /// constructed via [`GuitarPitchDetector::new`].
    pub modulation: ModulationConfig,
}

impl Default for DetectorConfig {
    fn default() -> Self {
        Self {
            sample_rate: 44_100,
            min_midi: MIN_MIDI,
            max_midi: MAX_MIDI,
            alpha: DEFAULT_ALPHA,
            detection_threshold: 0.15,
            max_polyphony: 6,
            modulation: ModulationConfig::default(),
        }
    }
}

/// Real-time polyphonic guitar pitch detector.
///
/// Feed consecutive audio frames to [`process`](GuitarPitchDetector::process);
/// the detector maintains resonator state between calls so that notes which
/// span multiple frames are tracked correctly.
///
/// Bend and vibrato classification is performed automatically: the `modulation`
/// field of each [`DetectedNote`] in the returned [`DetectionResult`] is
/// populated on every call.
#[derive(Debug)]
pub struct GuitarPitchDetector {
    bank: ResonatorBank,
    config: DetectorConfig,
    modulation: ModulationAnalyzer,
}

impl GuitarPitchDetector {
    /// Create a detector with default settings for the given sample rate and
    /// frame size.
    ///
    /// `frame_size` is used to compute the vibrato-rate estimation frame rate
    /// stored in the modulation configuration.
    pub fn new(sample_rate: u32, frame_size: usize) -> Self {
        let fps = if frame_size > 0 {
            sample_rate as f32 / frame_size as f32
        } else {
            ModulationConfig::default().frames_per_second
        };
        let config = DetectorConfig {
            sample_rate,
            modulation: ModulationConfig {
                frames_per_second: fps,
                ..ModulationConfig::default()
            },
            ..Default::default()
        };
        Self::with_config(config)
    }

    /// Create a detector with a fully customised [`DetectorConfig`].
    pub fn with_config(config: DetectorConfig) -> Self {
        let bank = ResonatorBank::new(
            config.min_midi,
            config.max_midi,
            config.sample_rate as f32,
            config.alpha,
        );
        let modulation = ModulationAnalyzer::new(config.modulation.clone());
        Self {
            bank,
            config,
            modulation,
        }
    }

    /// Process a frame of mono PCM samples (f32, normalized to −1.0 … 1.0).
    ///
    /// The function:
    /// 1. Feeds every sample through the resonator bank.
    /// 2. Collects resonator energies.
    /// 3. Keeps only local-maxima resonators (peak-picking) to avoid bleed
    ///    from adjacent semitones sharing similar energy.
    /// 4. Selects notes whose peak energy exceeds `detection_threshold × max_energy`.
    /// 5. Applies harmonic suppression to reduce octave errors.
    /// 6. Refines each peak frequency using parabolic interpolation of the
    ///    three-resonator neighbourhood (sub-semitone accuracy).
    /// 7. Maps surviving resonators to [`DetectedNote`] structs.
    /// 8. Classifies the pitch modulation (bend / vibrato) for each note.
    /// 9. Runs chord detection on the resulting pitch-class set.
    pub fn process(&mut self, samples: &[f32]) -> DetectionResult {
        self.bank.process_samples(samples);

        let energies = self.bank.energies();
        let max_energy = energies
            .iter()
            .cloned()
            .fold(0.0_f32, f32::max);

        // Return early if the signal is essentially silent.
        if max_energy < 1e-10 {
            return DetectionResult::default();
        }

        let threshold = self.config.detection_threshold * max_energy;

        // Keep only resonators that are local energy maxima: each candidate
        // must have strictly higher energy than both of its semitone neighbours.
        // This eliminates the energy "bleed" into adjacent resonators that
        // occurs when α is close to 1 (wide resonator bandwidth).
        let n = energies.len();
        let mut candidates: Vec<(usize, f32)> = energies
            .iter()
            .enumerate()
            .filter(|&(i, &e)| {
                if e <= threshold {
                    return false;
                }
                let left_ok = i == 0 || e > energies[i - 1];
                let right_ok = i == n - 1 || e > energies[i + 1];
                left_ok && right_ok
            })
            .map(|(i, &e)| (i, e))
            .collect();

        // Sort by energy descending so the strongest notes come first.
        candidates.sort_unstable_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

        // Cap at max_polyphony.
        candidates.truncate(self.config.max_polyphony);

        // Harmonic suppression: if two notes are detected where one is (close
        // to) an octave above the other, and the lower note is stronger, the
        // higher note is likely an overtone — remove it.
        let active_midis: Vec<u8> = candidates
            .iter()
            .map(|(i, _)| self.bank.resonators[*i].midi_note)
            .collect();

        let suppressed: Vec<bool> = active_midis
            .iter()
            .enumerate()
            .map(|(idx, &m)| {
                // Check if there is a note 12 semitones lower that is stronger.
                if m < 12 {
                    return false;
                }
                let lower = m - 12;
                if let Some(pos) = active_midis.iter().position(|&x| x == lower) {
                    // Lower note must be stronger (earlier in energy-sorted list).
                    return pos < idx;
                }
                false
            })
            .collect();

        // Build the final note list, refining frequency with parabolic
        // interpolation so that bends and vibrato within ±50 cents of a
        // semitone boundary are captured accurately.
        let mut notes: Vec<DetectedNote> = candidates
            .iter()
            .zip(suppressed.iter())
            .filter(|(_, &s)| !s)
            .map(|((i, e), _)| {
                let midi = self.bank.resonators[*i].midi_note;

                // Parabolic interpolation: fit a parabola to the energy of
                // this resonator and its immediate neighbours to find the
                // true spectral peak within ±0.5 semitones.
                let cents_offset = if *i > 0 && *i < n - 1 {
                    let el = energies[i - 1];
                    let ec = energies[*i];
                    let er = energies[i + 1];
                    let denom = el - 2.0 * ec + er;
                    if denom.abs() > 1e-20 {
                        let offset = 0.5 * (el - er) / denom; // semitones
                        offset.clamp(-0.5, 0.5) * 100.0 // → cents
                    } else {
                        0.0
                    }
                } else {
                    0.0
                };

                let nominal_freq = midi_to_freq(midi);
                let freq = nominal_freq * 2.0_f32.powf(cents_offset / 1200.0);

                DetectedNote {
                    frequency: freq,
                    midi_note: midi,
                    semitone: pitch_class(midi),
                    octave: octave(midi),
                    name: note_name(midi),
                    confidence: e / max_energy,
                    modulation: PitchModulation::Stable,
                }
            })
            .collect();

        // Classify bend / vibrato for each note using the running history.
        self.modulation.update(&mut notes);

        // Chord detection.
        let pitch_classes: Vec<u8> = notes.iter().map(|n| n.semitone).collect();
        let chord = detect_chord(&pitch_classes);

        DetectionResult { notes, chord }
    }

    /// Reset all resonator states (silence the detector).
    ///
    /// Call this between songs or after a long pause to avoid old note energy
    /// bleeding into the next detection window.  Also resets the pitch
    /// modulation history so bend / vibrato classification starts fresh.
    pub fn reset(&mut self) {
        self.bank.reset();
        self.modulation.reset();
    }

    /// Return the detector configuration.
    pub fn config(&self) -> &DetectorConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;

    fn sine(freq: f32, n_samples: usize, sr: f32) -> Vec<f32> {
        (0..n_samples)
            .map(|n| (TAU * freq * n as f32 / sr).sin())
            .collect()
    }

    const SR: u32 = 44_100;

    #[test]
    fn silence_returns_no_notes() {
        let mut det = GuitarPitchDetector::new(SR, 512);
        let result = det.process(&vec![0.0_f32; 512]);
        assert!(result.notes.is_empty());
        assert!(result.chord.is_none());
    }

    #[test]
    fn detects_a4_sine() {
        let mut det = GuitarPitchDetector::new(SR, 2048);
        let samples = sine(440.0, 4096, SR as f32);
        // Feed multiple frames to let resonators build up.
        let result = det.process(&samples);
        assert!(
            !result.notes.is_empty(),
            "Expected at least one note, got none"
        );
        // The strongest note should be A4 (MIDI 69) or very close.
        let top = &result.notes[0];
        assert!(
            (top.midi_note as i16 - 69).abs() <= 1,
            "Expected A4 (69), got MIDI {}",
            top.midi_note
        );
    }

    #[test]
    fn detects_e2_open_string() {
        let mut det = GuitarPitchDetector::new(SR, 2048);
        let e2_freq = 82.41_f32;
        let samples = sine(e2_freq, 4096, SR as f32);
        let result = det.process(&samples);
        assert!(!result.notes.is_empty());
        let top = &result.notes[0];
        assert!(
            (top.midi_note as i16 - 40).abs() <= 1,
            "Expected E2 (40), got MIDI {}",
            top.midi_note
        );
    }

    #[test]
    fn reset_clears_state() {
        let mut det = GuitarPitchDetector::new(SR, 512);
        // Excite the detector.
        det.process(&sine(440.0, 2048, SR as f32));
        // After reset, silence should give no notes.
        det.reset();
        let result = det.process(&vec![0.0; 512]);
        assert!(result.notes.is_empty());
    }

    #[test]
    fn with_config_applies_settings() {
        let config = DetectorConfig {
            sample_rate: 48_000,
            detection_threshold: 0.2,
            ..Default::default()
        };
        let det = GuitarPitchDetector::with_config(config.clone());
        assert_eq!(det.config().sample_rate, 48_000);
        assert!((det.config().detection_threshold - 0.2).abs() < 1e-6);
    }
}
