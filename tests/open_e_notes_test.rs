//! Tests that use a 48 kHz / 16-bit WAV file containing the six notes
//! A, B, C, D, E, F (the note names listed in the issue, in alphabetical order).
//! The corresponding pitches span the lower guitar range:
//!
//! | Label | Pitch | Frequency  | MIDI |
//! |-------|-------|------------|------|
//! | A     | A3    | 220.00 Hz  |  57  |
//! | B     | B3    | 246.94 Hz  |  59  |
//! | C     | C4    | 261.63 Hz  |  60  |
//! | D     | D3    | 146.83 Hz  |  50  |
//! | E     | E3    | 164.81 Hz  |  52  |
//! | F     | F3    | 174.61 Hz  |  53  |
//!
//! # WAV file structure — `tests/fixtures/open_e_notes_48k.wav`
//!
//! ```text
//! [A×8 bursts] [gap] [B×8 bursts] [gap] [C×8 bursts]
//!     [gap] [D×8 bursts] [gap] [E×8 bursts] [gap] [F×8 bursts]
//! ```
//!
//! Each "burst" is a 0.5-second sine tone.  Consecutive bursts within the
//! same note are separated by 0.2 seconds of silence.  The last burst of
//! each note section carries **no** trailing silence so that the resonator
//! energies remain high at the moment `process()` returns.  Between
//! consecutive notes there is a 1.0-second gap.
//!
//! The file is generated once per test binary run (via [`OnceLock`]) at
//! 48 000 Hz / 16-bit mono PCM and written to `tests/fixtures/` for
//! external playback or inspection.  Each individual note test then loads
//! the saved file from disk, extracts the corresponding sample range, and
//! verifies that the pitch detector correctly identifies the note.

use guitar_pitch_detection::{DetectorConfig, GuitarPitchDetector};
use std::f32::consts::TAU;
use std::path::PathBuf;
use std::sync::OnceLock;

// ── Audio parameters ──────────────────────────────────────────────────────────

/// WAV sample rate (Hz).
const SR: u32 = 48_000;
const SR_F: f32 = SR as f32;

/// Number of repeated tone bursts per note.
const REPEATS: usize = 8;

/// Duration of each tone burst (seconds).
const NOTE_ON_SECS: f32 = 0.5;

/// Silence between consecutive bursts within the same note section (seconds).
const NOTE_OFF_SECS: f32 = 0.2;

/// Silence gap between different notes (seconds).
const NOTE_GAP_SECS: f32 = 1.0;

// ── Note frequencies (equal temperament, A4 = 440 Hz) ────────────────────────

const A3_HZ: f32 = 220.00; // MIDI 57
const B3_HZ: f32 = 246.94; // MIDI 59
const C4_HZ: f32 = 261.63; // MIDI 60
const D3_HZ: f32 = 146.83; // MIDI 50
const E3_HZ: f32 = 164.81; // MIDI 52
const F3_HZ: f32 = 174.61; // MIDI 53

// ── Derived sample counts ─────────────────────────────────────────────────────

/// Samples in one ON burst.
fn on_samples() -> usize {
    (NOTE_ON_SECS * SR_F) as usize
}

/// Samples in one OFF gap within a note section.
fn off_samples() -> usize {
    (NOTE_OFF_SECS * SR_F) as usize
}

/// Samples in the inter-note silence gap.
fn gap_samples() -> usize {
    (NOTE_GAP_SECS * SR_F) as usize
}

/// Total samples for one complete note section.
///
/// The section contains `REPEATS` ON bursts and `REPEATS - 1` inter-burst
/// silences (no trailing silence, so the section always ends on a tone).
fn section_samples() -> usize {
    REPEATS * on_samples() + (REPEATS - 1) * off_samples()
}

// ── Pure-Rust WAV encoder / decoder (no external crates) ─────────────────────

/// Encode f32 samples as a mono 16-bit PCM WAV file.
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

/// Decode a mono 16-bit PCM WAV back to f32 samples in [-1.0, 1.0].
///
/// Assumes the standard 44-byte header written by [`encode_wav_i16`].
fn decode_wav_i16(wav: &[u8]) -> Vec<f32> {
    assert!(wav.len() >= 44, "WAV data too short (expected ≥ 44 bytes)");
    wav[44..]
        .chunks_exact(2)
        .map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32_768.0)
        .collect()
}

// ── Audio synthesis ───────────────────────────────────────────────────────────

/// Generate one complete note section: `REPEATS` tone bursts separated by
/// brief silences.  The section ends on the last sample of the last burst
/// (no trailing silence) so that the detector sees high resonator energy.
fn note_section(freq_hz: f32) -> Vec<f32> {
    let mut v = Vec::with_capacity(section_samples());
    for rep in 0..REPEATS {
        for i in 0..on_samples() {
            v.push((TAU * freq_hz * i as f32 / SR_F).sin());
        }
        // Add inter-burst silence after every burst except the last one.
        if rep + 1 < REPEATS {
            v.resize(v.len() + off_samples(), 0.0);
        }
    }
    v
}

