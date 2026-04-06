//! Guitar playing technique detection.
//!
//! Analyses the continuous pitch history produced by [`QPitchDetector`] to
//! recognise performance techniques: bends, slides, vibrato, palm mutes,
//! hammer-ons, and pull-offs.
//!
//! The pitch data comes directly from Q's BACF algorithm — **no pitch
//! detection is reimplemented here**.  This module only interprets the
//! trajectory of Q's output over time.
//!
//! [`QPitchDetector`]: crate::q_pitch::QPitchDetector

use std::collections::VecDeque;

use crate::types::GuitarTechnique;

// ── Detection thresholds ──────────────────────────────────────────────────────

/// Minimum pitch change (semitones) to classify a movement as a bend.
const BEND_MIN_SEMITONES: f32 = 0.25;
/// Minimum number of frames required to confirm a bend.
const BEND_MIN_FRAMES: usize = 9; // ≈ 100 ms at 86 fps (44100 / 512)
/// Maximum frame window used in bend analysis.
const BEND_MAX_FRAMES: usize = 34; // ≈ 400 ms
/// Fraction of frame-to-frame steps that must move in the same direction.
const BEND_MONOTONE_RATIO: f32 = 0.65;

/// Minimum pitch change (semitones) across a short window to call it a slide.
const SLIDE_MIN_SEMITONES: f32 = 1.5;
/// Maximum frame window in which a slide must complete.
const SLIDE_MAX_FRAMES: usize = 5; // ≈ 60 ms

/// Minimum RMS deviation from the mean pitch (semitones) for vibrato.
const VIBRATO_MIN_DEPTH: f32 = 0.10;
/// Vibrato oscillation rate bounds (Hz).
const VIBRATO_MIN_RATE_HZ: f32 = 3.0;
const VIBRATO_MAX_RATE_HZ: f32 = 9.0;

/// Palm-mute: tail energy / peak energy must be below this ratio.
const PALM_MUTE_DECAY_RATIO: f32 = 0.40;

// ── Internal frame type ───────────────────────────────────────────────────────

#[derive(Clone, Debug, Default)]
struct PitchFrame {
    /// Fractional MIDI note number (e.g. 69.0 = A4, 69.5 = halfway between A4 and A#4).
    fractional_midi: f32,
    /// Q's periodicity confidence [0, 1].
    energy: f32,
}

// ── Public helpers ────────────────────────────────────────────────────────────

/// Convert a frequency in Hz to a fractional MIDI note number.
///
/// A4 = 440 Hz → 69.0.  Returns 0.0 for non-positive or zero input.
pub fn freq_to_fractional_midi(freq: f32) -> f32 {
    if freq <= 0.0 {
        0.0
    } else {
        69.0 + 12.0 * (freq / 440.0).log2()
    }
}

// ── TechniqueDetector ─────────────────────────────────────────────────────────

/// Analyses a rolling window of Q-provided pitch frames to detect guitar
/// playing techniques.
///
/// Call [`push`](TechniqueDetector::push) once per audio frame with the
/// frequency and periodicity values from [`QPitchDetector`], then call
/// [`detect`](TechniqueDetector::detect) to read back any recognised
/// techniques.
#[derive(Debug)]
pub struct TechniqueDetector {
    /// Frames per second = sample_rate / frame_size.
    fps: f32,
    history: VecDeque<PitchFrame>,
    max_history: usize,
}

impl TechniqueDetector {
    /// Create a new detector.
    ///
    /// * `sample_rate` – audio sample rate in Hz.
    /// * `frame_size`  – number of samples per processing frame (e.g. 512).
    pub fn new(sample_rate: u32, frame_size: usize) -> Self {
        let fps = sample_rate as f32 / frame_size as f32;
        let max_history = (fps * 2.0) as usize + 1; // ≈ 2 seconds of history
        Self {
            fps,
            history: VecDeque::with_capacity(max_history),
            max_history,
        }
    }

