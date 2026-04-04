//! Integration tests for the guitar pitch detector.
//!
//! Each test exercises a real-world usage pattern:
//! - pure sine at a known guitar frequency
//! - polyphonic chord (summed sines)
//! - harmonic suppression (fundamental + octave)
//! - changing notes in successive frames
//! - edge cases (silence, very quiet input)

use guitar_pitch_detection::{ChordQuality, DetectorConfig, GuitarPitchDetector};
use std::f32::consts::TAU;

const SR: u32 = 44_100;
const SR_F: f32 = SR as f32;

fn sine(freq: f32, n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| (TAU * freq * i as f32 / SR_F).sin())
        .collect()
}

fn mix(signals: &[Vec<f32>]) -> Vec<f32> {
    let len = signals.iter().map(|s| s.len()).min().unwrap_or(0);
    (0..len)
        .map(|i| signals.iter().map(|s| s[i]).sum::<f32>() / signals.len() as f32)
        .collect()
}

// ── Monophonic detection ──────────────────────────────────────────────────────

#[test]
fn open_string_e2_detected() {
    let mut det = GuitarPitchDetector::new(SR, 4096);
    let result = det.process(&sine(82.41, 8192));
    assert!(!result.notes.is_empty(), "E2 not detected");
    let top = &result.notes[0];
    assert!(
        (top.midi_note as i16 - 40).abs() <= 1,
        "Expected MIDI 40 (E2), got {}",
        top.midi_note
    );
}

#[test]
fn open_string_a2_detected() {
    let mut det = GuitarPitchDetector::new(SR, 4096);
    let result = det.process(&sine(110.0, 8192));
    assert!(!result.notes.is_empty(), "A2 not detected");
    let top = &result.notes[0];
    assert!(
        (top.midi_note as i16 - 45).abs() <= 1,
        "Expected MIDI 45 (A2), got {}",
        top.midi_note
    );
}

#[test]
fn open_string_d3_detected() {
    let mut det = GuitarPitchDetector::new(SR, 4096);
    let result = det.process(&sine(146.83, 8192));
    assert!(!result.notes.is_empty(), "D3 not detected");
    let top = &result.notes[0];
    assert!(
        (top.midi_note as i16 - 50).abs() <= 1,
        "Expected MIDI 50 (D3), got {}",
        top.midi_note
    );
}

#[test]
fn open_string_g3_detected() {
    let mut det = GuitarPitchDetector::new(SR, 4096);
    let result = det.process(&sine(196.0, 8192));
    assert!(!result.notes.is_empty(), "G3 not detected");
    let top = &result.notes[0];
    assert!(
        (top.midi_note as i16 - 55).abs() <= 1,
        "Expected MIDI 55 (G3), got {}",
        top.midi_note
    );
}

#[test]
fn open_string_b3_detected() {
    let mut det = GuitarPitchDetector::new(SR, 4096);
    let result = det.process(&sine(246.94, 8192));
    assert!(!result.notes.is_empty(), "B3 not detected");
    let top = &result.notes[0];
    assert!(
        (top.midi_note as i16 - 59).abs() <= 1,
        "Expected MIDI 59 (B3), got {}",
        top.midi_note
    );
}

#[test]
fn open_string_e4_detected() {
    let mut det = GuitarPitchDetector::new(SR, 4096);
    let result = det.process(&sine(329.63, 8192));
    assert!(!result.notes.is_empty(), "E4 not detected");
    let top = &result.notes[0];
    assert!(
        (top.midi_note as i16 - 64).abs() <= 1,
        "Expected MIDI 64 (E4), got {}",
        top.midi_note
    );
}

// ── Chord detection ───────────────────────────────────────────────────────────

/// E minor open chord: E2 + B2 + E3 + G3 + B3 + E4
#[test]
fn e_minor_open_chord() {
    let e2 = 82.41_f32; // MIDI 40
    let b2 = 123.47_f32; // MIDI 47
    let e3 = 164.81_f32; // MIDI 52
    let g3 = 196.00_f32; // MIDI 55
    let b3 = 246.94_f32; // MIDI 59
    let e4 = 329.63_f32; // MIDI 64

    let mixed = mix(&[
        sine(e2, 8192),
        sine(b2, 8192),
        sine(e3, 8192),
        sine(g3, 8192),
        sine(b3, 8192),
        sine(e4, 8192),
    ]);

    let mut det = GuitarPitchDetector::new(SR, 4096);
    let result = det.process(&mixed);

    assert!(
        !result.notes.is_empty(),
        "No notes detected for E minor chord"
    );

    // The chord detector should identify Em (or at least Em or E5).
    if let Some(chord) = &result.chord {
        let name = chord.name.as_str();
        assert!(
            name == "Em" || name == "E5" || name == "E",
            "Unexpected chord: {name}"
        );
    }
}

