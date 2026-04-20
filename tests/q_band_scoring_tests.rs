//! Dataset-backed tests for Q band-pass lane scoring (Rocksmith-style).
//!
//! Uses representative IDMT-SMT-Guitar files already committed to this repo.

use guitar_pitch_detection::QBandpassFilter;
use std::path::Path;

const SR_FALLBACK: u32 = 44_100;
const FRAME: usize = 1024;

fn check(time_ms: f32, expected: &str, detected: &str, passed: bool) {
    println!(
        "[{:8.1} ms] Expected: {:<55}  |  Detected: {}  –  {}",
        time_ms,
        expected,
        detected,
        if passed { "OK" } else { "FAILED" }
    );
}

fn decode_wav_i16(wav: &[u8]) -> (Vec<f32>, u32) {
    if wav.len() < 44 {
        return (vec![], 0);
    }
    let channels = u16::from_le_bytes([wav[22], wav[23]]) as usize;
    let sr = u32::from_le_bytes([wav[24], wav[25], wav[26], wav[27]]);
    let bits = u16::from_le_bytes([wav[34], wav[35]]) as usize;

    let mut pos = 12usize;
    let (data_start, data_len) = loop {
        if pos + 8 > wav.len() {
            return (vec![], sr);
        }
        let id = &wav[pos..pos + 4];
        let len =
            u32::from_le_bytes([wav[pos + 4], wav[pos + 5], wav[pos + 6], wav[pos + 7]]) as usize;
        if id == b"data" {
            break (pos + 8, len);
        }
        pos += 8 + len;
    };

    let data = &wav[data_start..(data_start + data_len).min(wav.len())];
    let bytes_per_sample = bits / 8;
    let frame_bytes = bytes_per_sample * channels;
    if frame_bytes == 0 {
        return (vec![], sr);
    }

    let mut out = Vec::with_capacity(data.len() / frame_bytes);
    let mut i = 0usize;
    while i + frame_bytes <= data.len() {
        let mut acc = 0.0f32;
        for ch in 0..channels {
            let off = i + ch * bytes_per_sample;
            let s = match bits {
                16 => i16::from_le_bytes([data[off], data[off + 1]]) as f32 / 32_768.0,
                8 => (data[off] as f32 - 128.0) / 128.0,
                _ => 0.0,
            };
            acc += s;
        }
        out.push(acc / channels as f32);
        i += frame_bytes;
    }
    (out, sr)
}

fn read_wav(path: &Path) -> Option<(Vec<f32>, u32)> {
    let bytes = std::fs::read(path).ok()?;
    let decoded = decode_wav_i16(&bytes);
    if decoded.0.is_empty() {
        return None;
    }
    Some(decoded)
}

fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let power = samples.iter().map(|x| x * x).sum::<f32>() / samples.len() as f32;
    power.sqrt()
}

fn lane_score(samples: &[f32], sr: u32, center_hz: f32) -> (usize, f32, f32) {
    let mut bp = QBandpassFilter::new(center_hz, sr as f32, 8.0);
    let mut frame_rms = Vec::new();
    for frame in samples.chunks(FRAME) {
        let filtered = bp.process_frame(frame);
        frame_rms.push(rms(&filtered));
    }
    let peak = frame_rms.iter().copied().fold(0.0f32, f32::max);
    let avg = if frame_rms.is_empty() {
        0.0
    } else {
        frame_rms.iter().sum::<f32>() / frame_rms.len() as f32
    };
    let threshold = peak * 0.35;
    let hits = frame_rms
        .iter()
        .filter(|&&x| x >= threshold && x > 0.0)
        .count();
    (hits, peak, avg)
}

#[test]
fn band_filter_lane_scoring_detects_simultaneous_strings_from_dataset() {
    let e2_path = Path::new("tests/dataset/idmt_guitar/dataset2/G1/normal/G1_normal_E2.wav");
    let a2_path = Path::new("tests/dataset/idmt_guitar/dataset2/G1/normal/G1_normal_A2.wav");
    if !e2_path.exists() || !a2_path.exists() {
        println!(
            "[SKIP] Required dataset files not present:\n  {}\n  {}",
            e2_path.display(),
            a2_path.display()
        );
        return;
    }

    let (e2_samples, sr_e) = read_wav(e2_path).expect("failed to decode E2 wav");
    let (a2_samples, sr_a) = read_wav(a2_path).expect("failed to decode A2 wav");
    let sr = if sr_e == 0 { SR_FALLBACK } else { sr_e };
    assert_eq!(sr_e, sr_a, "dataset sample rates should match");

    let n = e2_samples.len().min(a2_samples.len());
    let e2 = &e2_samples[..n];
    let a2 = &a2_samples[..n];
    let mixed: Vec<f32> = e2
        .iter()
        .zip(a2.iter())
        .map(|(&e, &a)| (e + a) * 0.5)
        .collect();

    let (e_lane_hits_e2, e_lane_peak_e2, _) = lane_score(e2, sr, 82.41);
    let (e_lane_hits_a2, e_lane_peak_a2, _) = lane_score(a2, sr, 82.41);
    let (a_lane_hits_a2, a_lane_peak_a2, _) = lane_score(a2, sr, 110.0);
    let (a_lane_hits_e2, a_lane_peak_e2, _) = lane_score(e2, sr, 110.0);
    let (e_lane_hits_mix, e_lane_peak_mix, _) = lane_score(&mixed, sr, 82.41);
    let (a_lane_hits_mix, a_lane_peak_mix, _) = lane_score(&mixed, sr, 110.0);

    let t = n as f32 * 1_000.0 / sr as f32;

    let e_selective = e_lane_peak_e2 > e_lane_peak_a2 * 1.5;
    check(
        t,
        "E2 lane should respond stronger to E2 than A2",
        &format!(
            "peak(E2)={:.4}, peak(A2)={:.4}",
            e_lane_peak_e2, e_lane_peak_a2
        ),
        e_selective,
    );
    assert!(e_selective, "E2 lane lacked selectivity");

    let a_selective = a_lane_peak_a2 > a_lane_peak_e2 * 1.5;
    check(
        t,
        "A2 lane should respond stronger to A2 than E2",
        &format!(
            "peak(A2)={:.4}, peak(E2)={:.4}",
            a_lane_peak_a2, a_lane_peak_e2
        ),
        a_selective,
    );
    assert!(a_selective, "A2 lane lacked selectivity");

    let both_scored = e_lane_hits_mix > 0 && a_lane_hits_mix > 0;
    check(
        t,
        "Simultaneous E2 + A2 should score both lanes",
        &format!(
            "mix hits: E-lane={}, A-lane={}",
            e_lane_hits_mix, a_lane_hits_mix
        ),
        both_scored,
    );
    assert!(both_scored, "expected both lanes to score on mixed signal");

    check(
        t,
        "Mixed signal keeps E lane active",
        &format!("solo={}, mixed={}", e_lane_hits_e2, e_lane_hits_mix),
        e_lane_hits_mix >= e_lane_hits_e2 / 2,
    );
    assert!(e_lane_hits_mix >= e_lane_hits_e2 / 2);

    check(
        t,
        "Mixed signal keeps A lane active",
        &format!("solo={}, mixed={}", a_lane_hits_a2, a_lane_hits_mix),
        a_lane_hits_mix >= a_lane_hits_a2 / 2,
    );
    assert!(a_lane_hits_mix >= a_lane_hits_a2 / 2);

    let _ = (
        e_lane_hits_a2,
        a_lane_hits_e2,
        e_lane_peak_mix,
        a_lane_peak_mix,
    );
}
