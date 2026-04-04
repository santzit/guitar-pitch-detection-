# guitar-pitch-detection

A **Rust library** for real-time polyphonic guitar pitch detection.
Designed to be embedded in a [Godot 4](https://godotengine.org/) rhythm game (think Rocksmith) that needs to detect notes, chords, and guitar runs from live audio input.

## Algorithm

The core is a **complex resonator bank** — the same time-domain algorithm described in:

> *"A Computationally Efficient Method for Polyphonic Pitch Estimation"*
> Zhou, Reiss, Mattavelli, Zoia
> (implemented in C++ by [luciamarock/Polyphonic-Pitch-Detector-for-guitars](https://github.com/luciamarock/Polyphonic-Pitch-Detector-for-guitars))

One resonator is tuned to each semitone in the guitar range (C2 – E6, MIDI 36–88).
Each resonator runs the first-order IIR filter:

```
r[n] = x[n] + α · r[n-1] · e^(j·ω₀)
```

After processing a frame, the resonator with the highest energy corresponds to
the sounding pitch.

**Key properties**
| Property | Value |
|---|---|
| Latency | Configurable (default frame ≈ 11 ms at 44.1 kHz) |
| Polyphony | Up to 6 simultaneous notes (one per guitar string) |
| Dependencies | **Zero** — pure Rust, no MATLAB, no FFT library |
| Platforms | Any (Linux, macOS, Windows, Raspberry Pi, WASM) |

---

## Features

- 🎸 **All 6 open strings** — E2, A2, D3, G3, B3, E4 — detected accurately
- 🎵 **Chords** — Major, Minor, Dom7, Maj7, Min7, Dim, Aug, Sus2, Sus4, Power
- ⚡ **Real-time** — O(N\_notes × N\_samples) per frame, negligible CPU cost
- 🕹️ **Godot-ready** — C FFI layer (`extern "C"`) for GDExtension integration
- 🔧 **Configurable** — sample rate, alpha (decay), threshold, polyphony limit

---

## Quick start (Rust)

Add to `Cargo.toml`:

```toml
[dependencies]
guitar-pitch-detection = { git = "https://github.com/santzit/guitar-pitch-detection-" }
```

```rust
use guitar_pitch_detection::GuitarPitchDetector;

fn main() {
    let mut detector = GuitarPitchDetector::new(44_100, 512);

    // Replace with real PCM frames from cpal / rodio / etc.
    let samples: Vec<f32> = vec![0.0; 512];

    let result = detector.process(&samples);

    for note in &result.notes {
        println!("{} ({:.1} Hz)  confidence={:.2}", note.name, note.frequency, note.confidence);
    }
    if let Some(chord) = &result.chord {
        println!("Chord: {}", chord.name);
    }
}
```

### Custom configuration

```rust
use guitar_pitch_detection::{GuitarPitchDetector, DetectorConfig};

let config = DetectorConfig {
    sample_rate: 48_000,
    alpha: 0.998,               // narrower bandwidth → sharper frequency selectivity
    detection_threshold: 0.10,  // fraction of peak energy required to report a note
    max_polyphony: 6,
    ..Default::default()
};
let mut detector = GuitarPitchDetector::with_config(config);
```

---

## Godot 4 / GDExtension integration

### 1 — Build the shared library

```bash
# Linux
cargo build --release
# → target/release/libguitar_pitch_detection.so

# macOS
cargo build --release
# → target/release/libguitar_pitch_detection.dylib

# Windows
cargo build --release
# → target/release/guitar_pitch_detection.dll
```

### 2 — C API reference

| Function | Description |
|---|---|
| `gpd_create(sample_rate, frame_size)` | Allocate a detector; returns opaque pointer |
| `gpd_destroy(ptr)` | Free the detector |
| `gpd_process(ptr, samples, count, result)` | Process audio frame; fills `CDetectionResult` |
| `gpd_reset(ptr)` | Clear resonator states (between songs) |

`CDetectionResult` layout (see `src/ffi.rs`):

```c
typedef struct {
    uint32_t note_count;        // 0–6
    CNote    notes[6];
    uint8_t  has_chord;         // 1 = chord found
    char     chord_name[16];    // e.g. "Am\0"
    uint8_t  chord_root;        // pitch class 0–11, 255 = none
    uint8_t  chord_quality;     // 0=Major 1=Minor 2=Dom7 3=Maj7 4=Min7
                                // 5=Dim 6=Aug 7=Sus2 8=Sus4 9=Power
    float    chord_confidence;  // 0.0–1.0
} CDetectionResult;

typedef struct {
    float   frequency;
    uint8_t midi_note;
    uint8_t semitone;    // pitch class 0–11
    int8_t  octave;
    char    name[8];     // e.g. "E2\0"
    float   confidence;
} CNote;
```

### 3 — GDScript example (Godot 4)

```gdscript
var lib = NativeLibrary.new()
lib.open("res://libguitar_pitch_detection.so")

var detector = lib.call("gpd_create", 44100, 512)

func _process_audio_frame(pcm_data: PackedFloat32Array) -> void:
    var result = lib.call("gpd_process", detector, pcm_data, pcm_data.size())
    if result.note_count > 0:
        print("Note: ", result.notes[0].name)
    if result.has_chord:
        print("Chord: ", result.chord_name)
```

---

## Building and testing

```bash
# Run all tests
cargo test

# Lint
cargo clippy -- -D warnings

# Release build (shared library + Rust rlib)
cargo build --release
```

---

## Project structure

```
src/
  lib.rs         — crate root, public re-exports
  types.rs       — DetectedNote, DetectedChord, ChordQuality, DetectionResult
  notes.rs       — MIDI ↔ frequency conversion, note name table
  resonator.rs   — ComplexResonator, ResonatorBank
  chord.rs       — chord pattern matching
  detector.rs    — GuitarPitchDetector (main API)
  ffi.rs         — C FFI for Godot / GDExtension
tests/
  integration_tests.rs — end-to-end tests (all 6 open strings, chords, …)
```

---

## License

GNU General Public License v3.0 — see [LICENSE](LICENSE).
