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
//!
//! The same rules place them in Compare's "My notes" column (Compare with
//! Notes, outside reading mode): beside the text in the column Compare's
//! second translation would have, each labelled with its verses, or, in a
//! window too narrow for columns ([`STACKED_QUERY`]), under its first
//! verse.

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
/// for the notes to breathe at the margin's narrowest, and no more than a
/// 1280 px window leaves beside the Normal column at the default text size
/// (274px, or 266.5px beside Windows' scroll bar, with 15.5rem of it taken
/// by the margin and its gap).
pub const EDGE_REM: f64 = 1.125;
/// Between two notes stacked because they'd overlap, in px.
pub const STACK_GAP_PX: f64 = 8.0;
/// A note longer than this many lines shows them, faded at the foot, with
/// "more".
pub const FOLD_LINES: u32 = 6;
/// The same, in Compare's notes column (and under a verse, in a window too
/// narrow for it), which is wider than the margin.
pub const COLUMN_FOLD_LINES: u32 = 10;

/// Whether a note's words are cut short at the fold: they reach further
/// (`full`, px) than they're shown (`shown`), by more than a pixel of
/// rounding.
pub fn cut_short(full: f64, shown: f64) -> bool {
    full > shown + 1.0
}

/// Where Compare stacks its columns (the stylesheet's `.compare-grid`
/// query): the notes go under their verses instead of beside the text.
pub const STACKED_QUERY: &str = "(max-width: 640px)";

/// What Compare's notes column is called: its landmark and its heading.
pub const COLUMN_NAME: &str = "My notes";

/// Said at the top of the notes column when the chapter has no notes.
pub const NO_NOTES: &str = "No notes on this chapter yet. Select verses and choose Add note.";

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

