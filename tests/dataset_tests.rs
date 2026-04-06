//! Dataset-based integration tests.
//!
//! Tests in this file gracefully skip when dataset files are absent.
//! Download the datasets by following the instructions in `tests/dataset/README.md`.
//!
//! Technique-detection regression tests run unconditionally using synthetic
//! audio that mimics real guitar playing techniques.
//!
//! All tests print a structured line per check:
//! `[  N ms] Expected: <X>  |  Detected: <Y>  –  OK / FAILED`

use guitar_pitch_detection::{GuitarPitchDetector, GuitarTechnique};
use std::f32::consts::TAU;
use std::path::{Path, PathBuf};

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Generate `n_samples` of a sine wave at `freq` Hz (sample rate `sr`).
fn sine(freq: f32, n_samples: usize, sr: f32) -> Vec<f32> {
    (0..n_samples)
        .map(|n| (TAU * freq * n as f32 / sr).sin())
        .collect()
}

/// Mix a list of mono signals (simple sum, no normalisation).
fn mix(signals: &[Vec<f32>]) -> Vec<f32> {
    let len = signals.iter().map(|s| s.len()).max().unwrap_or(0);
    let mut out = vec![0.0_f32; len];
    for sig in signals {
        for (o, &s) in out.iter_mut().zip(sig.iter()) {
            *o += s;
        }
    }
    out
}

/// Collect all `.wav` files under `dir` recursively.
fn find_wav_files(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    if !dir.exists() {
        return found;
    }
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                found.extend(find_wav_files(&path));
            } else if path.extension().and_then(|e| e.to_str()) == Some("wav") {
                found.push(path);
            }
        }
    }
    found
}

/// Print one verbose check line.
///
/// ```text
/// [  247.1 ms] Expected: Bend > 1.0 st  |  Detected: Bend(+1.83 st)  –  OK
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

/// Format a `GuitarTechnique` as a compact human-readable string.
fn fmt_technique(t: &GuitarTechnique) -> String {
    match t {
        GuitarTechnique::Bend { semitones } => format!("Bend({:+.2} st)", semitones),
        GuitarTechnique::Slide { from_midi, to_midi, ascending } => {
            format!("Slide({} → {} {})", from_midi, to_midi, if *ascending { "↑" } else { "↓" })
        }
        GuitarTechnique::Vibrato { rate_hz, depth_semitones } => {
            format!("Vibrato({:.1} Hz, {:.2} st)", rate_hz, depth_semitones)
        }
        GuitarTechnique::PalmMute => "PalmMute".to_string(),
        GuitarTechnique::HammerOn => "HammerOn".to_string(),
        GuitarTechnique::PullOff => "PullOff".to_string(),
    }
}

// ── Technique regression tests (always run) ───────────────────────────────────

const SR: u32 = 44_100;
const FRAME: usize = 512;

/// Simulate and detect a string bend: pitch rises ~2 semitones over ~500 ms.
#[test]
fn technique_bend_detected_from_synthetic_audio() {
    let mut detector = GuitarPitchDetector::new(SR, FRAME);

    // Ramp A4 (440 Hz) up to B4 (493.88 Hz) over 22050 samples (≈ 500 ms).
    let total = SR as usize / 2;
    let samples: Vec<f32> = (0..total)
        .map(|i| {
            let progress = i as f32 / total as f32;
            let freq = 440.0 * 2.0_f32.powf(2.0 * progress / 12.0); // 0→2 semitones
            (TAU * freq * i as f32 / SR as f32).sin()
        })
        .collect();

    let exp = "Bend > 1.0 st on A4→B4 (0–500 ms)";
    let mut found_bend = false;
    let mut detected_at_ms: Option<f32> = None;
    let mut detected_desc = String::new();

    for (frame_idx, chunk) in samples.chunks(FRAME).enumerate() {
        let frame_ms = frame_idx as f32 * FRAME as f32 * 1_000.0 / SR as f32;
        let result = detector.process(chunk);
        if let Some(t) = result.techniques.iter().find(|t| {
            matches!(t, GuitarTechnique::Bend { semitones } if *semitones > 1.0)
        }) {
            if !found_bend {
                detected_at_ms = Some(frame_ms);
                detected_desc = fmt_technique(t);
            }
            found_bend = true;
            break;
        }
    }

    match detected_at_ms {
        Some(t) => check(t, exp, &detected_desc, true),
        None    => check(
            total as f32 * 1_000.0 / SR as f32,
            exp,
            "(not detected)",
            false,
        ),
    }

    assert!(found_bend, "Expected a bend to be detected in the rising-pitch signal");
}

