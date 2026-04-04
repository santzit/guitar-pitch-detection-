//! Complex resonator bank — the time-domain core of the pitch detector.
//!
//! A complex resonator tuned to frequency *f₀* is the first-order IIR filter
//!
//! ```text
//!   r[n] = x[n] + α · r[n-1] · e^(j·ω₀)
//! ```
//!
//! where `ω₀ = 2π f₀ / Fₛ` and `α ∈ (0, 1)` is a forgetting factor that
//! controls the decay time.  Energy at the resonator is `|r[n]|²`.
//!
//! ## Why complex resonators?
//!
//! The approach mirrors the algorithm described in
//! *"A Computationally Efficient Method for Polyphonic Pitch Estimation"*
//! (Zhou, Reiss, Mattavelli, Zoia) and implemented in the
//! [Polyphonic-Pitch-Detector-for-guitars](https://github.com/luciamarock/Polyphonic-Pitch-Detector-for-guitars)
//! project.  The key advantage over a plain FFT is that the resonator bank
//! is a **pure time-domain** calculation: no MATLAB, no external FFT library,
//! and negligible CPU footprint — it runs comfortably on a Raspberry Pi.

use std::f32::consts::TAU;

/// A single complex resonator.
#[derive(Debug, Clone)]
pub struct ComplexResonator {
    /// Centre frequency in Hz.
    pub frequency: f32,
    /// MIDI note this resonator is tuned to.
    pub midi_note: u8,
    /// Forgetting factor (0 < α < 1).  Default: [`DEFAULT_ALPHA`].
    #[allow(dead_code)]
    alpha: f32,
    /// Pre-computed `α · cos(ω₀)`.
    alpha_cos: f32,
    /// Pre-computed `α · sin(ω₀)`.
    alpha_sin: f32,
    /// Real part of the resonator state.
    real: f32,
    /// Imaginary part of the resonator state.
    imag: f32,
}

/// Default forgetting factor — gives ≈ 5 ms effective decay at 44 100 Hz and
/// a ~14 Hz half-power bandwidth, which is narrower than any semitone spacing
/// across the guitar range (smallest semitone at E2 ≈ 4.9 Hz; see notes.rs).
///
/// A narrower bandwidth is essential for reliable polyphonic detection: it
/// ensures that each sounding note produces a sharp, isolated energy peak and
/// that adjacent resonators (tuned to neighbouring semitones) do not receive
/// enough bleed energy to be mistaken for real notes.
///
/// Time constant τ = −1 / ln(α) ≈ 1000 samples ≈ 22 ms at 44 100 Hz.
pub const DEFAULT_ALPHA: f32 = 0.999;

impl ComplexResonator {
    /// Create a new resonator tuned to `frequency` Hz.
    pub fn new(frequency: f32, midi_note: u8, sample_rate: f32, alpha: f32) -> Self {
        let omega = TAU * frequency / sample_rate;
        Self {
            frequency,
            midi_note,
            alpha,
            alpha_cos: alpha * omega.cos(),
            alpha_sin: alpha * omega.sin(),
            real: 0.0,
            imag: 0.0,
        }
    }

    /// Feed one audio sample into the resonator.
    ///
    /// ```text
    ///   r[n] = x[n] + α · r[n-1] · e^(j·ω₀)
    /// ```
    ///
    /// Expanding into real/imaginary parts:
    /// ```text
    ///   real[n] = x[n] + α·cos(ω₀)·real[n-1] − α·sin(ω₀)·imag[n-1]
    ///   imag[n] =        α·sin(ω₀)·real[n-1] + α·cos(ω₀)·imag[n-1]
    /// ```
    #[inline]
    pub fn process(&mut self, sample: f32) {
        let new_real =
            sample + self.alpha_cos * self.real - self.alpha_sin * self.imag;
        let new_imag = self.alpha_sin * self.real + self.alpha_cos * self.imag;
        self.real = new_real;
        self.imag = new_imag;
    }

    /// Return the instantaneous energy `|r|² = real² + imag²`.
    #[inline]
    pub fn energy(&self) -> f32 {
        self.real * self.real + self.imag * self.imag
    }

    /// Reset internal state to zero (silence).
    #[inline]
    pub fn reset(&mut self) {
        self.real = 0.0;
        self.imag = 0.0;
    }
}

/// A bank of complex resonators covering the guitar frequency range.
///
/// One resonator is created per MIDI note in `[min_midi, max_midi]`.
#[derive(Debug, Clone)]
pub struct ResonatorBank {
    pub resonators: Vec<ComplexResonator>,
}

