//! A translation's footnotes kept as USFM (the BSB's), and how the app
//! shows them.
//!
//! Each footnote is stored exactly as its source has it, `\f` to `\f*`
//! (see the README's "Data provenance"), with where it stands in its
//! verse: a number of characters into the verse's stored text, which has
//! no trace of it. Nothing here changes what's stored. It only reads a
//! note's markers to show it as printed:
//!
//! - `\fr 3:16 ` is the verse the note is on. Every note in the bundled
//!   BSB is on its own verse, which the app shows anyway, so it isn't
//!   repeated.
//! - `\ft` is the note's text; `\fqa` an alternative reading, in italics.
//! - `\fv 21\fv*` is a verse number inside the note, set small and raised
//!   like the reader's ("BYZ and TR include ²¹But this kind …").
//! - `\ref John 1:1|JHN 1:1\ref*` is a reference: "John 1:1" is shown,
//!   the part after `|` (its address for software) isn't.
//!
//! A note with anything else in it (a marker the bundled BSB doesn't use)
//! is shown exactly as stored, markers and all.

use serde::Serialize;

use crate::domain::kjv_markup::{Footnote, Part, PartKind, VerseText, marker_label};

/// A footnote as stored: its USFM, and how many characters into its
/// verse's text it stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredFootnote {
    pub position: usize,
    pub usfm: String,
}

/// What a [`Segment`] of a footnote is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SegmentKind {
    /// The note's own words (`\ft`).
    Text,
    /// An alternative reading (`\fqa`), in italics.
    Alt,
    /// A verse number in the note (`\fv`).
    Verse,
}

/// One run of a footnote, in order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Segment {
    pub kind: SegmentKind,
    pub text: String,
}

/// The runs of a stored note, or `None` if it isn't one this reads (it's
/// then shown as stored).
pub fn segments(usfm: &str) -> Option<Vec<Segment>> {
    let body = usfm.strip_prefix("\\f + ")?.strip_suffix("\\f*")?;
    let mut out: Vec<Segment> = Vec::new();
    // The style in force, and the one a `\fv*` goes back to.
    let mut style: Option<SegmentKind> = None;
    let mut in_verse: Option<SegmentKind> = None;
    let mut in_ref = false;
    let mut rest = body;
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix('\\') {
            // A marker's name, then a `*` if it closes something.
            let mut len = after
                .find(|c: char| !c.is_ascii_alphanumeric())
                .unwrap_or(after.len());
            if after[len..].starts_with('*') {
                len += 1;
            }
            let name = &after[..len];
            let mut tail = &after[len..];
            if !name.ends_with('*') {
                tail = tail.strip_prefix(' ').unwrap_or(tail);
            }
            match name {
                "fr" => style = None,
                "ft" => style = Some(SegmentKind::Text),
                "fqa" => style = Some(SegmentKind::Alt),
                "fv" => {
                    in_verse = style;
                    style = Some(SegmentKind::Verse);
                }
                "fv*" => style = in_verse.take(),
                "ref" => in_ref = true,
                "ref*" => in_ref = false,
                _ => return None,
            }
            rest = tail;
            continue;
        }
        let end = rest.find('\\').unwrap_or(rest.len());
        let mut text = &rest[..end];
        rest = &rest[end..];
        if in_ref {
            // "John 1:1|JHN 1:1": what's shown, then its address.
            text = text.split_once('|').map_or(text, |(shown, _)| shown);
        }
        let Some(kind) = style else { continue };
        if text.is_empty() {
            continue;
        }
        match out.last_mut() {
            Some(last) if last.kind == kind => last.text.push_str(text),
            _ => out.push(Segment {
                kind,
                text: text.to_string(),
            }),
        }
    }
    // The note ends where its words do.
    if let Some(last) = out.last_mut() {
        last.text.truncate(last.text.trim_end().len());
    }
    out.retain(|s| !s.text.is_empty());
    (!out.is_empty()).then_some(out)
}

