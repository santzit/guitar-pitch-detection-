//! Integration tests for the guitar pitch detector.
//!
//! Each test exercises a real-world usage pattern:
//! - pure sine at a known guitar frequency
//! - polyphonic chord (summed sines)
//! - harmonic suppression (fundamental + octave)
//! - changing notes in successive frames
//! - edge cases (silence, very quiet input)
//!
//! All tests print a structured line per assertion:
//! `[  N ms] Expected: <X>  |  Detected: <Y>  –  OK / FAILED`

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

/// Convert a sample count to milliseconds at the test sample rate.
fn ms(n_samples: usize) -> f32 {
    n_samples as f32 * 1_000.0 / SR_F
}

/// Print a single verbose check line.
///
/// ```text
/// [  185.1 ms] Expected: E2 (MIDI 40, 82.41 Hz)  |  Detected: E2 (MIDI 40, 82.5 Hz, conf 0.87)  –  OK
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

// ── Monophonic detection ──────────────────────────────────────────────────────

#[test]
fn open_string_e2_detected() {
    let num_samples = 8192usize;
    let mut det = GuitarPitchDetector::new(SR, 4096);
    let result = det.process(&sine(82.41, num_samples));
    let t = ms(num_samples);
    let exp = "Note E2 (MIDI 40, 82.41 Hz)";
    if result.notes.is_empty() {
        check(t, exp, "(no notes detected)", false);
        panic!("E2 not detected");
    }
    let top = &result.notes[0];
    let det_str = format!(
        "{} (MIDI {}, {:.1} Hz, conf {:.2})",
        top.name, top.midi_note, top.frequency, top.confidence
    );
    let passed = (top.midi_note as i16 - 40).abs() <= 1;
    check(t, exp, &det_str, passed);
    assert!(passed, "Expected MIDI 40 (E2), got {}", top.midi_note);
}

#[test]
fn open_string_a2_detected() {
    let num_samples = 8192usize;
    let mut det = GuitarPitchDetector::new(SR, 4096);
    let result = det.process(&sine(110.0, num_samples));
    let t = ms(num_samples);
    let exp = "Note A2 (MIDI 45, 110.00 Hz)";
    if result.notes.is_empty() {
        check(t, exp, "(no notes detected)", false);
        panic!("A2 not detected");
    }
    let top = &result.notes[0];
    let det_str = format!(
        "{} (MIDI {}, {:.1} Hz, conf {:.2})",
        top.name, top.midi_note, top.frequency, top.confidence
    );
    let passed = (top.midi_note as i16 - 45).abs() <= 1;
    check(t, exp, &det_str, passed);
    assert!(passed, "Expected MIDI 45 (A2), got {}", top.midi_note);
}

#[test]
fn open_string_d3_detected() {
    let num_samples = 8192usize;
    let mut det = GuitarPitchDetector::new(SR, 4096);
    let result = det.process(&sine(146.83, num_samples));
    let t = ms(num_samples);
    let exp = "Note D3 (MIDI 50, 146.83 Hz)";
    if result.notes.is_empty() {
        check(t, exp, "(no notes detected)", false);
        panic!("D3 not detected");
    }
    let top = &result.notes[0];
    let det_str = format!(
        "{} (MIDI {}, {:.1} Hz, conf {:.2})",
        top.name, top.midi_note, top.frequency, top.confidence
    );
    let passed = (top.midi_note as i16 - 50).abs() <= 1;
    check(t, exp, &det_str, passed);
    assert!(passed, "Expected MIDI 50 (D3), got {}", top.midi_note);
}

#[test]
fn open_string_g3_detected() {
    let num_samples = 8192usize;
    let mut det = GuitarPitchDetector::new(SR, 4096);
    let result = det.process(&sine(196.0, num_samples));
    let t = ms(num_samples);
    let exp = "Note G3 (MIDI 55, 196.00 Hz)";
    if result.notes.is_empty() {
        check(t, exp, "(no notes detected)", false);
        panic!("G3 not detected");
    }
    let top = &result.notes[0];
    let det_str = format!(
        "{} (MIDI {}, {:.1} Hz, conf {:.2})",
        top.name, top.midi_note, top.frequency, top.confidence
    );
    let passed = (top.midi_note as i16 - 55).abs() <= 1;
    check(t, exp, &det_str, passed);
    assert!(passed, "Expected MIDI 55 (G3), got {}", top.midi_note);
}

#[test]
fn open_string_b3_detected() {
    let num_samples = 8192usize;
    let mut det = GuitarPitchDetector::new(SR, 4096);
    let result = det.process(&sine(246.94, num_samples));
    let t = ms(num_samples);
    let exp = "Note B3 (MIDI 59, 246.94 Hz)";
    if result.notes.is_empty() {
        check(t, exp, "(no notes detected)", false);
        panic!("B3 not detected");
    }
    let top = &result.notes[0];
    let det_str = format!(
        "{} (MIDI {}, {:.1} Hz, conf {:.2})",
        top.name, top.midi_note, top.frequency, top.confidence
    );
    let passed = (top.midi_note as i16 - 59).abs() <= 1;
    check(t, exp, &det_str, passed);
    assert!(passed, "Expected MIDI 59 (B3), got {}", top.midi_note);
}

