//! The selection card's words, markup and rules, free of the DOM: the
//! card's actions and Copy as…'s items, the selection's footnotes and
//! cross-references, a cross-reference read in place, the selection's
//! notes, the passage the note editor quotes, which verses a click selects,
//! which highlight controls show, what the status line says, and where the
//! card goes. `card` puts them on the page.
//!
//! The markup is the page's former templates' (`ui/index.html` at 5af9b54:
//! the `verseActions` and `copyMenu` markup, `renderCardReferences`,
//! `toggleXrefPreview`, `renderNotesSection`, `openNotePanel`), byte for
//! byte, spaces and line breaks included. Every word of scripture in it is
//! the backend's, as it sent it: drawn by `reader_text`'s `verse_html` and
//! `footnote_html`, escaped, never changed.

use crate::dto::{SelectionNote, Verse, Xref};
use crate::passage_reference::passage_reference;
use crate::reader_text::{escape_html, footnote_html, verse_html};
use crate::verse_parts::Footnote;

/// The highlighter colors, in the order the swatches show them. The
/// backend accepts exactly these.
pub const HIGHLIGHT_COLORS: [&str; 4] = ["yellow", "green", "blue", "pink"];

/// How many of the selection's notes show before "Show all".
pub const NOTES_SHOWN: usize = 3;

/// How long a status line stays (an error stays), and a deleted note's
/// Undo.
pub const STATUS_MS: i32 = 2500;
pub const UNDO_MS: i32 = 8000;

/// One of Copy as…'s formats: its name (`get_copy_text`'s), its item's
/// label, and what the status line adds after "Copied John 3:16–18".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CopyFormat {
    pub format: &'static str,
    pub label: &'static str,
    pub status: &'static str,
}

/// The menu's formats, in order.
pub const COPY_FORMATS: [CopyFormat; 4] = [
    CopyFormat {
        format: "reference",
        label: "Text with reference",
        status: "",
    },
    CopyFormat {
        format: "numbered",
        label: "With verse numbers",
        status: " with verse numbers",
    },
    CopyFormat {
        format: "markdown",
        label: "Markdown quote",
        status: " as a Markdown quote",
    },
    CopyFormat {
        format: "plain",
        label: "Plain text",
        status: " as plain text",
    },
];

/// The format named `format`, if it's one of the menu's.
pub fn copy_format(format: &str) -> Option<&'static CopyFormat> {
    COPY_FORMATS.iter().find(|f| f.format == format)
}

/// Which text Copy and Copy as… copy: the selection's, or the chapter's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    Selection,
    Chapter,
}

impl Slot {
    /// `selection` or `chapter`, as the page names them.
    pub fn named(name: &str) -> Option<Self> {
        match name {
            "selection" => Some(Self::Selection),
            "chapter" => Some(Self::Chapter),
            _ => None,
        }
    }
}

// ---- Markup.

/// The card's actions: Copy, Copy as…, Bookmark, + Add note, the
/// highlighter, the citation and the status line.
pub fn actions_html() -> String {
    let swatches: String = HIGHLIGHT_COLORS
        .iter()
        .map(|c| {
            format!(
                r#"<button type="button" class="highlight-swatch" data-color="{c}" aria-label="Highlight {c}" title="Highlight {c}" aria-pressed="false"></button>"#
            )
        })
        .collect();
    let s16 = " ".repeat(16);
    let s20 = " ".repeat(20);
    format!(
        "<button type=\"button\" class=\"verse-copy-btn\" title=\"Copy (Ctrl+C)\">Copy</button>\n\
         {s16}<button type=\"button\" class=\"verse-copy-as-btn\" aria-haspopup=\"menu\" aria-expanded=\"false\" aria-controls=\"copy-as-menu\" title=\"Copy as… (Ctrl+Shift+C)\" aria-keyshortcuts=\"Control+Shift+C\">Copy as…</button>\n\
         {s16}<button type=\"button\" class=\"verse-bookmark-btn\">&#9733; Bookmark</button>\n\
         {s16}<button type=\"button\" class=\"verse-note-btn\" title=\"Add a note (N)\" aria-keyshortcuts=\"N\"><span aria-hidden=\"true\">+</span> Add note</button>\n\
         {s16}<span class=\"verse-highlight\" role=\"group\" aria-label=\"Highlight\">\n\
         {s20}{swatches}\n\
         {s20}<button type=\"button\" class=\"highlight-remove\" hidden>Remove highlight</button>\n\
         {s16}</span>\n\
         {s16}<span class=\"verse-actions-ref\"></span>\n\
         {s16}<span class=\"verse-actions-status\" role=\"status\" aria-live=\"polite\"></span>"
    )
}

/// The Bookmark button's label, for a selection bookmarked or not (with
/// its star as an entity, as the page wrote it).
pub fn bookmark_label(bookmarked: bool) -> &'static str {
    if bookmarked {
        "&#9733; Remove bookmark"
    } else {
        "&#9733; Bookmark"
    }
}

