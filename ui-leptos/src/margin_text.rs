//! The margin notes' rules and markup, free of the DOM (`margin` puts them
//! on the page): the reader's notes on the chapter sit in the open space to
//! the right of the text, each beside its first verse, as in a printed
//! Bible's margin. Whether the margin shows (Aa's "Notes beside the text":
//! in reading mode, or always), which verse a note sits beside, how notes
//! that would overlap stack, whether there's room for the margin, and the
//! markup for the margin and for the markers that take its place in a
//! window too narrow for it. Notes are the reader's own words, shown as the
//! card shows them (`note_markdown`: the Markdown subset, everything else
//! escaped, the references typed in them links to their passages); nothing
//! here touches scripture.

use crate::dto::SelectionNote;
use crate::findings_text::{self, FindingGroup};
use crate::note_markdown::{self, Layout};
use crate::preferences::NotesBeside;
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
/// "more": enough for a sermon outline to read as one (B5).
pub const FOLD_LINES: u32 = 10;

/// Whether a note's words are cut short at the fold: they reach further
/// (`full`, px) than they're shown (`shown`), by more than a pixel of
/// rounding.
pub fn cut_short(full: f64, shown: f64) -> bool {
    full > shown + 1.0
}

/// Said at the top of the margin, with "Always", when the chapter has no
/// notes: the margin is there on every chapter, so it says why it's empty.
pub const NO_NOTES: &str = "No notes on this chapter yet. Select verses and choose Add note.";

/// What the margin shows beside a pane's chapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Shown {
    #[default]
    Nothing,
    /// The notes beside the text.
    Margin,
    /// A marker after each noted verse: no room for the margin.
    Markers,
    /// The margin with no notes in it, saying so ([`NO_NOTES`]).
    Empty,
}

impl Shown {
    /// Its name to the tests (`window.gospelGetterMargin.shown()`).
    pub fn name(self) -> &'static str {
        match self {
            Self::Nothing => "nothing",
            Self::Margin => "margin",
            Self::Markers => "markers",
            Self::Empty => "empty",
        }
    }
}

/// Where a pane's chapter is being read, as far as the margin cares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reading {
    /// Aa's "Notes beside the text".
    pub setting: NotesBeside,
    pub reading_mode: bool,
    /// Compare shows translations side by side: there's no room.
    pub comparing: bool,
    /// Two panes are open.
    pub split: bool,
}

/// What the margin shows for a pane's chapter read as `reading` says, with
/// notes or findings on it (`entries`) or none, the notes `known` (read for
/// this chapter, not still on their way), and `room` for the margin beside
/// the text ([`width`]). None while comparing, or where the setting doesn't
/// show them. Two panes open, outside reading mode, have no room for it
/// beside either: markers in each. Reading mode, which shows one pane
/// alone, has the margin as it always did, and with no notes keeps its
/// empty space; "Always" outside it says the chapter has none, once that's
/// known.
pub fn shown(reading: Reading, entries: bool, known: bool, room: bool) -> Shown {
    let wanted = match reading.setting {
        NotesBeside::Off => false,
        NotesBeside::Reading => reading.reading_mode,
        NotesBeside::Always => true,
    };
    if !wanted || reading.comparing {
        return Shown::Nothing;
    }
    let beside = room && !(reading.split && !reading.reading_mode);
    match (entries, beside) {
        (true, true) => Shown::Margin,
        (true, false) => Shown::Markers,
        (false, true) if known && !reading.reading_mode => Shown::Empty,
        (false, _) => Shown::Nothing,
    }
}

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