#[test]
fn open_string_e4_detected() {
    let num_samples = 8192usize;
    let mut det = GuitarPitchDetector::new(SR, 4096);
    let result = det.process(&sine(329.63, num_samples));
    let t = ms(num_samples);
    let exp = "Note E4 (MIDI 64, 329.63 Hz)";
    if result.notes.is_empty() {
        check(t, exp, "(no notes detected)", false);
        panic!("E4 not detected");
    }
    let top = &result.notes[0];
    let det_str = format!(
        "{} (MIDI {}, {:.1} Hz, conf {:.2})",
        top.name, top.midi_note, top.frequency, top.confidence
    );
    let passed = (top.midi_note as i16 - 64).abs() <= 1;
    check(t, exp, &det_str, passed);
    assert!(passed, "Expected MIDI 64 (E4), got {}", top.midi_note);
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

    let num_samples = 8192usize;
    let mixed = mix(&[
        sine(e2, num_samples),
        sine(b2, num_samples),
        sine(e3, num_samples),
        sine(g3, num_samples),
        sine(b3, num_samples),
        sine(e4, num_samples),
    ]);

    let mut det = GuitarPitchDetector::new(SR, 4096);
    let result = det.process(&mixed);
    let t = ms(num_samples);

    // Notes check
    let exp_notes = "Notes for E minor chord (E2 B2 E3 G3 B3 E4)";
    let det_notes = if result.notes.is_empty() {
        "(no notes detected)".to_string()
    } else {
        result.notes.iter().map(|n| n.name).collect::<Vec<_>>().join(", ")
    };
    check(t, exp_notes, &det_notes, !result.notes.is_empty());
    assert!(!result.notes.is_empty(), "No notes detected for E minor chord");

    // Chord name check
    let exp_chord = "Chord Em (or E5 / E)";
    if let Some(chord) = &result.chord {
        let name = chord.name.as_str();
        let valid = name == "Em" || name == "E5" || name == "E";
        check(t, exp_chord, &chord.name, valid);
        assert!(valid, "Unexpected chord: {name}");
    } else {
        check(t, exp_chord, "(no chord detected)", true); // chord detection is optional here
    }
}

/// A minor triad: A + C + E
#[test]
fn a_minor_triad_chord_name() {
    let a3 = 220.0_f32; // MIDI 57  pitch class 9
    let c4 = 261.63_f32; // MIDI 60  pitch class 0
    let e4 = 329.63_f32; // MIDI 64  pitch class 4

    let num_samples = 8192usize;
    let mixed = mix(&[sine(a3, num_samples), sine(c4, num_samples), sine(e4, num_samples)]);

    let mut det = GuitarPitchDetector::new(SR, 4096);
    let result = det.process(&mixed);
    let t = ms(num_samples);

    let exp_notes = "Notes for Am triad (A3 C4 E4)";
    let det_notes = if result.notes.is_empty() {
        "(no notes)".to_string()
    } else {
        result.notes.iter().map(|n| n.name).collect::<Vec<_>>().join(", ")
    };
    check(t, exp_notes, &det_notes, !result.notes.is_empty());
    assert!(!result.notes.is_empty(), "No notes for Am triad");

    let exp_chord = "Chord Am (quality=Minor, root=9)";
    if let Some(chord) = &result.chord {
        let passed = chord.quality == ChordQuality::Minor && chord.name == "Am";
        let det_chord = format!("{} (quality={:?})", chord.name, chord.quality);
        check(t, exp_chord, &det_chord, passed);
        assert_eq!(chord.quality, ChordQuality::Minor);
        assert_eq!(chord.name, "Am");
    } else {
        check(t, exp_chord, "(no chord detected)", false);
        panic!("No chord detected for Am triad");
    }
}

// ── Harmonic suppression ──────────────────────────────────────────────────────

/// If both the fundamental and its octave are sounding simultaneously, the
/// detector should report the fundamental as the stronger/primary note.
#[test]
fn harmonic_suppression_octave() {
    let a3 = 220.0_f32; // MIDI 57
    let a4 = 440.0_f32; // MIDI 69 — octave above
    let num_samples = 4096usize;

    // Make A4 louder to simulate a harmonic that might confuse the detector.
    let mixed: Vec<f32> = sine(a3, num_samples)
        .iter()
        .zip(sine(a4, num_samples).iter())
        .map(|(lo, hi)| lo + hi * 0.6)
        .collect();

    let mut det = GuitarPitchDetector::new(SR, 4096);
    let result = det.process(&mixed);
    let t = ms(num_samples);

    let exp = "Lowest note ≤ A3 (MIDI ≤58), not octave-confused with A4";
    assert!(!result.notes.is_empty());
    let lowest_midi = result.notes.iter().map(|n| n.midi_note).min().unwrap();
    let lowest_name = result
        .notes
        .iter()
        .min_by_key(|n| n.midi_note)
        .map(|n| n.name)
        .unwrap_or("?");
    let passed = lowest_midi <= 58;
    check(
        t,
        exp,
        &format!("{} (MIDI {})", lowest_name, lowest_midi),
        passed,
    );
    assert!(
        passed,
        "Expected A3 (≤58) as lowest note, got MIDI {lowest_midi}"
    );
}

