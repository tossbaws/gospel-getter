//! The bundled KJV's inline `{…}` markup, and how the app shows it.
//!
//! The KJV's source (thiagobodruk/bible) marks two things with braces, and
//! the database stores them verbatim (see the README's "Data provenance"):
//!
//! - **translator-supplied words**, which printed KJVs set in italics:
//!   "The LORD {is} my shepherd";
//! - **margin notes**: "{comprehended: or, did not admit, or, receive}",
//!   "{green...: Heb. pastures of tender grass}".
//!
//! Nothing here changes what's stored. It only decides how a verse is
//! shown ([`VerseText`]): supplied words as words (in italics where the
//! verse is read), each margin note as a footnote with a letter marker
//! where it stood, and the verse's plain text, for copying and search,
//! with the supplied words and without the notes. The notes export uses
//! the same parts (italics, notes left out).
//!
//! Which is which is where it stands: a span is a margin note if and only
//! if it's in the run of spans that ends the verse (after it, nothing but
//! spans and spaces). Every other span is supplied words.
//!
//! That's safe for the bundled text, as a survey of every brace in it
//! shows, and `every_kjv_verse_keeps_its_words` checks:
//!
//! - Every span in a verse's closing run is a note. All but two say so
//!   with a note marker (a colon, or "or,", "Heb." or "Gr."); the other
//!   two are Luke 10:22's and 17:36's notes on the Greek copies.
//! - No closing span is supplied words: supplied words are always followed
//!   by more of the verse, if only its final punctuation ("{him}.").
//! - Looking like a note isn't what decides it. In seven verses the
//!   source's braces take in the punctuation and words after an italic
//!   word ("in thine eyes, {tarry: for} I have learned", Genesis 30:27);
//!   those spans are followed by more of the verse, so they're supplied
//!   words, kept.
//!
//! A verse whose braces are nested or unbalanced (Micah 7:12, Romans 16:27,
//! 1 Corinthians 16:24, Hebrews 10:34) is shown exactly as stored, braces
//! and all.

use serde::Serialize;

/// The code of the translation with this markup. Only its verses are read
/// for it; any other translation's text is shown exactly as stored.
pub const KJV: &str = "kjv";

/// What a `{…}` span is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Span {
    /// Words the translators supplied: shown in italics.
    Supplied,
    /// A margin note: a footnote.
    MarginNote,
}

/// The byte ranges of `text`'s `{…}` spans (braces included), in order, or
/// `None` if its braces are nested or unbalanced.
fn brace_spans(text: &str) -> Option<Vec<(usize, usize)>> {
    let mut spans = Vec::new();
    let mut open = None;
    for (i, c) in text.char_indices() {
        match (c, open) {
            ('{', None) => open = Some(i),
            ('}', Some(start)) => {
                spans.push((start, i + 1));
                open = None;
            }
            ('{' | '}', _) => return None,
            _ => {}
        }
    }
    open.is_none().then_some(spans)
}

/// Each `{…}` span of `text` with what it is, or `None` if its braces are
/// nested or unbalanced (or a span is empty), so it can only be shown
/// exactly as stored.
pub fn classify(text: &str) -> Option<Vec<((usize, usize), Span)>> {
    let spans = brace_spans(text)?;
    // Where the run of spans that ends the verse begins: after it there's
    // nothing but spans and spaces.
    let mut tail = text.len();
    for &(start, end) in spans.iter().rev() {
        if text[end..tail].trim().is_empty() {
            tail = start;
        } else {
            break;
        }
    }
    spans
        .into_iter()
        .map(|(start, end)| {
            if text[start + 1..end - 1].trim().is_empty() {
                return None;
            }
            // The closing run is notes (see the module docs).
            let kind = if start >= tail {
                Span::MarginNote
            } else {
                Span::Supplied
            };
            Some(((start, end), kind))
        })
        .collect()
}

/// What a [`Part`] of a verse is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PartKind {
    /// The verse's own words.
    Text,
    /// Words the translators supplied, without their braces.
    Supplied,
    /// A margin note, shown as its footnote's marker.
    Note,
}

