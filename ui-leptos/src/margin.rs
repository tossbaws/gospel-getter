//! Margin notes: the reader's notes on the chapter on screen, in the open
//! space to the right of the text, each beside its first verse, as in a
//! printed Bible's margin (where each goes is `margin_text`'s). Aa's "Notes
//! beside the text" (and M) says when: in reading mode (the default), or
//! always, outside reading mode too, or never; a setting of this
//! computer's that no backup carries. Read-only: a note's reference, its
//! "more" or a click on it selects its verses and opens the selection card
//! with them, the note among their notes, where it's edited as ever.
//!
//! The margin is absolutely placed beside the reading column (in
//! `.chapter-main`, after the chapter in reading order), so drawing it never
//! moves the text. Outside reading mode, "Always" gives the page reading
//! mode's shape whether the chapter has notes or not (the stylesheet's
//! `data-margin-notes="always"` rules), so the text never jumps between
//! chapters, and a chapter without notes says so at the margin's top. There's
//! no margin while comparing translations, nor on paper (print leaves it
//! out), nor where there's no room for it (`margin_text::width`): then each
//! note gets a small marker after its verse instead, which opens the card
//! the same way. Two panes side by side have no room beside either, so each
//! has markers; reading mode shows the active one alone, with its margin.
//!
//! The selection card (`card`) hands over each pane's chapter's notes
//! whenever it has them ([`notes`]: a chapter shown, a note added, edited,
//! deleted or restored). The notes are placed again, at most once a frame,
//! whenever the page may have changed shape: the window resized, a reading
//! pane resized (text size, line spacing, width, layout, fonts), or a
//! setting on `<html>` changed (reading mode, Compare, the theme, this one).
//! It never scrolls the page; markers, which reflow the text, go in and out
//! through the reading place logic (`surfaces::keep_reading_place`).
//!
//! The chapter's AI-flagged findings (`findings`, handed over with
//! [`findings`]) go everywhere the notes do, by the same rules: one entry
//! per verse, after the reader's own notes beside it (C9), collapsed as the
//! violet badge and the verse until clicked (C7, `findings_text`); a marker
//! after the pencil in a narrow window. They're drawn collapsed every time.
//!
//! `window.gospelGetterMargin` says what's shown beside the active pane's
//! chapter and how long the last placing took, for the tests
//! ([`export_margin`]).

use std::cell::RefCell;
use std::collections::HashMap;

use wasm_bindgen::prelude::*;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{Element, HtmlElement, MouseEvent, MutationObserver, MutationObserverInit};

use crate::aa;
use crate::aa_text::Beside;
use crate::bridge;
use crate::card;
use crate::dom;
use crate::dto::SelectionNote;
use crate::findings_text::{self, FindingGroup};
use crate::margin_text::{self as text, Anchor, Entry, Reading, Shown};
use crate::pane::{self, PaneId, PerPane};
use crate::preferences;
use crate::prefs;
use crate::surfaces;

/// A note of the chapter, as the card has it (and as the backend sent it,
/// for the note editor), and where it sits.
struct Placed {
    note: SelectionNote,
    raw: JsValue,
    anchor: Anchor,
}

/// What the margin knows of one pane's chapter, and has drawn there.
#[derive(Default)]
struct InPane {
    /// The chapter on screen (book, chapter), the translation being read,
    /// and its notes, as the card handed them over.
    chapter: Option<(i64, i64, String)>,
    notes: Vec<(SelectionNote, JsValue)>,
    /// They're the chapter's (read for it), so none means it has none.
    known: bool,
    /// The chapter's AI findings, a group per verse, and the chapter (book,
    /// chapter) they were read for.
    findings: Vec<FindingGroup>,
    findings_of: Option<(i64, i64)>,
    /// Bumped by each new set of notes: they're drawn again.
    generation: u64,
    /// What's on the page: drawn for which generation, how, and the notes
    /// in the order drawn.
    drawn: Option<(u64, Shown)>,
    placed: Vec<Placed>,
    placed_findings: Vec<(FindingGroup, Anchor)>,
}

struct State {
    panes: PerPane<InPane>,
    /// A frame is asked for, to place the notes.
    frame_pending: bool,
    /// How long the last placing took, in milliseconds.
    last_ms: f64,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State {
        panes: PerPane::new(|_| InPane::default()),
        frame_pending: false,
        last_ms: 0.0,
    });
}