/// Each of `notes` (with its verses, `start`–`end`) and where it sits among
/// the verses on screen (`numbered`), in the order they go down the page:
/// by the verse each sits beside, then in the order given (the backend's).
/// With no verses on screen, none.
pub fn in_order<T>(
    notes: impl IntoIterator<Item = (T, i64, i64)>,
    numbered: &[i64],
) -> Vec<(T, Anchor)> {
    let mut placed: Vec<(T, Anchor)> = notes
        .into_iter()
        .filter_map(|(note, start, end)| Some((note, anchor(start, end, numbered)?)))
        .collect();
    placed.sort_by_key(|(_, anchor)| anchor.verse);
    placed
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

/// A note's verses, as the notes column labels it: "16", or "3–5" for a
/// range (its own verses, wherever it sits).
pub fn verses_label(start: i64, end: i64) -> String {
    if end > start {
        format!("{start}\u{2013}{end}")
    } else {
        start.to_string()
    }
}

/// What the reference line adds when the note isn't on the verse it's
/// beside.
pub const NOT_NUMBERED: &str = "not numbered in this translation";

/// One note in the margin: its reference (small; a button that opens the
/// card with it), then its words as Markdown, folded past [`FOLD_LINES`]
/// with "more" (which `margin` shows only where the words are cut short).
/// A note beside a verse it isn't on says so after its reference.
pub fn note_html(note: &SelectionNote, anchor: Anchor) -> String {
    labelled_note_html(note, anchor, None)
}

/// [`note_html`], its reference button reading `shown` instead (the notes
/// column's "3–5"), named in full for a screen reader; or, with none, the
/// reference.
fn labelled_note_html(note: &SelectionNote, anchor: Anchor, shown: Option<&str>) -> String {
    let id = note.id;
    let reference = escape_html(&note.reference);
    let (named, text) = match shown {
        Some(shown) => (format!(r#" aria-label="{reference}""#), escape_html(shown)),
        None => (String::new(), reference.clone()),
    };
    let (class, aside) = if anchor.own {
        ("", String::new())
    } else {
        (
            " is-elsewhere",
            format!(r#" <span class="margin-note-where">({NOT_NUMBERED})</span>"#),
        )
    };
    format!(
        r#"<article class="margin-note{class}" data-note-id="{id}" aria-label="{label}"><button type="button" class="margin-note-ref" data-note-id="{id}"{named}>{text}{aside}</button><div class="margin-note-body note-text" id="margin-note-body-{id}">{body}</div><button type="button" class="margin-note-more note-link" data-note-id="{id}" aria-label="More of the note on {reference}" hidden>more</button></article>"#,
        label = escape_html(&label(&note.reference)),
        body = note_markdown::note_html(&note.body, &note.links, &[], Layout::Blocks),
    )
}

/// A thin rule for each range among `notes`, to run beside its verses
/// (decoration, hidden from screen readers).
fn rules_html(notes: &[(&SelectionNote, Anchor)]) -> String {
    notes
        .iter()
        .filter(|(_, anchor)| anchor.is_range())
        .map(|(note, _)| {
            format!(
                r#"<span class="margin-rule" data-note-id="{}" aria-hidden="true"></span>"#,
                note.id
            )
        })
        .collect()
}

/// The margin: a complementary landmark, "Your notes", after the chapter in
/// reading order, its notes in verse order, and a thin rule beside each
/// range's verses (decoration, hidden from screen readers).
pub fn margin_html(notes: &[(&SelectionNote, Anchor)]) -> String {
    let rules = rules_html(notes);
    let items: String = notes
        .iter()
        .map(|(note, anchor)| note_html(note, *anchor))
        .collect();
    format!(
        r#"<aside class="margin-notes" role="complementary" aria-label="Your notes">{rules}{items}</aside>"#
    )
}

/// One note in Compare's notes column, or under its verse: as in the
/// margin ([`note_html`]), labelled with its verses ([`verses_label`]).
pub fn column_note_html(note: &SelectionNote, anchor: Anchor) -> String {
    labelled_note_html(
        note,
        anchor,
        Some(&verses_label(note.verse_start, note.verse_end)),
    )
}

/// Compare's notes column, a region named "My notes" beside the text: as
/// the margin, its notes in verse order and a rule beside each range; with
/// none (`known`: the chapter has none, rather than they're still on their
/// way or couldn't be read), one quiet line saying how to add one. Empty,
/// it's what the reader draws before the notes come.
pub fn column_html(notes: &[(&SelectionNote, Anchor)], known: bool) -> String {
    let inner = if notes.is_empty() {
        if known {
            format!(r#"<p class="notes-column-empty">{NO_NOTES}</p>"#)
        } else {
            String::new()
        }
    } else {
        let rules = rules_html(notes);
        let items: String = notes
            .iter()
            .map(|(note, anchor)| column_note_html(note, *anchor))
            .collect();
        format!("{rules}{items}")
    };
    format!(r#"<section class="notes-column" aria-label="{COLUMN_NAME}">{inner}</section>"#)
}

/// The notes that sit under one verse, in a window too narrow for the
/// column: each as in the column, after the verse in the text.
pub fn under_html(notes: &[(&SelectionNote, Anchor)]) -> String {
    notes
        .iter()
        .map(|(note, anchor)| {
            format!(
                r#"<div class="note-under">{}</div>"#,
                column_note_html(note, *anchor)
            )
        })
        .collect()
}

/// The narrow window's line for a chapter with no notes, above the text.
pub fn under_empty_html() -> String {
    format!(r#"<p class="note-under notes-column-empty">{NO_NOTES}</p>"#)
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
        // 16px rem: the gap and the edge take 42px (the edge 1.125rem, room
        // for the notes to breathe).
        assert_eq!(width(266.0, 16.0), Some(224.0));
        assert_eq!(width(265.9, 16.0), None);
        assert_eq!(width(300.0, 16.0), Some(258.0));
        assert_eq!(width(1000.0, 16.0), Some(320.0), "at most 20rem");
        assert_eq!(width(0.0, 16.0), None);
    }

    #[test]
    fn a_1280px_window_has_the_margin_beside_the_normal_column() {
        // The Normal column (732px at the default text size) centred in a
        // 1280 px window leaves 274px beside it; beside Windows' 15px
        // scroll bar, 266.5px.
        assert_eq!(width(274.0, 16.0), Some(232.0));
        assert_eq!(width(266.5, 16.0), Some(224.5));
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

    /// John 3's verses, in the KJV.
    fn john3() -> Vec<i64> {
        (1..=36).collect()
    }

    #[test]
    fn a_column_note_is_labelled_with_its_own_verses() {
        assert_eq!(verses_label(16, 16), "16");
        assert_eq!(verses_label(3, 5), "3\u{2013}5");
        let all = john3();
        let one = note(1, 2, 2, "John 3:2", "By night.");
        let range = note(2, 3, 5, "John 3:3\u{2013}5", "Born again.");
        let html = column_note_html(&one, anchor(2, 2, &all).unwrap());
        assert!(html.contains(
            r#"<button type="button" class="margin-note-ref" data-note-id="1" aria-label="John 3:2">2</button>"#
        ));
        assert!(html.contains(r#"aria-label="Note on John 3:2""#));
        let html = column_note_html(&range, anchor(3, 5, &all).unwrap());
        assert!(html.contains(
            r#"<button type="button" class="margin-note-ref" data-note-id="2" aria-label="John 3:3–5">3–5</button>"#
        ));
        // Beside a verse it isn't on: its own verses, and why.
        let bsb: Vec<i64> = (1..=27).filter(|&n| n != 21).collect();
        let gap = note(3, 21, 22, "Matthew 17:21\u{2013}22", "The gap.");
        let html = column_note_html(&gap, anchor(21, 22, &bsb).unwrap());
        assert!(html.contains(r#"class="margin-note is-elsewhere""#));
        assert!(html.contains(
            r#"aria-label="Matthew 17:21–22">21–22 <span class="margin-note-where">(not numbered in this translation)</span></button>"#
        ));
    }

    #[test]
    fn the_column_holds_its_notes_in_verse_order_with_a_rule_beside_each_range() {
        let all = john3();
        let notes = [
            note(5, 16, 16, "John 3:16", "Love."),
            note(1, 3, 5, "John 3:3\u{2013}5", "Born again."),
            note(2, 4, 4, "John 3:4", "Literally."),
        ];
        let placed = in_order(notes.iter().map(|n| (n, n.verse_start, n.verse_end)), &all);
        let ids: Vec<i64> = placed.iter().map(|(n, _)| n.id).collect();
        assert_eq!(ids, [1, 2, 5]);
        let html = column_html(&placed, true);
        assert_eq!(html.matches("margin-rule").count(), 1);
        assert!(html.starts_with(
            r#"<section class="notes-column" aria-label="My notes"><span class="margin-rule" data-note-id="1" aria-hidden="true"></span>"#
        ));
        assert!(html.ends_with("</article></section>"));
        let at = |id: i64| {
            html.find(&format!(
                r#"<article class="margin-note" data-note-id="{id}""#
            ))
            .unwrap()
        };
        assert!(at(1) < at(2) && at(2) < at(5));
        assert!(!html.contains("notes-column-empty"));
    }

    #[test]
    fn notes_on_adjacent_verses_stack_downward_in_the_column() {
        // 3–5 and 4 start a verse apart (verse 3 at 0px, 4 at 94px, 5 at
        // 188px): the 4's note, which would start inside the 3–5's (120px
        // tall), goes 8px below it; 16's, far down, stays at its verse.
        let all = john3();
        let notes = [(41, 4, 4), (35, 3, 5), (16, 16, 16)];
        let placed = in_order(notes.iter().map(|&(id, s, e)| (id, s, e)), &all);
        assert_eq!(
            placed.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
            [35, 41, 16]
        );
        let verse_top = |n: i64| (n - 3) as f64 * 94.0;
        let wanted: Vec<f64> = placed.iter().map(|(_, a)| verse_top(a.verse)).collect();
        assert_eq!(
            stack(&wanted, &[120.0, 40.0, 40.0], STACK_GAP_PX),
            [0.0, 128.0, 1222.0]
        );
        // Two notes on the same first verse: in the backend's order, the
        // second just below the first.
        let same = in_order([("a", 7, 8), ("b", 7, 7)], &all);
        assert_eq!(same.iter().map(|(n, _)| *n).collect::<Vec<_>>(), ["a", "b"]);
        assert_eq!(
            stack(&[500.0, 500.0], &[60.0, 20.0], STACK_GAP_PX),
            [500.0, 568.0]
        );
    }

    #[test]
    fn a_long_note_folds_after_ten_lines_in_the_column_and_six_in_the_margin() {
        assert_eq!(COLUMN_FOLD_LINES, 10);
        assert_eq!(FOLD_LINES, 6, "the margin's, unchanged");
        // At 0.8rem (12.8px) and a 1.45 line height a note shows at most
        // `lines` lines: the one line past them is cut short, a note that
        // fits isn't (a pixel's rounding allowed).
        let line = 12.8 * 1.45;
        let shown = |lines: u32, at_most: u32| line * f64::from(lines.min(at_most));
        let full = |lines: u32| line * f64::from(lines);
        assert!(!cut_short(full(10), shown(10, COLUMN_FOLD_LINES)));
        assert!(!cut_short(full(10) + 1.0, shown(10, COLUMN_FOLD_LINES)));
        assert!(cut_short(full(11), shown(11, COLUMN_FOLD_LINES)));
        assert!(!cut_short(full(6), shown(6, FOLD_LINES)));
        assert!(cut_short(full(7), shown(7, FOLD_LINES)));
    }

    #[test]
    fn a_chapter_without_notes_says_how_to_add_one() {
        assert_eq!(
            column_html(&[], true),
            r#"<section class="notes-column" aria-label="My notes"><p class="notes-column-empty">No notes on this chapter yet. Select verses and choose Add note.</p></section>"#
        );
        // Not while they're on their way (or couldn't be read): empty.
        assert_eq!(
            column_html(&[], false),
            r#"<section class="notes-column" aria-label="My notes"></section>"#
        );
        assert_eq!(
            under_empty_html(),
            r#"<p class="note-under notes-column-empty">No notes on this chapter yet. Select verses and choose Add note.</p>"#
        );
        assert!(in_order(std::iter::empty::<((), i64, i64)>(), &john3()).is_empty());
    }

    /// A note is on one chapter (its verses, `verse_start`–`verse_end`, are
    /// that chapter's), so none starts in the chapter before. What comes
    /// closest, a note that starts before the first verse on screen, sits
    /// as `anchor` says: beside the first verse, saying it isn't on it,
    /// labelled with its own verses, its rule from that first verse on.
    #[test]
    fn a_note_starting_before_the_first_verse_on_screen_sits_beside_the_first() {
        let from_two: Vec<i64> = (2..=36).collect();
        let at = anchor(1, 3, &from_two).unwrap();
        assert_eq!(
            at,
            Anchor {
                verse: 2,
                own: false,
                last: 3
            }
        );
        let n = note(9, 1, 3, "John 3:1\u{2013}3", "Before.");
        let html = column_note_html(&n, at);
        assert!(html.contains(
            r#"aria-label="John 3:1–3">1–3 <span class="margin-note-where">(not numbered in this translation)</span></button>"#
        ));
        let placed = in_order([(&n, 1, 3)], &from_two);
        assert_eq!(placed[0].1.verse, 2);
        assert_eq!(column_html(&placed, true).matches("margin-rule").count(), 1);
    }

    #[test]
    fn under_its_verse_a_note_is_labelled_as_in_the_column() {
        let all = john3();
        let n = note(4, 7, 8, "John 3:7\u{2013}8", "Outline.");
        let html = under_html(&[(&n, anchor(7, 8, &all).unwrap())]);
        assert!(html.starts_with(r#"<div class="note-under"><article class="margin-note" data-note-id="4" aria-label="Note on John 3:7–8">"#));
        assert!(html.contains(r#">7–8</button>"#));
        assert!(html.ends_with("</article></div>"));
    }
}