// ── Edge cases ────────────────────────────────────────────────────────────────

#[test]
fn silence_returns_empty_result() {
    let num_samples = 1024usize;
    let mut det = GuitarPitchDetector::new(SR, 512);
    let result = det.process(&vec![0.0_f32; num_samples]);
    let t = ms(num_samples);
    let passed_notes = result.notes.is_empty();
    let passed_chord = result.chord.is_none();
    check(t, "No notes on silence", if passed_notes { "(none)" } else { "(notes present)" }, passed_notes);
    check(t, "No chord on silence",  if passed_chord { "(none)" } else { "(chord present)" }, passed_chord);
    assert!(passed_notes);
    assert!(passed_chord);
}

#[test]
fn very_quiet_signal_below_noise_floor() {
    let num_samples = 2048usize;
    let mut det = GuitarPitchDetector::new(SR, 512);
    // Amplitude 1e-7 — well below a reasonable noise floor.
    let tiny: Vec<f32> = sine(440.0, num_samples).iter().map(|&x| x * 1e-7).collect();
    let result = det.process(&tiny);
    let t = ms(num_samples);
    // May or may not detect — just must not panic.
    let summary = if result.notes.is_empty() {
        "(no notes — below noise floor)".to_string()
    } else {
        format!("detected {} note(s) — amplitude 1e-7", result.notes.len())
    };
    check(t, "No crash on sub-noise-floor amplitude 1e-7", &summary, true);
    let _ = result;
}

#[test]
fn note_changes_between_frames() {
    let mut det = GuitarPitchDetector::new(SR, 2048);
    let num_samples = 4096usize;

    // Frame 1: A4
    let r1 = det.process(&sine(440.0, num_samples));
    let t1 = ms(num_samples);
    let notes1 = if r1.notes.is_empty() {
        "(no notes)".to_string()
    } else {
        r1.notes.iter().map(|n| n.name).collect::<Vec<_>>().join(", ")
    };
    check(t1, "Frame 1: Note A4 (MIDI 69, 440.00 Hz)", &notes1, !r1.notes.is_empty());
    assert!(!r1.notes.is_empty(), "Frame 1: expected A4");

    // Reset so previous energy doesn't bleed into the next note check.
    det.reset();

    // Frame 2: E4
    let r2 = det.process(&sine(329.63, num_samples));
    let t2 = t1 + ms(num_samples);
    if r2.notes.is_empty() {
        check(t2, "Frame 2: Note E4 (MIDI 64, 329.63 Hz)", "(no notes)", false);
        panic!("Frame 2: expected E4");
    }
    let top2 = &r2.notes[0];
    let notes2 = format!(
        "{} (MIDI {}, {:.1} Hz, conf {:.2})",
        top2.name, top2.midi_note, top2.frequency, top2.confidence
    );
    let passed = (top2.midi_note as i16 - 64).abs() <= 1;
    check(t2, "Frame 2: Note E4 (MIDI 64, 329.63 Hz)", &notes2, passed);
    assert!(passed, "Frame 2: expected E4 (64), got {}", top2.midi_note);
}

// ── Custom configuration ──────────────────────────────────────────────────────

#[test]
fn custom_sample_rate_48khz() {
    let config = DetectorConfig {
        sample_rate: 48_000,
        ..Default::default()
    };
    let mut det = GuitarPitchDetector::with_config(config);
    let num_samples = 4096usize;
    // 440 Hz sine at 48 kHz
    let samples: Vec<f32> = (0..num_samples)
        .map(|i| (TAU * 440.0 * i as f32 / 48_000.0).sin())
        .collect();
    let result = det.process(&samples);
    let t = num_samples as f32 * 1_000.0 / 48_000.0;
    let exp = "Note A4 (MIDI 69, 440.00 Hz) @ 48 kHz SR";
    if result.notes.is_empty() {
        check(t, exp, "(no notes detected)", false);
        panic!("Expected A4 detected at 48 kHz SR");
    }
    let top = &result.notes[0];
    let det_str = format!(
        "{} (MIDI {}, {:.1} Hz, conf {:.2})",
        top.name, top.midi_note, top.frequency, top.confidence
    );
    let passed = (top.midi_note as i16 - 69).abs() <= 1;
    check(t, exp, &det_str, passed);
    assert!(passed, "Expected MIDI 69 (A4), got {}", top.midi_note);
}

#[test]
fn confidence_is_normalized() {
    let num_samples = 8192usize;
    let mut det = GuitarPitchDetector::new(SR, 4096);
    let result = det.process(&sine(440.0, num_samples));
    let t = ms(num_samples);
    for note in &result.notes {
        let in_range = (0.0..=1.0).contains(&note.confidence);
        check(
            t,
            &format!("Confidence in [0.0, 1.0] for {}", note.name),
            &format!("{:.4}", note.confidence),
            in_range,
        );
        assert!(
            in_range,
            "confidence out of range: {}",
            note.confidence
        );
    }
}