/// Copy as…'s items, one per format.
pub fn copy_menu_html() -> String {
    COPY_FORMATS
        .iter()
        .map(|f| {
            format!(
                r#"<button type="button" class="copy-as-item" role="menuitem" tabindex="-1" data-format="{}">{}</button>"#,
                f.format, f.label
            )
        })
        .collect()
}

/// The selection's footnotes, each with its verse's number before its
/// letter when the selection is a range: `entries` are each verse with one
/// of its footnotes, in order. Empty when there are none.
pub fn footnotes_html(entries: &[(&Verse, &Footnote)], range: bool) -> String {
    if entries.is_empty() {
        return String::new();
    }
    let rows: String = entries
        .iter()
        .map(|(verse, footnote)| {
            let number = if range {
                verse.number.to_string()
            } else {
                String::new()
            };
            format!(
                r#"<span class="footnote-entry"><span class="footnote-label">{number}{}</span>{}</span>"#,
                escape_html(&footnote.label),
                footnote_html(footnote),
            )
        })
        .collect();
    format!(
        "<span class=\"xref-popup-title\">Footnotes</span>\n{}{rows}",
        " ".repeat(24)
    )
}

/// The selection's cross-references as chips, a row per verse that has
/// any (`rows`: each verse's number and its cross-references), each row
/// labeled "3:16" when the selection is a range. Empty when there are none.
pub fn xrefs_html(rows: &[(i64, &[Xref])], chapter: i64, range: bool) -> String {
    if rows.is_empty() {
        return String::new();
    }
    let (s24, s28) = (" ".repeat(24), " ".repeat(28));
    let rows: String = rows
        .iter()
        .map(|(number, xrefs)| {
            let label = if range {
                format!(r#"<span class="selection-xref-verse">{chapter}:{number}</span>"#)
            } else {
                String::new()
            };
            let chips: String = xrefs
                .iter()
                .map(|x| {
                    let end = x
                        .end_verse
                        .map(|end| format!(r#" data-end-verse="{end}""#))
                        .unwrap_or_default();
                    format!(
                        r#"<li><button type="button" class="xref-citation" data-book="{}" data-chapter="{}" data-verse="{}"{end} aria-expanded="false" aria-controls="xref-preview">{}</button></li>"#,
                        x.book_id,
                        x.chapter,
                        x.verse,
                        escape_html(&x.citation),
                    )
                })
                .collect();
            format!(
                "<div class=\"selection-xref-row\">\n{s28}{label}\n{s28}<ul class=\"xref-chips\" aria-labelledby=\"selection-xrefs-title\">{chips}</ul>\n{s24}</div>"
            )
        })
        .collect();
    format!(
        "<span class=\"xref-popup-title\" id=\"selection-xrefs-title\">Cross references</span>\n{s24}{rows}"
    )
}

/// A chip's citation as a passage's name, for when the backend couldn't
/// name it: its first hyphen between two digits as an en dash ("1 John
/// 4:9-10" → "1 John 4:9–10"), as the page's `/(\d)-(\d)/` did.
pub fn chip_reference(citation: &str) -> String {
    let chars: Vec<char> = citation.chars().collect();
    let at = chars
        .windows(3)
        .position(|w| w[0].is_ascii_digit() && w[1] == '-' && w[2].is_ascii_digit());
    match at {
        Some(i) => chars
            .iter()
            .enumerate()
            .map(|(j, &c)| if j == i + 1 { '\u{2013}' } else { c })
            .collect(),
        None => citation.to_string(),
    }
}

/// A cross-reference read in place: what `get_passage` answered for it, or
/// why it couldn't.
#[derive(Debug, Clone, Copy)]
pub struct Preview<'a> {
    /// The passage's name: the backend's, or the chip's ([`chip_reference`]).
    pub reference: &'a str,
    /// The translation its text is in.
    pub code: &'a str,
    /// Its verses (none if it couldn't be read).
    pub verses: &'a [Verse],
    /// What the backend said about it, or why it couldn't be read.
    pub notice: &'a str,
    /// The passage Go to opens.
    pub book_id: i64,
    pub chapter: i64,
    pub start: i64,
    pub end: i64,
}

/// The preview's title, its verses (no note markers), its notice, and Go
/// to.
pub fn preview_html(preview: &Preview<'_>) -> String {
    let reference = escape_html(preview.reference);
    let verses = preview
        .verses
        .iter()
        .map(|v| {
            format!(
                r#"<span class="xref-preview-verse"><span class="xref-preview-num">{}</span>{}</span>"#,
                v.number,
                verse_html(v, false)
            )
        })
        .collect::<Vec<_>>()
        .join(" ");
    let text = if verses.is_empty() {
        String::new()
    } else {
        format!(r#"<p class="xref-preview-text">{verses}</p>"#)
    };
    let notice = if preview.notice.is_empty() {
        String::new()
    } else {
        format!(
            r#"<p class="xref-preview-notice">{}</p>"#,
            escape_html(preview.notice)
        )
    };
    let s20 = " ".repeat(20);
    format!(
        "<p class=\"xref-preview-title\" id=\"xref-preview-title\">{reference} ({code})</p>\n{s20}{text}\n{s20}{notice}\n{s20}<button type=\"button\" class=\"xref-preview-go\" data-book=\"{book}\" data-chapter=\"{chapter}\" data-verse=\"{start}\" data-end-verse=\"{end}\">Go to {reference} <span aria-hidden=\"true\">→</span></button>",
        code = escape_html(&preview.code.to_uppercase()),
        book = preview.book_id,
        chapter = preview.chapter,
        start = preview.start,
        end = preview.end,
    )
}

/// Said in the preview when the passage couldn't be read.
pub fn preview_failed(reference: &str, failure: &str) -> String {
    format!("Couldn't read {reference}: {failure}")
}

/// The selection's Notes section: up to [`NOTES_SHOWN`] notes (all of
/// them with `show_all`), each with its "more" (shown by the card where
/// the text is cut short) and Edit, then "Show all"; or why they couldn't
/// be loaded.
pub fn notes_html(notes: &[SelectionNote], error: &str, show_all: bool) -> String {
    if !error.is_empty() {
        return format!(
            r#"<p class="verse-notes-title">Notes</p><p class="note-card-body is-expanded">{}</p>"#,
            escape_html(error)
        );
    }
    let shown = if show_all {
        notes
    } else {
        &notes[..notes.len().min(NOTES_SHOWN)]
    };
    let (s24, s28, s32) = (" ".repeat(24), " ".repeat(28), " ".repeat(32));
    let cards: String = shown
        .iter()
        .map(|n| {
            let reference = escape_html(&n.reference);
            format!(
                "<article class=\"note-card\" data-note-id=\"{id}\">\n\
                 {s28}<p class=\"note-card-head\"><span class=\"note-card-ref\">{reference}</span> <time class=\"note-card-date\" datetime=\"{created}\">{display}</time></p>\n\
                 {s28}<p class=\"note-card-body\" id=\"note-card-body-{id}\">{body}</p>\n\
                 {s28}<span class=\"note-card-actions\">\n\
                 {s32}<button type=\"button\" class=\"note-link note-more\" aria-expanded=\"false\" aria-controls=\"note-card-body-{id}\" hidden>more</button>\n\
                 {s32}<button type=\"button\" class=\"note-link note-edit\" data-note-id=\"{id}\" aria-label=\"Edit note on {reference}\">Edit</button>\n\
                 {s28}</span>\n\
                 {s24}</article>",
                id = n.id,
                created = escape_html(&n.created_at),
                display = escape_html(&n.created_at_display),
                body = escape_html(&n.body),
            )
        })
        .collect();
    let all = if shown.len() < notes.len() {
        format!(
            r#"<button type="button" class="note-link note-show-all">Show all {} notes</button>"#,
            notes.len()
        )
    } else {
        String::new()
    };
    format!(
        r#"<p class="verse-notes-title">Notes ({})</p>{cards}{all}"#,
        notes.len()
    )
}

/// The passage the note editor quotes: its verses as this translation
/// (`code`) numbers and shows them (KJV supplied words in italics, no note
/// markers), and, when `missing` of its verses aren't numbered there, a
/// line saying so.
pub fn note_passage_html(verses: &[&Verse], missing: i64, code: &str) -> String {
    let mut html = verses
        .iter()
        .map(|v| {
            format!(
                r#"<span class="note-passage-num">{}</span>{}"#,
                v.number,
                verse_html(v, false)
            )
        })
        .collect::<Vec<_>>()
        .join(" ");
    if missing > 0 {
        let lead = if verses.is_empty() {
            "It isn’t"
        } else {
            "Not all of it is"
        };
        html.push_str(&format!(
            " <em>{lead} numbered in the {}.</em>",
            escape_html(&code.to_uppercase())
        ));
    }
    html
}

// ---- What things are called, and what the status line says.

/// "John 3:16" or "John 3:16–18": the selection's (or a note's) passage.
pub fn reference(book_name: &str, chapter: i64, start: i64, end: i64) -> String {
    passage_reference(book_name, chapter, Some((start, end)))
}

/// "John 3:16 (WEB)", or "John 3:16–18 (WEB)" for a range.
pub fn citation(book_name: &str, chapter: i64, start: i64, end: i64, code: &str) -> String {
    format!(
        "{} ({})",
        reference(book_name, chapter, start, end),
        code.to_uppercase()
    )
}

/// Copy as…'s label: what it copies.
pub fn copy_menu_label(what: &str) -> String {
    format!("Copy {what} as")
}

/// Said once the text is on the clipboard: what was copied, and how.
pub fn copied(reference: &str, format: &CopyFormat) -> String {
    format!("Copied {reference}{}", format.status)
}

/// Said when the clipboard turned the text down (or it couldn't be read).
pub fn copy_failed(error: &str) -> String {
    format!("Couldn't copy: {error}")
}

/// Said once a bookmark is saved or removed, or when it couldn't be.
pub fn bookmark_done(removed: bool) -> &'static str {
    if removed {
        "Bookmark removed"
    } else {
        "Bookmarked"
    }
}

pub fn bookmark_failed(removing: bool, error: &str) -> String {
    let what = if removing { "remove" } else { "save" };
    format!("Couldn't {what} bookmark: {error}")
}

/// Said once the selection is highlighted (`color`) or not (`None`), or
/// when it couldn't be.
pub fn highlight_done(color: Option<&str>) -> String {
    match color {
        Some(color) => format!("Highlighted {color}"),
        None => "Highlight removed".to_string(),
    }
}

pub fn highlight_failed(color: Option<&str>, error: &str) -> String {
    let what = if color.is_some() {
        "highlight"
    } else {
        "remove the highlight"
    };
    format!("Couldn't {what}: {error}")
}

/// Said in the Notes section when the selection's notes couldn't be read.
pub fn notes_failed(error: &str) -> String {
    format!("Notes couldn’t be loaded: {error}")
}

/// Said once the note editor has done something.
pub const NOTE_DELETED: &str = "Note deleted · ";
pub const NOTE_SAVED: &str = "Note saved";
pub const NOTE_ADDED: &str = "Note added";
pub const NOTE_RESTORED: &str = "Note restored";
pub const EDITOR_UNAVAILABLE: &str = "The note editor isn’t available.";

/// Said in the toast when a note is deleted with the card gone.
pub fn note_deleted_elsewhere(reference: &str) -> String {
    format!("Deleted the note on {reference}.")
}

pub fn restore_failed(error: &str) -> String {
    format!("Couldn’t restore the note: {error}")
}

// ---- The selection.

/// The reader's verse selection: a contiguous, inclusive range of verse
/// numbers in one translation column of the chapter on screen. `anchor` is
/// the verse chosen first, which Shift-click (and Shift+j/k) extends from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    pub translation_code: String,
    pub anchor: i64,
    pub start: i64,
    pub end: i64,
}

impl Selection {
    /// From `anchor` to `focus`, either way round.
    pub fn new(translation_code: &str, anchor: i64, focus: i64) -> Self {
        Self {
            translation_code: translation_code.to_string(),
            anchor,
            start: anchor.min(focus),
            end: anchor.max(focus),
        }
    }

    /// Whether `number` in the column `code` is selected.
    pub fn covers(&self, code: &str, number: i64) -> bool {
        self.translation_code == code && number >= self.start && number <= self.end
    }
}

/// What a click on a verse does to the selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    Select(Selection),
    Clear,
    Keep,
}