/// The state, never held while the page is touched.
fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with_borrow_mut(f)
}

/// `pane`'s part of the state.
fn with_pane<R>(pane: PaneId, f: impl FnOnce(&mut InPane) -> R) -> R {
    with(|s| f(&mut s.panes[pane]))
}

/// `pane`'s chapter's notes (`book_id` `chapter`, read in `code`), each
/// with the backend's own object for it, from the card, and whether
/// they're `known` (read for this chapter, not on their way or
/// unreadable): drawn in the next frame.
pub fn notes(
    pane: PaneId,
    book_id: i64,
    chapter: i64,
    code: &str,
    notes: Vec<(SelectionNote, JsValue)>,
    known: bool,
) {
    with_pane(pane, |s| {
        s.chapter = Some((book_id, chapter, code.to_string()));
        s.notes = notes;
        s.known = known;
        s.generation += 1;
    });
    schedule();
}

/// `pane`'s chapter's AI findings (`book_id` `chapter`), a group per verse,
/// from `findings`: drawn in the next frame.
pub fn findings(pane: PaneId, book_id: i64, chapter: i64, groups: Vec<FindingGroup>) {
    with_pane(pane, |s| {
        s.findings = groups;
        s.findings_of = Some((book_id, chapter));
        s.generation += 1;
    });
    schedule();
}

/// `pane` closed: what the margin knew of its chapter goes.
pub fn forget(pane: PaneId) {
    with_pane(pane, |s| {
        let generation = s.generation + 1;
        *s = InPane {
            generation,
            ..InPane::default()
        };
    });
    schedule();
}

/// The chapter's findings, if they're for the chapter the notes are.
fn findings_here(s: &InPane) -> Vec<FindingGroup> {
    match (&s.chapter, s.findings_of) {
        (Some((book_id, chapter, _)), Some(of)) if of == (*book_id, *chapter) => s.findings.clone(),
        _ => Vec::new(),
    }
}

/// Places the notes again in the next frame: an entry grew or shrank (a
/// verse's findings opened or closed), or another pane became the active
/// one.
pub fn relayout() {
    schedule();
}

/// Places the notes in the next animation frame (one frame asked for at
/// most), when the page's layout for it is in.
fn schedule() {
    if with(|s| std::mem::replace(&mut s.frame_pending, true)) {
        return;
    }
    let placed = Closure::once_into_js(|| {
        with(|s| s.frame_pending = false);
        place();
    });
    let asked = web_sys::window()
        .is_some_and(|w| w.request_animation_frame(placed.unchecked_ref()).is_ok());
    if !asked {
        with(|s| s.frame_pending = false);
    }
}

fn root() -> Option<Element> {
    bridge::document()?.document_element()
}

fn now() -> f64 {
    web_sys::window()
        .and_then(|w| w.performance())
        .map_or(0.0, |p| p.now())
}

/// `pane`'s reading column, if its chapter is the one its notes are for.
fn column(pane: PaneId) -> Option<HtmlElement> {
    let (book_id, chapter, _) = with_pane(pane, |s| s.chapter.clone())?;
    let inner = pane.inner()?;
    let number = |name: &str| inner.get_attribute(name)?.parse::<i64>().ok();
    if number("data-current-book") != Some(book_id)
        || number("data-current-chapter") != Some(chapter)
    {
        return None;
    }
    inner
        .query_selector(".chapter-main")
        .ok()
        .flatten()?
        .dyn_into::<HtmlElement>()
        .ok()
}

/// The verses of the column being read, in order.
fn verses(column: &HtmlElement, code: &str) -> Vec<HtmlElement> {
    column
        .query_selector_all(&format!(
            r#".chapter-text > .verse[data-translation="{code}"][data-verse]"#
        ))
        .map(|found| dom::elements(&found))
        .unwrap_or_default()
}

fn number(verse: &Element) -> Option<i64> {
    verse.get_attribute("data-verse")?.parse().ok()
}

/// The verses by number: each read once, however many notes look one up.
fn by_number(verses: Vec<HtmlElement>) -> HashMap<i64, HtmlElement> {
    verses
        .into_iter()
        .filter_map(|v| Some((number(&v)?, v)))
        .collect()
}