/// A minor triad: A + C + E
#[test]
fn a_minor_triad_chord_name() {
    // Use mid-range octave frequencies for clarity.
    let a3 = 220.0_f32; // MIDI 57  pitch class 9
    let c4 = 261.63_f32; // MIDI 60  pitch class 0
    let e4 = 329.63_f32; // MIDI 64  pitch class 4

    let mixed = mix(&[sine(a3, 8192), sine(c4, 8192), sine(e4, 8192)]);

    let mut det = GuitarPitchDetector::new(SR, 4096);
    let result = det.process(&mixed);

    assert!(!result.notes.is_empty(), "No notes for Am triad");
    if let Some(chord) = &result.chord {
        assert_eq!(chord.quality, ChordQuality::Minor);
        assert_eq!(chord.name, "Am");
    }
}

// ── Harmonic suppression ──────────────────────────────────────────────────────

/// If both the fundamental and its octave are sounding simultaneously, the
/// detector should report the fundamental as the stronger/primary note.
#[test]
fn harmonic_suppression_octave() {
    let a3 = 220.0_f32; // MIDI 57
    let a4 = 440.0_f32; // MIDI 69 — octave above

    // Make A4 louder to simulate a harmonic that might confuse the detector.
    let mixed: Vec<f32> = sine(a3, 4096)
        .iter()
        .zip(sine(a4, 4096).iter())
        .map(|(lo, hi)| lo + hi * 0.6)
        .collect();

    let mut det = GuitarPitchDetector::new(SR, 4096);
    let result = det.process(&mixed);

    assert!(!result.notes.is_empty());
    // The lowest detected note should be A3 (57), not A4 (69).
    let lowest_midi = result.notes.iter().map(|n| n.midi_note).min().unwrap();
    assert!(
        lowest_midi <= 58,
        "Expected A3 (≤58) as lowest note, got MIDI {lowest_midi}"
    );
}

// ── Edge cases ────────────────────────────────────────────────────────────────

#[test]
fn silence_returns_empty_result() {
    let mut det = GuitarPitchDetector::new(SR, 512);
    let result = det.process(&vec![0.0_f32; 1024]);
    assert!(result.notes.is_empty());
    assert!(result.chord.is_none());
}

#[test]
fn very_quiet_signal_below_noise_floor() {
    let mut det = GuitarPitchDetector::new(SR, 512);
    // Amplitude 1e-7 — well below a reasonable noise floor.
    let tiny: Vec<f32> = sine(440.0, 2048).iter().map(|&x| x * 1e-7).collect();
    let result = det.process(&tiny);
    // May or may not detect — just must not panic.
    let _ = result;
}

#[test]
fn note_changes_between_frames() {
    let mut det = GuitarPitchDetector::new(SR, 2048);

    // Frame 1: A4
    let r1 = det.process(&sine(440.0, 4096));
    assert!(!r1.notes.is_empty(), "Frame 1: expected A4");

    // Reset so previous energy doesn't bleed into the next note check.
    det.reset();

    // Frame 2: E4
    let r2 = det.process(&sine(329.63, 4096));
    assert!(!r2.notes.is_empty(), "Frame 2: expected E4");
    let top2 = &r2.notes[0];
    assert!(
        (top2.midi_note as i16 - 64).abs() <= 1,
        "Frame 2: expected E4 (64), got {}",
        top2.midi_note
    );
}

// ── Custom configuration ──────────────────────────────────────────────────────

#[test]
fn custom_sample_rate_48khz() {
    let config = DetectorConfig {
        sample_rate: 48_000,
        ..Default::default()
    };
    let mut det = GuitarPitchDetector::with_config(config);
    // 440 Hz sine at 48 kHz
    let samples: Vec<f32> = (0..4096)
        .map(|i| (TAU * 440.0 * i as f32 / 48_000.0).sin())
        .collect();
    let result = det.process(&samples);
    assert!(
        !result.notes.is_empty(),
        "Expected A4 detected at 48 kHz SR"
    );
    let top = &result.notes[0];
    assert!(
        (top.midi_note as i16 - 69).abs() <= 1,
        "Expected MIDI 69 (A4), got {}",
        top.midi_note
    );
}

#[test]
fn confidence_is_normalized() {
    let mut det = GuitarPitchDetector::new(SR, 4096);
    let result = det.process(&sine(440.0, 8192));
    for note in &result.notes {
        assert!(
            (0.0..=1.0).contains(&note.confidence),
            "confidence out of range: {}",
            note.confidence
        );
    }
}
