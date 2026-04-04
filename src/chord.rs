//! Chord detection from a set of simultaneously active pitch classes.
//!
//! The algorithm works in three steps:
//! 1. Collect the pitch classes (0–11) from all detected notes.
//! 2. For each of the 12 possible roots, check whether the pitch-class set
//!    matches a known chord pattern.
//! 3. Return the best match (highest coverage), or `None` if no pattern
//!    reaches the minimum threshold.

use crate::notes::NOTE_NAMES_12;
use crate::types::{ChordQuality, DetectedChord};

/// Interval sets for each recognised chord quality, expressed as semitones
/// above the root.
///
/// Order matters: more specific patterns should appear before less specific
/// ones so that e.g. `Dominant7` is preferred over bare `Major` when all
/// four notes are present.
const CHORD_PATTERNS: &[(&[u8], ChordQuality)] = &[
    (&[0, 4, 7, 11], ChordQuality::Major7),
    (&[0, 3, 7, 10], ChordQuality::Minor7),
    (&[0, 4, 7, 10], ChordQuality::Dominant7),
    (&[0, 4, 7], ChordQuality::Major),
    (&[0, 3, 7], ChordQuality::Minor),
    (&[0, 3, 6], ChordQuality::Diminished),
    (&[0, 4, 8], ChordQuality::Augmented),
    (&[0, 2, 7], ChordQuality::Sus2),
    (&[0, 5, 7], ChordQuality::Sus4),
    (&[0, 7], ChordQuality::Power),
];

/// Minimum fraction of chord tones that must be present for a chord to be
/// reported.  A value of `1.0` requires a perfect match.
const MIN_MATCH_RATIO: f32 = 1.0;

/// Try to recognise a chord from `pitch_classes` (each 0–11).
///
/// Returns the best-matching [`DetectedChord`], or `None` when no known
/// pattern matches.
pub fn detect_chord(pitch_classes: &[u8]) -> Option<DetectedChord> {
    if pitch_classes.len() < 2 {
        return None;
    }

    let mut best: Option<DetectedChord> = None;
    let mut best_score = 0.0_f32;

    for root in 0u8..12 {
        for (pattern, quality) in CHORD_PATTERNS {
            // Translate pattern so that root maps to 0.
            let chord_pcs: Vec<u8> = pattern.iter().map(|&i| (root + i) % 12).collect();

            // Count how many chord tones are present in the detected set.
            let matched = chord_pcs
                .iter()
                .filter(|pc| pitch_classes.contains(pc))
                .count();

            let match_ratio = matched as f32 / chord_pcs.len() as f32;

            // Only accept complete matches (all chord tones present).
            if match_ratio < MIN_MATCH_RATIO {
                continue;
            }

            // Also ensure there are no strongly "foreign" notes — limit
            // extra notes to at most one (allows passing tones).
            let extra = pitch_classes
                .iter()
                .filter(|pc| !chord_pcs.contains(pc))
                .count();
            if extra > 1 {
                continue;
            }

            // Score = match_ratio − small penalty for extra notes.
            let score = match_ratio - extra as f32 * 0.1;
            if score > best_score {
                best_score = score;
                let root_name = NOTE_NAMES_12[root as usize];
                best = Some(DetectedChord {
                    name: format!("{}{}", root_name, quality.suffix()),
                    root,
                    quality: quality.clone(),
                    confidence: match_ratio,
                });
            }
        }
    }

    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ChordQuality;

    fn chord(pcs: &[u8]) -> Option<DetectedChord> {
        detect_chord(pcs)
    }

    #[test]
    fn a_minor_chord() {
        // A minor = A(9) + C(0) + E(4)
        let c = chord(&[9, 0, 4]).unwrap();
        assert_eq!(c.root, 9);
        assert_eq!(c.quality, ChordQuality::Minor);
        assert_eq!(c.name, "Am");
    }

    #[test]
    fn g_major_chord() {
        // G major = G(7) + B(11) + D(2)
        let c = chord(&[7, 11, 2]).unwrap();
        assert_eq!(c.root, 7);
        assert_eq!(c.quality, ChordQuality::Major);
        assert_eq!(c.name, "G");
    }

    #[test]
    fn e_power_chord() {
        // E5 = E(4) + B(11)
        let c = chord(&[4, 11]).unwrap();
        assert_eq!(c.root, 4);
        assert_eq!(c.quality, ChordQuality::Power);
        assert_eq!(c.name, "E5");
    }

    #[test]
    fn dominant_seven() {
        // A7 = A(9) + C#(1) + E(4) + G(7)
        let c = chord(&[9, 1, 4, 7]).unwrap();
        assert_eq!(c.root, 9);
        assert_eq!(c.quality, ChordQuality::Dominant7);
        assert_eq!(c.name, "A7");
    }

    #[test]
    fn no_match_for_random_notes() {
        // C + D + F# — no root+3rd+5th, root+4th+5th, or root+5th pair exists
        // among these three pitch classes, so no standard chord matches.
        assert!(chord(&[0, 2, 6]).is_none());
    }

    #[test]
    fn single_note_returns_none() {
        assert!(chord(&[4]).is_none());
    }

    #[test]
    fn sus4_chord() {
        // A + D + E = pitch classes [9, 2, 4].
        // This note set is enharmonically Asus4 (A+D+E) and Dsus2 (D+E+A) —
        // both are valid interpretations, so accept either.
        let c = chord(&[9, 2, 4]).unwrap();
        assert!(
            matches!(c.quality, ChordQuality::Sus4 | ChordQuality::Sus2),
            "Expected Sus4 or Sus2, got {:?}",
            c.quality
        );
    }
}
