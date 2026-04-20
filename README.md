# q-rs (guitar-pitch-detection)

A **Rust crate** exposing cycfi/q APIs for band-pass filtering and pitch detection,
plus real-time polyphonic guitar pitch detection.
Designed to be embedded in a [Godot 4](https://godotengine.org/) rhythm game (think Rocksmith) that
detects notes, chords, and playing techniques from live audio input.

---

## What's new in v0.2

| Area | Change |
|---|---|
| **Pitch engine** | Replaced pure resonator bank with [cycfi/q](https://github.com/cycfi/q) (C++ BACF algorithm) via Rust FFI — faster lock-on, better accuracy on real guitar signals |
| **Audio I/O** | Unified under the **cpal** ecosystem: `rodio` decodes WAV / OGG / MP3 / FLAC; `cpal` drives live USB capture (Rocksmith Real Tone Cable) |
| **DSP utilities** | [dasp](https://github.com/RustAudio/dasp) added for sample-format conversions (i16 ↔ f32 ↔ u16) |
| **Techniques** | Bend, slide, vibrato, palm mute detected from Q's continuous pitch stream |
| **Datasets** | GuitarSet representative samples included in `tests/dataset/guitarset/` |

---

## Architecture

```
┌──────────────────── GuitarPitchDetector ────────────────────────┐
│                                                                  │
│  Audio frame (f32 mono)                                          │
│        │                                                         │
│        ├──► cycfi/q (C++ BACF, via FFI)                         │
│        │        └─► QPitchDetector → frequency + periodicity     │
│        │                    └──► TechniqueDetector               │
│        │                              └─► bend / slide /         │
│        │                                  vibrato / palm-mute    │
│        │                                                         │
│        └──► ResonatorBank (IIR, 53 semitones)                   │
│                 └─► peak-pick → DetectedNote[]                   │
│                           └─► chord pattern match                │
│                                                                  │
│  Returns: DetectionResult { notes, chord, techniques }           │
└──────────────────────────────────────────────────────────────────┘
```

**Key properties**

| Property | Value |
|---|---|
| Latency | Configurable (default frame ≈ 11 ms at 44.1 kHz) |
| Polyphony | Up to 6 simultaneous notes |
| Pitch engine | cycfi/q BACF (C++20, MIT) — no pitch code reimplemented |
| Build deps | `cc` (compile Q wrapper), `dasp_sample` |
| Runtime deps | none by default; `rodio` + `cpal` with `audio_input` feature |
| Platforms | Linux, macOS, Windows, Raspberry Pi |

---

## Quick start (Rust)

```toml
[dependencies]
q-rs = { git = "https://github.com/santzit/guitar-pitch-detection-" }

# Enable live capture + file decoding (requires ALSA headers on Linux):
# q-rs = { git = "...", features = ["audio_input"] }
```

## Exposed Q APIs (Rust)

```rust
use guitar_pitch_detection::{QBandpassFilter, QPitchDetector};

let mut bp = QBandpassFilter::new(110.0, 44_100.0, 8.0);
let y = bp.process(0.25);

let mut pd = QPitchDetector::new_guitar(44_100.0);
let _ready = pd.process(y);
let _freq_hz = pd.frequency();
```

```rust
use guitar_pitch_detection::{GuitarPitchDetector, GuitarTechnique};

fn main() {
    let mut detector = GuitarPitchDetector::new(44_100, 512);

    // Replace with real PCM frames from cpal / rodio / file reader.
    let samples: Vec<f32> = vec![0.0; 512];

    let result = detector.process(&samples);

    for note in &result.notes {
        println!("{} ({:.1} Hz)  confidence={:.2}",
                 note.name, note.frequency, note.confidence);
    }
    if let Some(chord) = &result.chord {
        println!("Chord: {}", chord.name);
    }
    for technique in &result.techniques {
        match technique {
            GuitarTechnique::Bend { semitones } =>
                println!("Bend  {semitones:+.2} semitones"),
            GuitarTechnique::Slide { from_midi, to_midi, .. } =>
                println!("Slide {from_midi} → {to_midi}"),
            GuitarTechnique::Vibrato { rate_hz, depth_semitones } =>
                println!("Vibrato {rate_hz:.1} Hz  ±{depth_semitones:.2} st"),
            GuitarTechnique::PalmMute =>
                println!("Palm mute"),
            _ => {}
        }
    }
}
```

---

## Audio input

Both paths use the same **cpal** ecosystem — there is no separate WAV library.

### WAV / audio file decoding

```rust
#[cfg(feature = "audio_input")]
{
    use guitar_pitch_detection::audio_input::FileReader;

    // Supports WAV (16/24/32-bit), OGG, MP3, FLAC via rodio + symphonia.
    let mut reader = FileReader::open("recording.wav")?;
    let mut detector = GuitarPitchDetector::new(reader.sample_rate(), 512);

    while let Some(frame) = reader.next_frame(512) {
        let result = detector.process(&frame);
        // …
    }
}
```

### Live capture — Rocksmith Real Tone Cable / USB audio

```rust
#[cfg(feature = "audio_input")]
{
    use guitar_pitch_detection::audio_input::{list_input_devices, LiveCapture};

    // Enumerate system input devices.
    println!("{:?}", list_input_devices());

    // Open the Rocksmith Real Tone Cable (partial name match, case-insensitive).
    let capture = LiveCapture::open(Some("Rocksmith"))?;
    let mut detector = GuitarPitchDetector::new(capture.sample_rate(), 512);

    loop {
        let frame = capture.read_frame(512);
        let result = detector.process(&frame);
        // …
    }
}
```

> **Linux prerequisite:** `sudo apt install libasound2-dev`

---

## Guitar techniques detected

| Technique | Description |
|---|---|
| `Bend { semitones }` | Monotone pitch rise/fall (string bend or release) |
| `Slide { from, to, ascending }` | Rapid pitch jump across ≥ 1.5 semitones |
| `Vibrato { rate_hz, depth_semitones }` | Periodic pitch oscillation (3–9 Hz, ≥ 0.1 st) |
| `PalmMute` | Rapid energy decay after the attack transient |
| `HammerOn` | *(reserved)* |
| `PullOff` | *(reserved)* |

---

## Chord types detected

Major, Minor, Dominant7, Major7, Minor7, Diminished, Augmented, Sus2, Sus4, Power

---

## Custom configuration

```rust
use guitar_pitch_detection::{GuitarPitchDetector, DetectorConfig};

let config = DetectorConfig {
    sample_rate: 48_000,
    alpha: 0.998,               // narrower bandwidth → sharper selectivity
    detection_threshold: 0.10,  // fraction of peak energy to report a note
    max_polyphony: 6,
    ..Default::default()
};
let mut detector = GuitarPitchDetector::with_config(config);
```

---

## Building

```bash
# Prerequisites (Linux only, for audio_input feature)
sudo apt install libasound2-dev

# Also initialise the cycfi/q and infra submodules:
git submodule update --init --recursive
```

```bash
# Debug build + test
cargo test

# Release shared library (for Godot GDExtension)
cargo build --release
# Linux  → target/release/libguitar_pitch_detection.so
# macOS  → target/release/libguitar_pitch_detection.dylib
# Windows→ target/release/guitar_pitch_detection.dll

# Lint
cargo clippy -- -D warnings

# With audio_input (live capture + file decoding)
cargo build --release --features audio_input
```

---

## Project structure

```
src/
  lib.rs           — crate root, public re-exports
  types.rs         — DetectedNote, DetectedChord, ChordQuality,
                     GuitarTechnique, DetectionResult
  notes.rs         — MIDI ↔ frequency conversion, note name table
  resonator.rs     — ComplexResonator, ResonatorBank (polyphonic layer)
  chord.rs         — chord pattern matching
  detector.rs      — GuitarPitchDetector (main API, orchestrates both layers)
  q_wrapper.{hpp,cpp} — C shim around cycfi::q::pitch_detector
  q_sys.rs         — raw unsafe FFI bindings
  q_pitch.rs       — safe QPitchDetector Rust wrapper
  techniques.rs    — TechniqueDetector (bend / slide / vibrato / palm-mute)
  audio_input.rs   — FileReader + LiveCapture (feature = "audio_input")
  ffi.rs           — C FFI for Godot / GDExtension

vendor/
  q/               — cycfi/q submodule (MIT)
  infra/           — cycfi/infra submodule (MIT, required by q)

tests/
  integration_tests.rs      — end-to-end (all 6 open strings, chords, …)
  open_e_notes_test.rs      — WAV-based individual note tests
  wav_chord_tests.rs        — WAV-based chord tests
  dataset_tests.rs          — GuitarSet + IDMT dataset tests + technique regression
  dataset/
    guitarset/audio/mic/    — GuitarSet representative samples
                              (replace with real dataset — see README inside)
    idmt_guitar/            — IDMT-SMT-Guitar representative samples
                              (replace with full dataset from zenodo.org/record/7544110)
    README.md               — dataset download instructions
```

---

## Godot 4 / GDExtension integration

The C FFI is unchanged from v0.1.  See `src/ffi.rs` for full struct layouts.

```c
// C API
GPitchDetector* gpd_create(uint32_t sample_rate, uint32_t frame_size);
void            gpd_destroy(GPitchDetector* ptr);
int             gpd_process(GPitchDetector* ptr,
                            const float* samples, uint32_t count,
                            CDetectionResult* out);
void            gpd_reset(GPitchDetector* ptr);
```

---

## Datasets

GuitarSet representative samples (following real naming conventions) are
included in `tests/dataset/guitarset/audio/mic/`.  To replace them with the
full 1.7 GB dataset:

```bash
pip install mirdata
python3 -c "
import mirdata
gs = mirdata.initialize('guitarset', data_home='tests/dataset/guitarset')
gs.download(partial_download=['audio_mic'])
"
```

IDMT-SMT-Guitar representative samples (156 WAV files matching the real dataset's
naming and directory convention) are included in `tests/dataset/idmt_guitar/`.
To replace them with the full dataset, download from
https://zenodo.org/record/7544110 and extract into `tests/dataset/idmt_guitar/`.

```bash
unzip IDMT-SMT-Guitar_V2.zip -d tests/dataset/idmt_guitar
```

---

## License

GNU General Public License v3.0 — see [LICENSE](LICENSE).

cycfi/q and cycfi/infra (in `vendor/`) are MIT licensed.
