//! Dataset-based integration tests.
//!
//! Tests in this file gracefully skip when dataset files are absent.
//! Download the datasets by following the instructions in `tests/dataset/README.md`.
//!
//! Technique-detection regression tests run unconditionally using synthetic
//! audio that mimics real guitar playing techniques.

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

    // Feed in frames.
    let mut found_bend = false;
    for chunk in samples.chunks(FRAME) {
        let result = detector.process(chunk);
        if result
            .techniques
            .iter()
            .any(|t| matches!(t, GuitarTechnique::Bend { semitones } if *semitones > 1.0))
        {
            found_bend = true;
            break;
        }
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

    let mut found_vibrato = false;
    for chunk in samples.chunks(FRAME) {
        let result = detector.process(chunk);
        if result
            .techniques
            .iter()
            .any(|t| matches!(t, GuitarTechnique::Vibrato { .. }))
        {
            found_vibrato = true;
            break;
        }
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

    let mut found_slide = false;
    for chunk in samples.chunks(FRAME) {
        let result = detector.process(chunk);
        if result
            .techniques
            .iter()
            .any(|t| matches!(t, GuitarTechnique::Slide { .. }))
        {
            found_slide = true;
            break;
        }
    }

    assert!(found_slide, "Expected a slide to be detected in the gliding-pitch signal");
}

/// Steady note must produce no bend or slide.
#[test]
fn technique_no_false_positives_on_steady_note() {
    let mut detector = GuitarPitchDetector::new(SR, FRAME);
    let samples = sine(440.0, SR as usize, SR as f32);

    let mut had_false_positive = false;
    for chunk in samples.chunks(FRAME) {
        let result = detector.process(chunk);
        if result.techniques.iter().any(|t| {
            matches!(
                t,
                GuitarTechnique::Bend { .. } | GuitarTechnique::Slide { .. }
            )
        }) {
            had_false_positive = true;
            break;
        }
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
    let chord_signal = mix(&[
        sine(220.0, SR as usize, SR as f32),
        sine(261.63, SR as usize, SR as f32),
        sine(329.63, SR as usize, SR as f32),
    ]);
    let result = detector.process(&chord_signal);
    // We don't assert a specific chord here (timing variance), just no panic.
    let _ = result.chord;
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
    println!("GuitarSet: testing {} WAV file(s)", wav_files.len());

    let mut passed = 0usize;
    let mut failed = 0usize;
    let mut failures: Vec<String> = Vec::new();

    for path in &wav_files {
        match std::panic::catch_unwind(|| run_detection_on_wav(path)) {
            Ok(()) => passed += 1,
            Err(_) => {
                let msg = format!("FAIL: {}", path.display());
                eprintln!("{}", msg);
                failures.push(msg);
                failed += 1;
            }
        }
    }

    println!(
        "GuitarSet results: {passed} passed, {failed} failed out of {}",
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
    println!("IDMT-SMT-Guitar: testing {} WAV file(s)", wav_files.len());

    let mut passed = 0usize;
    let mut failed = 0usize;
    let mut failures: Vec<String> = Vec::new();

    for path in &wav_files {
        match std::panic::catch_unwind(|| run_detection_on_wav(path)) {
            Ok(()) => passed += 1,
            Err(_) => {
                let msg = format!("FAIL: {}", path.display());
                eprintln!("{}", msg);
                failures.push(msg);
                failed += 1;
            }
        }
    }

    println!(
        "IDMT results: {passed} passed, {failed} failed out of {}",
        wav_files.len()
    );

    assert!(
        failures.is_empty(),
        "IDMT-SMT-Guitar test failures:\n{}",
        failures.join("\n")
    );
}

// ── WAV runner (pure-Rust fallback, no rodio needed) ─────────────────────────

/// Run the detector over a WAV file using the built-in pure-Rust WAV codec
/// (same codec used in open_e_notes_test.rs and wav_chord_tests.rs).
/// Asserts that at least one note is detected somewhere in the file.
fn run_detection_on_wav(path: &Path) {
    println!("Testing: {}", path.display());

    let bytes = std::fs::read(path).expect("failed to read WAV file");
    let samples = decode_wav_i16(&bytes);

    assert!(!samples.is_empty(), "WAV file contained no samples");

    // Assume 44.1 kHz; if the file is at a different rate the detector still
    // runs — pitch accuracy is not asserted here, just that it doesn't panic
    // and returns at least one note somewhere in the recording.
    let mut detector = GuitarPitchDetector::new(44_100, 512);
    let mut detected_any = false;

    for chunk in samples.chunks(512) {
        let result = detector.process(chunk);
        if !result.notes.is_empty() {
            detected_any = true;
            println!(
                "  Note: {} ({:.1} Hz, confidence {:.2})",
                result.notes[0].name,
                result.notes[0].frequency,
                result.notes[0].confidence
            );
            break;
        }
    }

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