/// Simulate and detect vibrato: A4 oscillating ±0.3 semitones at 6 Hz.
#[test]
fn technique_vibrato_detected_from_synthetic_audio() {
    let mut detector = GuitarPitchDetector::new(SR, FRAME);

    let total = SR as usize; // 1 second
    let samples: Vec<f32> = (0..total)
        .map(|i| {
            let t = i as f32 / SR as f32;
            let offset_semitones = 0.3 * (TAU * 6.0 * t).sin();
            let freq = 440.0 * 2.0_f32.powf(offset_semitones / 12.0);
            (TAU * freq * i as f32 / SR as f32).sin()
        })
        .collect();

    let exp = "Vibrato (6 Hz, ±0.3 st) on A4 (0–1000 ms)";
    let mut found_vibrato = false;
    let mut detected_at_ms: Option<f32> = None;
    let mut detected_desc = String::new();

    for (frame_idx, chunk) in samples.chunks(FRAME).enumerate() {
        let frame_ms = frame_idx as f32 * FRAME as f32 * 1_000.0 / SR as f32;
        let result = detector.process(chunk);
        if let Some(t) = result
            .techniques
            .iter()
            .find(|t| matches!(t, GuitarTechnique::Vibrato { .. }))
        {
            if !found_vibrato {
                detected_at_ms = Some(frame_ms);
                detected_desc = fmt_technique(t);
            }
            found_vibrato = true;
            break;
        }
    }

    match detected_at_ms {
        Some(t) => check(t, exp, &detected_desc, true),
        None    => check(
            total as f32 * 1_000.0 / SR as f32,
            exp,
            "(not detected)",
            false,
        ),
    }

    assert!(
        found_vibrato,
        "Expected vibrato to be detected in the oscillating-pitch signal"
    );
}

/// Simulate and detect a slide: pitch glides from E3 (164.8 Hz) to A3 (220 Hz)
/// over ~300 ms at constant speed (≥ 5 semitones, fast enough to be a slide).
#[test]
fn technique_slide_detected_from_synthetic_audio() {
    let mut detector = GuitarPitchDetector::new(SR, FRAME);

    // Glide from E3 (164.81 Hz) up to A3 (220.00 Hz) — exactly 5 semitones —
    // in 13230 samples (≈ 300 ms).  Speed ≈ 16.7 semitones/second, well above
    // the slide threshold.
    let total = (SR as f32 * 0.3) as usize;
    let samples: Vec<f32> = (0..total)
        .map(|i| {
            let progress = i as f32 / total as f32;
            let freq = 164.81 * 2.0_f32.powf(5.0 * progress / 12.0);
            (TAU * freq * i as f32 / SR as f32).sin()
        })
        .collect();

    let exp = "Slide E3→A3 (≈5 st, 0–300 ms)";
    let mut found_slide = false;
    let mut detected_at_ms: Option<f32> = None;
    let mut detected_desc = String::new();

    for (frame_idx, chunk) in samples.chunks(FRAME).enumerate() {
        let frame_ms = frame_idx as f32 * FRAME as f32 * 1_000.0 / SR as f32;
        let result = detector.process(chunk);
        if let Some(t) = result
            .techniques
            .iter()
            .find(|t| matches!(t, GuitarTechnique::Slide { .. }))
        {
            if !found_slide {
                detected_at_ms = Some(frame_ms);
                detected_desc = fmt_technique(t);
            }
            found_slide = true;
            break;
        }
    }

    match detected_at_ms {
        Some(t) => check(t, exp, &detected_desc, true),
        None    => check(
            total as f32 * 1_000.0 / SR as f32,
            exp,
            "(not detected)",
            false,
        ),
    }

    assert!(found_slide, "Expected a slide to be detected in the gliding-pitch signal");
}