/// The elements `selector` finds in `root`, by their `data-note-id`.
fn by_note_id(root: &Element, selector: &str) -> HashMap<i64, HtmlElement> {
    root.query_selector_all(selector)
        .map(|found| dom::elements(&found))
        .unwrap_or_default()
        .into_iter()
        .filter_map(|el| Some((el.get_attribute("data-note-id")?.parse().ok()?, el)))
        .collect()
}

/// The elements `selector` finds in `root`, by their `data-finding-key`.
fn by_finding_key(root: &Element, selector: &str) -> HashMap<String, HtmlElement> {
    root.query_selector_all(selector)
        .map(|found| dom::elements(&found))
        .unwrap_or_default()
        .into_iter()
        .filter_map(|el| Some((el.get_attribute("data-finding-key")?, el)))
        .collect()
}

/// The page's rem, in px.
fn rem() -> f64 {
    root()
        .and_then(|html| {
            web_sys::window()?
                .get_computed_style(&html)
                .ok()
                .flatten()?
                .get_property_value("font-size")
                .ok()
        })
        .and_then(|size| size.trim_end_matches("px").parse::<f64>().ok())
        .unwrap_or(16.0)
}

/// The room beside the column, from its right edge to the window's (its
/// scroll bar left out), in px.
fn room(column: &HtmlElement) -> f64 {
    let window_width = root().map_or(0, |html| html.client_width());
    f64::from(window_width) - column.get_bounding_client_rect().right()
}

/// What the margin should show now beside `pane`'s `column`
/// (`margin_text::shown`): nothing in a pane out of view.
fn wanted(pane: PaneId, column: &HtmlElement) -> Shown {
    if !pane::in_view().any(|shown| shown == pane) {
        return Shown::Nothing;
    }
    let reading = Reading {
        setting: aa::notes_beside(),
        reading_mode: prefs::on(&preferences::READER_MODE),
        comparing: aa::beside() == Beside::Translations,
        split: pane::split(),
    };
    let (entries, known) = with_pane(pane, |s| {
        (
            !(s.notes.is_empty() && findings_here(s).is_empty()),
            s.known,
        )
    });
    let room = text::width(room(column), rem()).is_some();
    text::shown(reading, entries, known, room)
}

/// Draws what's wanted beside each pane's chapter (again, if the notes or
/// what's wanted changed, or the pane was drawn again), then places the
/// margin's notes beside their verses.
fn place() {
    let started = now();
    for pane in [PaneId::First, PaneId::Second] {
        place_in(pane);
    }
    let took = now() - started;
    with(|s| s.last_ms = took);
}

/// [`place`], in `pane`.
fn place_in(pane: PaneId) {
    let column = pane::shown()
        .any(|shown| shown == pane)
        .then(|| column(pane))
        .flatten();
    let shown = column
        .as_ref()
        .map_or(Shown::Nothing, |column| wanted(pane, column));
    let generation = with_pane(pane, |s| s.generation);
    let on_page = column.as_ref().is_some_and(|column| {
        let found = |selector: &str| column.query_selector(selector).ok().flatten().is_some();
        match shown {
            Shown::Margin | Shown::Empty => found(".margin-notes"),
            Shown::Markers => found(".margin-marker"),
            Shown::Nothing => true,
        }
    });
    if with_pane(pane, |s| s.drawn) != Some((generation, shown)) || !on_page {
        clear(pane);
        if let Some(column) = &column {
            draw(pane, column, shown);
        }
        with_pane(pane, |s| s.drawn = Some((generation, shown)));
    }
    if let (Shown::Margin, Some(column)) = (shown, &column) {
        place_margin(pane, column);
    }
}

/// Takes the margin and the markers off `pane`'s chapter.
fn clear(pane: PaneId) {
    for aside in pane.query_all(".margin-notes") {
        aside.remove();
    }
    let markers = pane.query_all(".margin-marker, .finding-inline");
    if !markers.is_empty() {
        surfaces::keep_reading_place(
            || {
                for marker in markers {
                    marker.remove();
                }
            },
            None,
        );
    }
    with_pane(pane, |s| {
        s.placed.clear();
        s.placed_findings.clear();
    });
}

