//! Concatenate GuitarSet (and optionally IDMT-SMT-Guitar) sample recordings
//! into a single test-fixture WAV file for use in automated tests.
//!
//! # Usage
//! ```shell
//! cargo run --bin make_test_wav
//! ```
//!
//! Reads all `.wav` files found under
//! * `tests/dataset/guitarset/audio/mic/`
//! * `tests/dataset/idmt_guitar/`
//!
//! …concatenates their mono samples and writes the result to
//! `tests/dataset/test_fixture.wav`, capped at **3 minutes** of audio.
//!
//! Run this once to generate the fixture; then `cargo test` will pick it up.

use std::path::{Path, PathBuf};

// ── Constants ──────────────────────────────────────────────────────────────────

/// Target sample rate.  WAV files at a different rate are still appended —
/// they will play back at the wrong pitch, but for smoke-testing the detector
/// that is acceptable.
const TARGET_SR: u32 = 44_100;

/// Maximum number of mono samples to include (3 minutes × 44100 Hz).
const MAX_SAMPLES: usize = TARGET_SR as usize * 180;

/// Output path (relative to the crate root, i.e. the directory containing Cargo.toml).
const OUTPUT_PATH: &str = "tests/dataset/test_fixture.wav";

/// Input directories to scan (relative to crate root).
const INPUT_DIRS: &[&str] = &[
    "tests/dataset/guitarset/audio/mic",
    "tests/dataset/idmt_guitar",
];

// ── Entry point ────────────────────────────────────────────────────────────────

fn main() {
    let mut all_samples: Vec<f32> = Vec::with_capacity(MAX_SAMPLES);

    // Collect candidate WAV files from every input directory.
    let mut wav_files: Vec<PathBuf> = INPUT_DIRS
        .iter()
        .flat_map(|dir| find_wav_files(Path::new(dir)))
        .collect();

    wav_files.sort();

    if wav_files.is_empty() {
        eprintln!("No WAV files found in the dataset directories.");
        eprintln!("Expected files in one of:");
        for dir in INPUT_DIRS {
            eprintln!("  {}", dir);
        }
        eprintln!("See tests/dataset/README.md for download instructions.");
        std::process::exit(1);
    }

    println!("Found {} WAV file(s). Concatenating (cap = 3 min)…", wav_files.len());

    for path in &wav_files {
        if all_samples.len() >= MAX_SAMPLES {
            break;
        }

        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("  skip: {} ({})", path.display(), e);
                continue;
            }
        };

        let (samples, sr) = decode_wav_mono(&bytes);

        if samples.is_empty() {
            eprintln!(
                "  skip: {} (empty or unsupported WAV format)",
                path.display()
            );
            continue;
        }

        if sr != TARGET_SR {
            eprintln!(
                "  note: {} is at {} Hz (target {} Hz) — appended as-is",
                path.display(),
                sr,
                TARGET_SR
            );
        }

        let remaining = MAX_SAMPLES - all_samples.len();
        let take = samples.len().min(remaining);
        all_samples.extend_from_slice(&samples[..take]);

        println!(
            "  + {} ({} samples, {} Hz)",
            path.file_name().unwrap_or_default().to_string_lossy(),
            take,
            sr
        );
    }

    if all_samples.is_empty() {
        eprintln!("No samples collected — aborting.");
        std::process::exit(1);
    }

    let duration_s = all_samples.len() as f32 / TARGET_SR as f32;
    println!(
        "\nTotal samples : {}\nDuration      : {:.1} s ({:.1} min)",
        all_samples.len(),
        duration_s,
        duration_s / 60.0
    );

    write_wav_mono(Path::new(OUTPUT_PATH), &all_samples, TARGET_SR);
    println!("Written        : {}", OUTPUT_PATH);
}

// ── Pure-Rust WAV helpers ──────────────────────────────────────────────────────

/// Recursively collect all `.wav` files under `dir`.
fn find_wav_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if !dir.exists() {
        return out;
    }
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                out.extend(find_wav_files(&path));
            } else if path.extension().and_then(|e| e.to_str()) == Some("wav") {
                out.push(path);
            }
        }
    }
    out
}

