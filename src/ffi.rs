//! C-compatible FFI layer for Godot GDExtension (and any other C host).
//!
//! ## How to use from Godot 4
//!
//! 1. Build the shared library:
//!    ```bash
//!    cargo build --release
//!    # Linux:   target/release/libguitar_pitch_detection.so
//!    # macOS:   target/release/libguitar_pitch_detection.dylib
//!    # Windows: target/release/guitar_pitch_detection.dll
//!    ```
//! 2. In your GDScript / GDExtension binding, load the library and call the
//!    functions below via `NativeLibrary` + a `[GDExtension]` description, or
//!    use GDScript's `OS.load_dll` for quick prototyping.
//!
//! ## Memory ownership
//!
//! - [`gpd_create`] allocates a `GuitarPitchDetector` on the heap and returns
//!   a raw pointer.  **The caller owns this pointer** and must eventually pass
//!   it to [`gpd_destroy`].
//! - [`gpd_process`] writes results into a caller-provided [`CDetectionResult`]
//!   struct — no heap allocation during detection.

use crate::{detector::GuitarPitchDetector, types::ChordQuality};
use std::ffi::CStr;
use std::os::raw::{c_char, c_int};

/// C-compatible representation of a single detected note.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CNote {
    /// Detected frequency in Hz.
    pub frequency: f32,
    /// MIDI note number (40 = E2, 69 = A4, …).
    pub midi_note: u8,
    /// Pitch class 0–11.
    pub semitone: u8,
    /// Octave number (2 for E2, 4 for A4, …).
    pub octave: i8,
    /// Null-terminated note name, e.g. b"E2\0".  Max 7 chars + NUL.
    pub name: [c_char; 8],
    /// Normalized confidence 0.0–1.0.
    pub confidence: f32,
}

impl Default for CNote {
    fn default() -> Self {
        Self {
            frequency: 0.0,
            midi_note: 0,
            semitone: 0,
            octave: 0,
            name: [0; 8],
            confidence: 0.0,
        }
    }
}

/// C-compatible detection result written by [`gpd_process`].
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CDetectionResult {
    /// Number of valid entries in `notes` (0–6).
    pub note_count: u32,
    /// Up to 6 simultaneous notes (guitar has 6 strings).
    pub notes: [CNote; 6],
    /// 1 if a chord was recognised, 0 otherwise.
    pub has_chord: u8,
    /// Null-terminated chord name, e.g. b"Am\0".  Max 15 chars + NUL.
    pub chord_name: [c_char; 16],
    /// Root pitch class of the chord (0–11), or 255 if none.
    pub chord_root: u8,
    /// Chord quality as an integer — see `gpd_chord_quality_*` constants.
    pub chord_quality: u8,
    /// Chord confidence 0.0–1.0.
    pub chord_confidence: f32,
}

impl Default for CDetectionResult {
    fn default() -> Self {
        Self {
            note_count: 0,
            notes: [CNote::default(); 6],
            has_chord: 0,
            chord_name: [0; 16],
            chord_root: 255,
            chord_quality: 0,
            chord_confidence: 0.0,
        }
    }
}

/// Chord quality integer codes used in [`CDetectionResult::chord_quality`].
pub mod chord_quality_code {
    pub const MAJOR: u8 = 0;
    pub const MINOR: u8 = 1;
    pub const DOMINANT7: u8 = 2;
    pub const MAJOR7: u8 = 3;
    pub const MINOR7: u8 = 4;
    pub const DIMINISHED: u8 = 5;
    pub const AUGMENTED: u8 = 6;
    pub const SUS2: u8 = 7;
    pub const SUS4: u8 = 8;
    pub const POWER: u8 = 9;
}