/// `pane`'s chapter's notes in the order they go down the margin (by the
/// verse each sits beside, then as the backend ordered them), each with
/// where it sits among `numbered` (`margin_text::in_order`).
fn anchored(pane: PaneId, numbered: &[i64]) -> Vec<Placed> {
    let notes = with_pane(pane, |s| s.notes.clone());
    text::in_order(
        notes.into_iter().map(|(note, raw)| {
            let (start, end) = (note.verse_start, note.verse_end);
            ((note, raw), start, end)
        }),
        numbered,
    )
    .into_iter()
    .map(|((note, raw), anchor)| Placed { note, raw, anchor })
    .collect()
}

/// `pane`'s chapter's findings in the order they go down the page, each
/// with where it sits among `numbered`, as a note on its verses would.
fn anchored_findings(pane: PaneId, numbered: &[i64]) -> Vec<(FindingGroup, Anchor)> {
    let groups = with_pane(pane, |s| findings_here(s));
    text::in_order(
        groups.into_iter().map(|g| {
            let (start, end) = (g.verse_start, g.verse_end);
            (g, start, end)
        }),
        numbered,
    )
}

/// The notes and findings placed, in the order they go down the page.
fn all_entries<'a>(
    placed: &'a [Placed],
    findings: &'a [(FindingGroup, Anchor)],
) -> Vec<(Entry<'a>, Anchor)> {
    let notes: Vec<(&SelectionNote, Anchor)> = placed.iter().map(|p| (&p.note, p.anchor)).collect();
    let findings: Vec<(&FindingGroup, Anchor)> = findings.iter().map(|(g, a)| (g, *a)).collect();
    text::entries(&notes, &findings)
}

fn draw(pane: PaneId, column: &HtmlElement, shown: Shown) {
    let Some(code) = with_pane(pane, |s| s.chapter.as_ref().map(|c| c.2.clone())) else {
        return;
    };
    let verses = verses(column, &code);
    let numbered: Vec<i64> = verses.iter().filter_map(|v| number(v)).collect();
    let verses = by_number(verses);
    let placed = anchored(pane, &numbered);
    let placed_findings = anchored_findings(pane, &numbered);
    match shown {
        Shown::Nothing => {}
        Shown::Margin | Shown::Empty => {
            let html = if shown == Shown::Empty {
                text::empty_html()
            } else {
                text::margin_entries_html(&all_entries(&placed, &placed_findings))
            };
            let _ = column.insert_adjacent_html("beforeend", &html);
            if let Some(aside) = column.query_selector(".margin-notes").ok().flatten() {
                listen(pane, &aside);
            }
        }
        Shown::Markers => {
            // The reader's pencil first, then the violet AI marker (C9).
            surfaces::keep_reading_place(
                || {
                    for p in &placed {
                        if let Some(verse) = verses.get(&p.anchor.verse) {
                            let _ = verse
                                .insert_adjacent_html("beforeend", &text::marker_html(&p.note));
                        }
                    }
                    for (group, anchor) in &placed_findings {
                        if let Some(verse) = verses.get(&anchor.verse) {
                            let _ = verse.insert_adjacent_html(
                                "beforeend",
                                &findings_text::marker_html(group, pane.name()),
                            );
                        }
                    }
                },
                None,
            );
            for marker in column
                .query_selector_all(".margin-marker, .finding-inline")
                .map(|found| dom::elements(&found))
                .unwrap_or_default()
            {
                listen(pane, &marker);
            }
        }
    }
    with_pane(pane, |s| {
        s.placed = placed;
        s.placed_findings = placed_findings;
    });
}

/// A click on a note, its reference or "more" (or a marker; Enter and
/// Space click them) opens it; the click goes no further, so it neither
/// selects the verse a marker is in nor clears the selection. A verse's
/// findings, their toggle and their actions are `findings`'.
fn listen(pane: PaneId, target: &Element) {
    dom::on(target, "keydown", |event| {
        if let Some(event) = event.dyn_ref::<web_sys::KeyboardEvent>() {
            crate::findings::on_key(event);
        }
    });
    dom::on(target, "click", move |event| {
        if crate::findings::on_click(&event) {
            return;
        }
        let note_id = event
            .target()
            .and_then(|t| t.dyn_into::<Element>().ok())
            .and_then(|t| t.closest("[data-note-id]").ok().flatten())
            .and_then(|el| el.get_attribute("data-note-id"))
            .and_then(|id| id.parse::<i64>().ok());
        if let Some(note_id) = note_id {
            event.stop_propagation();
            if let Some(event) = event.dyn_ref::<MouseEvent>() {
                event.prevent_default();
            }
            open(pane, note_id);
        }
    });
}

