//! Guitar note frequency table and MIDI / frequency / name conversion helpers.
//!
//! Standard guitar range: E2 (MIDI 40, 82.41 Hz) – B5 (MIDI 83, 987.77 Hz).
//! We include a small margin on each side so the detector can handle drop
//! tunings (down to C2, MIDI 36) and harmonics up to MIDI 88 (E6).

/// Lowest MIDI note covered by the detector (C2 = 36, one step below drop-C).
pub const MIN_MIDI: u8 = 36;

/// Highest MIDI note covered by the detector (E6 = 88).
pub const MAX_MIDI: u8 = 88;

/// Number of notes in the resonator bank.
pub const NUM_NOTES: usize = (MAX_MIDI - MIN_MIDI + 1) as usize;

/// The 12 chromatic note names (using sharps).
pub const NOTE_NAMES_12: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

/// Pre-computed table of short note names for every MIDI note 0–127.
///
/// Generated at compile time via a `const fn` equivalent — built as a
/// `static` array using an initialisation macro.
pub static NOTE_NAME_TABLE: [&str; 128] = build_note_name_table();

const fn build_note_name_table() -> [&'static str; 128] {
    // Hand-encode all 128 MIDI notes.  `const fn` cannot use format!, so we
    // use a precomputed literal list.
    [
        "C-1", "C#-1", "D-1", "D#-1", "E-1", "F-1", "F#-1", "G-1", "G#-1", "A-1", "A#-1",
        "B-1", // 0-11
        "C0", "C#0", "D0", "D#0", "E0", "F0", "F#0", "G0", "G#0", "A0", "A#0", "B0", // 12-23
        "C1", "C#1", "D1", "D#1", "E1", "F1", "F#1", "G1", "G#1", "A1", "A#1", "B1", // 24-35
        "C2", "C#2", "D2", "D#2", "E2", "F2", "F#2", "G2", "G#2", "A2", "A#2", "B2", // 36-47
        "C3", "C#3", "D3", "D#3", "E3", "F3", "F#3", "G3", "G#3", "A3", "A#3", "B3", // 48-59
        "C4", "C#4", "D4", "D#4", "E4", "F4", "F#4", "G4", "G#4", "A4", "A#4", "B4", // 60-71
        "C5", "C#5", "D5", "D#5", "E5", "F5", "F#5", "G5", "G#5", "A5", "A#5", "B5", // 72-83
        "C6", "C#6", "D6", "D#6", "E6", "F6", "F#6", "G6", "G#6", "A6", "A#6", "B6", // 84-95
        "C7", "C#7", "D7", "D#7", "E7", "F7", "F#7", "G7", "G#7", "A7", "A#7", "B7", // 96-107
        "C8", "C#8", "D8", "D#8", "E8", "F8", "F#8", "G8", "G#8", "A8", "A#8", "B8", // 108-119
        "C9", "C#9", "D9", "D#9", "E9", "F9", "F#9", "G9", // 120-127
    ]
}

/// Convert a MIDI note number to its frequency in Hz.
///
/// Uses the equal-temperament formula: `f = 440 * 2^((midi - 69) / 12)`.
#[inline]
pub fn midi_to_freq(midi: u8) -> f32 {
    440.0_f32 * 2.0_f32.powf((midi as f32 - 69.0) / 12.0)
}

/// Convert a frequency in Hz to the nearest MIDI note number.
///
/// Returns `None` when the frequency is below 8 Hz or above 12 543 Hz
/// (outside the MIDI range).
pub fn freq_to_midi(freq: f32) -> Option<u8> {
    if freq < 8.0 {
        return None;
    }
    let midi_f = 69.0 + 12.0 * (freq / 440.0).log2();
    if !(0.0..=127.0).contains(&midi_f) {
        return None;
    }
    Some(midi_f.round() as u8)
}

/// Return the pitch class (0–11) for a MIDI note.
#[inline]
pub fn pitch_class(midi: u8) -> u8 {
    midi % 12
}

/// Return the octave number for a MIDI note (C4 = octave 4, MIDI 60).
#[inline]
pub fn octave(midi: u8) -> i8 {
    (midi as i8) / 12 - 1
}

/// Return the short note name for a MIDI note, e.g. `"E2"`, `"A#3"`.
///
/// Panics if `midi > 127`.
#[inline]
pub fn note_name(midi: u8) -> &'static str {
    NOTE_NAME_TABLE[midi as usize]
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn a4_is_440hz() {
        assert_relative_eq!(midi_to_freq(69), 440.0, epsilon = 0.01);
    }

    #[test]
    fn e2_open_string() {
        assert_relative_eq!(midi_to_freq(40), 82.41, epsilon = 0.1);
    }

    #[test]
    fn freq_to_midi_roundtrip() {
        for m in MIN_MIDI..=MAX_MIDI {
            let f = midi_to_freq(m);
            let back = freq_to_midi(f).unwrap();
            assert_eq!(back, m, "MIDI round-trip failed for {m}");
        }
    }

    #[test]
    fn note_names_spot_check() {
        assert_eq!(note_name(40), "E2");
        assert_eq!(note_name(45), "A2");
        assert_eq!(note_name(69), "A4");
        assert_eq!(note_name(60), "C4");
    }
}