/// What goes down the margin beside a verse: one of the reader's notes, or
/// the verse's AI findings, one entry for them all (`findings_text`).
#[derive(Debug, Clone, Copy)]
pub enum Entry<'a> {
    Note(&'a SelectionNote),
    Findings(&'a FindingGroup),
}

/// The reader's notes and the verses' findings, each where it sits, in the
/// order they go down the page: by the verse each sits beside, the reader's
/// own notes there first, then its findings (C9), each kind in the order
/// given.
pub fn entries<'a>(
    notes: &[(&'a SelectionNote, Anchor)],
    findings: &[(&'a FindingGroup, Anchor)],
) -> Vec<(Entry<'a>, Anchor)> {
    let mut all: Vec<(Entry<'a>, Anchor, u8)> = notes
        .iter()
        .map(|&(note, anchor)| (Entry::Note(note), anchor, 0))
        .chain(
            findings
                .iter()
                .map(|&(group, anchor)| (Entry::Findings(group), anchor, 1)),
        )
        .collect();
    all.sort_by_key(|(_, anchor, kind)| (anchor.verse, *kind));
    all.into_iter()
        .map(|(entry, anchor, _)| (entry, anchor))
        .collect()
}

/// The reader's notes among `entries`.
fn notes_of<'a>(entries: &[(Entry<'a>, Anchor)]) -> Vec<(&'a SelectionNote, Anchor)> {
    entries
        .iter()
        .filter_map(|&(entry, anchor)| match entry {
            Entry::Note(note) => Some((note, anchor)),
            Entry::Findings(_) => None,
        })
        .collect()
}

/// The margin: a complementary landmark, "Your notes", after the chapter in
/// reading order, its notes in verse order, and a thin rule beside each
/// range's verses (decoration, hidden from screen readers).
#[cfg(test)]
pub fn margin_html(notes: &[(&SelectionNote, Anchor)]) -> String {
    margin_entries_html(&entries(notes, &[]))
}

/// The margin, the verses' AI findings among the reader's notes, each
/// after the notes beside its verse.
pub fn margin_entries_html(entries: &[(Entry, Anchor)]) -> String {
    let rules = rules_html(&notes_of(entries));
    let items: String = entries
        .iter()
        .map(|(entry, anchor)| match entry {
            Entry::Note(note) => note_html(note, *anchor),
            Entry::Findings(group) => findings_text::margin_html(group),
        })
        .collect();
    format!(
        r#"<aside class="margin-notes" role="complementary" aria-label="Your notes">{rules}{items}</aside>"#
    )
}

/// The margin for a chapter with no notes (nor findings), with "Always":
/// the landmark as ever, and one quiet line at its top saying how to add
/// one.
pub fn empty_html() -> String {
    format!(
        r#"<aside class="margin-notes" role="complementary" aria-label="Your notes"><p class="margin-notes-empty">{NO_NOTES}</p></aside>"#
    )
}

/// A noted verse's marker, where there's no room for the margin (or two
/// panes share the window): a small button after the verse that opens the
/// card with the note.
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
    fn the_margin_holds_its_notes_in_verse_order_with_a_rule_beside_each_range() {
        let all = john3();
        let notes = [
            note(5, 16, 16, "John 3:16", "Love."),
            note(1, 3, 5, "John 3:3\u{2013}5", "Born again."),
            note(2, 4, 4, "John 3:4", "Literally."),
        ];
        let placed = in_order(notes.iter().map(|n| (n, n.verse_start, n.verse_end)), &all);
        let ids: Vec<i64> = placed.iter().map(|(n, _)| n.id).collect();
        assert_eq!(ids, [1, 2, 5]);
        let html = margin_html(&placed);
        assert_eq!(html.matches("margin-rule").count(), 1);
        assert!(html.starts_with(
            r#"<aside class="margin-notes" role="complementary" aria-label="Your notes"><span class="margin-rule" data-note-id="1" aria-hidden="true"></span>"#
        ));
        assert!(html.ends_with("</article></aside>"));
        // Each labelled with its full reference, the one label style.
        assert!(html.contains(
            r#"<button type="button" class="margin-note-ref" data-note-id="1">John 3:3–5</button>"#
        ));
        let at = |id: i64| {
            html.find(&format!(
                r#"<article class="margin-note" data-note-id="{id}""#
            ))
            .unwrap()
        };
        assert!(at(1) < at(2) && at(2) < at(5));
        assert!(!html.contains("margin-notes-empty"));
    }

    #[test]
    fn notes_on_adjacent_verses_stack_downward() {
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
    fn a_long_note_folds_after_ten_lines() {
        assert_eq!(FOLD_LINES, 10, "B5: everywhere");
        // At 0.8rem (12.8px) and a 1.45 line height a note shows at most
        // ten lines: the one line past them is cut short, a note that fits
        // isn't (a pixel's rounding allowed).
        let line = 12.8 * 1.45;
        let shown = |lines: u32| line * f64::from(lines.min(FOLD_LINES));
        let full = |lines: u32| line * f64::from(lines);
        assert!(!cut_short(full(10), shown(10)));
        assert!(!cut_short(full(10) + 1.0, shown(10)));
        assert!(cut_short(full(11), shown(11)));
        assert!(!cut_short(full(6), shown(6)));
    }

    #[test]
    fn an_empty_margin_says_how_to_add_a_note() {
        assert_eq!(
            empty_html(),
            r#"<aside class="margin-notes" role="complementary" aria-label="Your notes"><p class="margin-notes-empty">No notes on this chapter yet. Select verses and choose Add note.</p></aside>"#
        );
        assert!(in_order(std::iter::empty::<((), i64, i64)>(), &john3()).is_empty());
    }

    /// A note is on one chapter (its verses, `verse_start`–`verse_end`, are
    /// that chapter's), so none starts in the chapter before. What comes
    /// closest, a note that starts before the first verse on screen, sits
    /// as `anchor` says: beside the first verse, saying it isn't on it,
    /// its rule from that first verse on.
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
        let html = note_html(&n, at);
        assert!(html.contains(
            r#">John 3:1–3 <span class="margin-note-where">(not numbered in this translation)</span></button>"#
        ));
        let placed = in_order([(&n, 1, 3)], &from_two);
        assert_eq!(placed[0].1.verse, 2);
        assert_eq!(margin_html(&placed).matches("margin-rule").count(), 1);
    }

    /// Every setting, in reading mode or not, comparing or not, with one
    /// pane or two, with notes or none, with room for the margin or not.
    #[test]
    fn what_the_margin_shows() {
        use NotesBeside::{Always, Off, Reading as InReadingMode};
        let at = |setting, reading_mode, comparing, split| Reading {
            setting,
            reading_mode,
            comparing,
            split,
        };
        for setting in [Off, InReadingMode, Always] {
            for reading_mode in [false, true] {
                for comparing in [false, true] {
                    for split in [false, true] {
                        for entries in [false, true] {
                            for room in [false, true] {
                                let reading = at(setting, reading_mode, comparing, split);
                                let got = shown(reading, entries, true, room);
                                let on = match setting {
                                    Off => false,
                                    InReadingMode => reading_mode,
                                    Always => true,
                                };
                                let want = if !on || comparing {
                                    Shown::Nothing
                                } else if split && !reading_mode {
                                    // Two panes side by side: markers in each.
                                    if entries {
                                        Shown::Markers
                                    } else {
                                        Shown::Nothing
                                    }
                                } else if entries {
                                    if room { Shown::Margin } else { Shown::Markers }
                                } else if room && !reading_mode {
                                    Shown::Empty
                                } else {
                                    Shown::Nothing
                                };
                                assert_eq!(
                                    got, want,
                                    "{setting:?} reading mode {reading_mode} comparing \
                                     {comparing} split {split} notes {entries} room {room}"
                                );
                            }
                        }
                    }
                }
            }
        }
        // The cases the reader sees, by name.
        let always = at(Always, false, false, false);
        assert_eq!(shown(always, true, true, true), Shown::Margin);
        assert_eq!(shown(always, true, true, false), Shown::Markers);
        assert_eq!(shown(always, false, true, true), Shown::Empty);
        // Not before the chapter's notes are known to be none.
        assert_eq!(shown(always, false, false, true), Shown::Nothing);
        // In reading mode with no notes: its empty space, as ever.
        assert_eq!(
            shown(at(Always, true, false, false), false, true, true),
            Shown::Nothing
        );
        assert_eq!(
            shown(at(InReadingMode, false, false, false), true, true, true),
            Shown::Nothing,
            "the default, out of reading mode"
        );
        assert_eq!(
            shown(at(InReadingMode, true, false, true), true, true, true),
            Shown::Margin,
            "reading mode with two panes shows the active one alone"
        );
        assert_eq!(
            shown(at(Always, false, false, true), true, true, true),
            Shown::Markers
        );
        assert_eq!(
            shown(at(Always, false, true, false), true, true, true),
            Shown::Nothing,
            "comparing translations"
        );
        assert_eq!(Shown::default().name(), "nothing");
        assert_eq!(Shown::Empty.name(), "empty");
    }
}