// ── File generation (thread-safe, runs at most once per test binary) ──────────

/// Cached sample data for the whole 6-note WAV.
static WAV_SAMPLES: OnceLock<Vec<f32>> = OnceLock::new();

/// Generate and persist the open-E-notes WAV, then return the decoded samples.
///
/// Saves `tests/fixtures/open_e_notes_48k.wav`.  On subsequent calls the
/// cached sample vector is returned immediately.
fn open_e_notes_samples() -> &'static Vec<f32> {
    WAV_SAMPLES.get_or_init(|| {
        let freqs = [A3_HZ, B3_HZ, C4_HZ, D3_HZ, E3_HZ, F3_HZ];
        let gap = vec![0.0_f32; gap_samples()];

        let mut all: Vec<f32> = Vec::new();
        for (i, &freq) in freqs.iter().enumerate() {
            if i > 0 {
                all.extend_from_slice(&gap);
            }
            all.extend_from_slice(&note_section(freq));
        }

        // Encode as 48 kHz / 16-bit PCM WAV.
        let wav = encode_wav_i16(&all, SR);

        // Write to tests/fixtures/ for external playback / inspection.
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures");
        if std::fs::create_dir_all(&dir).is_ok() {
            let path = dir.join("open_e_notes_48k.wav");
            let _ = std::fs::write(&path, &wav);
        }

        // Decode back from WAV bytes — exercises the reader path and ensures
        // the samples the tests use are identical to what is on disk.
        decode_wav_i16(&wav)
    })
}

// ── Sample-offset helpers ─────────────────────────────────────────────────────

/// Start sample index of note `idx` within the combined WAV.
///
/// Notes are ordered A=0, B=1, C=2, D=3, E=4, F=5.  Each section occupies
/// `section_samples()` samples; consecutive sections are separated by
/// `gap_samples()` of silence.
fn note_start(idx: usize) -> usize {
    idx * (section_samples() + gap_samples())
}

// ── Detection helper ──────────────────────────────────────────────────────────

/// Feed `samples` to a fresh detector configured for 48 kHz and return the
/// detection result.
fn detect_section(samples: &[f32]) -> guitar_pitch_detection::DetectionResult {
    let config = DetectorConfig {
        sample_rate: SR,
        ..Default::default()
    };
    let mut det = GuitarPitchDetector::with_config(config);
    det.process(samples)
}

/// Print one verbose check line.
///
/// ```text
/// [  900.0 ms] Expected: A3 (MIDI 57, 220.00 Hz)  |  Detected: A3 (MIDI 57, 220.1 Hz, conf 0.91)  –  OK
/// ```
fn check(time_ms: f32, expected: &str, detected: &str, passed: bool) {
    println!(
        "[{:8.1} ms] Expected: {:<45}  |  Detected: {}  –  {}",
        time_ms,
        expected,
        detected,
        if passed { "OK" } else { "FAILED" }
    );
}

// ── Per-note tests ────────────────────────────────────────────────────────────
//
// Each test:
//  1. Calls `open_e_notes_samples()` which (on first call) generates the WAV,
//     writes it to `tests/fixtures/open_e_notes_48k.wav`, and decodes it back.
//  2. Extracts the sample range for the specific note.
//  3. Runs the pitch detector on that range.
//  4. Prints: Expected note name/MIDI | Detected note/MIDI at N ms – OK/FAILED
//  5. Asserts the expected MIDI note is among the detected notes.

/// A3 — 220.00 Hz, MIDI 57.
#[test]
fn wav_48k_open_e_note_a() {
    let all = open_e_notes_samples();
    let start = note_start(0);
    let seg = &all[start..start + section_samples()];
    let t = (start + seg.len()) as f32 * 1_000.0 / SR_F;
    let result = detect_section(seg);

    let exp = "Note A3 (MIDI 57, 220.00 Hz)";
    let det_str = notes_summary(&result);
    let passed = !result.notes.is_empty()
        && result.notes.iter().any(|n| (n.midi_note as i16 - 57).abs() <= 1);
    check(t, exp, &det_str, passed);

    assert!(!result.notes.is_empty(), "A3: no notes detected");
    assert!(
        result.notes.iter().any(|n| (n.midi_note as i16 - 57).abs() <= 1),
        "A3: expected MIDI 57, got {:?}",
        result.notes.iter().map(|n| n.midi_note).collect::<Vec<_>>()
    );
}