    /// Push the latest reading from Q's pitch detector.
    ///
    /// * `frequency_hz`  – detected frequency in Hz (0.0 = silent / unvoiced).
    /// * `periodicity`   – Q's periodicity confidence [0, 1].
    pub fn push(&mut self, frequency_hz: f32, periodicity: f32) {
        if self.history.len() >= self.max_history {
            self.history.pop_front();
        }
        self.history.push_back(PitchFrame {
            fractional_midi: freq_to_fractional_midi(frequency_hz),
            energy: periodicity,
        });
    }

    /// Analyse the stored history and return all currently detected techniques.
    pub fn detect(&self) -> Vec<GuitarTechnique> {
        let mut out = Vec::new();

        let active: Vec<&PitchFrame> = self
            .history
            .iter()
            .filter(|f| f.energy > 0.05 && f.fractional_midi > 0.0)
            .collect();

        if active.len() < 2 {
            return out;
        }

        // Vibrato takes priority over bend (oscillating pitch ≠ monotone pitch rise).
        if let Some(vib) = self.detect_vibrato(&active) {
            out.push(vib);
        } else if let Some(bend) = self.detect_bend(&active) {
            out.push(bend);
        }

        if let Some(slide) = self.detect_slide(&active) {
            if !out.contains(&slide) {
                out.push(slide);
            }
        }

        if let Some(pm) = self.detect_palm_mute() {
            out.push(pm);
        }

        out
    }

    /// Clear all history.
    pub fn reset(&mut self) {
        self.history.clear();
    }

    // ── private ──────────────────────────────────────────────────────────────

    fn detect_bend(&self, active: &[&PitchFrame]) -> Option<GuitarTechnique> {
        let n = active.len();
        if n < BEND_MIN_FRAMES {
            return None;
        }
        let window = &active[n.saturating_sub(BEND_MAX_FRAMES)..];
        if window.len() < BEND_MIN_FRAMES {
            return None;
        }

        let first = window.first()?.fractional_midi;
        let last = window.last()?.fractional_midi;
        let delta = last - first;

        if delta.abs() < BEND_MIN_SEMITONES {
            return None;
        }

        let dir = delta.signum();
        let monotone = window
            .windows(2)
            .filter(|w| (w[1].fractional_midi - w[0].fractional_midi).signum() == dir)
            .count();
        let ratio = monotone as f32 / (window.len() - 1) as f32;

        if ratio >= BEND_MONOTONE_RATIO {
            Some(GuitarTechnique::Bend { semitones: delta })
        } else {
            None
        }
    }

    fn detect_slide(&self, active: &[&PitchFrame]) -> Option<GuitarTechnique> {
        let n = active.len();
        if n < 2 {
            return None;
        }
        let window = &active[n.saturating_sub(SLIDE_MAX_FRAMES)..];
        if window.len() < 2 {
            return None;
        }

        let first = window.first()?;
        let last = window.last()?;
        let delta = last.fractional_midi - first.fractional_midi;

        if delta.abs() >= SLIDE_MIN_SEMITONES {
            Some(GuitarTechnique::Slide {
                from_midi: first.fractional_midi.round() as u8,
                to_midi: last.fractional_midi.round() as u8,
                ascending: delta > 0.0,
            })
        } else {
            None
        }
    }