/// Opens note `note_id`, in `pane`: its verses selected and the card with
/// them, the note among their notes (`margin_text::selection`). A note no
/// selection of this translation's verses can reach (past the chapter's
/// end here) opens in the note editor, as the Library's Edit opens it. The
/// pointer pressed in the pane made it the active one (`pane`), whose
/// verses the card selects.
fn open(pane: PaneId, note_id: i64) {
    let found = with_pane(pane, |s| {
        let code = s.chapter.as_ref()?.2.clone();
        let p = s.placed.iter().find(|p| p.note.id == note_id)?;
        Some((code, p.note.clone(), p.raw.clone(), p.anchor))
    });
    let Some((code, note, raw, anchor)) = found else {
        return;
    };
    pane::set_active(pane);
    let numbered: Vec<i64> = column(pane)
        .map(|column| verses(&column, &code))
        .unwrap_or_default()
        .iter()
        .filter_map(|v| number(v))
        .collect();
    match text::selection(anchor, note.verse_start, note.verse_end, &numbered) {
        Some((start, end)) => card::select_for_note(&code, start, end),
        None => card::edit_note(pane, &raw),
    }
}

/// Sets the margin's width, then puts each note beside its verse (or,
/// where they'd overlap, below the one above), folded if it's long, and
/// each range's rule beside its verses ([`place_notes`]).
fn place_margin(pane: PaneId, column: &HtmlElement) {
    let Some(aside) = column
        .query_selector(".margin-notes")
        .ok()
        .flatten()
        .and_then(|el| el.dyn_into::<HtmlElement>().ok())
    else {
        return;
    };
    let rem = rem();
    let Some(width) = text::width(room(column), rem) else {
        return;
    };
    let _ = aside
        .style()
        .set_property("--margin-width", &format!("{width}px"));
    let _ = aside
        .style()
        .set_property("--margin-fold-lines", &text::FOLD_LINES.to_string());
    // The margin is placed at the column's top.
    place_notes(
        pane,
        column,
        &aside,
        column.get_bounding_client_rect().top(),
    );
}

/// `note`'s part `class`.
fn part(note: &HtmlElement, class: &str) -> Option<HtmlElement> {
    note.query_selector(class)
        .ok()
        .flatten()
        .and_then(|el| el.dyn_into::<HtmlElement>().ok())
}

/// Whether `note`'s words are cut short at the fold.
fn is_cut_short(note: &HtmlElement) -> bool {
    part(note, ".margin-note-body").is_some_and(|body| {
        text::cut_short(
            f64::from(body.scroll_height()),
            f64::from(body.client_height()),
        )
    })
}

/// Shows `note` folded (faded at the foot, with "more") or not.
fn fold(note: &HtmlElement, folded: bool) {
    let _ = note.class_list().toggle_with_force("is-folded", folded);
    if let Some(more) = part(note, ".margin-note-more") {
        more.set_hidden(!folded);
    }
}