/// A plain click on verse `number` of the column `code`: selects it, or
/// clears it if it was the one verse selected.
pub fn toggle(selection: Option<&Selection>, code: &str, number: i64) -> Change {
    match selection {
        Some(s) if s.translation_code == code && s.start == number && s.end == number => {
            Change::Clear
        }
        _ => Change::Select(Selection::new(code, number, number)),
    }
}

/// A Shift-click on verse `number` of the column `code`: selects it if
/// nothing is, extends the selection from its anchor in the same column,
/// and leaves a selection in another column as it is.
pub fn extend(selection: Option<&Selection>, code: &str, number: i64) -> Change {
    match selection {
        None => Change::Select(Selection::new(code, number, number)),
        Some(s) if s.translation_code == code => {
            Change::Select(Selection::new(code, s.anchor, number))
        }
        Some(_) => Change::Keep,
    }
}

/// The highlight controls for the selected verses' colors (`None` where a
/// verse has none): whether each of [`HIGHLIGHT_COLORS`]'s swatches is
/// pressed (the whole selection has its color), and whether Remove
/// highlight shows (any selected verse has one).
pub fn highlight_controls(colors: &[Option<&str>]) -> ([bool; 4], bool) {
    let pressed = HIGHLIGHT_COLORS
        .map(|swatch| !colors.is_empty() && colors.iter().all(|c| *c == Some(swatch)));
    (pressed, colors.iter().any(Option::is_some))
}