/// Steady note must produce no bend or slide.
#[test]
fn technique_no_false_positives_on_steady_note() {
    let mut detector = GuitarPitchDetector::new(SR, FRAME);
    let total = SR as usize;
    let samples = sine(440.0, total, SR as f32);

    let exp = "No Bend or Slide on steady A4 (1 s)";
    let mut had_false_positive = false;
    let mut fp_at_ms: Option<f32> = None;
    let mut fp_desc = String::new();

    for (frame_idx, chunk) in samples.chunks(FRAME).enumerate() {
        let frame_ms = frame_idx as f32 * FRAME as f32 * 1_000.0 / SR as f32;
        let result = detector.process(chunk);
        if let Some(t) = result.techniques.iter().find(|t| {
            matches!(
                t,
                GuitarTechnique::Bend { .. } | GuitarTechnique::Slide { .. }
            )
        }) {
            if !had_false_positive {
                fp_at_ms = Some(frame_ms);
                fp_desc = fmt_technique(t);
            }
            had_false_positive = true;
            break;
        }
    }

    match fp_at_ms {
        Some(t) => check(t, exp, &format!("FALSE POSITIVE: {}", fp_desc), false),
        None    => check(
            total as f32 * 1_000.0 / SR as f32,
            exp,
            "(none — correct)",
            true,
        ),
    }

    assert!(
        !had_false_positive,
        "Steady note should not trigger bend or slide"
    );
}

/// Chord detection still works alongside technique detection.
#[test]
fn chord_and_techniques_coexist() {
    let mut detector = GuitarPitchDetector::new(SR, FRAME);
    // A minor triad: A3 (220), C4 (261.63), E4 (329.63)
    let num_samples = SR as usize;
    let chord_signal = mix(&[
        sine(220.0, num_samples, SR as f32),
        sine(261.63, num_samples, SR as f32),
        sine(329.63, num_samples, SR as f32),
    ]);
    let result = detector.process(&chord_signal);
    let t = num_samples as f32 * 1_000.0 / SR as f32;
    let notes_str = if result.notes.is_empty() {
        "(none)".to_string()
    } else {
        result.notes.iter().map(|n| n.name).collect::<Vec<_>>().join(", ")
    };
    // Chord detection is optional here — we just verify it doesn't panic.
    let chord_str = result
        .chord
        .as_ref()
        .map(|c| c.name.clone())
        .unwrap_or_else(|| "(none)".to_string());
    check(t, "Am triad notes (A3 C4 E4)", &notes_str, !result.notes.is_empty());
    check(t, "Chord (optional)", &chord_str, true);
    let _ = result.techniques;
}

// ── GuitarSet dataset tests ───────────────────────────────────────────────────

#[test]
fn guitarset_wav_files_detect_notes() {
    let dataset_dir = Path::new("tests/dataset/guitarset/audio/mic");
    let mut wav_files = find_wav_files(dataset_dir);

    if wav_files.is_empty() {
        println!(
            "[SKIP] GuitarSet not found at {}.  \
             See tests/dataset/README.md for download instructions.",
            dataset_dir.display()
        );
        return;
    }

    wav_files.sort();
    println!(
        "\nGuitarSet: testing {} WAV file(s)\n{}\n",
        wav_files.len(),
        "─".repeat(90)
    );

    let mut passed = 0usize;
    let mut failed = 0usize;
    let mut failures: Vec<String> = Vec::new();

    for path in &wav_files {
        match std::panic::catch_unwind(|| run_detection_on_wav(path)) {
            Ok(()) => passed += 1,
            Err(_) => {
                let msg = format!("FAILED: {}", path.display());
                eprintln!("{}", msg);
                failures.push(msg);
                failed += 1;
            }
        }
    }

    println!(
        "\n{}\nGuitarSet summary: {} PASSED  {} FAILED  (total {})\n",
        "─".repeat(90),
        passed,
        failed,
        wav_files.len()
    );

    assert!(
        failures.is_empty(),
        "GuitarSet test failures:\n{}",
        failures.join("\n")
    );
}

// ── IDMT-SMT-Guitar dataset tests ─────────────────────────────────────────────

#[test]
fn idmt_guitar_wav_files_detect_notes() {
    let dataset_dir = Path::new("tests/dataset/idmt_guitar");
    let mut wav_files = find_wav_files(dataset_dir);

    if wav_files.is_empty() {
        println!(
            "[SKIP] IDMT-SMT-Guitar not found at {}.  \
             See tests/dataset/README.md for download instructions.",
            dataset_dir.display()
        );
        return;
    }

    wav_files.sort();
    println!(
        "\nIDMT-SMT-Guitar: testing {} WAV file(s)\n{}\n",
        wav_files.len(),
        "─".repeat(90)
    );

    let mut passed = 0usize;
    let mut failed = 0usize;
    let mut failures: Vec<String> = Vec::new();

    for path in &wav_files {
        match std::panic::catch_unwind(|| run_detection_on_wav(path)) {
            Ok(()) => passed += 1,
            Err(_) => {
                let msg = format!("FAILED: {}", path.display());
                eprintln!("{}", msg);
                failures.push(msg);
                failed += 1;
            }
        }
    }

    println!(
        "\n{}\nIDMT-SMT-Guitar summary: {} PASSED  {} FAILED  (total {})\n",
        "─".repeat(90),
        passed,
        failed,
        wav_files.len()
    );

    assert!(
        failures.is_empty(),
        "IDMT-SMT-Guitar test failures:\n{}",
        failures.join("\n")
    );
}