fn quality_to_code(q: &ChordQuality) -> u8 {
    match q {
        ChordQuality::Major => chord_quality_code::MAJOR,
        ChordQuality::Minor => chord_quality_code::MINOR,
        ChordQuality::Dominant7 => chord_quality_code::DOMINANT7,
        ChordQuality::Major7 => chord_quality_code::MAJOR7,
        ChordQuality::Minor7 => chord_quality_code::MINOR7,
        ChordQuality::Diminished => chord_quality_code::DIMINISHED,
        ChordQuality::Augmented => chord_quality_code::AUGMENTED,
        ChordQuality::Sus2 => chord_quality_code::SUS2,
        ChordQuality::Sus4 => chord_quality_code::SUS4,
        ChordQuality::Power => chord_quality_code::POWER,
    }
}

/// Write a Rust `&str` into a fixed-size null-terminated C char array.
///
/// **Silent truncation**: if `s` is longer than `buf.len() - 1` bytes,
/// the string is silently truncated to fit.  Callers must ensure that the
/// buffer is large enough for all expected values.  The fixed-size buffers
/// in [`CDetectionResult`] are dimensioned to hold the longest possible
/// chord name (e.g. "C#maj7" = 6 bytes) and note name (e.g. "A#4" = 3
/// bytes), so truncation should never occur in normal use.
fn str_to_c_buf(s: &str, buf: &mut [c_char]) {
    let bytes = s.as_bytes();
    let len = bytes.len().min(buf.len() - 1);
    for (i, &b) in bytes[..len].iter().enumerate() {
        buf[i] = b as c_char;
    }
    buf[len] = 0;
}

// ── Public C API ─────────────────────────────────────────────────────────────

/// Allocate and return a new `GuitarPitchDetector`.
///
/// # Parameters
/// - `sample_rate`: Audio sample rate in Hz (e.g. 44100).
/// - `frame_size`: Typical number of samples per [`gpd_process`] call.
///   This is advisory — frames of any size are accepted.
///
/// # Returns
/// Opaque pointer to the detector.  Must be freed with [`gpd_destroy`].
///
/// # Safety
/// The returned pointer is valid until [`gpd_destroy`] is called.
#[no_mangle]
pub extern "C" fn gpd_create(sample_rate: u32, frame_size: u32) -> *mut GuitarPitchDetector {
    let detector = Box::new(GuitarPitchDetector::new(sample_rate, frame_size as usize));
    Box::into_raw(detector)
}

/// Free a detector previously created by [`gpd_create`].
///
/// # Safety
/// `detector` must be a valid, non-null pointer returned by [`gpd_create`]
/// that has not already been freed.
#[no_mangle]
pub unsafe extern "C" fn gpd_destroy(detector: *mut GuitarPitchDetector) {
    if !detector.is_null() {
        drop(Box::from_raw(detector));
    }
}

/// Process a frame of audio samples and write pitch-detection results.
///
/// # Parameters
/// - `detector`: Pointer returned by [`gpd_create`].
/// - `samples`: Pointer to `sample_count` mono f32 PCM samples (−1.0 … 1.0).
/// - `sample_count`: Number of samples in `samples`.
/// - `result`: Output parameter — filled with detected notes and chord.
///
/// # Returns
/// `0` on success, `-1` on null-pointer error.
///
/// # Safety
/// - `detector` must be a valid non-null pointer from [`gpd_create`].
/// - `samples` must point to at least `sample_count` valid `f32` values.
/// - `result` must point to a valid, writable [`CDetectionResult`].
#[no_mangle]
pub unsafe extern "C" fn gpd_process(
    detector: *mut GuitarPitchDetector,
    samples: *const f32,
    sample_count: u32,
    result: *mut CDetectionResult,
) -> c_int {
    if detector.is_null() || samples.is_null() || result.is_null() {
        return -1;
    }

    let det = &mut *detector;
    let slice = std::slice::from_raw_parts(samples, sample_count as usize);
    let detection = det.process(slice);

    let out = &mut *result;
    *out = CDetectionResult::default();

    let count = detection.notes.len().min(6);
    out.note_count = count as u32;
    for (i, note) in detection.notes.iter().take(6).enumerate() {
        out.notes[i].frequency = note.frequency;
        out.notes[i].midi_note = note.midi_note;
        out.notes[i].semitone = note.semitone;
        out.notes[i].octave = note.octave;
        out.notes[i].confidence = note.confidence;
        str_to_c_buf(note.name, &mut out.notes[i].name);
    }

    if let Some(chord) = &detection.chord {
        out.has_chord = 1;
        out.chord_root = chord.root;
        out.chord_quality = quality_to_code(&chord.quality);
        out.chord_confidence = chord.confidence;
        str_to_c_buf(&chord.name, &mut out.chord_name);
    }

    0
}