impl ResonatorBank {
    /// Build a bank of resonators covering every semitone from `min_midi`
    /// to `max_midi` (inclusive).
    pub fn new(min_midi: u8, max_midi: u8, sample_rate: f32, alpha: f32) -> Self {
        let resonators = (min_midi..=max_midi)
            .map(|midi| {
                let freq = crate::notes::midi_to_freq(midi);
                ComplexResonator::new(freq, midi, sample_rate, alpha)
            })
            .collect();
        Self { resonators }
    }

    /// Feed a slice of audio samples into every resonator in the bank.
    pub fn process_samples(&mut self, samples: &[f32]) {
        for &s in samples {
            for r in &mut self.resonators {
                r.process(s);
            }
        }
    }

    /// Return the current energy vector (one value per resonator).
    pub fn energies(&self) -> Vec<f32> {
        self.resonators.iter().map(|r| r.energy()).collect()
    }

    /// Reset all resonators.
    pub fn reset(&mut self) {
        for r in &mut self.resonators {
            r.reset();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn resonator_excites_at_tuned_frequency() {
        let sr = 44100.0_f32;
        let freq = 440.0_f32; // A4
        let mut r = ComplexResonator::new(freq, 69, sr, 0.9);

        // Feed a pure sine at 440 Hz for 1024 samples.
        for n in 0..1024_usize {
            let sample = (std::f32::consts::TAU * freq * n as f32 / sr).sin();
            r.process(sample);
        }
        // Resonator should have accumulated significant energy.
        assert!(r.energy() > 1.0, "energy={}", r.energy());
    }

    #[test]
    fn resonator_decays_to_silence() {
        let sr = 44100.0_f32;
        let freq = 220.0_f32;
        let mut r = ComplexResonator::new(freq, 57, sr, 0.9);

        // Excite with 256 samples of sine.
        for n in 0..256_usize {
            let s = (std::f32::consts::TAU * freq * n as f32 / sr).sin();
            r.process(s);
        }
        let excited = r.energy();

        // Feed silence for 1024 samples.
        for _ in 0..1024 {
            r.process(0.0);
        }
        let decayed = r.energy();

        // Energy should have dropped significantly.
        assert!(
            decayed < excited * 0.01,
            "decayed={decayed:.6e}, excited={excited:.6e}"
        );
    }

    #[test]
    fn bank_has_correct_size() {
        let bank = ResonatorBank::new(36, 88, 44100.0, DEFAULT_ALPHA);
        assert_eq!(bank.resonators.len(), (88 - 36 + 1) as usize);
    }

    #[test]
    fn resonator_energy_at_wrong_frequency_is_low() {
        let sr = 44100.0_f32;
        // Use a high alpha so the resonator has a narrow bandwidth
        // (~14 Hz half-power BW), making it selective enough to discriminate
        // A4 (440 Hz) from E4 (329.63 Hz), which are 110 Hz apart.
        let narrow_alpha = 0.998_f32;

        // Resonator tuned to A4 (440 Hz), fed an E4 (329.63 Hz) signal.
        let mut r_a4 = ComplexResonator::new(440.0, 69, sr, narrow_alpha);
        // Resonator tuned to E4 (329.63 Hz), fed an E4 signal.
        let mut r_e4 = ComplexResonator::new(329.63, 64, sr, narrow_alpha);

        let e4 = 329.63_f32;
        // Use enough samples to approach steady state (α^N → 0 requires N >> τ).
        // τ = -1/ln(0.998) ≈ 500 samples; 8192 ≫ τ.
        for n in 0..8192_usize {
            let s = (std::f32::consts::TAU * e4 * n as f32 / sr).sin();
            r_a4.process(s);
            r_e4.process(s);
        }

        // The correctly-tuned resonator should have much more energy.
        assert!(
            r_e4.energy() > r_a4.energy() * 5.0,
            "r_e4={:.2e}, r_a4={:.2e}",
            r_e4.energy(),
            r_a4.energy()
        );
    }

    #[test]
    fn resonator_reset_clears_state() {
        let sr = 44100.0_f32;
        let freq = 196.0_f32;
        let mut r = ComplexResonator::new(freq, 55, sr, DEFAULT_ALPHA);
        for n in 0..512_usize {
            r.process((std::f32::consts::TAU * freq * n as f32 / sr).sin());
        }
        assert!(r.energy() > 0.0);
        r.reset();
        assert_relative_eq!(r.energy(), 0.0, epsilon = 1e-30);
    }
}
