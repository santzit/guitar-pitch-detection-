//! WAV-based tests for the six major guitar chords: A, B, C, D, E, F.
//!
//! Each test:
//! 1. Synthesises the chord as a sum of three pure sine waves (root, major
//!    third, perfect fifth) at equal amplitude.
//! 2. Encodes the audio as a standard 16-bit PCM mono WAV file and writes it
//!    to `tests/fixtures/<Name>_major.wav` so it can be inspected or played
//!    back externally with any audio player.
//! 3. Decodes the WAV bytes back to f32 samples and feeds them to the
//!    `GuitarPitchDetector`.
//! 4. Asserts that every pitch class belonging to the chord (root, major
//!    third, perfect fifth) is detected, and that the chord is named correctly.
//!
//! The WAV encoding/decoding is pure Rust — no external crates required.

use guitar_pitch_detection::{ChordQuality, GuitarPitchDetector};
use std::f32::consts::TAU;
use std::path::PathBuf;

const SR: u32 = 44_100;
const SR_F: f32 = SR as f32;

/// Duration of each generated chord WAV (1 second = 44 100 samples).
/// A longer window lets the resonators reach steady-state cleanly.
const CHORD_SECS: f32 = 1.0;
const N_SAMPLES: usize = (SR_F * CHORD_SECS) as usize;

// ── Named frequency constants (equal-temperament, A4 = 440 Hz) ───────────────

// Octave 3
const D3_HZ: f32 = 146.83; // MIDI 50 — root of D Major
const E3_HZ: f32 = 164.81; // MIDI 52 — root of E Major
const F3_HZ: f32 = 174.61; // MIDI 53 — root of F Major
const F_SHARP_3_HZ: f32 = 185.00; // MIDI 54 — major 3rd of D Major
const G_SHARP_3_HZ: f32 = 207.65; // MIDI 56 — major 3rd of E Major
const A3_HZ: f32 = 220.00; // MIDI 57 — root of A Major / 5th of D & F Major
const B3_HZ: f32 = 246.94; // MIDI 59 — root of B Major / 5th of E Major

// Octave 4
const C4_HZ: f32 = 261.63; // MIDI 60 — root of C Major / 5th of F Major
const C_SHARP_4_HZ: f32 = 277.18; // MIDI 61 — major 3rd of A Major
const D_SHARP_4_HZ: f32 = 311.13; // MIDI 63 — major 3rd of B Major
const E4_HZ: f32 = 329.63; // MIDI 64 — major 3rd of C Major / 5th of A Major
const F_SHARP_4_HZ: f32 = 369.99; // MIDI 66 — major 3rd of B Major (5th)
const G4_HZ: f32 = 392.00; // MIDI 67 — perfect 5th of C Major

/// Maximum absolute error introduced by rounding f32 → i16 → f32.
///
/// A 16-bit signed sample spans ±32 767.  The quantisation step is
/// `1 / 32 768 ≈ 3.05 × 10⁻⁵`.  We allow two steps to account for the
/// conversion in both directions (encode then decode).
const I16_QUANTIZATION_TOLERANCE: f32 = 2.0 / 32_768.0;

// ── Minimal pure-Rust WAV writer / reader ────────────────────────────────────