// ---- Where the card goes.
//
// Measured each time it opens or its contents change (not on scroll: it's
// placed in the reading column, so it moves with the verses):
// - under the last selected verse, if it fits in the window below it;
// - otherwise above the first selected verse, if it fits there;
// - otherwise on whichever side has more room, scrolling inside a shorter
//   card;
// - and if neither side has CARD_MIN px (a selection taller than the
//   window), docked to the bottom of the window, across the reading
//   column.
// It never covers the selected verses unless docked, and stays inside the
// reading column: as wide as it, up to CARD_WIDTH, at the selected verse's
// left edge where there's room (the selected column, while comparing). In
// the Flowing layout a verse is a run of lines that can start and end
// mid-line, so "under" is under the last selected verse's last line,
// "above" is above the first one's first line, and the card starts at the
// column's left edge.

pub const CARD_GAP: f64 = 8.0;
pub const CARD_MIN: f64 = 120.0;
pub const CARD_WIDTH: f64 = 544.0;

/// A box on screen, as `getBoundingClientRect` measures it.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub left: f64,
    pub top: f64,
    pub bottom: f64,
    pub width: f64,
}

/// Where the card was, which a preview opening or closing keeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Below,
    Above,
    Docked,
}

/// JavaScript's `Math.min` and `Math.max`: NaN if either is.
fn js_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else {
        a.min(b)
    }
}

