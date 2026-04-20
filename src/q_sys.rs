//! Raw unsafe FFI bindings to the cycfi/q C++ pitch detector.
//!
//! These declarations mirror `src/q_wrapper.hpp` exactly.  All safe usage
//! lives in [`crate::q_pitch`].

use std::os::raw::{c_float, c_int};

/// Opaque C++ object.  Never construct or dereference from Rust.
#[repr(C)]
pub struct QPitchDetectorHandle {
    _private: [u8; 0],
}

/// Opaque C++ band-pass filter object.
#[repr(C)]
pub struct QBandpassFilterHandle {
    _private: [u8; 0],
}

extern "C" {
    /// Create a pitch detector for the given frequency range and sample rate.
    ///
    /// Returns `null` on allocation failure (extremely rare).
    pub fn q_pd_create(
        min_freq_hz: c_float,
        max_freq_hz: c_float,
        sample_rate: c_float,
        hysteresis_db: c_float,
    ) -> *mut QPitchDetectorHandle;

    /// Free a detector.  Safe to call with `null`.
    pub fn q_pd_destroy(pd: *mut QPitchDetectorHandle);

    /// Feed one audio sample.
    ///
    /// Returns 1 when a new pitch period has been analysed; 0 otherwise.
    pub fn q_pd_process(pd: *mut QPitchDetectorHandle, sample: c_float) -> c_int;

    /// Most recently detected frequency in Hz (0 if nothing detected yet).
    pub fn q_pd_get_frequency(pd: *mut QPitchDetectorHandle) -> c_float;

    /// Periodicity confidence in [0, 1] (higher = more periodic = more confident).
    pub fn q_pd_get_periodicity(pd: *mut QPitchDetectorHandle) -> c_float;

    /// Reset internal state.
    pub fn q_pd_reset(pd: *mut QPitchDetectorHandle);

    /// Create a Q constant-skirt-gain band-pass filter.
    pub fn q_bp_create(
        center_freq_hz: c_float,
        sample_rate: c_float,
        q_factor: c_float,
    ) -> *mut QBandpassFilterHandle;

    /// Free a band-pass filter. Safe to call with `null`.
    pub fn q_bp_destroy(bp: *mut QBandpassFilterHandle);

    /// Reconfigure band-pass filter center frequency / Q.
    pub fn q_bp_config(
        bp: *mut QBandpassFilterHandle,
        center_freq_hz: c_float,
        sample_rate: c_float,
        q_factor: c_float,
    );

    /// Feed one sample through the band-pass filter.
    pub fn q_bp_process(bp: *mut QBandpassFilterHandle, sample: c_float) -> c_float;

    /// Reset band-pass filter delay buffers.
    pub fn q_bp_reset(bp: *mut QBandpassFilterHandle);
}