/// One piece of a verse, in order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Part {
    pub kind: PartKind,
    /// The words, or for a note its text exactly as stored inside the
    /// braces.
    pub text: String,
    /// A note's marker letter ("a", "b", …); `None` for anything else.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// One of a verse's margin notes, as a footnote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Footnote {
    /// Its marker letter, lettered per verse from "a".
    pub label: String,
    /// The note exactly as stored inside the braces, e.g.
    /// "lamp: or, candle".
    pub text: String,
}

/// A verse as the app shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VerseText {
    /// The verse's plain text, for copying, previews and search: supplied
    /// words as plain words, margin notes left out. A verse without the
    /// markup (any other translation, or a KJV verse whose markup isn't
    /// well formed) is its stored text.
    pub text: String,
    /// The verse in order: the non-note parts' texts, joined, are `text`.
    pub parts: Vec<Part>,
    /// The verse's margin notes, in order.
    pub footnotes: Vec<Footnote>,
}

impl VerseText {
    /// A verse of the translation `translation_code`, from its stored text.
    pub fn new(translation_code: &str, stored: &str) -> Self {
        if translation_code == KJV
            && let Some(verse) = marked(stored)
        {
            return verse;
        }
        Self::as_stored(stored)
    }

    /// A verse shown exactly as stored: one text part, no footnotes.
    pub fn as_stored(stored: &str) -> Self {
        Self {
            text: stored.to_string(),
            parts: vec![Part {
                kind: PartKind::Text,
                text: stored.to_string(),
                label: None,
            }],
            footnotes: Vec::new(),
        }
    }
}

/// A KJV verse's markup read into parts, or `None` when it should be shown
/// exactly as stored instead: no braces, braces that aren't well formed, a
/// line break, or nothing left once the notes are out. Each note takes the
/// spaces before it with it, so the marker sits right after the word or
/// punctuation it followed.
pub fn marked(stored: &str) -> Option<VerseText> {
    if stored.contains('\n') {
        return None;
    }
    let spans = classify(stored)?;
    if spans.is_empty() {
        return None;
    }
    let mut parts = Vec::new();
    let mut footnotes = Vec::new();
    let mut pos = 0;
    let mut last_was_note = false;
    for ((start, end), kind) in spans {
        let before = &stored[pos..start];
        let inner = &stored[start + 1..end - 1];
        match kind {
            Span::MarginNote => {
                push_text(&mut parts, before.trim_end());
                let label = marker_label(footnotes.len());
                parts.push(Part {
                    kind: PartKind::Note,
                    text: inner.to_string(),
                    label: Some(label.clone()),
                });
                footnotes.push(Footnote {
                    label,
                    text: inner.to_string(),
                });
            }
            Span::Supplied => {
                push_text(&mut parts, before);
                parts.push(Part {
                    kind: PartKind::Supplied,
                    text: inner.to_string(),
                    label: None,
                });
            }
        }
        last_was_note = kind == Span::MarginNote;
        pos = end;
    }
    let rest = &stored[pos..];
    push_text(
        &mut parts,
        if last_was_note { rest.trim_end() } else { rest },
    );
    let text: String = parts
        .iter()
        .filter(|p| p.kind != PartKind::Note)
        .map(|p| p.text.as_str())
        .collect();
    (!text.trim().is_empty()).then_some(VerseText {
        text,
        parts,
        footnotes,
    })
}

fn push_text(parts: &mut Vec<Part>, text: &str) {
    if !text.is_empty() {
        parts.push(Part {
            kind: PartKind::Text,
            text: text.to_string(),
            label: None,
        });
    }
}

