//! Interactive terminal CLI for real-time guitar pitch detection.
//!
//! # Usage
//! ```shell
//! cargo run --bin cli --features audio_input
//! ```
//!
//! The app:
//!   1. Lists all available audio input devices.
//!   2. Prompts the user to select one (or press Enter for the system default).
//!   3. Starts real-time pitch detection and prints detected notes, chords and
//!      playing techniques to the terminal as they arrive.
//!   4. Press Ctrl+C to exit.

// When the crate is compiled WITHOUT the `audio_input` feature we still want
// this binary to compile — it just prints a helpful error and exits.
#[cfg(not(feature = "audio_input"))]
fn main() {
    eprintln!("This binary requires the `audio_input` Cargo feature.");
    eprintln!("Rebuild with:  cargo run --bin cli --features audio_input");
    std::process::exit(1);
}

#[cfg(feature = "audio_input")]
fn main() {
    use guitar_pitch_detection::audio_input::{list_input_devices, LiveCapture};
    use guitar_pitch_detection::GuitarPitchDetector;
    use std::io::{self, BufRead, Write};

    // ── Banner ────────────────────────────────────────────────────────────────

    println!("╔══════════════════════════════════════════╗");
    println!("║   Guitar Pitch Detection  –  CLI  Test   ║");
    println!("╚══════════════════════════════════════════╝");
    println!();

    // ── List devices ──────────────────────────────────────────────────────────

    let devices = list_input_devices();

    if devices.is_empty() {
        eprintln!("No audio input devices were found on this system.");
        std::process::exit(1);
    }

    println!("Available audio input devices:");
    println!("  [0]  System default");
    for (i, name) in devices.iter().enumerate() {
        println!("  [{}]  {}", i + 1, name);
    }
    println!();

    // ── Device selection ──────────────────────────────────────────────────────

    print!("Select a device [0–{}] (default 0): ", devices.len());
    io::stdout().flush().unwrap();

    let stdin = io::stdin();
    let line = stdin
        .lock()
        .lines()
        .next()
        .unwrap_or_else(|| Ok(String::new()))
        .unwrap_or_default();

    let choice: usize = line.trim().parse().unwrap_or(0);

    let capture = if choice == 0 || choice > devices.len() {
        println!("Opening system default input device…");
        LiveCapture::new()
    } else {
        let name = &devices[choice - 1];
        println!("Opening device: {}", name);
        // Pass a fragment of the name so LiveCapture's substring match finds it.
        LiveCapture::open(Some(name.as_str()))
    };

    let capture = match capture {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to open audio device: {}", e);
            std::process::exit(1);
        }
    };

    // ── Detection loop ────────────────────────────────────────────────────────

    let sample_rate = capture.sample_rate();
    const FRAME: usize = 512;
    let mut detector = GuitarPitchDetector::new(sample_rate, FRAME);

    println!();
    println!(
        "Capturing at {} Hz.  Play your guitar — detected notes appear below.",
        sample_rate
    );
    println!("Press Ctrl+C to exit.\n");

    loop {
        let frame = capture.read_frame(FRAME);
        let result = detector.process(&frame);

        if !result.notes.is_empty() {
            // Notes
            let note_list: Vec<&str> = result.notes.iter().map(|n| n.name).collect();
            print!("Notes: [{}]", note_list.join(", "));

            // Chord (if any)
            if let Some(chord) = &result.chord {
                print!("  Chord: {}", chord.name);
            }

            // Techniques (if any)
            if !result.techniques.is_empty() {
                let tech_strs: Vec<String> =
                    result.techniques.iter().map(format_technique).collect();
                print!("  Techniques: [{}]", tech_strs.join(", "));
            }

            println!();
        }

        // Yield the thread briefly so we don't pin a CPU core.
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

// ── Formatting helpers ─────────────────────────────────────────────────────────

#[cfg(feature = "audio_input")]
fn format_technique(t: &guitar_pitch_detection::GuitarTechnique) -> String {
    use guitar_pitch_detection::GuitarTechnique;
    match t {
        GuitarTechnique::Bend { semitones } => {
            if *semitones >= 0.0 {
                format!("Bend(+{:.1}st)", semitones)
            } else {
                format!("Bend({:.1}st)", semitones)
            }
        }
        GuitarTechnique::Slide {
            from_midi,
            to_midi,
            ascending,
        } => {
            let dir = if *ascending { "↑" } else { "↓" };
            format!("Slide({} → {} {})", from_midi, to_midi, dir)
        }
        GuitarTechnique::Vibrato {
            rate_hz,
            depth_semitones,
        } => format!("Vibrato({:.1} Hz, {:.2}st)", rate_hz, depth_semitones),
        GuitarTechnique::PalmMute => "PalmMute".to_string(),
        GuitarTechnique::HammerOn => "HammerOn".to_string(),
        GuitarTechnique::PullOff => "PullOff".to_string(),
    }
}
