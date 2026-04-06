//! Safe Rust wrapper around the cycfi/q `pitch_detector`.
//!
//! [cycfi/q](https://github.com/cycfi/q) implements a Binary Autocorrelation
//! Function (BACF) pitch detection algorithm that is fast, accurate, and
//! designed specifically for guitar and bass signals.  This module owns the
//! opaque C++ object through a raw pointer and exposes a safe, ergonomic API.

use crate::q_sys::{
    q_pd_create, q_pd_destroy, q_pd_get_frequency, q_pd_get_periodicity, q_pd_process,
    q_pd_reset, QPitchDetectorHandle,
};

// ── Guitar-range constants ────────────────────────────────────────────────────

/// Lowest open-string frequency: E2 ≈ 82.41 Hz (standard tuning).
pub const GUITAR_MIN_FREQ_HZ: f32 = 82.41;

/// Highest reachable fret frequency: E6 ≈ 1318.51 Hz (22nd fret, high-E string).
pub const GUITAR_MAX_FREQ_HZ: f32 = 1318.51;

/// Hysteresis in dB that suppresses spurious pitch shifts between adjacent frames.
/// −45 dB is the value used in the Q library's own guitar examples.
pub const DEFAULT_HYSTERESIS_DB: f32 = -45.0;

// ── QPitchDetector ────────────────────────────────────────────────────────────

/// Safe Rust wrapper around Q's `pitch_detector`.
///
/// Owns the underlying C++ object exclusively; `Drop` calls `q_pd_destroy`.
///
/// # Usage
///
/// ```ignore
/// use guitar_pitch_detection::q_pitch::QPitchDetector;
///
/// let mut pd = QPitchDetector::new_guitar(44_100.0);
/// for sample in audio_frame {
///     if pd.process(*sample) {
///         println!("Detected {:.1} Hz (periodicity {:.2})", pd.frequency(), pd.periodicity());
///     }
/// }
/// ```
pub struct QPitchDetector {
    ptr: *mut QPitchDetectorHandle,
}

// The raw pointer is exclusively owned — no shared mutable state is visible
// from outside, so these marker impls are sound.
unsafe impl Send for QPitchDetector {}
unsafe impl Sync for QPitchDetector {}

impl QPitchDetector {
    /// Create a detector tuned to the full standard guitar range
    /// (E2 82.41 Hz – E6 1318.51 Hz).
    pub fn new_guitar(sample_rate: f32) -> Self {
        Self::new(
            GUITAR_MIN_FREQ_HZ,
            GUITAR_MAX_FREQ_HZ,
            sample_rate,
            DEFAULT_HYSTERESIS_DB,
        )
    }

    /// Create a detector with custom frequency bounds and hysteresis.
    ///
    /// # Panics
    /// Panics if Q fails to allocate the internal object (OOM, which is
    /// extremely rare and unrecoverable anyway).
    pub fn new(min_freq_hz: f32, max_freq_hz: f32, sample_rate: f32, hysteresis_db: f32) -> Self {
        let ptr =
            unsafe { q_pd_create(min_freq_hz, max_freq_hz, sample_rate, hysteresis_db) };
        assert!(!ptr.is_null(), "q_pd_create returned null — out of memory?");
        Self { ptr }
    }

    /// Feed one mono audio sample (f32, normalised −1 … 1).
    ///
    /// Returns `true` when Q has finished analysing a new pitch period.
    /// At that point [`frequency`](Self::frequency) and
    /// [`periodicity`](Self::periodicity) contain fresh values.
    #[inline]
    pub fn process(&mut self, sample: f32) -> bool {
        unsafe { q_pd_process(self.ptr, sample) != 0 }
    }

    /// Most recently detected frequency in Hz.
    ///
    /// Returns `0.0` if no pitch has been detected yet or after [`reset`](Self::reset).
    #[inline]
    pub fn frequency(&self) -> f32 {
        unsafe { q_pd_get_frequency(self.ptr) }
    }

    /// Periodicity confidence for the last detected pitch, in [0, 1].
    ///
    /// Values ≥ 0.8 indicate a strongly tonal (periodic) signal.  Use this as
    /// a quality gate when deciding whether to trust the frequency reading.
    #[inline]
    pub fn periodicity(&self) -> f32 {
        unsafe { q_pd_get_periodicity(self.ptr) }
    }

    /// Reset the detector's internal state.
    ///
    /// Call this between songs, after a long silence, or whenever you want to
    /// discard accumulated pitch history.
    pub fn reset(&mut self) {
        unsafe { q_pd_reset(self.ptr) };
    }
}

impl Drop for QPitchDetector {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe { q_pd_destroy(self.ptr) };
        }
    }
}

impl std::fmt::Debug for QPitchDetector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QPitchDetector")
            .field("frequency_hz", &self.frequency())
            .field("periodicity", &self.periodicity())
            .finish()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;

    const SR: f32 = 44_100.0;

    #[test]
    fn creates_and_drops_without_crash() {
        let _pd = QPitchDetector::new_guitar(SR);
    }

    #[test]
    fn silence_gives_zero_frequency() {
        let mut pd = QPitchDetector::new_guitar(SR);
        for _ in 0..2048 {
            pd.process(0.0);
        }
        assert_eq!(pd.frequency(), 0.0);
    }

    #[test]
    fn detects_a4_sine_wave() {
        let mut pd = QPitchDetector::new_guitar(SR);
        // Q needs several pitch periods to lock on; feed ~2 s of A4.
        for i in 0..(SR as usize * 2) {
            let s = (TAU * 440.0 * i as f32 / SR).sin();
            pd.process(s);
        }
        let freq = pd.frequency();
        assert!(
            (freq - 440.0).abs() < 5.0,
            "Expected ~440 Hz, got {freq:.2} Hz"
        );
    }

    #[test]
    fn reset_zeroes_frequency() {
        let mut pd = QPitchDetector::new_guitar(SR);
        for i in 0..(SR as usize) {
            pd.process((TAU * 440.0 * i as f32 / SR).sin());
        }
        pd.reset();
        assert_eq!(pd.frequency(), 0.0);
    }
}
