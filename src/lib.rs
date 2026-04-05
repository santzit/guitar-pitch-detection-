//! `guitar_pitch_detection` — Rust library for real-time polyphonic guitar
//! pitch detection.
//!
//! # Overview
//!
//! The crate implements a **complex resonator bank** — the same core algorithm
//! used in the [Polyphonic-Pitch-Detector-for-guitars](https://github.com/luciamarock/Polyphonic-Pitch-Detector-for-guitars)
//! project — entirely in Rust, with no external dependencies or MATLAB.
//!
//! One resonator is tuned to each semitone in the guitar range (C2–E6).
//! Processing a frame of audio samples through the bank takes O(N_notes ×
//! N_samples) operations and runs well below real-time on any modern CPU,
//! including Raspberry Pi.
//!
//! # Entry points
//!
//! | Use case                     | Type / function                         |
//! |------------------------------|-----------------------------------------|
//! | Rust application / Godot RS  | [`GuitarPitchDetector`]                 |
//! | Custom configuration         | [`detector::DetectorConfig`]            |
//! | Bend / vibrato detection     | [`modulation::ModulationAnalyzer`]      |
//! | Pitch shift (capo / tuning)  | [`modulation::shift_result`]            |
//! | C / GDExtension FFI          | [`ffi::gpd_create`] …                   |
//!
//! # Example
//!
//! ```rust
//! use guitar_pitch_detection::{GuitarPitchDetector, modulation::shift_result};
//!
//! let mut detector = GuitarPitchDetector::new(44_100, 512);
//! let samples = vec![0.0_f32; 512]; // replace with real PCM data
//! let result = detector.process(&samples);
//!
//! for note in &result.notes {
//!     println!("{} ({:.1} Hz) — {:?}", note.name, note.frequency, note.modulation);
//! }
//! if let Some(chord) = &result.chord {
//!     println!("Chord: {}", chord.name);
//! }
//!
//! // Capo on fret 2: shift every detected note down by 2 semitones.
//! let concert_pitch = shift_result(&result, -2);
//! ```

pub mod chord;
pub mod detector;
pub mod ffi;
pub mod modulation;
pub mod notes;
pub mod resonator;
pub mod types;

// Re-export the most commonly used items at the crate root.
pub use detector::{DetectorConfig, GuitarPitchDetector};
pub use modulation::{shift_result, ModulationAnalyzer, ModulationConfig};
pub use types::{ChordQuality, DetectedChord, DetectedNote, DetectionResult, PitchModulation};