/// Decode a WAV byte slice into mono f32 samples.
///
/// Supports PCM 8-bit and 16-bit, mono and stereo (stereo is downmixed by
/// averaging).  Returns `(samples, sample_rate)`.
fn decode_wav_mono(wav: &[u8]) -> (Vec<f32>, u32) {
    if wav.len() < 44 {
        return (vec![], 0);
    }
    // Verify RIFF/WAVE magic.
    if &wav[0..4] != b"RIFF" || &wav[8..12] != b"WAVE" {
        return (vec![], 0);
    }

    let num_channels = u16::from_le_bytes([wav[22], wav[23]]) as usize;
    let sample_rate = u32::from_le_bytes([wav[24], wav[25], wav[26], wav[27]]);
    let bits_per_sample = u16::from_le_bytes([wav[34], wav[35]]) as usize;

    if num_channels == 0 || bits_per_sample == 0 {
        return (vec![], sample_rate);
    }

    // Walk chunks to find "data".
    let mut pos = 12usize;
    let data_start;
    let data_len;
    loop {
        if pos + 8 > wav.len() {
            return (vec![], sample_rate);
        }
        let chunk_id = &wav[pos..pos + 4];
        let chunk_size =
            u32::from_le_bytes([wav[pos + 4], wav[pos + 5], wav[pos + 6], wav[pos + 7]]) as usize;
        if chunk_id == b"data" {
            data_start = pos + 8;
            data_len = chunk_size;
            break;
        }
        pos += 8 + chunk_size;
        if pos >= wav.len() {
            return (vec![], sample_rate);
        }
    }

    let data_end = (data_start + data_len).min(wav.len());
    let data = &wav[data_start..data_end];
    let bytes_per_sample = bits_per_sample / 8;
    let frame_bytes = bytes_per_sample * num_channels;

    if frame_bytes == 0 {
        return (vec![], sample_rate);
    }

    let mut samples = Vec::with_capacity(data.len() / frame_bytes);
    let mut i = 0;
    while i + frame_bytes <= data.len() {
        let mut sum = 0.0f32;
        for ch in 0..num_channels {
            let off = i + ch * bytes_per_sample;
            let s = match bits_per_sample {
                8 => (data[off] as f32 - 128.0) / 128.0,
                16 => i16::from_le_bytes([data[off], data[off + 1]]) as f32 / 32_768.0,
                24 => {
                    // 24-bit signed PCM — sign-extend to i32.
                    let raw = i32::from_le_bytes([data[off], data[off + 1], data[off + 2], 0]);
                    let signed = if raw & 0x0080_0000 != 0 {
                        raw | !0x00FF_FFFF_u32 as i32
                    } else {
                        raw
                    };
                    signed as f32 / 8_388_608.0
                }
                32 => i32::from_le_bytes([data[off], data[off + 1], data[off + 2], data[off + 3]])
                    as f32
                    / 2_147_483_648.0,
                _ => 0.0,
            };
            sum += s;
        }
        samples.push(sum / num_channels as f32);
        i += frame_bytes;
    }

    (samples, sample_rate)
}

/// Write mono f32 samples as a 16-bit PCM WAV file.
fn write_wav_mono(path: &Path, samples: &[f32], sample_rate: u32) {
    use std::io::Write;

    let num_samples = samples.len();
    let data_size = num_samples * 2; // 16-bit = 2 bytes per sample
    let file_size = 36 + data_size; // RIFF payload size

    let mut buf: Vec<u8> = Vec::with_capacity(8 + file_size);

    // RIFF header
    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&(file_size as u32).to_le_bytes());
    buf.extend_from_slice(b"WAVE");

    // fmt chunk (16 bytes payload, PCM = format 1)
    buf.extend_from_slice(b"fmt ");
    buf.extend_from_slice(&16u32.to_le_bytes());
    buf.extend_from_slice(&1u16.to_le_bytes()); // PCM
    buf.extend_from_slice(&1u16.to_le_bytes()); // mono
    buf.extend_from_slice(&sample_rate.to_le_bytes());
    let byte_rate = sample_rate * 2; // sr × channels(1) × bytes_per_sample(2)
    buf.extend_from_slice(&byte_rate.to_le_bytes());
    buf.extend_from_slice(&2u16.to_le_bytes()); // block align
    buf.extend_from_slice(&16u16.to_le_bytes()); // bits per sample

    // data chunk
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&(data_size as u32).to_le_bytes());
    for &s in samples {
        let clamped = s.clamp(-1.0, 1.0);
        let pcm = (clamped * 32_767.0) as i16;
        buf.extend_from_slice(&pcm.to_le_bytes());
    }

    let mut file = std::fs::File::create(path)
        .unwrap_or_else(|e| panic!("Cannot create {}: {}", path.display(), e));
    file.write_all(&buf)
        .unwrap_or_else(|e| panic!("Cannot write {}: {}", path.display(), e));
}