/// Reset all resonator states in the detector (silence it).
///
/// Call between songs or after a long break to avoid stale energy.
///
/// # Safety
/// `detector` must be a valid non-null pointer from [`gpd_create`].
#[no_mangle]
pub unsafe extern "C" fn gpd_reset(detector: *mut GuitarPitchDetector) {
    if !detector.is_null() {
        (*detector).reset();
    }
}

/// Read back the chord name from a [`CDetectionResult`] as a Rust string slice.
///
/// Utility exposed for unit-testing the FFI layer from Rust.
pub fn chord_name_from_result(result: &CDetectionResult) -> &str {
    let cstr = unsafe { CStr::from_ptr(result.chord_name.as_ptr()) };
    cstr.to_str().unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;

    fn sine_samples(freq: f32, n: usize, sr: f32) -> Vec<f32> {
        (0..n)
            .map(|i| (TAU * freq * i as f32 / sr).sin())
            .collect()
    }

    #[test]
    fn ffi_create_and_destroy() {
        let ptr = gpd_create(44100, 512);
        assert!(!ptr.is_null());
        unsafe { gpd_destroy(ptr) };
    }

    #[test]
    fn ffi_process_silence_gives_no_notes() {
        let ptr = gpd_create(44100, 512);
        let silence = vec![0.0_f32; 512];
        let mut result = CDetectionResult::default();
        let rc = unsafe {
            gpd_process(ptr, silence.as_ptr(), silence.len() as u32, &mut result)
        };
        assert_eq!(rc, 0);
        assert_eq!(result.note_count, 0);
        assert_eq!(result.has_chord, 0);
        unsafe { gpd_destroy(ptr) };
    }

    #[test]
    fn ffi_process_a4_detects_note() {
        let ptr = gpd_create(44100, 4096);
        let samples = sine_samples(440.0, 4096, 44100.0);
        let mut result = CDetectionResult::default();
        let rc = unsafe {
            gpd_process(ptr, samples.as_ptr(), samples.len() as u32, &mut result)
        };
        assert_eq!(rc, 0);
        assert!(result.note_count > 0, "Expected at least one note");
        unsafe { gpd_destroy(ptr) };
    }

    #[test]
    fn ffi_null_detector_returns_error() {
        let mut result = CDetectionResult::default();
        let samples = vec![0.0_f32; 128];
        let rc = unsafe {
            gpd_process(
                std::ptr::null_mut(),
                samples.as_ptr(),
                samples.len() as u32,
                &mut result,
            )
        };
        assert_eq!(rc, -1);
    }

    #[test]
    fn ffi_reset_clears_state() {
        let ptr = gpd_create(44100, 512);
        let samples = sine_samples(440.0, 2048, 44100.0);
        let mut result = CDetectionResult::default();
        unsafe { gpd_process(ptr, samples.as_ptr(), samples.len() as u32, &mut result) };
        unsafe { gpd_reset(ptr) };
        let silence = vec![0.0_f32; 512];
        unsafe { gpd_process(ptr, silence.as_ptr(), silence.len() as u32, &mut result) };
        assert_eq!(result.note_count, 0);
        unsafe { gpd_destroy(ptr) };
    }
}
