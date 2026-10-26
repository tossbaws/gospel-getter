//! The reader's words, markup and rules, free of the DOM: the chapter as
//! markup (its verses, the faded neighbors or the Bible's edge cards,
//! Compare's grid, the chapter's end, its Footnotes and Cross references),
//! a verse's and a footnote's text as markup, the translations' records,
//! what the reference button and the window title say, which translations
//! Compare fetches, and the passage history's bookkeeping. `reader` puts
//! them on the page.
//!
//! The markup is the page's former templates' (`ui/index.html` at
//! 4a7f6f5: `verseHtml` to `renderReadingPane`), byte for byte, spaces and
//! line breaks included (the page rebuilds the pane from it, and the
//! Flowing layout shows the line break between verses as a space). Every
//! word of scripture is the backend's, as it sent it: escaped, set in
//! italics or raised where the backend's parts say, never changed.

use crate::dto::{ChapterSide, Compare, Edge, Reading, Translation, Verse, Xref};
use crate::passage_reference::verse_range;
use crate::verse_parts::{Footnote, PartKind, SegmentKind};

// ---- Text as markup.

/// `text` escaped for markup, as the page's `escapeHtml`: `&`, `<`, `>`,
/// `"` and `'`.
pub fn escape_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    push_escaped(&mut out, text);
    out
}

fn push_escaped(out: &mut String, text: &str) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
}