    fn detect_vibrato(&self, active: &[&PitchFrame]) -> Option<GuitarTechnique> {
        let n = active.len();
        let min_frames = (self.fps * 0.5) as usize;
        if n < min_frames.max(6) {
            return None;
        }

        let mean = active.iter().map(|f| f.fractional_midi).sum::<f32>() / n as f32;
        let deviations: Vec<f32> = active.iter().map(|f| f.fractional_midi - mean).collect();

        let rms = (deviations.iter().map(|d| d * d).sum::<f32>() / n as f32).sqrt();
        if rms < VIBRATO_MIN_DEPTH {
            return None;
        }

        // Count zero crossings to estimate oscillation rate.
        let crossings = deviations
            .windows(2)
            .filter(|w| w[0].signum() != w[1].signum())
            .count();
        let rate_hz = (crossings as f32 / 2.0) * self.fps / n as f32;

        if (VIBRATO_MIN_RATE_HZ..=VIBRATO_MAX_RATE_HZ).contains(&rate_hz) {
            Some(GuitarTechnique::Vibrato {
                rate_hz,
                depth_semitones: rms,
            })
        } else {
            None
        }
    }

    fn detect_palm_mute(&self) -> Option<GuitarTechnique> {
        let frames: Vec<&PitchFrame> = self.history.iter().collect();
        let n = frames.len();
        if n < 4 {
            return None;
        }

        let peak = frames.iter().map(|f| f.energy).fold(0.0_f32, f32::max);
        let tail_start = n.saturating_sub((self.fps * 0.15) as usize);
        let tail = frames[tail_start..]
            .iter()
            .map(|f| f.energy)
            .fold(0.0_f32, f32::max);

        if peak > 0.15 && tail < peak * PALM_MUTE_DECAY_RATIO {
            Some(GuitarTechnique::PalmMute)
        } else {
            None
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;

    fn push_steady(td: &mut TechniqueDetector, freq: f32, frames: usize) {
        for _ in 0..frames {
            td.push(freq, 0.9);
        }
    }

    #[test]
    fn freq_to_midi_a4() {
        assert!((freq_to_fractional_midi(440.0) - 69.0).abs() < 1e-3);
    }

    #[test]
    fn freq_to_midi_octave_up() {
        assert!((freq_to_fractional_midi(880.0) - 81.0).abs() < 1e-3);
    }

    #[test]
    fn freq_to_midi_silence() {
        assert_eq!(freq_to_fractional_midi(0.0), 0.0);
    }

    #[test]
    fn steady_note_no_bend() {
        let mut td = TechniqueDetector::new(44_100, 512);
        push_steady(&mut td, 440.0, 100);
        let t = td.detect();
        assert!(
            !t.iter().any(|x| matches!(x, GuitarTechnique::Bend { .. })),
            "Steady note must not trigger a bend; got {t:?}"
        );
    }

    #[test]
    fn rising_pitch_is_bend() {
        let mut td = TechniqueDetector::new(44_100, 512);
        // Linearly rise by 2 semitones over 40 frames.
        for i in 0..40 {
            let midi = 69.0_f32 + 2.0 * i as f32 / 40.0;
            let freq = 440.0 * 2.0_f32.powf((midi - 69.0) / 12.0);
            td.push(freq, 0.9);
        }
        let t = td.detect();
        assert!(
            t.iter()
                .any(|x| matches!(x, GuitarTechnique::Bend { semitones } if *semitones > 1.5)),
            "Expected a >1.5 semitone bend; got {t:?}"
        );
    }

    #[test]
    fn oscillating_pitch_is_vibrato() {
        let mut td = TechniqueDetector::new(44_100, 512);
        let fps = 44_100.0_f32 / 512.0;
        let frames = (fps * 0.8) as usize;
        for i in 0..frames {
            let t = i as f32 / fps;
            let offset = 0.3 * (2.0 * std::f32::consts::PI * 6.0 * t).sin();
            let freq = 440.0 * 2.0_f32.powf(offset / 12.0);
            td.push(freq, 0.9);
        }
        let t = td.detect();
        assert!(
            t.iter().any(|x| matches!(x, GuitarTechnique::Vibrato { .. })),
            "Expected vibrato; got {t:?}"
        );
    }

    #[test]
    fn reset_clears_history() {
        let mut td = TechniqueDetector::new(44_100, 512);
        push_steady(&mut td, 440.0, 50);
        td.reset();
        assert!(td.history.is_empty());
    }
}
