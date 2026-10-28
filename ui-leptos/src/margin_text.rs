//! The margin notes' rules and markup, free of the DOM (`margin` puts them
//! on the page): in reading mode, the reader's notes on the chapter sit in
//! the open space to the right of the text, each beside its first verse,
//! as in a printed Bible's margin. Which verse a note sits beside, how
//! notes that would overlap stack, whether there's room for the margin, and
//! the markup for the margin and for the markers that take its place in a
//! window too narrow for it. Notes are the reader's own words, shown as
//! the card shows them (`note_markdown`: the Markdown subset, everything
//! else escaped, the references typed in them links to their passages);
//! nothing here touches scripture.

use crate::dto::SelectionNote;
use crate::note_markdown::{self, Layout};
use crate::reader_text::escape_html;

/// The narrowest the margin is drawn, in rem (224px at the page's 16px).
/// With less room beside the text there's a marker after each noted verse
/// instead.
pub const MIN_WIDTH_REM: f64 = 14.0;
/// The widest it's drawn, in rem: wider, its lines get too long to read.
pub const MAX_WIDTH_REM: f64 = 20.0;
/// Between the text and the margin (the reading column's own padding, so
/// the margin starts at its container's edge), in rem.
pub const GAP_REM: f64 = 1.5;
/// Kept free between the margin and the window's right edge, in rem: room
/// for the notes to breathe at the margin's narrowest.
pub const EDGE_REM: f64 = 2.0;
/// Between two notes stacked because they'd overlap, in px.
pub const STACK_GAP_PX: f64 = 8.0;
/// A note longer than this many lines shows them, faded at the foot, with
/// "more".
pub const FOLD_LINES: u32 = 6;

/// Where a note sits: beside `verse`, which is its own first verse
/// (`own`), or, where the translation doesn't number that one (Matthew
/// 17:21 in the BSB), the nearest verse before it it does; its thin rule
/// runs on to `last`, the last of its verses on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Anchor {
    pub verse: i64,
    pub own: bool,
    pub last: i64,
}

impl Anchor {
    /// Whether the note's rule runs beside more than one verse.
    pub fn is_range(self) -> bool {
        self.last > self.verse
    }
}

/// Where the note on `start`–`end` sits among the verses on screen,
/// `numbered` (in order): beside its first verse, or the nearest one
/// before it (or, with none before, the first). None with no verses.
pub fn anchor(start: i64, end: i64, numbered: &[i64]) -> Option<Anchor> {
    let first = *numbered.first()?;
    let (verse, own) = if numbered.contains(&start) {
        (start, true)
    } else {
        (
            numbered
                .iter()
                .copied()
                .filter(|&n| n < start)
                .max()
                .unwrap_or(first),
            false,
        )
    };
    let last = numbered
        .iter()
        .copied()
        .filter(|&n| n >= start && n <= end)
        .max()
        .unwrap_or(verse)
        .max(verse);
    Some(Anchor { verse, own, last })
}

/// The verses a margin note on `start`–`end` (or its marker) selects to
/// open the card with it, the card's notes being those overlapping the
/// selection: its own verses on screen; beside a verse before it, from
/// that one on to the first verse after the gap (the BSB's Matthew
/// 17:20–22 for a note on 17:21). None where no selection of the verses on
/// screen (`numbered`) can reach it: past the chapter's last verse here.
pub fn selection(anchor: Anchor, start: i64, end: i64, numbered: &[i64]) -> Option<(i64, i64)> {
    if anchor.own {
        return Some((anchor.verse, anchor.last));
    }
    if anchor.verse > end {
        return None;
    }
    let to = numbered.iter().copied().filter(|&n| n >= start).min()?;
    Some((anchor.verse, to.max(anchor.last)))
}

/// Each note's top, given where it would be (`wanted`: its verse's top, in
/// order down the page) and how tall it is (`heights`): there, unless that
/// would overlap the note above, in which case it goes just below it, `gap`
/// apart. The tops never decrease, so no two notes overlap.
pub fn stack(wanted: &[f64], heights: &[f64], gap: f64) -> Vec<f64> {
    let mut free_from = f64::NEG_INFINITY;
    wanted
        .iter()
        .zip(heights)
        .map(|(&want, &height)| {
            let top = want.max(free_from);
            free_from = top + height + gap;
            top
        })
        .collect()
}

/// The margin's width, given the room from the text's right edge to the
/// window's (`room`, px) and the page's rem (px): what's left after the gap
/// and the edge, up to [`MAX_WIDTH_REM`]; none if that's under
/// [`MIN_WIDTH_REM`].
pub fn width(room: f64, rem: f64) -> Option<f64> {
    let left = room - (GAP_REM + EDGE_REM) * rem;
    (left >= MIN_WIDTH_REM * rem).then(|| left.min(MAX_WIDTH_REM * rem))
}

/// What a note is called, for a screen reader and as its marker's tooltip.
pub fn label(reference: &str) -> String {
    format!("Note on {reference}")
}