impl VerseText {
    /// A verse with footnotes kept apart from its text: the text as
    /// stored, cut where each note stands, each note a lettered footnote.
    /// A note whose place isn't in the text (it can't be, for the bundled
    /// BSB; a test checks) goes at the end.
    pub fn with_footnotes(stored: &str, notes: &[StoredFootnote]) -> Self {
        if notes.is_empty() {
            return Self::as_stored(stored);
        }
        let mut parts = Vec::new();
        let mut footnotes = Vec::new();
        let mut taken = 0; // bytes of `stored` already in parts
        for (index, note) in notes.iter().enumerate() {
            let at = stored
                .char_indices()
                .nth(note.position)
                .map_or(stored.len(), |(i, _)| i)
                .max(taken);
            if at > taken {
                parts.push(Part {
                    kind: PartKind::Text,
                    text: stored[taken..at].to_string(),
                    label: None,
                });
            }
            taken = at;
            let label = marker_label(index);
            let segments = segments(&note.usfm);
            let shown = match &segments {
                Some(s) => s.iter().map(|s| s.text.as_str()).collect(),
                None => note.usfm.clone(),
            };
            parts.push(Part {
                kind: PartKind::Note,
                text: shown.clone(),
                label: Some(label.clone()),
            });
            footnotes.push(Footnote {
                label,
                lemma: None,
                rest: shown.clone(),
                text: shown,
                segments,
            });
        }
        if taken < stored.len() {
            parts.push(Part {
                kind: PartKind::Text,
                text: stored[taken..].to_string(),
                label: None,
            });
        }
        Self {
            text: stored.to_string(),
            parts,
            footnotes,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(kind: SegmentKind, text: &str) -> Segment {
        Segment {
            kind,
            text: text.to_string(),
        }
    }

    #[test]
    fn a_note_is_its_text_and_its_alternative_readings() {
        assert_eq!(
            segments(
                "\\f + \\fr 1:6 \\ft Or \\fqa a canopy \\ft or \\fqa a firmament \\ft or \\fqa a vault\\ft ; also in verses 7, 8, 14, 15, 17, and 20\\f*"
            ),
            Some(vec![
                seg(SegmentKind::Text, "Or "),
                seg(SegmentKind::Alt, "a canopy "),
                seg(SegmentKind::Text, "or "),
                seg(SegmentKind::Alt, "a firmament "),
                seg(SegmentKind::Text, "or "),
                seg(SegmentKind::Alt, "a vault"),
                seg(
                    SegmentKind::Text,
                    "; also in verses 7, 8, 14, 15, 17, and 20"
                ),
            ])
        );
    }

    #[test]
    fn a_reference_shows_its_words_and_a_verse_number_is_its_own_run() {
        assert_eq!(
            segments(
                "\\f + \\fr 17:20 \\ft BYZ and TR include \\fqa \\fv 21\\fv*But this kind does not come out except by prayer and fasting\\ft ; see \\ref Mark 9:29|MRK 9:29\\ref*.\\f*"
            ),
            Some(vec![
                seg(SegmentKind::Text, "BYZ and TR include "),
                seg(SegmentKind::Verse, "21"),
                seg(
                    SegmentKind::Alt,
                    "But this kind does not come out except by prayer and fasting"
                ),
                seg(SegmentKind::Text, "; see Mark 9:29."),
            ])
        );
    }

    #[test]
    fn a_note_this_cannot_read_is_shown_as_stored() {
        assert_eq!(segments("\\f + \\fr 1:1 \\fk word\\f*"), None);
        assert_eq!(segments("not a note"), None);
        let verse = VerseText::with_footnotes(
            "In the beginning.",
            &[StoredFootnote {
                position: 16,
                usfm: "\\f + \\fk odd\\f*".to_string(),
            }],
        );
        assert_eq!(verse.footnotes[0].text, "\\f + \\fk odd\\f*");
        assert_eq!(verse.footnotes[0].segments, None);
    }

    #[test]
    fn notes_cut_the_text_where_they_stand_and_the_text_is_unchanged() {
        let stored = "And God said, “Let there be light,” and there was light.";
        let verse = VerseText::with_footnotes(
            stored,
            &[StoredFootnote {
                position: 35,
                usfm: "\\f + \\fr 1:3 \\ft Cited in \\ref 2 Corinthians 4:6|2CO 4:6\\ref*\\f*"
                    .to_string(),
            }],
        );
        assert_eq!(verse.text, stored);
        let kinds: Vec<_> = verse
            .parts
            .iter()
            .map(|p| (p.kind, p.text.as_str()))
            .collect();
        assert_eq!(
            kinds,
            [
                (PartKind::Text, "And God said, “Let there be light,”"),
                (PartKind::Note, "Cited in 2 Corinthians 4:6"),
                (PartKind::Text, " and there was light."),
            ]
        );
        assert_eq!(verse.footnotes[0].label, "a");
        assert_eq!(verse.footnotes[0].rest, "Cited in 2 Corinthians 4:6");
    }
}