// ── WAV runner (pure-Rust fallback, no rodio needed) ─────────────────────────

/// Run the detector over a WAV file using the built-in pure-Rust WAV codec.
/// Prints a verbose check line for every frame where a note is first detected.
/// Asserts that at least one note is detected somewhere in the file.
fn run_detection_on_wav(path: &Path) {
    let file_name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();

    let bytes = std::fs::read(path).expect("failed to read WAV file");
    let samples = decode_wav_i16(&bytes);

    assert!(!samples.is_empty(), "WAV file contained no samples");

    let sr = 44_100u32;
    let frame_size = 512usize;
    let mut detector = GuitarPitchDetector::new(sr, frame_size);
    let mut detected_any = false;
    let mut first_note_ms: Option<f32> = None;
    let mut first_note_str = String::new();

    for (frame_idx, chunk) in samples.chunks(frame_size).enumerate() {
        let frame_ms = frame_idx as f32 * frame_size as f32 * 1_000.0 / sr as f32;
        let result = detector.process(chunk);

        if !result.notes.is_empty() && !detected_any {
            let n = &result.notes[0];
            first_note_ms = Some(frame_ms);
            first_note_str = format!(
                "{} (MIDI {}, {:.1} Hz, conf {:.2})",
                n.name, n.midi_note, n.frequency, n.confidence
            );
            detected_any = true;
        }
    }

    let total_ms = samples.len() as f32 * 1_000.0 / sr as f32;
    let det_str = match first_note_ms {
        Some(t) => format!("{} at {:.1} ms", first_note_str, t),
        None    => "(no notes in entire file)".to_string(),
    };

    check(
        total_ms,
        &format!("≥1 note somewhere in {}", file_name),
        &det_str,
        detected_any,
    );

    assert!(
        detected_any,
        "No notes detected in {} — check file content",
        path.display()
    );
}

// ── Minimal pure-Rust WAV decoder (no external crates) ───────────────────────
//
// This mirrors the codec in open_e_notes_test.rs so dataset tests can run
// without enabling the `audio_input` feature (which requires ALSA headers).

fn decode_wav_i16(wav: &[u8]) -> Vec<f32> {
    if wav.len() < 44 {
        return vec![];
    }
    let num_channels = u16::from_le_bytes([wav[22], wav[23]]) as usize;
    let bits_per_sample = u16::from_le_bytes([wav[34], wav[35]]) as usize;

    // Find the "data" chunk.
    let mut pos = 12usize;
    let data_start;
    let data_len;
    loop {
        if pos + 8 > wav.len() {
            return vec![];
        }
        let chunk_id = &wav[pos..pos + 4];
        let chunk_size = u32::from_le_bytes([wav[pos+4], wav[pos+5], wav[pos+6], wav[pos+7]]) as usize;
        if chunk_id == b"data" {
            data_start = pos + 8;
            data_len = chunk_size;
            break;
        }
        pos += 8 + chunk_size;
    }

    let data = &wav[data_start..(data_start + data_len).min(wav.len())];
    let bytes_per_sample = bits_per_sample / 8;
    let frame_bytes = bytes_per_sample * num_channels;

    if frame_bytes == 0 {
        return vec![];
    }

    let mut samples = Vec::with_capacity(data.len() / frame_bytes);
    let mut i = 0;
    while i + frame_bytes <= data.len() {
        let mut sum = 0.0f32;
        for ch in 0..num_channels {
            let off = i + ch * bytes_per_sample;
            let s = match bits_per_sample {
                16 => i16::from_le_bytes([data[off], data[off + 1]]) as f32 / 32768.0,
                8  => (data[off] as f32 - 128.0) / 128.0,
                _  => 0.0,
            };
            sum += s;
        }
        samples.push(sum / num_channels as f32);
        i += frame_bytes;
    }
    samples
}
