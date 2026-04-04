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
    notes::{midi_to_freq, note_name, octave, pitch_class, MAX_MIDI, MIN_MIDI},
    resonator::{ResonatorBank, DEFAULT_ALPHA},
    types::{DetectedNote, DetectionResult},
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
        }
    }
}

/// Real-time polyphonic guitar pitch detector.
///
/// Feed consecutive audio frames to [`process`](GuitarPitchDetector::process);
/// the detector maintains resonator state between calls so that notes which
/// span multiple frames are tracked correctly.
#[derive(Debug)]
pub struct GuitarPitchDetector {
    bank: ResonatorBank,
    config: DetectorConfig,
}

impl GuitarPitchDetector {
    /// Create a detector with default settings for the given sample rate and
    /// frame size.
    ///
    /// `frame_size` is only used for documentation / planning purposes here;
    /// the detector accepts frames of any size in [`process`].
    pub fn new(sample_rate: u32, _frame_size: usize) -> Self {
        let config = DetectorConfig {
            sample_rate,
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
        Self { bank, config }
    }

    /// Process a frame of mono PCM samples (f32, normalized to −1.0 … 1.0).
    ///
    /// The function:
    /// 1. Feeds every sample through the resonator bank.
    /// 2. Collects resonator energies.
    /// 3. Selects notes whose energy exceeds `detection_threshold × max_energy`.
    /// 4. Applies harmonic suppression to reduce octave errors.
    /// 5. Maps surviving resonators to [`DetectedNote`] structs.
    /// 6. Runs chord detection on the resulting pitch-class set.
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

        // Collect (index_in_bank, energy) pairs above the threshold.
        let mut candidates: Vec<(usize, f32)> = energies
            .iter()
            .enumerate()
            .filter(|(_, &e)| e > threshold)
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

        // Build the final note list.
        let notes: Vec<DetectedNote> = candidates
            .iter()
            .zip(suppressed.iter())
            .filter(|(_, &s)| !s)
            .map(|((i, e), _)| {
                let midi = self.bank.resonators[*i].midi_note;
                let freq = midi_to_freq(midi);
                DetectedNote {
                    frequency: freq,
                    midi_note: midi,
                    semitone: pitch_class(midi),
                    octave: octave(midi),
                    name: note_name(midi),
                    confidence: e / max_energy,
                }
            })
            .collect();

        // Chord detection.
        let pitch_classes: Vec<u8> = notes.iter().map(|n| n.semitone).collect();
        let chord = detect_chord(&pitch_classes);

        DetectionResult { notes, chord }
    }

    /// Reset all resonator states (silence the detector).
    ///
    /// Call this between songs or after a long pause to avoid old note energy
    /// bleeding into the next detection window.
    pub fn reset(&mut self) {
        self.bank.reset();
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
