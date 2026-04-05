//! Core public types returned by the guitar pitch detector.

/// Pitch modulation technique detected on a note.
///
/// The detector tracks the per-note frequency history and classifies one of:
/// * [`PitchModulation::Bend`]    — sustained rise or fall from the nominal pitch.
/// * [`PitchModulation::Vibrato`] — rapid oscillation around the nominal pitch.
/// * [`PitchModulation::Stable`]  — no significant modulation.
///
/// Use [`crate::modulation::ModulationAnalyzer`] to populate this field after
/// each [`crate::GuitarPitchDetector::process`] call.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum PitchModulation {
    /// No significant pitch deviation detected.
    #[default]
    Stable,
    /// String bend: pitch is deviating from the nominal MIDI-note frequency.
    ///
    /// Positive `cents` = upward bend (pitch is sharp); negative = downward.
    /// 100 cents equals one semitone.
    Bend {
        /// Signed deviation from the nominal note frequency in cents.
        cents: f32,
    },
    /// Vibrato: pitch oscillates periodically around the nominal note.
    Vibrato {
        /// Half the peak-to-peak oscillation depth in cents (always ≥ 0).
        depth_cents: f32,
        /// Estimated oscillation rate in Hz.
        rate_hz: f32,
    },
}

/// A single musical note detected in the audio signal.
#[derive(Debug, Clone, PartialEq)]
pub struct DetectedNote {
    /// Detected frequency in Hz (sub-semitone accurate via parabolic
    /// interpolation of resonator energies).
    pub frequency: f32,
    /// MIDI note number (40 = E2, 69 = A4, …).
    pub midi_note: u8,
    /// Pitch class 0–11 (0 = C, 1 = C#, … 11 = B).
    pub semitone: u8,
    /// Octave number (E2 → 2, A4 → 4, …).
    pub octave: i8,
    /// Short note name, e.g. "E2", "A#3".
    pub name: &'static str,
    /// Normalized confidence 0.0–1.0 (fraction of maximum resonator energy).
    pub confidence: f32,
    /// Pitch modulation technique applied to this note.
    ///
    /// Populated by [`crate::modulation::ModulationAnalyzer::update`]; defaults
    /// to [`PitchModulation::Stable`] when the analyser is not used.
    pub modulation: PitchModulation,
}

/// Quality (type) of a detected chord.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChordQuality {
    /// Root + maj 3rd + 5th.
    Major,
    /// Root + min 3rd + 5th.
    Minor,
    /// Root + maj 3rd + 5th + min 7th.
    Dominant7,
    /// Root + maj 3rd + 5th + maj 7th.
    Major7,
    /// Root + min 3rd + 5th + min 7th.
    Minor7,
    /// Root + min 3rd + dim 5th.
    Diminished,
    /// Root + maj 3rd + aug 5th.
    Augmented,
    /// Root + maj 2nd + 5th.
    Sus2,
    /// Root + 4th + 5th.
    Sus4,
    /// Root + 5th only (power chord, common in rock/metal).
    Power,
}

impl ChordQuality {
    /// Return the common suffix used in chord names (e.g. "m", "maj7", "7").
    pub fn suffix(&self) -> &'static str {
        match self {
            ChordQuality::Major => "",
            ChordQuality::Minor => "m",
            ChordQuality::Dominant7 => "7",
            ChordQuality::Major7 => "maj7",
            ChordQuality::Minor7 => "m7",
            ChordQuality::Diminished => "dim",
            ChordQuality::Augmented => "aug",
            ChordQuality::Sus2 => "sus2",
            ChordQuality::Sus4 => "sus4",
            ChordQuality::Power => "5",
        }
    }
}

/// A chord detected from a set of simultaneously active notes.
#[derive(Debug, Clone, PartialEq)]
pub struct DetectedChord {
    /// Human-readable chord name, e.g. "Am", "Gmaj7", "E5".
    pub name: String,
    /// Root pitch class 0–11 (0 = C, …).
    pub root: u8,
    /// Chord quality (Major, Minor, …).
    pub quality: ChordQuality,
    /// Normalized confidence 0.0–1.0 based on how well the detected notes
    /// match the chord pattern.
    pub confidence: f32,
}

/// The result returned by [`GuitarPitchDetector::process`].
#[derive(Debug, Clone, Default)]
pub struct DetectionResult {
    /// All notes detected in this audio frame (up to 6 for a guitar).
    pub notes: Vec<DetectedNote>,
    /// Chord inferred from the detected notes, if one was recognised.
    pub chord: Option<DetectedChord>,
}