/// A verse's text as the backend shaped it (`verse_parts`): KJV supplied
/// words in italics and, with `markers`, each margin note as its
/// footnote's letter where it stood. Every part is escaped. A verse
/// without parts is its text.
pub fn verse_html(verse: &Verse, markers: bool) -> String {
    let Some(parts) = &verse.parts else {
        return escape_html(&verse.text);
    };
    let mut out = String::new();
    for part in parts {
        match part.kind {
            PartKind::Supplied => {
                out.push_str(r#"<em class="supplied">"#);
                push_escaped(&mut out, &part.text);
                out.push_str("</em>");
            }
            PartKind::Note if markers => {
                out.push_str(r#"<sup class="fn-marker" aria-hidden="true">"#);
                push_escaped(&mut out, part.label.as_deref().unwrap_or("undefined"));
                out.push_str("</sup>");
            }
            PartKind::Note => {}
            PartKind::Text => push_escaped(&mut out, &part.text),
        }
    }
    out
}

/// A footnote's note as printed footnotes set it: the verse's words it's
/// on (its lemma) in italics, then the rest as stored, "*lamp*: or,
/// candle"; without a lemma, all plain. A USFM note (the BSB's) comes in
/// runs instead: its words, its alternative readings in italics, and verse
/// numbers set small and raised, as the reader's are.
pub fn footnote_html(footnote: &Footnote) -> String {
    let mut out = String::new();
    if let Some(segments) = &footnote.segments {
        for segment in segments {
            match segment.kind {
                SegmentKind::Alt => {
                    out.push_str(r#"<em class="footnote-alt">"#);
                    push_escaped(&mut out, &segment.text);
                    out.push_str("</em>");
                }
                SegmentKind::Verse => {
                    out.push_str(r#"<sup class="footnote-verse">"#);
                    push_escaped(&mut out, &segment.text);
                    out.push_str("</sup>");
                }
                SegmentKind::Text => push_escaped(&mut out, &segment.text),
            }
        }
        return out;
    }
    if let Some(lemma) = footnote.lemma.as_deref().filter(|l| !l.is_empty()) {
        out.push_str(r#"<em class="footnote-lemma">"#);
        push_escaped(&mut out, lemma);
        out.push_str("</em>");
    }
    push_escaped(&mut out, &footnote.rest);
    out
}

// ---- The chapter's parts.

/// Faded neighbors show supplied words in italics but no note markers:
/// their footnotes are listed only for the chapter being read. Only the
/// heading is a control.
fn side_chapter(side: &ChapterSide) -> String {
    let verses = side
        .verses
        .iter()
        .map(|v| {
            format!(
                r#"<p class="verse" data-book="{}" data-chapter="{}" data-verse="{n}"><span class="verse-num">{n}</span><span class="verse-text">{}</span></p>"#,
                side.book_id,
                side.chapter,
                verse_html(v, false),
                n = v.number,
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "<section>\n{s20}<button type=\"button\" class=\"chapter-heading-link\" data-book=\"{book}\" data-chapter=\"{chapter}\">{name} {chapter}</button>\n{s20}<div class=\"chapter-text\">{verses}</div>\n{s16}</section>",
        book = side.book_id,
        chapter = side.chapter,
        name = escape_html(&side.book_name),
        s20 = spaces(20),
        s16 = spaces(16),
    )
}

/// Before Genesis 1 and after Revelation 22 there's no neighboring
/// chapter: its place holds a quiet card, the backend's words about the
/// Bible's shape. No scripture text.
fn edge_card(edge: Option<&Edge>) -> String {
    let Some(edge) = edge else {
        return String::new();
    };
    let detail = edge
        .detail
        .as_deref()
        .filter(|d| !d.is_empty())
        .map(|d| format!(r#"<p class="edge-card-detail">{}</p>"#, escape_html(d)))
        .unwrap_or_default();
    format!(
        "<div class=\"edge-card\">\n{s20}<p class=\"edge-card-title\">{}</p>\n{s20}{detail}\n{s16}</div>",
        escape_html(&edge.title),
        s20 = spaces(20),
        s16 = spaces(16),
    )
}

/// One selectable verse of the chapter being read, in the column `code`.
/// Cross-references are keyed by verse number, so each column shows the
/// same ones for the same number; footnotes are the translation's own.
fn verse_row(verse: &Verse, book_id: i64, chapter: i64, code: &str, xrefs: &[Xref]) -> String {
    let has_xref = if xrefs.is_empty() { "" } else { " has-xref" };
    let (has_footnotes, cue) = if verse.footnotes.is_empty() {
        ("", "")
    } else {
        (
            " has-footnotes",
            r#"<span class="footnote-cue visually-hidden">(has footnotes) </span>"#,
        )
    };
    format!(
        r#"<p class="verse{has_xref}{has_footnotes}" data-book="{book_id}" data-chapter="{chapter}" data-verse="{n}" data-translation="{code}" data-code="{upper}"><span class="verse-num">{n}</span>{cue}<span class="verse-text">{text}</span></p>"#,
        n = verse.number,
        code = escape_html(code),
        upper = escape_html(&code.to_uppercase()),
        text = verse_html(verse, true),
    )
}

/// The chapter's two disclosures, each remembering being opened or closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Disclosure {
    Footnotes,
    Xrefs,
}

impl Disclosure {
    /// Its name in `data-disclosure`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Footnotes => "footnotes",
            Self::Xrefs => "xrefs",
        }
    }

    /// Its local storage key; the value is `open` or `closed`.
    pub fn key(self) -> &'static str {
        match self {
            Self::Footnotes => "gospel-getter-footnotes",
            Self::Xrefs => "gospel-getter-cross-references",
        }
    }

    /// The one `data-disclosure` names.
    pub fn named(name: &str) -> Option<Self> {
        match name {
            "footnotes" => Some(Self::Footnotes),
            "xrefs" => Some(Self::Xrefs),
            _ => None,
        }
    }
}

/// Whether each disclosure is open, as stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Opened {
    pub footnotes: bool,
    pub xrefs: bool,
}

fn disclosure_toggle(kind: Disclosure, controls: &str, label: &str, open: bool) -> String {
    format!(
        r#"<button type="button" class="disclosure-toggle" data-disclosure="{}" aria-expanded="{open}" aria-controls="{controls}"><span class="disclosure-chevron" aria-hidden="true"></span>{label}</button>"#,
        kind.name(),
    )
}

/// The keys' hint's arrows, drawn: the UI sans at the hint's size falls
/// back to a face whose arrows are tiny.
const ARROW_LEFT: &str = r#"<svg viewBox="0 0 16 16" aria-hidden="true" focusable="false"><path d="M13 8H3M7 4 3 8l4 4"/></svg>"#;
const ARROW_RIGHT: &str = r#"<svg viewBox="0 0 16 16" aria-hidden="true" focusable="false"><path d="M3 8h10M9 4l4 4-4 4"/></svg>"#;

/// A key cap in the hint; a drawn one is named for screen readers.
fn hint_key(name: &str, drawn: Option<&str>) -> String {
    match drawn {
        Some(drawn) => {
            format!(r#"<kbd><span class="visually-hidden">{name}</span>{drawn}</kbd>"#)
        }
        None => format!("<kbd>{name}</kbd>"),
    }
}

/// Which way a chapter step goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Which {
    Prev,
    Next,
}

impl Which {
    /// As the page names it (`prev`, `next`).
    pub fn name(self) -> &'static str {
        match self {
            Self::Prev => "prev",
            Self::Next => "next",
        }
    }
}

/// The chapter's next step, after its last verse: the chapters before and
/// after it (the backend's, so books are crossed) as two large buttons,
/// one at Genesis 1 and at Revelation 22; the keys' hint; Copy chapter….
fn chapter_end(dto: &Reading) -> String {
    let step = |side: Option<&ChapterSide>, which: Which| {
        let Some(side) = side else {
            return String::new();
        };
        let name = format!("{} {}", escape_html(&side.book_name), side.chapter);
        let (label, text) = match which {
            Which::Prev => (
                format!("{name}, previous chapter"),
                format!(r#"<span aria-hidden="true">&larr;</span> {name}"#),
            ),
            Which::Next => (
                format!("{name}, next chapter"),
                format!(r#"{name} <span aria-hidden="true">&rarr;</span>"#),
            ),
        };
        format!(
            r#"<button type="button" class="chapter-step chapter-step-{}" data-book="{}" data-chapter="{}" aria-label="{label}">{text}</button>"#,
            which.name(),
            side.book_id,
            side.chapter,
        )
    };
    format!(
        "<nav class=\"chapter-end\" aria-label=\"Previous and next chapter\">{}{}</nav>\n{s20}<p class=\"nav-hint\">Use {} and {} to move a chapter at a time, and {} for all keyboard shortcuts.</p>\n{s20}<p class=\"chapter-copy\"><button type=\"button\" class=\"chapter-copy-btn\" aria-haspopup=\"menu\" aria-expanded=\"false\" aria-controls=\"copy-as-menu\">Copy chapter…</button></p>",
        step(dto.prev.as_ref(), Which::Prev),
        step(dto.next.as_ref(), Which::Next),
        hint_key("Left arrow", Some(ARROW_LEFT)),
        hint_key("Right arrow", Some(ARROW_RIGHT)),
        hint_key("?", None),
        s20 = spaces(20),
    )
}

/// A column on screen: its translation and verses.
#[derive(Debug, Clone, Copy)]
pub struct Column<'a> {
    pub code: &'a str,
    pub verses: &'a [Verse],
}

/// The footnotes of the chapter being read, one row each ("119:105a lamp:
/// or, candle"), for every column on screen that has any. Named for its
/// translation while comparing, where it could be either column's.
fn footnote_summary(chapter: i64, columns: &[Column<'_>], comparing: bool, open: bool) -> String {
    columns
        .iter()
        .map(|c| {
            let rows: String = c
                .verses
                .iter()
                .flat_map(|v| {
                    v.footnotes.iter().map(move |f| {
                        format!(
                            "<div class=\"footnote-row\">\n{s32}<span class=\"footnote-ref\">{chapter}:{}{}</span>\n{s32}<span class=\"footnote-text\">{}</span>\n{s28}</div>",
                            v.number,
                            escape_html(&f.label),
                            footnote_html(f),
                            s32 = spaces(32),
                            s28 = spaces(28),
                        )
                    })
                })
                .collect();
            if rows.is_empty() {
                return String::new();
            }
            let count: usize = c.verses.iter().map(|v| v.footnotes.len()).sum();
            let code = escape_html(&c.code.to_uppercase());
            let title = if comparing {
                format!("{code} footnotes ({count})")
            } else {
                format!("Footnotes ({count})")
            };
            let list = format!("footnote-list-{}", escape_html(c.code));
            format!(
                "<div class=\"footnote-summary\" data-code=\"{code}\">\n{s28}<h3 class=\"footnote-summary-title\">{}</h3>\n{s28}<div class=\"footnote-list disclosure-body\" id=\"{list}\"{}>{rows}</div>\n{s24}</div>",
                disclosure_toggle(Disclosure::Footnotes, &list, &title, open),
                if open { "" } else { " hidden" },
                s28 = spaces(28),
                s24 = spaces(24),
            )
        })
        .collect()
}

/// A cross-reference chip's verses: `data-end-verse` only for a range.
fn end_verse(xref: &Xref) -> String {
    xref.end_verse
        .map(|end| format!(r#" data-end-verse="{end}""#))
        .unwrap_or_default()
}

/// The cross-references' credit, with its links (which the backend opens).
const XREF_CREDIT: &str = r#"<p class="xref-credit">Cross-reference data from <a class="credit-link" data-link="openbible" href="https://www.openbible.info/labs/cross-references/">OpenBible.info</a>, licensed under <a class="credit-link" data-link="cc-by" href="https://creativecommons.org/licenses/by/4.0/">CC BY 4.0</a>.</p>"#;

/// The chapter's Cross references: each verse's chips, and the
/// OpenBible.info credit with its links (which the backend opens).
fn xref_summary(dto: &Reading, open: bool) -> String {
    if !dto.has_chapter_xrefs {
        return String::new();
    }
    let rows: String = dto
        .current_verses
        .iter()
        .filter(|v| !v.xrefs.is_empty())
        .map(|v| {
            let chips: String = v
                .xrefs
                .iter()
                .map(|x| {
                    format!(
                        r#"<span class="xref-summary-item"><button type="button" class="xref-summary-chip" data-book="{}" data-chapter="{}" data-verse="{}"{}>{}</button></span>"#,
                        x.book_id,
                        x.chapter,
                        x.verse,
                        end_verse(x),
                        escape_html(&x.citation),
                    )
                })
                .collect();
            format!(
                "<div class=\"xref-summary-row\">\n{s28}<span class=\"xref-summary-verse\">{}:{}</span>\n{s28}<span class=\"xref-summary-chips\">{chips}</span>\n{s24}</div>",
                dto.current_chapter,
                v.number,
                s28 = spaces(28),
                s24 = spaces(24),
            )
        })
        .collect();
    let count: usize = dto.current_verses.iter().map(|v| v.xrefs.len()).sum();
    format!(
        "<div class=\"xref-summary\">\n{s20}<h3 class=\"xref-summary-title\">{}</h3>\n{s20}<div class=\"xref-summary-list disclosure-body\" id=\"xref-summary-list\"{}>{rows}</div>\n{s20}{XREF_CREDIT}\n{s16}</div>",
        disclosure_toggle(
            Disclosure::Xrefs,
            "xref-summary-list",
            &format!("Cross references ({count})"),
            open,
        ),
        if open { "" } else { " hidden" },
        s20 = spaces(20),
        s16 = spaces(16),
    )
}

/// Compare's rows: one per verse number any column numbers, in order, with
/// each column's verse of that number, or `None` where it doesn't number
/// it. Nothing is merged, moved or renumbered to make the rows line up.
pub fn compare_rows<'a>(columns: &[&'a [Verse]]) -> Vec<(i64, Vec<Option<&'a Verse>>)> {
    let mut numbers: Vec<i64> = columns
        .iter()
        .flat_map(|verses| verses.iter().map(|v| v.number))
        .collect();
    numbers.sort_unstable();
    numbers.dedup();
    numbers
        .into_iter()
        .map(|n| {
            let cells = columns
                .iter()
                .map(|verses| verses.iter().find(|v| v.number == n))
                .collect();
            (n, cells)
        })
        .collect()
}

/// Compare: every translation side by side, one row per verse number; a
/// verse a translation doesn't number says so in its cell.
fn compare_grid(dto: &Reading, compare: &Compare) -> String {
    let heads: String = compare
        .columns
        .iter()
        .map(|c| {
            format!(
                r#"<div class="compare-head" role="columnheader">{} <span class="compare-head-code">({})</span></div>"#,
                escape_html(&c.name),
                escape_html(&c.code.to_uppercase()),
            )
        })
        .collect();
    let verses: Vec<&[Verse]> = compare
        .columns
        .iter()
        .map(|c| c.verses.as_slice())
        .collect();
    let rows: String = compare_rows(&verses)
        .into_iter()
        .flat_map(|(n, cells)| {
            cells
                .into_iter()
                .zip(&compare.columns)
                .map(move |(cell, column)| match cell {
                    Some(v) => verse_row(
                        v,
                        dto.current_book_id,
                        dto.current_chapter,
                        &column.code,
                        xrefs_of(dto, n),
                    ),
                    None => {
                        let upper = escape_html(&column.code.to_uppercase());
                        format!(
                            r#"<p class="compare-gap" data-gap-verse="{n}" data-code="{upper}"><span class="verse-num">{n}</span><span class="compare-gap-note">Not numbered in the {upper}</span></p>"#
                        )
                    }
                })
        })
        .collect();
    format!(
        r#"<div class="compare-grid" data-columns="{}">{heads}{rows}</div>"#,
        compare.columns.len()
    )
}

/// The cross-references of the chapter's verse `number` (the reading's,
/// the same in every column).
fn xrefs_of(dto: &Reading, number: i64) -> &[Xref] {
    dto.current_verses
        .iter()
        .find(|v| v.number == number)
        .map_or(&[], |v| v.xrefs.as_slice())
}

/// The whole reading pane: `#reading-pane-inner` with the chapter on
/// screen and its neighbors (`data-current-*`, `data-prev-*`,
/// `data-next-*`), the verses (in the translation `code`) or Compare's
/// grid, the chapter's end and its two lists. While comparing, the columns
/// take the neighbors' place.
pub fn reading_pane(dto: &Reading, compare: Option<&Compare>, code: &str, open: Opened) -> String {
    let attrs = |side: Option<&ChapterSide>, which: &str| {
        side.map(|s| {
            format!(
                r#" data-{which}-book="{}" data-{which}-chapter="{}""#,
                s.book_id, s.chapter
            )
        })
        .unwrap_or_default()
    };
    let body = match compare {
        Some(compare) => compare_grid(dto, compare),
        None => format!(
            r#"<div class="chapter-text">{}</div>"#,
            dto.current_verses
                .iter()
                .map(|v| verse_row(v, dto.current_book_id, dto.current_chapter, code, &v.xrefs))
                .collect::<Vec<_>>()
                .join("\n")
        ),
    };
    let side = |s: Option<&ChapterSide>, edge: Option<&Edge>, which: Which| {
        if compare.is_some() {
            return String::new();
        }
        let inner = match s {
            Some(s) => side_chapter(s),
            None => edge_card(edge),
        };
        format!(
            r#"<div class="chapter-side chapter-side-{}">{inner}</div>"#,
            which.name()
        )
    };
    let columns: Vec<Column<'_>> = match compare {
        Some(compare) => compare
            .columns
            .iter()
            .map(|c| Column {
                code: &c.code,
                verses: &c.verses,
            })
            .collect(),
        None => vec![Column {
            code,
            verses: &dto.current_verses,
        }],
    };
    format!(
        "<div id=\"reading-pane-inner\"\n{s21}data-current-book=\"{book}\"\n{s21}data-current-chapter=\"{chapter}\"{prev}{next}\n{s16}>\n{s20}<div class=\"reading-row{comparing}\">\n{s24}{before}\n{s24}<section class=\"chapter-main\">\n{s28}<h2 class=\"chapter-heading\">{name} {chapter}</h2>\n{s28}{body}\n{s28}{end}\n{s28}{footnotes}\n{s28}{xrefs}\n{s24}</section>\n{s24}{after}\n{s20}</div>\n{s16}</div>",
        book = dto.current_book_id,
        chapter = dto.current_chapter,
        prev = attrs(dto.prev.as_ref(), "prev"),
        next = attrs(dto.next.as_ref(), "next"),
        comparing = if compare.is_some() {
            " is-comparing"
        } else {
            ""
        },
        before = side(dto.prev.as_ref(), dto.prev_edge.as_ref(), Which::Prev),
        name = escape_html(&dto.current_book_name),
        end = chapter_end(dto),
        footnotes = footnote_summary(
            dto.current_chapter,
            &columns,
            compare.is_some(),
            open.footnotes
        ),
        xrefs = xref_summary(dto, open.xrefs),
        after = side(dto.next.as_ref(), dto.next_edge.as_ref(), Which::Next),
        s16 = spaces(16),
        s20 = spaces(20),
        s21 = spaces(21),
        s24 = spaces(24),
        s28 = spaces(28),
    )
}

fn spaces(n: usize) -> String {
    " ".repeat(n)
}

// ---- The translations and the reference.

/// The translation button's record, `#translation-select`'s options: each
/// translation (with its date, licence and credit for Aa and About), the
/// one being read selected.
pub fn translation_options(translations: &[Translation], current: &str) -> String {
    translations
        .iter()
        .map(|t| {
            let selected = if t.code == current { " selected" } else { "" };
            let date = t
                .date
                .as_deref()
                .filter(|d| !d.is_empty())
                .map(|d| format!(r#" data-date="{}""#, escape_html(d)))
                .unwrap_or_default();
            format!(
                r#"<option value="{}"{selected}{date} data-licence="{}" data-credit="{}">{}</option>"#,
                escape_html(&t.code),
                escape_html(&t.licence),
                escape_html(&t.credit),
                escape_html(&t.name),
            )
        })
        .collect()
}

/// What the reference button and the window title say.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference {
    /// `#ref-text`: "John 3 · KJV", or "John 3 · KJV + WEB" comparing.
    pub text: String,
    /// `#ref-btn`'s `aria-label`, the translations named in full.
    pub label: String,
    /// `document.title`, which the backend copies to the window.
    pub title: String,
}

/// Where the reader is: the chapter (`book_name` `chapter`) in the
/// translations `codes` on screen, each named as `name_of` names it.
pub fn reference(
    book_name: &str,
    chapter: i64,
    codes: &[&str],
    name_of: impl Fn(&str) -> String,
) -> Reference {
    let names: Vec<String> = codes.iter().map(|c| name_of(c)).collect();
    let place = format!("{book_name} {chapter}");
    let spoken = match names.split_last() {
        Some((last, rest)) if !rest.is_empty() => format!("{} and {last}", rest.join(", ")),
        Some((last, _)) => last.clone(),
        None => String::new(),
    };
    let upper: Vec<String> = codes.iter().map(|c| c.to_uppercase()).collect();
    let text = format!("{place} · {}", upper.join(" + "));
    Reference {
        label: format!("{place}, {spoken}. Choose a book and chapter"),
        title: format!("{text} — Gospel Getter"),
        text,
    }
}

/// The translation button's text and name: "KJV", "Translation: King James
/// Version"; without one, nothing and "Translation".
pub fn translation_button(code: &str, name: &str) -> (String, String) {
    if code.is_empty() {
        (String::new(), "Translation".to_string())
    } else {
        (code.to_uppercase(), format!("Translation: {name}"))
    }
}

// ---- Compare.

/// The translations Compare shows, left to right: the one being read, then
/// the one beside it, or just the one if there's no other.
pub fn compare_codes(current: &str, partner: Option<&str>) -> Vec<String> {
    match partner {
        Some(partner) => vec![current.to_string(), partner.to_string()],
        None => vec![current.to_string()],
    }
}

/// What a chapter is fetched for: Compare off (`""`), or on with these
/// translations ("kjv web").
pub fn compare_layout(comparing: bool, codes: &[String]) -> String {
    if comparing {
        codes.join(" ")
    } else {
        String::new()
    }
}

/// Whether `compare` is the chapter on screen (`book_id`, `chapter`) in
/// the translations `codes` Compare shows now.
pub fn compare_is_current(compare: &Compare, book_id: i64, chapter: i64, codes: &[String]) -> bool {
    compare.book_id == book_id
        && compare.chapter == chapter
        && compare
            .columns
            .iter()
            .map(|c| c.code.as_str())
            .eq(codes.iter().map(String::as_str))
}

/// Said above the chapter when Compare couldn't fetch the other
/// translation.
pub const COMPARE_FAILED: &str =
    "The translations couldn't be compared for this chapter, so it's shown in one translation.";

// ---- Opening a passage.

/// Said when a passage opened isn't numbered in the translation being
/// read (`name`): past the chapter's end there (it ends at `last`), or a
/// verse it leaves out mid-chapter.
pub fn not_numbered(
    book_name: &str,
    chapter: i64,
    start: i64,
    end: i64,
    name: &str,
    last: i64,
) -> String {
    let place = format!("{book_name} {chapter}");
    let range = verse_range(start, end);
    let where_ = if start > last {
        format!(" — {place} ends at verse {last} there")
    } else {
        String::new()
    };
    format!("{place}:{range} isn't numbered in the {name}{where_}. Showing the whole chapter.")
}

/// Said by `[` and `]` at the Bible's first or last book.
pub fn no_book(book_name: &str, which: Which) -> String {
    let edge = match which {
        Which::Prev => "first",
        Which::Next => "last",
    };
    format!("{book_name} is the Bible’s {edge} book.")
}

// ---- Passage history.

/// One place in the passage history: a chapter, the verses a jump landed
/// on (if it selected any), and how far down the page was when the reader
/// left it.
#[derive(Debug, Clone, PartialEq)]
pub struct Place {
    pub book_id: i64,
    pub chapter: i64,
    pub start: Option<i64>,
    pub end: Option<i64>,
    pub scroll_y: Option<f64>,
}

impl Place {
    /// A chapter, from where it was opened.
    pub fn chapter(book_id: i64, chapter: i64) -> Self {
        Self {
            book_id,
            chapter,
            start: None,
            end: None,
            scroll_y: None,
        }
    }
}

/// The most places the history keeps; the oldest go.
pub const HISTORY_LIMIT: usize = 50;

/// What going back or forward past the history's ends says.
pub fn history_end(step: i32) -> &'static str {
    if step < 0 {
        "Nothing earlier in this session."
    } else {
        "Nothing later in this session."
    }
}

/// The passage history, for this session only: the jumps (going somewhere
/// by choosing it), each a [`Place`]. Moving on from a place (a chapter
/// turn) moves the place with the reader, so Back returns to where they
/// actually left. A jump after going back drops the places ahead, as a
/// browser does.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct History {
    places: Vec<Place>,
    /// The place on screen (or on its way), or none yet.
    index: Option<usize>,
    /// Whether `places[index]` is what's on screen: not while a jump, or a
    /// step back or forward, is on its way.
    shown: bool,
    /// Bumped by each jump and step, so a step overtaken by a newer one
    /// does nothing once it's done.
    token: u64,
}

impl History {
    /// Starts over at the chapter being read.
    pub fn start(&mut self, book_id: i64, chapter: i64) {
        self.places = vec![Place::chapter(book_id, chapter)];
        self.index = Some(0);
        self.shown = true;
    }

    /// Whether there's any place at all (the history has started).
    pub fn started(&self) -> bool {
        !self.places.is_empty()
    }

    /// Brings the place on screen up to date as the reader leaves it: the
    /// chapter they're on now (`reading`), and how far down they are.
    pub fn remember_leaving(&mut self, reading: Option<(i64, i64)>, scroll_y: f64) {
        let (Some((book_id, chapter)), Some(index), true) = (reading, self.index, self.shown)
        else {
            return;
        };
        let Some(place) = self.places.get_mut(index) else {
            return;
        };
        if place.book_id != book_id || place.chapter != chapter {
            *place = Place::chapter(book_id, chapter);
        }
        place.scroll_y = Some(scroll_y);
    }

    /// A jump begins (the reader is leaving, see [`Self::remember_leaving`]).
    pub fn begin_jump(&mut self) {
        self.shown = false;
        self.token += 1;
    }

    /// A jump has landed on `place` (or not, when `landed` is `None`):
    /// recorded, unless it's the place already there.
    pub fn end_jump(&mut self, landed: Option<Place>) {
        if let Some(place) = landed {
            let here = self.index.and_then(|i| self.places.get(i));
            let same = here.is_some_and(|h| {
                h.book_id == place.book_id
                    && h.chapter == place.chapter
                    && h.start == place.start
                    && h.end == place.end
            });
            if !same {
                self.places.truncate(self.index.map_or(0, |i| i + 1));
                self.places.push(place);
                if self.places.len() > HISTORY_LIMIT {
                    let over = self.places.len() - HISTORY_LIMIT;
                    self.places.drain(..over);
                }
                self.index = Some(self.places.len() - 1);
            }
        }
        self.shown = true;
    }

    /// Whether there's a place `step` away (-1 back, 1 forward).
    pub fn can_go(&self, step: i32) -> bool {
        self.target(step).is_some()
    }

    fn target(&self, step: i32) -> Option<usize> {
        let index = self.index.map_or(-1, |i| i as i64) + i64::from(step);
        usize::try_from(index)
            .ok()
            .filter(|&i| i < self.places.len())
    }

    /// Steps back or forward: the place to open, and the step's token, or
    /// `None` at the history's end.
    pub fn begin_go(&mut self, step: i32) -> Option<(u64, Place)> {
        let target = self.target(step)?;
        self.index = Some(target);
        self.shown = false;
        self.token += 1;
        Some((self.token, self.places[target].clone()))
    }

    /// Whether the step with `token` is still the latest.
    pub fn current(&self, token: u64) -> bool {
        self.token == token
    }

    /// The step (or jump) is done: the place is on screen.
    pub fn shown(&mut self) {
        self.shown = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::verse_parts::{Part, Segment};

    fn verse(number: i64, text: &str) -> Verse {
        Verse {
            number,
            text: text.to_string(),
            parts: Some(vec![Part {
                kind: PartKind::Text,
                text: text.to_string(),
                label: None,
            }]),
            footnotes: Vec::new(),
            xrefs: Vec::new(),
        }
    }

    fn part(kind: PartKind, text: &str, label: Option<&str>) -> Part {
        Part {
            kind,
            text: text.to_string(),
            label: label.map(str::to_string),
        }
    }

    #[test]
    fn escaping_is_the_page_s() {
        assert_eq!(
            escape_html(r#"<b title="x">Tom & Jerry's</b>"#),
            "&lt;b title=&quot;x&quot;&gt;Tom &amp; Jerry&#39;s&lt;/b&gt;"
        );
        assert_eq!(
            escape_html("“In the beginning” — é"),
            "“In the beginning” — é"
        );
    }

    #[test]
    fn a_verse_s_parts_are_italics_and_markers_its_words_untouched() {
        let mut v = verse(1, "");
        v.parts = Some(vec![
            part(PartKind::Text, "The LORD ", None),
            part(PartKind::Supplied, "is", None),
            part(PartKind::Text, " my shepherd; I shall not want.", None),
            part(PartKind::Note, "want: Heb. <lack>", Some("a")),
        ]);
        assert_eq!(
            verse_html(&v, true),
            r#"The LORD <em class="supplied">is</em> my shepherd; I shall not want.<sup class="fn-marker" aria-hidden="true">a</sup>"#
        );
        assert_eq!(
            verse_html(&v, false),
            r#"The LORD <em class="supplied">is</em> my shepherd; I shall not want."#
        );
        // Without parts, the verse is its text, escaped.
        let plain = Verse {
            parts: None,
            ..verse(2, "a < b & 'c'")
        };
        assert_eq!(verse_html(&plain, true), "a &lt; b &amp; &#39;c&#39;");
    }

    #[test]
    fn a_footnote_is_its_lemma_in_italics_then_the_rest_or_its_runs() {
        let note = Footnote {
            label: "a".into(),
            lemma: Some("lamp".into()),
            rest: ": or, candle".into(),
            text: "lamp: or, candle".into(),
            segments: None,
        };
        assert_eq!(
            footnote_html(&note),
            r#"<em class="footnote-lemma">lamp</em>: or, candle"#
        );
        let bare = Footnote {
            lemma: None,
            rest: "sickle; or, scythe".into(),
            ..note.clone()
        };
        assert_eq!(footnote_html(&bare), "sickle; or, scythe");
        let usfm = Footnote {
            segments: Some(vec![
                Segment {
                    kind: SegmentKind::Text,
                    text: "BYZ and TR include ".into(),
                },
                Segment {
                    kind: SegmentKind::Verse,
                    text: "21".into(),
                },
                Segment {
                    kind: SegmentKind::Alt,
                    text: "But this kind <does> not".into(),
                },
            ]),
            ..note
        };
        assert_eq!(
            footnote_html(&usfm),
            r#"BYZ and TR include <sup class="footnote-verse">21</sup><em class="footnote-alt">But this kind &lt;does&gt; not</em>"#
        );
    }

    #[test]
    fn compare_rows_line_up_by_number_with_gaps() {
        let kjv = [verse(1, "a"), verse(2, "b"), verse(4, "d")];
        let bsb = [verse(1, "A"), verse(3, "C"), verse(4, "D")];
        let rows = compare_rows(&[&kjv, &bsb]);
        let shape: Vec<(i64, Vec<Option<&str>>)> = rows
            .iter()
            .map(|(n, cells)| {
                (
                    *n,
                    cells.iter().map(|c| c.map(|v| v.text.as_str())).collect(),
                )
            })
            .collect();
        assert_eq!(
            shape,
            vec![
                (1, vec![Some("a"), Some("A")]),
                (2, vec![Some("b"), None]),
                (3, vec![None, Some("C")]),
                (4, vec![Some("d"), Some("D")]),
            ]
        );
        assert!(compare_rows(&[]).is_empty());
    }

    fn reading() -> Reading {
        let mut v1 = verse(1, "In the beginning");
        v1.xrefs = vec![Xref {
            book_id: 43,
            chapter: 1,
            verse: 1,
            end_verse: Some(3),
            citation: "John 1:1-3".into(),
        }];
        v1.footnotes = vec![Footnote {
            label: "a".into(),
            lemma: None,
            rest: "Or, heavens".into(),
            text: "Or, heavens".into(),
            segments: None,
        }];
        Reading {
            current_book_id: 1,
            current_chapter: 1,
            current_book_name: "Genesis".into(),
            current_verses: vec![v1, verse(2, "And the earth")],
            prev: None,
            next: Some(ChapterSide {
                book_id: 1,
                chapter: 2,
                book_name: "Genesis".into(),
                verses: vec![verse(1, "Thus")],
            }),
            prev_edge: Some(Edge {
                title: "The Old Testament".into(),
                detail: Some("Genesis · 50 chapters".into()),
            }),
            next_edge: None,
            prev_book: None,
            next_book: None,
            has_chapter_xrefs: true,
        }
    }

    /// The page's own `renderReadingPane` output for this chapter (at
    /// 4a7f6f5, both lists closed, run on the same answer), byte for byte.
    const GENESIS_1: &str = include_str!("../fixtures/genesis-1.html");

    #[test]
    fn the_pane_is_the_page_s_markup_byte_for_byte() {
        assert_eq!(
            reading_pane(&reading(), None, "kjv", Opened::default()),
            GENESIS_1
        );
    }

    #[test]
    fn opened_lists_show_and_say_so() {
        let open = Opened {
            footnotes: true,
            xrefs: true,
        };
        let pane = reading_pane(&reading(), None, "kjv", open);
        assert!(pane.contains(r#"id="footnote-list-kjv"><div class="footnote-row">"#));
        assert!(pane.contains(r#"id="xref-summary-list"><div class="xref-summary-row">"#));
        assert_eq!(pane.matches(r#"aria-expanded="true""#).count(), 2);
    }

    #[test]
    fn compare_takes_the_neighbors_place_and_marks_gaps() {
        let dto = reading();
        let compare = Compare {
            book_id: 1,
            chapter: 1,
            columns: vec![
                crate::dto::CompareColumn {
                    code: "kjv".into(),
                    name: "King James Version".into(),
                    verses: vec![verse(1, "In the beginning"), verse(2, "And the earth")],
                },
                crate::dto::CompareColumn {
                    code: "bsb".into(),
                    name: "Berean Standard Bible".into(),
                    verses: vec![verse(1, "In the beginning God")],
                },
            ],
        };
        let pane = reading_pane(&dto, Some(&compare), "kjv", Opened::default());
        assert!(pane.contains(r#"<div class="reading-row is-comparing">"#));
        assert!(!pane.contains("chapter-side"));
        assert!(pane.contains(r#"<div class="compare-grid" data-columns="2"><div class="compare-head" role="columnheader">King James Version <span class="compare-head-code">(KJV)</span></div>"#));
        assert!(pane.contains(r#"<p class="compare-gap" data-gap-verse="2" data-code="BSB"><span class="verse-num">2</span><span class="compare-gap-note">Not numbered in the BSB</span></p>"#));
        // Each column's verse 1 has verse 1's cross-references.
        assert_eq!(pane.matches(r#"class="verse has-xref""#).count(), 2);
        // No footnotes in either column here: no list.
        assert!(!pane.contains("footnote-summary"));
        // The neighbors' attributes stay, so ← and → turn as before.
        assert!(pane.contains(r#" data-next-book="1" data-next-chapter="2""#));
    }

    #[test]
    fn options_record_each_translation() {
        let translations = vec![
            Translation {
                code: "kjv".into(),
                name: "King James Version".into(),
                date: Some("1611/1769".into()),
                licence: "public domain".into(),
                credit: "KJV & co".into(),
            },
            Translation {
                code: "web".into(),
                name: "World English Bible".into(),
                date: None,
                licence: "public domain".into(),
                credit: "WEB".into(),
            },
        ];
        assert_eq!(
            translation_options(&translations, "web"),
            concat!(
                r#"<option value="kjv" data-date="1611/1769" data-licence="public domain" data-credit="KJV &amp; co">King James Version</option>"#,
                r#"<option value="web" selected data-licence="public domain" data-credit="WEB">World English Bible</option>"#,
            )
        );
    }

    #[test]
    fn the_reference_names_the_chapter_and_its_translations() {
        let name = |code: &str| match code {
            "kjv" => "King James Version".to_string(),
            "web" => "World English Bible".to_string(),
            other => other.to_uppercase(),
        };
        assert_eq!(
            reference("John", 3, &["kjv"], name),
            Reference {
                text: "John 3 · KJV".into(),
                label: "John 3, King James Version. Choose a book and chapter".into(),
                title: "John 3 · KJV — Gospel Getter".into(),
            }
        );
        let both = reference("John", 3, &["kjv", "web"], name);
        assert_eq!(both.text, "John 3 · KJV + WEB");
        assert_eq!(
            both.label,
            "John 3, King James Version and World English Bible. Choose a book and chapter"
        );
        assert_eq!(
            translation_button("bsb", "Berean Standard Bible"),
            ("BSB".into(), "Translation: Berean Standard Bible".into())
        );
        assert_eq!(
            translation_button("", ""),
            (String::new(), "Translation".into())
        );
    }

    #[test]
    fn compare_fetches_for_its_layout() {
        let codes = compare_codes("kjv", Some("web"));
        assert_eq!(compare_layout(true, &codes), "kjv web");
        assert_eq!(compare_layout(false, &codes), "");
        assert_eq!(compare_codes("kjv", None), vec!["kjv".to_string()]);
        let compare = Compare {
            book_id: 43,
            chapter: 3,
            columns: vec![
                crate::dto::CompareColumn {
                    code: "kjv".into(),
                    name: String::new(),
                    verses: Vec::new(),
                },
                crate::dto::CompareColumn {
                    code: "web".into(),
                    name: String::new(),
                    verses: Vec::new(),
                },
            ],
        };
        assert!(compare_is_current(&compare, 43, 3, &codes));
        assert!(!compare_is_current(&compare, 43, 4, &codes));
        assert!(!compare_is_current(
            &compare,
            43,
            3,
            &compare_codes("kjv", Some("bsb"))
        ));
    }

    #[test]
    fn notices_say_what_isn_t_there() {
        assert_eq!(
            not_numbered("Matthew", 17, 21, 21, "Berean Standard Bible", 27),
            "Matthew 17:21 isn't numbered in the Berean Standard Bible. Showing the whole chapter."
        );
        assert_eq!(
            not_numbered("Malachi", 4, 7, 9, "King James Version", 6),
            "Malachi 4:7–9 isn't numbered in the King James Version — Malachi 4 ends at verse 6 there. Showing the whole chapter."
        );
        assert_eq!(
            no_book("Genesis", Which::Prev),
            "Genesis is the Bible’s first book."
        );
        assert_eq!(
            no_book("Revelation", Which::Next),
            "Revelation is the Bible’s last book."
        );
        assert_eq!(history_end(-1), "Nothing earlier in this session.");
        assert_eq!(history_end(1), "Nothing later in this session.");
    }

    fn landed(book_id: i64, chapter: i64, verses: Option<(i64, i64)>) -> Option<Place> {
        Some(Place {
            start: verses.map(|v| v.0),
            end: verses.map(|v| v.1),
            ..Place::chapter(book_id, chapter)
        })
    }

    #[test]
    fn history_records_jumps_and_steps_back_and_forward() {
        let mut h = History::default();
        assert!(!h.can_go(-1) && !h.can_go(1));
        h.start(43, 3);
        assert!(!h.can_go(-1));
        // A jump to Romans 8:28, then a turn to Romans 9, then a jump.
        h.remember_leaving(Some((43, 3)), 120.0);
        h.begin_jump();
        h.end_jump(landed(45, 8, Some((28, 28))));
        h.remember_leaving(Some((45, 9)), 40.0);
        h.begin_jump();
        h.end_jump(landed(1, 1, None));
        assert!(h.can_go(-1) && !h.can_go(1));
        let (token, place) = h.begin_go(-1).unwrap();
        assert!(h.current(token));
        // The place moved with the reader: Romans 9, from where they left.
        assert_eq!(
            place,
            Place {
                scroll_y: Some(40.0),
                ..Place::chapter(45, 9)
            }
        );
        h.shown();
        let (_, first) = h.begin_go(-1).unwrap();
        assert_eq!(first.scroll_y, Some(120.0));
        h.shown();
        assert!(!h.can_go(-1) && h.can_go(1));
        // A jump from here drops the places ahead.
        h.begin_jump();
        h.end_jump(landed(19, 23, None));
        assert!(!h.can_go(1));
        assert_eq!(h.places.len(), 2);
    }

    #[test]
    fn history_skips_the_same_place_keeps_fifty_and_drops_overtaken_steps() {
        let mut h = History::default();
        h.start(1, 1);
        h.begin_jump();
        h.end_jump(landed(1, 1, None));
        assert_eq!(h.places.len(), 1, "the same place isn't recorded twice");
        h.begin_jump();
        h.end_jump(None);
        assert_eq!(h.places.len(), 1, "a jump that didn't land isn't either");
        for chapter in 2..=60 {
            h.begin_jump();
            h.end_jump(landed(1, chapter, None));
        }
        assert_eq!(h.places.len(), HISTORY_LIMIT);
        assert_eq!(h.places[0].chapter, 11);
        let (older, _) = h.begin_go(-1).unwrap();
        let (newer, _) = h.begin_go(-1).unwrap();
        assert!(!h.current(older) && h.current(newer));
        // While a step is on its way, leaving remembers nothing.
        let before = h.clone();
        h.remember_leaving(Some((2, 2)), 9.0);
        assert_eq!(h, before);
    }
}
