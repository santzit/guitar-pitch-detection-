//! Safe Rust wrapper around cycfi/q band-pass filters.

use crate::q_sys::{
    q_bp_config, q_bp_create, q_bp_destroy, q_bp_process, q_bp_reset, QBandpassFilterHandle,
};

/// Safe wrapper around Q's `bandpass_csg`.
pub struct QBandpassFilter {
    ptr: *mut QBandpassFilterHandle,
    center_freq_hz: f32,
    sample_rate: f32,
    q_factor: f32,
}

unsafe impl Send for QBandpassFilter {}
unsafe impl Sync for QBandpassFilter {}

impl QBandpassFilter {
    /// Construct a new band-pass filter.
    pub fn new(center_freq_hz: f32, sample_rate: f32, q_factor: f32) -> Self {
        let ptr = unsafe { q_bp_create(center_freq_hz, sample_rate, q_factor) };
        assert!(!ptr.is_null(), "q_bp_create returned null — out of memory?");
        Self {
            ptr,
            center_freq_hz,
            sample_rate,
            q_factor,
        }
    }

    /// Update center frequency / sample rate / Q.
    pub fn config(&mut self, center_freq_hz: f32, sample_rate: f32, q_factor: f32) {
        unsafe { q_bp_config(self.ptr, center_freq_hz, sample_rate, q_factor) };
        self.center_freq_hz = center_freq_hz;
        self.sample_rate = sample_rate;
        self.q_factor = q_factor;
    }

    /// Process one mono sample.
    #[inline]
    pub fn process(&mut self, sample: f32) -> f32 {
        unsafe { q_bp_process(self.ptr, sample) }
    }

    /// Process an entire frame and return the filtered samples.
    pub fn process_frame(&mut self, samples: &[f32]) -> Vec<f32> {
        samples.iter().map(|&s| self.process(s)).collect()
    }

    /// Reset delay-line state.
    pub fn reset(&mut self) {
        unsafe { q_bp_reset(self.ptr) };
    }

    /// Current center frequency in Hz.
    pub fn center_freq_hz(&self) -> f32 {
        self.center_freq_hz
    }

    /// Current Q factor.
    pub fn q_factor(&self) -> f32 {
        self.q_factor
    }
}

impl Drop for QBandpassFilter {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe { q_bp_destroy(self.ptr) };
        }
    }
}

impl std::fmt::Debug for QBandpassFilter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QBandpassFilter")
            .field("center_freq_hz", &self.center_freq_hz)
            .field("sample_rate", &self.sample_rate)
            .field("q_factor", &self.q_factor)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;

    const SR: f32 = 44_100.0;

    fn sine(freq: f32, n_samples: usize) -> Vec<f32> {
        (0..n_samples)
            .map(|n| (TAU * freq * n as f32 / SR).sin())
            .collect()
    }

    fn rms(samples: &[f32]) -> f32 {
        let power = samples.iter().map(|x| x * x).sum::<f32>() / samples.len().max(1) as f32;
        power.sqrt()
    }

    #[test]
    fn creates_and_processes() {
        let mut bp = QBandpassFilter::new(82.41, SR, 6.0);
        let out = bp.process(0.5);
        assert!(out.is_finite());
    }

    #[test]
    fn centered_signal_has_higher_energy() {
        let n = 8_192;
        let low_e = sine(82.41, n);
        let high_e = sine(329.63, n);

        let mut bp = QBandpassFilter::new(82.41, SR, 8.0);
        let low_out = bp.process_frame(&low_e);
        bp.reset();
        let high_out = bp.process_frame(&high_e);

        let low_rms = rms(&low_out);
        let high_rms = rms(&high_out);

        assert!(
            low_rms > high_rms * 2.0,
            "Expected 82.41 Hz RMS > 2x 329.63 Hz RMS, got low={low_rms:.4}, high={high_rms:.4}"
        );
    }
}