/// Puts each note in `aside` beside its verse in `pane`'s `column` (or,
/// where they'd overlap, below the one above), folded if it's long, and
/// each range's rule beside its verses, every top from `top_of` (the
/// aside's own top, px): every box read in one pass, then every top
/// written.
fn place_notes(pane: PaneId, column: &HtmlElement, aside: &HtmlElement, top_of: f64) {
    let flowing = root()
        .and_then(|html| html.get_attribute("data-layout"))
        .as_deref()
        == Some("flowing");
    let Some(code) = with_pane(pane, |s| s.chapter.as_ref().map(|c| c.2.clone())) else {
        return;
    };
    let verses = by_number(verses(column, &code));
    // Each entry down the page: a note by its id, a verse's findings by
    // their key, in the order they go (`margin_text::entries`).
    let ids: Vec<(Option<i64>, String, Anchor)> = with_pane(pane, |s| {
        all_entries(&s.placed, &s.placed_findings)
            .into_iter()
            .map(|(entry, anchor)| match entry {
                Entry::Note(note) => (Some(note.id), String::new(), anchor),
                Entry::Findings(group) => (None, group.key(), anchor),
            })
            .collect()
    });
    let mut found = by_note_id(aside, ".margin-note");
    let mut found_findings = by_finding_key(aside, ".margin-finding");
    let rules = by_note_id(aside, ".margin-rule");
    let notes: Vec<Option<HtmlElement>> = ids
        .iter()
        .map(|(id, key, _)| match id {
            Some(id) => found.remove(id),
            None => found_findings.remove(key),
        })
        .collect();

    // Every box read in one pass (the page laid out once): which notes are
    // cut short (their words past the fold; "more" overlays the faded foot,
    // so folding changes no note's height), where each would go, and how
    // tall each is. Then every write.
    let folded: Vec<bool> = notes
        .iter()
        .map(|note| note.as_ref().is_some_and(is_cut_short))
        .collect();
    let mark = |n: i64| -> Option<Element> {
        let verse = verses.get(&n)?;
        if flowing {
            verse.query_selector(".verse-num").ok().flatten()
        } else {
            Some(verse.clone().into())
        }
    };
    let wanted: Vec<f64> = ids
        .iter()
        .map(|&(_, _, anchor)| {
            mark(anchor.verse).map_or(0.0, |el| el.get_bounding_client_rect().top() - top_of)
        })
        .collect();
    let spans: Vec<Option<(f64, f64)>> = ids
        .iter()
        .map(|&(id, _, anchor)| {
            (id.is_some() && anchor.is_range()).then(|| {
                let top = verses
                    .get(&anchor.verse)
                    .map_or(0.0, |v| v.get_bounding_client_rect().top());
                let bottom = verses
                    .get(&anchor.last)
                    .map_or(top, |v| v.get_bounding_client_rect().bottom());
                (top - top_of, bottom - top)
            })
        })
        .collect();
    let heights: Vec<f64> = notes
        .iter()
        .map(|note| note.as_ref().map_or(0.0, |n| f64::from(n.offset_height())))
        .collect();
    let tops = text::stack(&wanted, &heights, text::STACK_GAP_PX);

    for ((((note, top), (id, _, _)), span), folded) in
        notes.iter().zip(&tops).zip(&ids).zip(&spans).zip(&folded)
    {
        if let Some(note) = note {
            let _ = note.style().set_property("top", &format!("{top}px"));
            fold(note, *folded);
        }
        if let (Some((span_top, span_height)), Some(rule)) =
            (span, id.and_then(|id| rules.get(&id)))
        {
            let _ = rule.style().set_property("top", &format!("{span_top}px"));
            let _ = rule
                .style()
                .set_property("height", &format!("{span_height}px"));
        }
    }
}

/// `window.gospelGetterMargin`: `shown()`, what the margin shows beside
/// the active pane's chapter now ("margin", "markers", "empty" or
/// "nothing"), and `layoutMs()`, how long its last placing took. For the
/// tests.
fn export_margin() {
    let Some(window) = web_sys::window() else {
        return;
    };
    let global = js_sys::Object::new();
    let shown = Closure::<dyn Fn() -> JsValue>::new(|| {
        let shown = with(|s| {
            s.panes[pane::active()]
                .drawn
                .map_or(Shown::Nothing, |(_, shown)| shown)
        });
        JsValue::from_str(shown.name())
    });
    let layout_ms = Closure::<dyn Fn() -> JsValue>::new(|| JsValue::from_f64(with(|s| s.last_ms)));
    let _ = js_sys::Reflect::set(&global, &JsValue::from_str("shown"), shown.as_ref());
    let _ = js_sys::Reflect::set(&global, &JsValue::from_str("layoutMs"), layout_ms.as_ref());
    let _ = js_sys::Reflect::set(&window, &JsValue::from_str("gospelGetterMargin"), &global);
    shown.forget();
    layout_ms.forget();
}

/// A pane's chapter reflowed (`pane`, which watches both): the notes are
/// placed again.
pub fn reflowed() {
    schedule();
}

/// Places the notes again whenever the page may have changed shape: the
/// window resized, or a setting on `<html>` changed (and a pane's chapter
/// reflowed, [`reflowed`]).
pub fn mount() {
    let Some(window) = web_sys::window() else {
        return;
    };
    dom::on(&window, "resize", |_| schedule());
    let changed = Closure::<dyn Fn()>::new(schedule);
    if let (Some(html), Ok(observer)) = (
        root(),
        MutationObserver::new(changed.as_ref().unchecked_ref()),
    ) {
        let init = MutationObserverInit::new();
        init.set_attributes(true);
        let _ = observer.observe_with_options(&html, &init);
    }
    changed.forget();
    export_margin();
}
