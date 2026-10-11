//! The bundled KJV's inline `{…}` markup, as the notes export shows it.
//!
//! The KJV's source (thiagobodruk/bible) marks two things with braces, and
//! stores them verbatim (see the README's "Data provenance"):
//!
//! - **translator-supplied words**, which printed KJVs set in italics:
//!   "The LORD {is} my shepherd";
//! - **margin notes**: "{comprehended: or, did not admit, or, receive}",
//!   "{green...: Heb. pastures of tender grass}".
//!
//! The export sets supplied words in italics and leaves margin notes out.
//! Only the export does this: the reading view, Copy, search, the database
//! and the JSON backup keep the text exactly as stored.
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
//! 1 Corinthians 16:24, Hebrews 10:34) is quoted exactly as stored.

use super::escape_inline;

/// What a `{…}` span is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Span {
    /// Words the translators supplied: shown in italics.
    Supplied,
    /// A margin note: left out.
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
/// nested or unbalanced (or a span is empty), so it can only be quoted
/// exactly.
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

/// A KJV verse as escaped Markdown, with supplied words in italics and
/// margin notes (and the space before each) left out, or `None` when it
/// should be quoted exactly as stored instead: no braces, braces that
/// aren't well formed, a line break, or nothing left once notes are out.
pub fn to_markdown(text: &str) -> Option<String> {
    if text.contains('\n') {
        return None;
    }
    let spans = classify(text)?;
    if spans.is_empty() {
        return None;
    }
    let mut out = String::with_capacity(text.len());
    let mut pos = 0;
    let mut last_was_note = false;
    for ((start, end), kind) in spans {
        let before = &text[pos..start];
        match kind {
            Span::MarginNote => out.push_str(&escape_inline(before.trim_end())),
            Span::Supplied => {
                out.push_str(&escape_inline(before));
                // Escaped first, so the words can't form other Markdown;
                // the asterisks are the only markup added.
                out.push('*');
                out.push_str(&escape_inline(&text[start + 1..end - 1]));
                out.push('*');
            }
        }
        last_was_note = kind == Span::MarginNote;
        pos = end;
    }
    let rest = &text[pos..];
    out.push_str(&escape_inline(if last_was_note {
        rest.trim_end()
    } else {
        rest
    }));
    (!out.trim().is_empty()).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_support::bundled;

    fn md(text: &str) -> Option<String> {
        to_markdown(text)
    }

    #[test]
    fn supplied_words_are_italic_and_escaped() {
        assert_eq!(
            md("The LORD {is} my shepherd; I shall not want.").as_deref(),
            Some("The LORD *is* my shepherd; I shall not want.")
        );
        // Beside punctuation, at the start and the end, and two in a row.
        assert_eq!(
            md("{In} that day {it is}, {so}.").as_deref(),
            Some("*In* that day *it is*, *so*.")
        );
        assert_eq!(
            md("whom thou hast given me, {be} with me {where I am}.").as_deref(),
            Some("whom thou hast given me, *be* with me *where I am*.")
        );
        // With nothing at all after it, a span ends the verse, so it's
        // taken for a note; no supplied words in the bundled KJV do (see
        // `every_kjv_verse_keeps_its_words`).
        assert_eq!(md("with me {where I am}").as_deref(), Some("with me"));
        assert_eq!(
            md("but {there was} {none} to save").as_deref(),
            Some("but *there was* *none* to save")
        );
        // The words are escaped before the asterisks go on.
        assert_eq!(md("a {*b_ [c]} d").as_deref(), Some(r"a *\*b\_ \[c\]* d"));
        // Semicolons inside supplied words are just words.
        assert_eq!(
            md("vow, {it; and} her husband").as_deref(),
            Some("vow, *it; and* her husband")
        );
    }

    #[test]
    fn margin_notes_are_left_out_with_their_space() {
        // John 1:5.
        assert_eq!(
            md(
                "And the light shineth in darkness; and the darkness comprehended it not. \
                {comprehended: or, did not admit, or, receive}"
            )
            .as_deref(),
            Some("And the light shineth in darkness; and the darkness comprehended it not.")
        );
        // Several notes, after supplied words; colon-less markers.
        assert_eq!(
            md(
                "So foolish {was} I, and ignorant: I was {as} a beast before thee. \
                {ignorant: Heb. I knew not} {before Heb. with}"
            )
            .as_deref(),
            Some("So foolish *was* I, and ignorant: I was *as* a beast before thee.")
        );
        assert_eq!(
            md("and feed in the strength. {feed or, rule}").as_deref(),
            Some("and feed in the strength.")
        );
        assert_eq!(md("it. {sickle; or, scythe}").as_deref(), Some("it."));
    }

    #[test]
    fn a_span_followed_by_more_of_the_verse_is_kept_however_it_looks() {
        // Genesis 30:27: the braces take in the colon and "for".
        assert_eq!(
            md("in thine eyes, {tarry: for} I have learned by experience").as_deref(),
            Some("in thine eyes, *tarry: for* I have learned by experience")
        );
        // Followed only by the verse's final punctuation: still the verse's.
        assert_eq!(
            md("will reveal {him: or, Heb. it}.").as_deref(),
            Some("will reveal *him: or, Heb. it*.")
        );
    }

    #[test]
    fn the_notes_in_luke_without_a_marker_are_left_out_too() {
        assert_eq!(
            md(
                "All things are delivered to me of my Father: and no man knoweth who the Son \
                is, but the Father; and who the Father is, but the Son, and {he} to whom the \
                Son will reveal {him}. {many ancient copies add these words at the beginning \
                of verse, and turning to his disciples, he said}"
            )
            .as_deref(),
            Some(
                "All things are delivered to me of my Father: and no man knoweth who the Son \
                 is, but the Father; and who the Father is, but the Son, and *he* to whom the \
                 Son will reveal *him*."
            )
        );
        assert_eq!(
            md(
                "Two {men} shall be in the field; the one shall be taken, and the other left. \
                {this verse is not found in most of the Greek copies}"
            )
            .as_deref(),
            Some("Two *men* shall be in the field; the one shall be taken, and the other left.")
        );
    }

    #[test]
    fn malformed_markup_is_quoted_exactly() {
        // Hebrews 10:34, with the source's unmatched brace.
        let hebrews = "For ye had compassion of me in my bonds, and took joyfully the spoiling of \
                       your goods, knowing in yourselves that ye have in heaven a better and an \
                       enduring substance. {in yourselves...: or, that ye have in or, for} \
                       yourselves}";
        assert_eq!(md(hebrews), None);
        assert_eq!(classify(hebrews), None);
        // Nested (Micah 7:12), opened and never closed, and empty.
        assert_eq!(md("a {{and from} b: or, c}"), None);
        assert_eq!(md("a {b"), None);
        assert_eq!(md("a {} b"), None);
        // No braces at all: nothing to do.
        assert_eq!(md("In the beginning God created"), None);
    }

    /// Markdown back to the text it shows: escapes resolved, italics'
    /// asterisks gone.
    fn shown(markdown: &str) -> String {
        let mut out = String::new();
        let mut chars = markdown.chars();
        while let Some(c) = chars.next() {
            match c {
                '\\' => out.extend(chars.next()),
                '*' => {}
                c => out.push(c),
            }
        }
        out
    }

    /// Every verse of the bundled KJV: only malformed verses keep braces,
    /// none is emptied, and what's shown is the verse's own words, in
    /// order, less exactly the margin notes' words.
    #[test]
    fn every_kjv_verse_keeps_its_words() {
        let mut malformed = Vec::new();
        let (mut changed, mut notes, mut supplied) = (0, 0, 0);
        let mut unmarked = Vec::new();
        for book in bundled("kjv") {
            for (c, chapter) in book.chapters.iter().enumerate() {
                for (v, text) in chapter.iter().enumerate() {
                    if !text.contains(['{', '}']) {
                        assert_eq!(md(text), None);
                        continue;
                    }
                    let Some(spans) = classify(text) else {
                        assert_eq!(md(text), None);
                        malformed.push(format!("{} {}:{}", book.name, c + 1, v + 1));
                        continue;
                    };
                    let out = md(text).expect("well-formed markup is rewritten");
                    changed += 1;
                    let shown = shown(&out);
                    assert!(!shown.contains(['{', '}']), "{shown}");
                    assert!(!shown.trim().is_empty());
                    // The original's words, less the notes', exactly.
                    let mut expected = String::new();
                    let mut pos = 0;
                    for ((start, end), kind) in &spans {
                        expected.push_str(&text[pos..*start]);
                        match kind {
                            Span::Supplied => {
                                supplied += 1;
                                expected.push_str(&text[start + 1..end - 1]);
                            }
                            Span::MarginNote => {
                                notes += 1;
                                // Why the closing run is safe to drop: each
                                // of its spans looks like a note, but two.
                                let inner = &text[start + 1..end - 1];
                                let looks_like_a_note = inner.contains(':')
                                    || inner
                                        .split(|c: char| c.is_whitespace() || c == ';')
                                        .any(|w| matches!(w, "or," | "Heb." | "Gr."));
                                if !looks_like_a_note {
                                    unmarked.push(format!("{} {}:{}", book.name, c + 1, v + 1));
                                }
                            }
                        }
                        pos = *end;
                    }
                    expected.push_str(&text[pos..]);
                    let words =
                        |s: &str| s.split_whitespace().map(str::to_string).collect::<Vec<_>>();
                    assert_eq!(words(&shown), words(&expected), "{}", book.name);
                    // And so a subsequence of the original's.
                    let original = words(&text.replace(['{', '}'], ""));
                    let mut rest = original.iter();
                    assert!(
                        words(&shown).iter().all(|w| rest.any(|o| o == w)),
                        "{shown}"
                    );
                    // No space before punctuation or at the end, and no
                    // double spaces, where a note was taken out.
                    assert!(!shown.ends_with(' ') && !shown.contains("  "), "{shown:?}");
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
        assert_eq!((changed, notes, supplied), (17_362, 7_862, 21_521));
    }
}