/// Encode f32 samples as a mono 16-bit PCM WAV file in a `Vec<u8>`.
///
/// The 44-byte header describes standard PCM (format tag 1), mono channel,
/// 44 100 Hz sample rate, 16 bits per sample.
fn encode_wav_i16(samples: &[f32], sample_rate: u32) -> Vec<u8> {
    let pcm: Vec<i16> = samples
        .iter()
        .map(|&x| (x.clamp(-1.0, 1.0) * 32_767.0) as i16)
        .collect();

    let data_len = (pcm.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);

    // RIFF chunk descriptor
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes()); // ChunkSize
    out.extend_from_slice(b"WAVE");

    // fmt sub-chunk (PCM = format tag 1)
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // Subchunk1Size
    out.extend_from_slice(&1u16.to_le_bytes()); // AudioFormat: PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // NumChannels: mono
    out.extend_from_slice(&sample_rate.to_le_bytes()); // SampleRate
    out.extend_from_slice(&(sample_rate * 2).to_le_bytes()); // ByteRate
    out.extend_from_slice(&2u16.to_le_bytes()); // BlockAlign
    out.extend_from_slice(&16u16.to_le_bytes()); // BitsPerSample

    // data sub-chunk
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in pcm {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

/// Decode a mono 16-bit PCM WAV `&[u8]` back to f32 samples in [-1.0, 1.0].
///
/// Expects the standard 44-byte header written by [`encode_wav_i16`].
fn decode_wav_i16(wav: &[u8]) -> Vec<f32> {
    assert!(wav.len() >= 44, "WAV data too short (expected ≥ 44 bytes)");
    wav[44..]
        .chunks_exact(2)
        .map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32_768.0)
        .collect()
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Return the `tests/fixtures/` directory (absolute path).
fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

/// Synthesise a major chord, save as WAV, and return decoded f32 samples.
///
/// `name`  – chord root letter, e.g. `"A"` (used for the fixture filename).
/// `freqs` – [root_hz, major_third_hz, fifth_hz]
///
/// Side effect: writes `tests/fixtures/{name}_major.wav` for external playback.
fn chord_wav_samples(name: &str, freqs: &[f32; 3]) -> Vec<f32> {
    // Generate equal-amplitude sine mix, normalized to [-1, 1].
    let n_components = freqs.len() as f32;
    let f32_samples: Vec<f32> = (0..N_SAMPLES)
        .map(|i| {
            freqs
                .iter()
                .map(|&f| (TAU * f * i as f32 / SR_F).sin())
                .sum::<f32>()
                / n_components
        })
        .collect();

    let wav_bytes = encode_wav_i16(&f32_samples, SR);

    // Write fixture file so it can be played back with any WAV player.
    let dir = fixture_dir();
    if std::fs::create_dir_all(&dir).is_ok() {
        let path = dir.join(format!("{name}_major.wav"));
        let _ = std::fs::write(&path, &wav_bytes);
    }

    // Round-trip: decode the WAV bytes back to f32 (exercises the WAV reader).
    decode_wav_i16(&wav_bytes)
}

/// Run the pitch detector on a slice of f32 samples.
fn run_detector(samples: &[f32]) -> guitar_pitch_detection::DetectionResult {
    let mut det = GuitarPitchDetector::new(SR, samples.len());
    det.process(samples)
}

/// Assert that the three pitch classes of a major chord are all present in the
/// detection result, and that the chord is identified with the correct name.
///
/// Pitch-class layout for a major chord rooted at `root`:
/// - root         (interval 0)
/// - major third  (interval 4)
/// - perfect fifth(interval 7)
fn assert_major_chord_detected(
    result: &guitar_pitch_detection::DetectionResult,
    root_pc: u8,
    chord_name: &str,
) {
    assert!(
        !result.notes.is_empty(),
        "{chord_name} Major: no notes detected at all"
    );

    let detected_pcs: std::collections::HashSet<u8> =
        result.notes.iter().map(|n| n.semitone).collect();

    let expected_pcs = [root_pc % 12, (root_pc + 4) % 12, (root_pc + 7) % 12];
    for &pc in &expected_pcs {
        assert!(
            detected_pcs.contains(&pc),
            "{chord_name} Major: pitch class {pc} not detected; got {:?}",
            detected_pcs
        );
    }

    let chord = result
        .chord
        .as_ref()
        .unwrap_or_else(|| panic!("{chord_name} Major: no chord detected; notes={detected_pcs:?}"));

    assert_eq!(
        chord.quality,
        ChordQuality::Major,
        "{chord_name}: expected Major quality, got {:?}",
        chord.quality
    );
    assert_eq!(
        chord.root, root_pc,
        "{chord_name}: expected root pitch-class {root_pc}, got {}",
        chord.root
    );
    assert_eq!(
        chord.name, chord_name,
        "Expected chord name '{chord_name}', got '{}'",
        chord.name
    );
}

// ── WAV chord tests ───────────────────────────────────────────────────────────
//
// Voicing reference (close-position, guitar-range octaves):
//
//   Chord │ Root          │ Major 3rd      │ Perfect 5th
//   ──────┼───────────────┼────────────────┼─────────────
//   A     │ A3  220.00 Hz │ C#4 277.18 Hz  │ E4  329.63 Hz
//   B     │ B3  246.94 Hz │ D#4 311.13 Hz  │ F#4 369.99 Hz
//   C     │ C4  261.63 Hz │ E4  329.63 Hz  │ G4  392.00 Hz
//   D     │ D3  146.83 Hz │ F#3 185.00 Hz  │ A3  220.00 Hz
//   E     │ E3  164.81 Hz │ G#3 207.65 Hz  │ B3  246.94 Hz
//   F     │ F3  174.61 Hz │ A3  220.00 Hz  │ C4  261.63 Hz

/// A Major: A3 (220.00 Hz) + C#4 (277.18 Hz) + E4 (329.63 Hz)
///
/// Pitch classes: A=9, C#=1, E=4
#[test]
fn wav_a_major() {
    let samples = chord_wav_samples("A", &[A3_HZ, C_SHARP_4_HZ, E4_HZ]);
    let result = run_detector(&samples);
    assert_major_chord_detected(&result, 9, "A");
}

/// B Major: B3 (246.94 Hz) + D#4 (311.13 Hz) + F#4 (369.99 Hz)
///
/// Pitch classes: B=11, D#=3, F#=6
#[test]
fn wav_b_major() {
    let samples = chord_wav_samples("B", &[B3_HZ, D_SHARP_4_HZ, F_SHARP_4_HZ]);
    let result = run_detector(&samples);
    assert_major_chord_detected(&result, 11, "B");
}

/// C Major: C4 (261.63 Hz) + E4 (329.63 Hz) + G4 (392.00 Hz)
///
/// Pitch classes: C=0, E=4, G=7
#[test]
fn wav_c_major() {
    let samples = chord_wav_samples("C", &[C4_HZ, E4_HZ, G4_HZ]);
    let result = run_detector(&samples);
    assert_major_chord_detected(&result, 0, "C");
}

/// D Major: D3 (146.83 Hz) + F#3 (185.00 Hz) + A3 (220.00 Hz)
///
/// Pitch classes: D=2, F#=6, A=9
#[test]
fn wav_d_major() {
    let samples = chord_wav_samples("D", &[D3_HZ, F_SHARP_3_HZ, A3_HZ]);
    let result = run_detector(&samples);
    assert_major_chord_detected(&result, 2, "D");
}

/// E Major: E3 (164.81 Hz) + G#3 (207.65 Hz) + B3 (246.94 Hz)
///
/// Pitch classes: E=4, G#=8, B=11
#[test]
fn wav_e_major() {
    let samples = chord_wav_samples("E", &[E3_HZ, G_SHARP_3_HZ, B3_HZ]);
    let result = run_detector(&samples);
    assert_major_chord_detected(&result, 4, "E");
}

/// F Major: F3 (174.61 Hz) + A3 (220.00 Hz) + C4 (261.63 Hz)
///
/// Pitch classes: F=5, A=9, C=0
#[test]
fn wav_f_major() {
    let samples = chord_wav_samples("F", &[F3_HZ, A3_HZ, C4_HZ]);
    let result = run_detector(&samples);
    assert_major_chord_detected(&result, 5, "F");
}

// ── WAV round-trip sanity check ───────────────────────────────────────────────

/// Verify that the WAV encoder and decoder are inverses of each other.
///
/// Encodes a known sine wave to WAV bytes and decodes it back; the recovered
/// samples should match the originals within the 16-bit quantisation error
/// (~3 × 10⁻⁵).
#[test]
fn wav_encode_decode_roundtrip() {
    let original: Vec<f32> = (0..1024)
        .map(|i| (TAU * 440.0 * i as f32 / SR_F).sin())
        .collect();

    let wav_bytes = encode_wav_i16(&original, SR);
    let decoded = decode_wav_i16(&wav_bytes);

    assert_eq!(decoded.len(), original.len());
    for (i, (&orig, &dec)) in original.iter().zip(decoded.iter()).enumerate() {
        let err = (orig - dec).abs();
        assert!(
            err < I16_QUANTIZATION_TOLERANCE,
            "sample {i}: original={orig:.6}, decoded={dec:.6}, err={err:.6e}"
        );
    }
}