fn js_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else {
        a.max(b)
    }
}

/// JavaScript's truthiness of a number: not 0 and not NaN.
fn truthy(n: f64) -> bool {
    n != 0.0 && !n.is_nan()
}

/// The card's width and left edge in its column (`column`), from the first
/// selected verse's first line (`first`).
pub fn card_box(column: Rect, first: Rect, flowing: bool) -> (f64, f64) {
    let narrow = js_min(column.width, CARD_WIDTH);
    let width = if truthy(narrow) { narrow } else { column.width };
    let left = if flowing {
        0.0
    } else {
        js_max(0.0, js_min(first.left - column.left, column.width - width))
    };
    (width, left)
}

/// What [`placement`] decided: the side, and the card's height cap if it
/// has to scroll inside (never when docked).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    pub side: Side,
    pub max_height: Option<f64>,
}

/// The measurements [`placement`] decides from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Room {
    /// The card's height as it would be, up to its stylesheet's cap.
    pub wanted: f64,
    /// The window's height, and the top bar's bottom edge.
    pub window: f64,
    pub bar: f64,
    /// The first selected verse's first line, and the last one's last line.
    pub first: Rect,
    pub last: Rect,
}

/// Which side the card goes on, and how tall it may be; `keep` is the side
/// it's on while a preview opens or closes in it.
pub fn placement(room: &Room, keep: Option<Side>) -> Placement {
    let below = room.window - room.last.bottom - 2.0 * CARD_GAP;
    let above = room.first.top - room.bar - 2.0 * CARD_GAP;
    let wanted = room.wanted;
    let mut side = Side::Below;
    let mut space = wanted;
    match keep {
        Some(kept @ (Side::Above | Side::Below)) => {
            side = kept;
            let there = if kept == Side::Above { above } else { below };
            space = js_min(wanted, js_max(there, CARD_MIN));
        }
        _ if below < wanted => {
            if above >= wanted {
                side = Side::Above;
            } else {
                side = if above > below {
                    Side::Above
                } else {
                    Side::Below
                };
                space = js_max(above, below);
            }
        }
        _ => {}
    }
    if keep == Some(Side::Docked) || (keep.is_none() && space < js_min(wanted, CARD_MIN)) {
        return Placement {
            side: Side::Docked,
            max_height: None,
        };
    }
    Placement {
        side,
        max_height: (space < wanted).then_some(space),
    }
}

/// The card's top in its column: under the last line, or above the first
/// line by its own height (`height`, once capped).
pub fn card_top(side: Side, column: Rect, room: &Room, height: f64) -> f64 {
    match side {
        Side::Above => room.first.top - column.top - CARD_GAP - height,
        _ => room.last.bottom - column.top + CARD_GAP,
    }
}

