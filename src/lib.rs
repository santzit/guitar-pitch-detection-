//! `guitar_pitch_detection` — Real-time polyphonic guitar pitch detection.
//!
//! # Architecture
//!
//! | Layer                  | Crate / module         | Purpose                                      |
//! |------------------------|------------------------|----------------------------------------------|
//! | Pitch detection        | `cycfi/q` (C++ via FFI)| BACF algorithm — fast, accurate, guitar-tuned|
//! | Polyphonic notes       | [`resonator`]          | IIR bank — all 53 semitones simultaneously   |
//! | Chord recognition      | [`chord`]              | Pattern-matches pitch-class sets             |
//! | Technique detection    | [`techniques`]         | Bend / slide / vibrato / palm-mute from Q    |
//! | Audio I/O (optional)   | [`audio_input`]        | rodio (WAV/MP3/…) + cpal (Real Tone Cable)   |
//! | C / GDExtension FFI    | [`ffi`]                | Exposes the detector to Godot / C callers    |
//!
//! # Quick start
//!
//! ```rust
//! use guitar_pitch_detection::GuitarPitchDetector;
//!
//! let mut detector = GuitarPitchDetector::new(44_100, 512);
//! let samples = vec![0.0_f32; 512]; // replace with real PCM data
//! let result = detector.process(&samples);
//!
//! for note in &result.notes {
//!     println!("{} ({:.1} Hz)", note.name, note.frequency);
//! }
//! if let Some(chord) = &result.chord {
//!     println!("Chord: {}", chord.name);
//! }
//! for technique in &result.techniques {
//!     println!("Technique: {technique:?}");
//! }
//! ```

pub mod chord;
pub mod detector;
pub mod ffi;
pub mod notes;
pub mod q_band;
pub mod q_pitch;
pub mod q_sys;
pub mod resonator;
pub mod techniques;
pub mod types;

#[cfg(feature = "audio_input")]
pub mod audio_input;

// Re-export the most commonly used items at the crate root.
pub use detector::{DetectorConfig, GuitarPitchDetector};
pub use q_band::QBandpassFilter;
pub use q_pitch::QPitchDetector;
pub use types::{ChordQuality, DetectedChord, DetectedNote, DetectionResult, GuitarTechnique};