/// The marker for a verse's `index`th note (from 0): "a" to "z", then
/// "aa", "ab", … so there's always one.
pub fn marker_label(index: usize) -> String {
    let mut n = index + 1;
    let mut out = Vec::new();
    while n > 0 {
        n -= 1;
        out.push(char::from(b'a' + (n % 26) as u8));
        n /= 26;
    }
    out.iter().rev().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_support::bundled;

    fn kjv(text: &str) -> VerseText {
        VerseText::new(KJV, text)
    }

    fn text(s: &str) -> Part {
        Part {
            kind: PartKind::Text,
            text: s.to_string(),
            label: None,
        }
    }

    fn supplied(s: &str) -> Part {
        Part {
            kind: PartKind::Supplied,
            text: s.to_string(),
            label: None,
        }
    }

    fn note(label: &str, s: &str) -> Part {
        Part {
            kind: PartKind::Note,
            text: s.to_string(),
            label: Some(label.to_string()),
        }
    }

    #[test]
    fn supplied_words_are_parts_of_their_own() {
        let verse = kjv("The LORD {is} my shepherd; I shall not want.");
        assert_eq!(verse.text, "The LORD is my shepherd; I shall not want.");
        assert_eq!(
            verse.parts,
            [
                text("The LORD "),
                supplied("is"),
                text(" my shepherd; I shall not want.")
            ]
        );
        assert!(verse.footnotes.is_empty());
        // At the start and the end, beside punctuation, and two in a row.
        assert_eq!(
            kjv("{In} that day {it is}, {so}.").parts,
            [
                supplied("In"),
                text(" that day "),
                supplied("it is"),
                text(", "),
                supplied("so"),
                text(".")
            ]
        );
        assert_eq!(
            kjv("but {there was} {none} to save").parts,
            [
                text("but "),
                supplied("there was"),
                text(" "),
                supplied("none"),
                text(" to save")
            ]
        );
        // With nothing at all after it, a span ends the verse, so it's
        // taken for a note; no supplied words in the bundled KJV do (see
        // `every_kjv_verse_keeps_its_words`).
        let verse = kjv("with me {where I am}");
        assert_eq!(verse.text, "with me");
        assert_eq!(verse.parts, [text("with me"), note("a", "where I am")]);
    }

    #[test]
    fn margin_notes_become_lettered_footnotes() {
        // Psalms 119:105.
        let verse = kjv(
            "NUN. Thy word {is} a lamp unto my feet, and a light unto my path. \
                         {lamp: or, candle}",
        );
        assert_eq!(
            verse.text,
            "NUN. Thy word is a lamp unto my feet, and a light unto my path."
        );
        assert_eq!(
            verse.parts,
            [
                text("NUN. Thy word "),
                supplied("is"),
                text(" a lamp unto my feet, and a light unto my path."),
                note("a", "lamp: or, candle"),
            ]
        );
        assert_eq!(
            verse.footnotes,
            [Footnote {
                label: "a".to_string(),
                text: "lamp: or, candle".to_string()
            }]
        );
        // Several notes after supplied words, lettered in order, each
        // exactly as stored; colon-less markers too.
        let verse = kjv(
            "So foolish {was} I, and ignorant: I was {as} a beast before thee. \
                         {ignorant: Heb. I knew not} {before Heb. with}  {c}",
        );
        assert_eq!(
            verse.text,
            "So foolish was I, and ignorant: I was as a beast before thee."
        );
        assert_eq!(
            verse.parts[4..],
            [
                text(" a beast before thee."),
                note("a", "ignorant: Heb. I knew not"),
                note("b", "before Heb. with"),
                note("c", "c"),
            ]
        );
        let labels: Vec<_> = verse.footnotes.iter().map(|f| f.label.as_str()).collect();
        assert_eq!(labels, ["a", "b", "c"]);
        // Letters start again at "a" in every verse.
        assert_eq!(kjv("it. {sickle; or, scythe}").footnotes[0].label, "a");
    }

    #[test]
    fn markers_go_past_z() {
        let labels: Vec<_> = [0, 1, 25, 26, 27, 51, 52, 701, 702]
            .into_iter()
            .map(marker_label)
            .collect();
        assert_eq!(labels, ["a", "b", "z", "aa", "ab", "az", "ba", "zz", "aaa"]);
    }

    #[test]
    fn a_span_followed_by_more_of_the_verse_is_kept_however_it_looks() {
        // Genesis 30:27: the braces take in the colon and "for".
        let verse = kjv("in thine eyes, {tarry: for} I have learned by experience");
        assert_eq!(
            verse.text,
            "in thine eyes, tarry: for I have learned by experience"
        );
        assert_eq!(verse.parts[1], supplied("tarry: for"));
        assert!(verse.footnotes.is_empty());
        // Followed only by the verse's final punctuation: still the verse's.
        let verse = kjv("will reveal {him: or, Heb. it}.");
        assert_eq!(verse.text, "will reveal him: or, Heb. it.");
        assert!(verse.footnotes.is_empty());
    }

    #[test]
    fn the_notes_in_luke_without_a_marker_are_footnotes_too() {
        let verse = kjv(
            "All things are delivered to me of my Father: and no man knoweth who the Son is, but \
             the Father; and who the Father is, but the Son, and {he} to whom the Son will \
             reveal {him}. {many ancient copies add these words at the beginning of verse, and \
             turning to his disciples, he said}",
        );
        assert!(
            verse
                .text
                .ends_with("and he to whom the Son will reveal him.")
        );
        assert_eq!(
            verse.footnotes[0].text,
            "many ancient copies add these words at the beginning of verse, and turning to his \
             disciples, he said"
        );
        let verse = kjv(
            "Two {men} shall be in the field; the one shall be taken, and the other \
                         left. {this verse is not found in most of the Greek copies}",
        );
        assert_eq!(
            verse.text,
            "Two men shall be in the field; the one shall be taken, and the other left."
        );
        assert_eq!(
            verse.parts.last(),
            Some(&note(
                "a",
                "this verse is not found in most of the Greek copies"
            ))
        );
    }

    #[test]
    fn malformed_markup_is_shown_exactly_as_stored() {
        // Hebrews 10:34, with the source's unmatched brace.
        let hebrews = "For ye had compassion of me in my bonds, and took joyfully the spoiling of \
                       your goods, knowing in yourselves that ye have in heaven a better and an \
                       enduring substance. {in yourselves...: or, that ye have in or, for} \
                       yourselves}";
        assert_eq!(classify(hebrews), None);
        assert_eq!(kjv(hebrews), VerseText::as_stored(hebrews));
        // Nested (Micah 7:12), opened and never closed, and empty.
        for stored in ["a {{and from} b: or, c}", "a {b", "a {} b", "a } b"] {
            assert_eq!(kjv(stored), VerseText::as_stored(stored), "{stored}");
        }
        // A line break, or nothing left without the notes.
        assert_eq!(kjv("a {b} c\nd"), VerseText::as_stored("a {b} c\nd"));
        assert_eq!(
            kjv(" {only a note}"),
            VerseText::as_stored(" {only a note}")
        );
        // No braces at all: nothing to do.
        let plain = "In the beginning God created";
        assert_eq!(kjv(plain), VerseText::as_stored(plain));
    }

    #[test]
    fn other_translations_are_shown_exactly_as_stored() {
        let stored = "the {Word} was. {a note}";
        assert_eq!(VerseText::new("web", stored), VerseText::as_stored(stored));
        assert_eq!(VerseText::new("KJV", stored), VerseText::as_stored(stored));
    }

    /// Every verse of the bundled KJV: only malformed verses keep braces,
    /// none is emptied, the parts make up the plain text in order, the
    /// footnotes are the note parts lettered from "a", and what's shown is
    /// the verse's own words, in order, less exactly the margin notes'
    /// words. Every WEB verse is untouched.
    #[test]
    fn every_kjv_verse_keeps_its_words() {
        let mut malformed = Vec::new();
        let (mut changed, mut notes, mut supplied) = (0, 0, 0);
        let mut unmarked = Vec::new();
        let mut colons = Vec::new();
        for book in bundled("kjv") {
            for (c, chapter) in book.chapters.iter().enumerate() {
                for (v, stored) in chapter.iter().enumerate() {
                    let at = format!("{} {}:{}", book.name, c + 1, v + 1);
                    let verse = kjv(stored);
                    if !stored.contains(['{', '}']) {
                        assert_eq!(verse, VerseText::as_stored(stored));
                        continue;
                    }
                    let Some(spans) = classify(stored) else {
                        assert_eq!(verse, VerseText::as_stored(stored));
                        malformed.push(at);
                        continue;
                    };
                    changed += 1;
                    assert!(!verse.text.contains(['{', '}']), "{at}");
                    assert!(!verse.text.trim().is_empty(), "{at}");
                    // The parts, less the notes, are the text.
                    let joined: String = verse
                        .parts
                        .iter()
                        .filter(|p| p.kind != PartKind::Note)
                        .map(|p| p.text.as_str())
                        .collect();
                    assert_eq!(joined, verse.text, "{at}");
                    assert!(verse.parts.iter().all(|p| !p.text.is_empty()), "{at}");
                    // The footnotes are the note parts, lettered a, b, c….
                    let from_parts: Vec<_> = verse
                        .parts
                        .iter()
                        .filter(|p| p.kind == PartKind::Note)
                        .map(|p| (p.label.clone().unwrap(), p.text.clone()))
                        .collect();
                    let footnotes: Vec<_> = verse
                        .footnotes
                        .iter()
                        .map(|f| (f.label.clone(), f.text.clone()))
                        .collect();
                    assert_eq!(from_parts, footnotes, "{at}");
                    for (i, f) in verse.footnotes.iter().enumerate() {
                        assert_eq!(f.label, marker_label(i), "{at}");
                    }
                    assert!(
                        verse
                            .parts
                            .iter()
                            .all(|p| (p.kind == PartKind::Note) == p.label.is_some()),
                        "{at}"
                    );
                    // The original's words, less the notes', exactly.
                    let mut expected = String::new();
                    let mut pos = 0;
                    let mut note_texts = Vec::new();
                    for ((start, end), kind) in &spans {
                        expected.push_str(&stored[pos..*start]);
                        let inner = &stored[start + 1..end - 1];
                        match kind {
                            Span::Supplied => {
                                supplied += 1;
                                expected.push_str(inner);
                                if inner.contains(':') {
                                    colons.push(at.clone());
                                }
                            }
                            Span::MarginNote => {
                                notes += 1;
                                note_texts.push(inner.to_string());
                                // Why the closing run is safe to take out:
                                // each of its spans looks like a note, but
                                // two.
                                let looks_like_a_note = inner.contains(':')
                                    || inner
                                        .split(|c: char| c.is_whitespace() || c == ';')
                                        .any(|w| matches!(w, "or," | "Heb." | "Gr."));
                                if !looks_like_a_note {
                                    unmarked.push(at.clone());
                                }
                            }
                        }
                        pos = *end;
                    }
                    expected.push_str(&stored[pos..]);
                    let words =
                        |s: &str| s.split_whitespace().map(str::to_string).collect::<Vec<_>>();
                    assert_eq!(words(&verse.text), words(&expected), "{at}");
                    // Each footnote is its note exactly as stored.
                    let texts: Vec<_> = verse.footnotes.iter().map(|f| f.text.clone()).collect();
                    assert_eq!(texts, note_texts, "{at}");
                    // No space before a marker or at the end, and no
                    // double spaces, where a note was taken out.
                    assert!(
                        !verse.text.ends_with(' ') && !verse.text.contains("  "),
                        "{at}: {:?}",
                        verse.text
                    );
                }
            }
        }
        assert_eq!(
            malformed,
            [
                "Micah 7:12",
                "Romans 16:27",
                "1 Corinthians 16:24",
                "Hebrews 10:34"
            ]
        );
        assert_eq!(unmarked, ["Luke 10:22", "Luke 17:36"]);
        assert_eq!(colons.len(), 7, "{colons:?}");
        assert!(colons.contains(&"Genesis 30:27".to_string()));
        assert_eq!((changed, notes, supplied), (17_362, 7_862, 21_521));

        for book in bundled("web") {
            for chapter in &book.chapters {
                for stored in chapter {
                    assert_eq!(VerseText::new("web", stored), VerseText::as_stored(stored));
                }
            }
        }
    }
}