/// A CSS length in pixels, the number written as JavaScript writes it in
/// a template (`-0` as `0`).
pub fn px(n: f64) -> String {
    if n == 0.0 {
        return "0px".to_string();
    }
    if n.is_infinite() {
        let sign = if n < 0.0 { "-" } else { "" };
        return format!("{sign}Infinitypx");
    }
    format!("{n}px")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::verse_parts::{Part, PartKind};

    fn verse(number: i64, text: &str) -> Verse {
        Verse {
            number,
            text: text.to_string(),
            parts: None,
            footnotes: Vec::new(),
            xrefs: Vec::new(),
        }
    }

    fn footnote(label: &str, lemma: Option<&str>, rest: &str) -> Footnote {
        Footnote {
            label: label.to_string(),
            lemma: lemma.map(str::to_string),
            rest: rest.to_string(),
            text: rest.to_string(),
            segments: None,
        }
    }

    fn xref(book_id: i64, chapter: i64, verse: i64, end: Option<i64>, citation: &str) -> Xref {
        Xref {
            book_id,
            chapter,
            verse,
            end_verse: end,
            citation: citation.to_string(),
        }
    }

    fn note(id: i64, body: &str) -> SelectionNote {
        SelectionNote {
            id,
            book_id: 43,
            chapter: 3,
            verse_start: 16,
            verse_end: 16,
            reference: "John 3:16".to_string(),
            body: body.to_string(),
            created_at: "2026-10-09T12:00:00Z".to_string(),
            created_at_display: "Oct 9, 2026".to_string(),
        }
    }

    #[test]
    fn the_actions_are_the_pages_markup() {
        let html = actions_html();
        assert!(html.starts_with(
            r#"<button type="button" class="verse-copy-btn" title="Copy (Ctrl+C)">Copy</button>
                <button type="button" class="verse-copy-as-btn""#
        ));
        assert!(html.contains(
            "aria-label=\"Highlight\">\n                    <button type=\"button\" class=\"highlight-swatch\" data-color=\"yellow\" aria-label=\"Highlight yellow\" title=\"Highlight yellow\" aria-pressed=\"false\"></button><button type=\"button\" class=\"highlight-swatch\" data-color=\"green\""
        ));
        assert!(html.ends_with(
            "                <span class=\"verse-actions-ref\"></span>\n                <span class=\"verse-actions-status\" role=\"status\" aria-live=\"polite\"></span>"
        ));
        assert_eq!(html.matches("highlight-swatch").count(), 4);
        assert_eq!(bookmark_label(false), "&#9733; Bookmark");
        assert_eq!(bookmark_label(true), "&#9733; Remove bookmark");
    }

    #[test]
    fn copy_as_lists_the_four_formats() {
        assert_eq!(
            copy_menu_html(),
            r#"<button type="button" class="copy-as-item" role="menuitem" tabindex="-1" data-format="reference">Text with reference</button><button type="button" class="copy-as-item" role="menuitem" tabindex="-1" data-format="numbered">With verse numbers</button><button type="button" class="copy-as-item" role="menuitem" tabindex="-1" data-format="markdown">Markdown quote</button><button type="button" class="copy-as-item" role="menuitem" tabindex="-1" data-format="plain">Plain text</button>"#
        );
        let markdown = copy_format("markdown").expect("markdown");
        assert_eq!(
            copied("John 3:16–18", markdown),
            "Copied John 3:16–18 as a Markdown quote"
        );
        assert_eq!(
            copied("John 3", copy_format("reference").expect("reference")),
            "Copied John 3"
        );
        assert!(copy_format("rtf").is_none());
        assert_eq!(copy_failed("denied"), "Couldn't copy: denied");
        assert_eq!(Slot::named("chapter"), Some(Slot::Chapter));
        assert_eq!(Slot::named("book"), None);
    }

    #[test]
    fn citations_name_the_passage_and_translation() {
        assert_eq!(citation("John", 3, 16, 16, "web"), "John 3:16 (WEB)");
        assert_eq!(citation("John", 3, 16, 18, "kjv"), "John 3:16–18 (KJV)");
        assert_eq!(reference("1 John", 4, 9, 10), "1 John 4:9–10");
        assert_eq!(copy_menu_label("John 3"), "Copy John 3 as");
    }

    #[test]
    fn footnotes_are_labeled_by_verse_in_a_range() {
        let v = verse(1, "In the beginning");
        let f = footnote("a", Some("lamp"), ": or, candle");
        assert_eq!(
            footnotes_html(&[(&v, &f)], false),
            "<span class=\"xref-popup-title\">Footnotes</span>\n                        <span class=\"footnote-entry\"><span class=\"footnote-label\">a</span><em class=\"footnote-lemma\">lamp</em>: or, candle</span>"
        );
        assert!(
            footnotes_html(&[(&v, &f)], true).contains(r#"<span class="footnote-label">1a</span>"#)
        );
        assert_eq!(footnotes_html(&[], true), "");
    }

    #[test]
    fn cross_references_are_chips_by_verse() {
        let xrefs = [
            xref(43, 3, 16, None, "John 3:16"),
            xref(62, 4, 9, Some(10), "1 John 4:9-10 <&>"),
        ];
        let html = xrefs_html(&[(16, &xrefs)], 3, false);
        assert_eq!(
            html,
            "<span class=\"xref-popup-title\" id=\"selection-xrefs-title\">Cross references</span>\n                        <div class=\"selection-xref-row\">\n                            \n                            <ul class=\"xref-chips\" aria-labelledby=\"selection-xrefs-title\"><li><button type=\"button\" class=\"xref-citation\" data-book=\"43\" data-chapter=\"3\" data-verse=\"16\" aria-expanded=\"false\" aria-controls=\"xref-preview\">John 3:16</button></li><li><button type=\"button\" class=\"xref-citation\" data-book=\"62\" data-chapter=\"4\" data-verse=\"9\" data-end-verse=\"10\" aria-expanded=\"false\" aria-controls=\"xref-preview\">1 John 4:9-10 &lt;&amp;&gt;</button></li></ul>\n                        </div>"
        );
        assert!(xrefs_html(&[(16, &xrefs)], 3, true)
            .contains("<div class=\"selection-xref-row\">\n                            <span class=\"selection-xref-verse\">3:16</span>\n"));
        assert_eq!(xrefs_html(&[], 3, true), "");
    }

    #[test]
    fn a_chips_citation_gets_its_dash() {
        assert_eq!(chip_reference("1 John 4:9-10"), "1 John 4:9–10");
        assert_eq!(chip_reference("Gen 1:1-2, 3-4"), "Gen 1:1–2, 3-4");
        assert_eq!(chip_reference("Jude 1:3"), "Jude 1:3");
        assert_eq!(chip_reference("A-B 1-"), "A-B 1-");
    }

    #[test]
    fn the_preview_reads_the_passage_with_go_to() {
        let mut v = verse(9, "In this was manifested");
        v.parts = Some(vec![
            Part {
                kind: PartKind::Text,
                text: "In this ".to_string(),
                label: None,
            },
            Part {
                kind: PartKind::Supplied,
                text: "was".to_string(),
                label: None,
            },
            Part {
                kind: PartKind::Note,
                text: String::new(),
                label: Some("a".to_string()),
            },
        ]);
        let verses = [v, verse(10, "Herein is love")];
        let html = preview_html(&Preview {
            reference: "1 John 4:9–10",
            code: "kjv",
            verses: &verses,
            notice: "",
            book_id: 62,
            chapter: 4,
            start: 9,
            end: 10,
        });
        assert_eq!(
            html,
            "<p class=\"xref-preview-title\" id=\"xref-preview-title\">1 John 4:9–10 (KJV)</p>\n                    <p class=\"xref-preview-text\"><span class=\"xref-preview-verse\"><span class=\"xref-preview-num\">9</span>In this <em class=\"supplied\">was</em></span> <span class=\"xref-preview-verse\"><span class=\"xref-preview-num\">10</span>Herein is love</span></p>\n                    \n                    <button type=\"button\" class=\"xref-preview-go\" data-book=\"62\" data-chapter=\"4\" data-verse=\"9\" data-end-verse=\"10\">Go to 1 John 4:9–10 <span aria-hidden=\"true\">→</span></button>"
        );
        let failed = preview_failed("Jude 1:3", "no such book");
        let html = preview_html(&Preview {
            reference: "Jude 1:3",
            code: "web",
            verses: &[],
            notice: &failed,
            book_id: 65,
            chapter: 1,
            start: 3,
            end: 3,
        });
        assert!(html.contains(
            "(WEB)</p>\n                    \n                    <p class=\"xref-preview-notice\">Couldn&#39;t read Jude 1:3: no such book</p>\n"
        ));
    }

    #[test]
    fn notes_show_three_then_show_all() {
        let notes: Vec<_> = (1..=4).map(|i| note(i, &format!("Note <{i}>"))).collect();
        let html = notes_html(&notes, "", false);
        assert!(html.starts_with(r#"<p class="verse-notes-title">Notes (4)</p><article class="note-card" data-note-id="1">"#));
        assert_eq!(html.matches("<article").count(), 3);
        assert!(html.ends_with(r#"</article><button type="button" class="note-link note-show-all">Show all 4 notes</button>"#));
        assert!(html.contains(
            "<article class=\"note-card\" data-note-id=\"1\">\n                            <p class=\"note-card-head\"><span class=\"note-card-ref\">John 3:16</span> <time class=\"note-card-date\" datetime=\"2026-10-09T12:00:00Z\">Oct 9, 2026</time></p>\n                            <p class=\"note-card-body\" id=\"note-card-body-1\">Note &lt;1&gt;</p>\n                            <span class=\"note-card-actions\">\n                                <button type=\"button\" class=\"note-link note-more\" aria-expanded=\"false\" aria-controls=\"note-card-body-1\" hidden>more</button>\n                                <button type=\"button\" class=\"note-link note-edit\" data-note-id=\"1\" aria-label=\"Edit note on John 3:16\">Edit</button>\n                            </span>\n                        </article>"
        ));
        let all = notes_html(&notes, "", true);
        assert_eq!(all.matches("<article").count(), 4);
        assert!(!all.contains("note-show-all"));
        assert_eq!(
            notes_html(&notes, "Notes couldn’t be loaded: <gone>", false),
            r#"<p class="verse-notes-title">Notes</p><p class="note-card-body is-expanded">Notes couldn’t be loaded: &lt;gone&gt;</p>"#
        );
        assert_eq!(notes_failed("gone"), "Notes couldn’t be loaded: gone");
    }

    #[test]
    fn the_editors_quote_says_what_isnt_numbered() {
        let (a, b) = (verse(20, "Behold, I stand"), verse(21, "To him"));
        assert_eq!(
            note_passage_html(&[&a, &b], 0, "kjv"),
            r#"<span class="note-passage-num">20</span>Behold, I stand <span class="note-passage-num">21</span>To him"#
        );
        assert_eq!(
            note_passage_html(&[&a], 1, "bsb"),
            r#"<span class="note-passage-num">20</span>Behold, I stand <em>Not all of it is numbered in the BSB.</em>"#
        );
        assert_eq!(
            note_passage_html(&[], 1, "bsb"),
            " <em>It isn’t numbered in the BSB.</em>"
        );
    }

    #[test]
    fn clicks_select_toggle_and_extend() {
        let one = Selection::new("kjv", 5, 5);
        assert_eq!(toggle(None, "kjv", 5), Change::Select(one.clone()));
        assert_eq!(toggle(Some(&one), "kjv", 5), Change::Clear);
        assert_eq!(
            toggle(Some(&one), "web", 5),
            Change::Select(Selection::new("web", 5, 5))
        );
        let range = Selection::new("kjv", 5, 3);
        assert_eq!((range.start, range.end, range.anchor), (3, 5, 5));
        assert_eq!(
            toggle(Some(&range), "kjv", 3),
            Change::Select(Selection::new("kjv", 3, 3))
        );
        assert_eq!(
            extend(None, "kjv", 7),
            Change::Select(Selection::new("kjv", 7, 7))
        );
        assert_eq!(
            extend(Some(&one), "kjv", 2),
            Change::Select(Selection::new("kjv", 5, 2))
        );
        assert_eq!(extend(Some(&one), "web", 2), Change::Keep);
        assert!(range.covers("kjv", 4) && !range.covers("web", 4) && !range.covers("kjv", 6));
    }

    #[test]
    fn highlight_controls_follow_the_selection() {
        assert_eq!(highlight_controls(&[]), ([false; 4], false));
        assert_eq!(
            highlight_controls(&[Some("green"), Some("green")]),
            ([false, true, false, false], true)
        );
        assert_eq!(
            highlight_controls(&[Some("green"), None]),
            ([false; 4], true)
        );
        assert_eq!(highlight_controls(&[None]), ([false; 4], false));
        assert_eq!(highlight_done(Some("pink")), "Highlighted pink");
        assert_eq!(highlight_done(None), "Highlight removed");
        assert_eq!(highlight_failed(Some("pink"), "x"), "Couldn't highlight: x");
        assert_eq!(
            highlight_failed(None, "x"),
            "Couldn't remove the highlight: x"
        );
        assert_eq!(bookmark_failed(true, "x"), "Couldn't remove bookmark: x");
        assert_eq!(bookmark_failed(false, "x"), "Couldn't save bookmark: x");
    }

    fn rect(left: f64, top: f64, bottom: f64, width: f64) -> Rect {
        Rect {
            left,
            top,
            bottom,
            width,
        }
    }

    #[test]
    fn the_card_sits_at_the_verse_inside_its_column() {
        let column = rect(100.0, 0.0, 2000.0, 700.0);
        assert_eq!(
            card_box(column, rect(150.0, 0.0, 0.0, 0.0), false),
            (544.0, 50.0)
        );
        // Never past the column's right edge, nor left of it.
        assert_eq!(
            card_box(column, rect(500.0, 0.0, 0.0, 0.0), false),
            (544.0, 156.0)
        );
        assert_eq!(
            card_box(column, rect(50.0, 0.0, 0.0, 0.0), false),
            (544.0, 0.0)
        );
        assert_eq!(
            card_box(column, rect(500.0, 0.0, 0.0, 0.0), true),
            (544.0, 0.0)
        );
        assert_eq!(
            card_box(rect(0.0, 0.0, 0.0, 300.0), rect(0.0, 0.0, 0.0, 0.0), false),
            (300.0, 0.0)
        );
        // No layout (jsdom): nothing measured.
        assert_eq!(
            card_box(Rect::default(), Rect::default(), false),
            (0.0, 0.0)
        );
    }

    fn room(wanted: f64, first_top: f64, last_bottom: f64) -> Room {
        Room {
            wanted,
            window: 800.0,
            bar: 50.0,
            first: rect(0.0, first_top, first_top + 20.0, 0.0),
            last: rect(0.0, last_bottom - 20.0, last_bottom, 0.0),
        }
    }

    #[test]
    fn below_then_above_then_the_roomier_side_then_docked() {
        let below = Placement {
            side: Side::Below,
            max_height: None,
        };
        assert_eq!(placement(&room(200.0, 100.0, 300.0), None), below);
        assert_eq!(
            placement(&room(200.0, 500.0, 700.0), None),
            Placement {
                side: Side::Above,
                max_height: None
            }
        );
        // 450 above and 184 below: above, scrolling inside.
        assert_eq!(
            placement(&room(600.0, 516.0, 600.0), None),
            Placement {
                side: Side::Above,
                max_height: Some(450.0)
            }
        );
        // A selection taller than the window: docked.
        assert_eq!(
            placement(&room(300.0, 100.0, 750.0), None).side,
            Side::Docked
        );
        // A short card never docks for want of CARD_MIN it doesn't need.
        assert_eq!(placement(&room(40.0, 60.0, 740.0), None).side, Side::Below);
        // A preview keeps the card where it is.
        assert_eq!(
            placement(&room(600.0, 100.0, 300.0), Some(Side::Above)),
            Placement {
                side: Side::Above,
                max_height: Some(120.0)
            }
        );
        assert_eq!(
            placement(&room(200.0, 500.0, 700.0), Some(Side::Below)).side,
            Side::Below
        );
        assert_eq!(
            placement(&room(200.0, 100.0, 300.0), Some(Side::Docked)).side,
            Side::Docked
        );
        let r = room(200.0, 500.0, 700.0);
        let column = rect(0.0, 40.0, 0.0, 0.0);
        assert_eq!(card_top(Side::Below, column, &r, 200.0), 668.0);
        assert_eq!(card_top(Side::Above, column, &r, 200.0), 252.0);
    }

    #[test]
    fn lengths_are_written_as_javascript_writes_them() {
        assert_eq!(px(544.0), "544px");
        assert_eq!(px(12.5), "12.5px");
        assert_eq!(px(-0.0), "0px");
        assert_eq!(px(-3.25), "-3.25px");
        assert_eq!(px(0.1 + 0.2), "0.30000000000000004px");
    }
}