/// B3 — 246.94 Hz, MIDI 59.
#[test]
fn wav_48k_open_e_note_b() {
    let all = open_e_notes_samples();
    let start = note_start(1);
    let seg = &all[start..start + section_samples()];
    let t = (start + seg.len()) as f32 * 1_000.0 / SR_F;
    let result = detect_section(seg);

    let exp = "Note B3 (MIDI 59, 246.94 Hz)";
    let det_str = notes_summary(&result);
    let passed = !result.notes.is_empty()
        && result.notes.iter().any(|n| (n.midi_note as i16 - 59).abs() <= 1);
    check(t, exp, &det_str, passed);

    assert!(!result.notes.is_empty(), "B3: no notes detected");
    assert!(
        result.notes.iter().any(|n| (n.midi_note as i16 - 59).abs() <= 1),
        "B3: expected MIDI 59, got {:?}",
        result.notes.iter().map(|n| n.midi_note).collect::<Vec<_>>()
    );
}

/// C4 — 261.63 Hz, MIDI 60.
#[test]
fn wav_48k_open_e_note_c() {
    let all = open_e_notes_samples();
    let start = note_start(2);
    let seg = &all[start..start + section_samples()];
    let t = (start + seg.len()) as f32 * 1_000.0 / SR_F;
    let result = detect_section(seg);

    let exp = "Note C4 (MIDI 60, 261.63 Hz)";
    let det_str = notes_summary(&result);
    let passed = !result.notes.is_empty()
        && result.notes.iter().any(|n| (n.midi_note as i16 - 60).abs() <= 1);
    check(t, exp, &det_str, passed);

    assert!(!result.notes.is_empty(), "C4: no notes detected");
    assert!(
        result.notes.iter().any(|n| (n.midi_note as i16 - 60).abs() <= 1),
        "C4: expected MIDI 60, got {:?}",
        result.notes.iter().map(|n| n.midi_note).collect::<Vec<_>>()
    );
}

/// D3 — 146.83 Hz, MIDI 50.
#[test]
fn wav_48k_open_e_note_d() {
    let all = open_e_notes_samples();
    let start = note_start(3);
    let seg = &all[start..start + section_samples()];
    let t = (start + seg.len()) as f32 * 1_000.0 / SR_F;
    let result = detect_section(seg);

    let exp = "Note D3 (MIDI 50, 146.83 Hz)";
    let det_str = notes_summary(&result);
    let passed = !result.notes.is_empty()
        && result.notes.iter().any(|n| (n.midi_note as i16 - 50).abs() <= 1);
    check(t, exp, &det_str, passed);

    assert!(!result.notes.is_empty(), "D3: no notes detected");
    assert!(
        result.notes.iter().any(|n| (n.midi_note as i16 - 50).abs() <= 1),
        "D3: expected MIDI 50, got {:?}",
        result.notes.iter().map(|n| n.midi_note).collect::<Vec<_>>()
    );
}

/// E3 — 164.81 Hz, MIDI 52.
#[test]
fn wav_48k_open_e_note_e() {
    let all = open_e_notes_samples();
    let start = note_start(4);
    let seg = &all[start..start + section_samples()];
    let t = (start + seg.len()) as f32 * 1_000.0 / SR_F;
    let result = detect_section(seg);

    let exp = "Note E3 (MIDI 52, 164.81 Hz)";
    let det_str = notes_summary(&result);
    let passed = !result.notes.is_empty()
        && result.notes.iter().any(|n| (n.midi_note as i16 - 52).abs() <= 1);
    check(t, exp, &det_str, passed);

    assert!(!result.notes.is_empty(), "E3: no notes detected");
    assert!(
        result.notes.iter().any(|n| (n.midi_note as i16 - 52).abs() <= 1),
        "E3: expected MIDI 52, got {:?}",
        result.notes.iter().map(|n| n.midi_note).collect::<Vec<_>>()
    );
}

/// F3 — 174.61 Hz, MIDI 53.
#[test]
fn wav_48k_open_e_note_f() {
    let all = open_e_notes_samples();
    let start = note_start(5);
    let seg = &all[start..start + section_samples()];
    let t = (start + seg.len()) as f32 * 1_000.0 / SR_F;
    let result = detect_section(seg);

    let exp = "Note F3 (MIDI 53, 174.61 Hz)";
    let det_str = notes_summary(&result);
    let passed = !result.notes.is_empty()
        && result.notes.iter().any(|n| (n.midi_note as i16 - 53).abs() <= 1);
    check(t, exp, &det_str, passed);

    assert!(!result.notes.is_empty(), "F3: no notes detected");
    assert!(
        result.notes.iter().any(|n| (n.midi_note as i16 - 53).abs() <= 1),
        "F3: expected MIDI 53, got {:?}",
        result.notes.iter().map(|n| n.midi_note).collect::<Vec<_>>()
    );
}

// ── Formatting helper ─────────────────────────────────────────────────────────

/// Summarise detected notes as a human-readable string.
fn notes_summary(result: &guitar_pitch_detection::DetectionResult) -> String {
    if result.notes.is_empty() {
        return "(no notes detected)".to_string();
    }
    result
        .notes
        .iter()
        .map(|n| format!("{} (MIDI {}, {:.1} Hz, conf {:.2})", n.name, n.midi_note, n.frequency, n.confidence))
        .collect::<Vec<_>>()
        .join("; ")
}