/// What the reference line adds when the note isn't on the verse it's
/// beside.
pub const NOT_NUMBERED: &str = "not numbered in this translation";

/// One note in the margin: its reference (small; a button that opens the
/// card with it), then its words as Markdown, folded past [`FOLD_LINES`]
/// with "more" (which `margin` shows only where the words are cut short).
/// A note beside a verse it isn't on says so after its reference.
pub fn note_html(note: &SelectionNote, anchor: Anchor) -> String {
    let id = note.id;
    let reference = escape_html(&note.reference);
    let (class, aside) = if anchor.own {
        ("", String::new())
    } else {
        (
            " is-elsewhere",
            format!(r#" <span class="margin-note-where">({NOT_NUMBERED})</span>"#),
        )
    };
    format!(
        r#"<article class="margin-note{class}" data-note-id="{id}" aria-label="{label}"><button type="button" class="margin-note-ref" data-note-id="{id}">{reference}{aside}</button><div class="margin-note-body note-text" id="margin-note-body-{id}">{body}</div><button type="button" class="margin-note-more note-link" data-note-id="{id}" aria-label="More of the note on {reference}" hidden>more</button></article>"#,
        label = escape_html(&label(&note.reference)),
        body = note_markdown::note_html(&note.body, &note.links, &[], Layout::Blocks),
    )
}

/// The margin: a complementary landmark, "Your notes", after the chapter in
/// reading order, its notes in verse order, and a thin rule beside each
/// range's verses (decoration, hidden from screen readers).
pub fn margin_html(notes: &[(&SelectionNote, Anchor)]) -> String {
    let rules: String = notes
        .iter()
        .filter(|(_, anchor)| anchor.is_range())
        .map(|(note, _)| {
            format!(
                r#"<span class="margin-rule" data-note-id="{}" aria-hidden="true"></span>"#,
                note.id
            )
        })
        .collect();
    let items: String = notes
        .iter()
        .map(|(note, anchor)| note_html(note, *anchor))
        .collect();
    format!(
        r#"<aside class="margin-notes" role="complementary" aria-label="Your notes">{rules}{items}</aside>"#
    )
}

/// A noted verse's marker, where there's no room for the margin: a small
/// button after the verse that opens the card with the note.
pub fn marker_html(note: &SelectionNote) -> String {
    let label = escape_html(&label(&note.reference));
    format!(
        r#"<button type="button" class="margin-marker" data-note-id="{}" aria-label="{label}" title="{label}">&#9998;</button>"#,
        note.id
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(id: i64, start: i64, end: i64, reference: &str, body: &str) -> SelectionNote {
        SelectionNote {
            id,
            book_id: 43,
            chapter: 3,
            verse_start: start,
            verse_end: end,
            reference: reference.to_string(),
            body: body.to_string(),
            created_at: "2026-10-27T00:00:00.000Z".to_string(),
            created_at_display: "Oct 27, 2026".to_string(),
            links: Vec::new(),
        }
    }

    #[test]
    fn a_note_sits_beside_its_first_verse_or_the_nearest_before_it() {
        let all: Vec<i64> = (1..=36).collect();
        assert_eq!(
            anchor(16, 16, &all),
            Some(Anchor {
                verse: 16,
                own: true,
                last: 16
            })
        );
        assert_eq!(
            anchor(16, 18, &all),
            Some(Anchor {
                verse: 16,
                own: true,
                last: 18
            })
        );
        assert!(anchor(16, 18, &all).unwrap().is_range());
        // Matthew 17 in the BSB has no verse 21.
        let bsb: Vec<i64> = (1..=27).filter(|&n| n != 21).collect();
        assert_eq!(
            anchor(21, 21, &bsb),
            Some(Anchor {
                verse: 20,
                own: false,
                last: 20
            })
        );
        assert_eq!(
            anchor(20, 22, &bsb),
            Some(Anchor {
                verse: 20,
                own: true,
                last: 22
            })
        );
        assert_eq!(
            anchor(21, 22, &bsb),
            Some(Anchor {
                verse: 20,
                own: false,
                last: 22
            })
        );
        // Past the chapter's end in this translation (Romans 16:25–27 in
        // the WEB, which ends at 24): beside its last verse.
        let web: Vec<i64> = (1..=24).collect();
        assert_eq!(
            anchor(25, 27, &web),
            Some(Anchor {
                verse: 24,
                own: false,
                last: 24
            })
        );
        // Before the first verse there is: beside the first.
        assert_eq!(
            anchor(1, 1, &[2, 3]),
            Some(Anchor {
                verse: 2,
                own: false,
                last: 2
            })
        );
        assert_eq!(anchor(1, 1, &[]), None);
    }

    #[test]
    fn a_margin_note_selects_its_verses_or_across_the_gap_to_them() {
        let select = |start, end, numbered: &[i64]| {
            selection(anchor(start, end, numbered).unwrap(), start, end, numbered)
        };
        let all: Vec<i64> = (1..=36).collect();
        assert_eq!(select(16, 18, &all), Some((16, 18)));
        assert_eq!(select(16, 16, &all), Some((16, 16)));
        // The BSB's Matthew 17 has no verse 21: 20 to 22 holds it.
        let bsb: Vec<i64> = (1..=27).filter(|&n| n != 21).collect();
        assert_eq!(select(21, 21, &bsb), Some((20, 22)));
        assert_eq!(select(20, 22, &bsb), Some((20, 22)));
        assert_eq!(select(21, 22, &bsb), Some((20, 22)));
        // Past the chapter's end here (the WEB's Romans 16 ends at 24), or
        // before its first verse: no selection reaches it.
        let web: Vec<i64> = (1..=24).collect();
        assert_eq!(select(25, 27, &web), None);
        assert_eq!(select(1, 1, &[2, 3]), None);
    }

    #[test]
    fn notes_that_would_overlap_stack_downward_in_order() {
        // Apart: each at its verse.
        assert_eq!(
            stack(&[0.0, 100.0, 300.0], &[40.0, 40.0, 40.0], 8.0),
            [0.0, 100.0, 300.0]
        );
        // Two notes on one verse, and a third close below: each just under
        // the one before, 8px apart, then back at its verse once there's
        // room.
        assert_eq!(
            stack(&[0.0, 0.0, 30.0, 500.0], &[40.0, 60.0, 20.0, 10.0], 8.0),
            [0.0, 48.0, 116.0, 500.0]
        );
        // Never overlapping, never above its verse.
        let wanted: Vec<f64> = (0..176).map(|n| f64::from(n) * 30.0).collect();
        let heights = vec![45.0; 176];
        let tops = stack(&wanted, &heights, 8.0);
        for (i, top) in tops.iter().enumerate() {
            assert!(*top >= wanted[i]);
            if i > 0 {
                assert!(*top >= tops[i - 1] + heights[i - 1] + 8.0);
            }
        }
        assert!(stack(&[], &[], 8.0).is_empty());
    }

    #[test]
    fn the_margin_needs_14rem_beside_the_text() {
        // 16px rem: the gap and the edge take 56px (the edge 2rem, room
        // for the notes to breathe).
        assert_eq!(width(280.0, 16.0), Some(224.0));
        assert_eq!(width(279.9, 16.0), None);
        assert_eq!(width(300.0, 16.0), Some(244.0));
        assert_eq!(width(1000.0, 16.0), Some(320.0), "at most 20rem");
        assert_eq!(width(0.0, 16.0), None);
    }

    #[test]
    fn the_margin_is_a_landmark_of_labelled_notes_in_verse_order() {
        let all: Vec<i64> = (1..=36).collect();
        let a = note(1, 16, 18, "John 3:16–18", "For <God> & \"world\"\nso loved");
        let b = note(2, 21, 21, "John 3:21", "Light.");
        let html = margin_html(&[
            (&a, anchor(16, 18, &all).unwrap()),
            (&b, anchor(21, 21, &all).unwrap()),
        ]);
        assert!(html.starts_with(
            r#"<aside class="margin-notes" role="complementary" aria-label="Your notes">"#
        ));
        // One rule, for the range.
        assert_eq!(html.matches("margin-rule").count(), 1);
        assert!(html.contains(r#"<span class="margin-rule" data-note-id="1" aria-hidden="true">"#));
        assert!(html.contains(
            r#"<article class="margin-note" data-note-id="1" aria-label="Note on John 3:16–18">"#
        ));
        assert!(html.contains(
            r#"<button type="button" class="margin-note-ref" data-note-id="1">John 3:16–18</button>"#
        ));
        // The reader's words, as the card shows them: Markdown, anything
        // else escaped, a line break kept.
        assert!(html.contains(
            r#"<div class="margin-note-body note-text" id="margin-note-body-1"><p>For &lt;God&gt; &amp; &quot;world&quot;<br>so loved</p></div>"#
        ));
        assert!(
            html.contains(r#"aria-label="More of the note on John 3:16–18" hidden>more</button>"#)
        );
        assert!(
            html.find("data-note-id=\"1\" aria-label").unwrap()
                < html.find("data-note-id=\"2\" aria-label").unwrap()
        );
    }

    #[test]
    fn a_note_beside_a_verse_it_is_not_on_says_so() {
        let bsb: Vec<i64> = (1..=27).filter(|&n| n != 21).collect();
        let n = note(7, 21, 21, "Matthew 17:21", "The gap.");
        let html = note_html(&n, anchor(21, 21, &bsb).unwrap());
        assert!(html.contains(r#"class="margin-note is-elsewhere""#));
        assert!(html.contains(
            r#">Matthew 17:21 <span class="margin-note-where">(not numbered in this translation)</span></button>"#
        ));
        assert_eq!(
            marker_html(&n),
            r#"<button type="button" class="margin-marker" data-note-id="7" aria-label="Note on Matthew 17:21" title="Note on Matthew 17:21">&#9998;</button>"#
        );
    }
}
