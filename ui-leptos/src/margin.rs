//! Margin notes in reading mode: the reader's notes on the chapter on
//! screen, in the open space to the right of the text, each beside its
//! first verse, as in a printed Bible's margin (where each goes is
//! `margin_text`'s). On by default; Aa's "Notes in the margin" turns them
//! off, a setting of this computer's that no backup carries. Read-only: a
//! note's reference, its "more" or a click on it selects its verses and
//! opens the selection card with them, the note among their notes, where
//! it's edited as ever.
//!
//! The margin is absolutely placed beside the reading column (in
//! `.chapter-main`, after the chapter in reading order), so turning it on
//! or off never moves the text. It's there only in reading mode, not while
//! comparing, on screen (print leaves it out), and where there's room for
//! it (`margin_text::width`). With less room, each note gets a small marker
//! after its verse instead, which opens the card the same way.
//!
//! The selection card (`card`) hands over the chapter's notes whenever it
//! has them ([`notes`]: a chapter shown, a note added, edited, deleted or
//! restored). The notes are placed again, at most once a frame, whenever
//! the page may have changed shape: the window resized, the reading pane
//! resized (text size, line spacing, width, layout, fonts), or a setting on
//! `<html>` changed (reading mode, Compare, the theme, this one). It never
//! scrolls the page; markers, which reflow the text, go in and out through
//! the reading place logic (`surfaces::keep_reading_place`).
//!
//! `window.gospelGetterMargin` says what's shown and how long the last
//! placing took, for the tests ([`export_margin`]).

use std::cell::RefCell;
use std::collections::HashMap;

use wasm_bindgen::prelude::*;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{Element, HtmlElement, MouseEvent, MutationObserver, MutationObserverInit};

use crate::bridge;
use crate::card;
use crate::dom;
use crate::dto::SelectionNote;
use crate::margin_text::{self as text, Anchor};
use crate::preferences;
use crate::prefs;
use crate::surfaces;

/// What the margin shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Shown {
    #[default]
    Nothing,
    Margin,
    Markers,
}

impl Shown {
    fn name(self) -> &'static str {
        match self {
            Self::Nothing => "nothing",
            Self::Margin => "margin",
            Self::Markers => "markers",
        }
    }
}

/// A note of the chapter, as the card has it (and as the backend sent it,
/// for the note editor), and where it sits.
struct Placed {
    note: SelectionNote,
    raw: JsValue,
    anchor: Anchor,
}

#[derive(Default)]
struct State {
    /// The chapter on screen (book, chapter), the translation being read,
    /// and its notes, as the card handed them over.
    chapter: Option<(i64, i64, String)>,
    notes: Vec<(SelectionNote, JsValue)>,
    /// Bumped by each new set of notes: they're drawn again.
    generation: u64,
    /// What's on the page: drawn for which generation, how, and the notes
    /// in the order drawn.
    drawn: Option<(u64, Shown)>,
    placed: Vec<Placed>,
    /// A frame is asked for, to place the notes.
    frame_pending: bool,
    /// How long the last placing took, in milliseconds.
    last_ms: f64,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

/// The state, never held while the page is touched.
fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with_borrow_mut(f)
}

/// The chapter's notes (`book_id` `chapter`, read in `code`), each with
/// the backend's own object for it, from the card: drawn in the next frame.
pub fn notes(book_id: i64, chapter: i64, code: &str, notes: Vec<(SelectionNote, JsValue)>) {
    with(|s| {
        s.chapter = Some((book_id, chapter, code.to_string()));
        s.notes = notes;
        s.generation += 1;
    });
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

/// The reading column of the chapter the notes are for, if it's the one on
/// screen.
fn column() -> Option<HtmlElement> {
    let (book_id, chapter, _) = with(|s| s.chapter.clone())?;
    let inner = bridge::document()?.get_element_by_id("reading-pane-inner")?;
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

/// What the margin should show now, for `column`.
fn wanted(column: &HtmlElement) -> Shown {
    let attr = |name: &str| root().and_then(|html| html.get_attribute(name));
    let shown = attr("data-reader-mode").as_deref() == Some("on")
        && attr("data-compare").as_deref() != Some("on")
        && prefs::on(&preferences::MARGIN_NOTES)
        && !with(|s| s.notes.is_empty());
    if !shown {
        Shown::Nothing
    } else if text::width(room(column), rem()).is_some() {
        Shown::Margin
    } else {
        Shown::Markers
    }
}

/// Draws what's wanted (again, if the notes or what's wanted changed, or
/// the pane was drawn again), then places the margin's notes beside their
/// verses.
fn place() {
    let started = now();
    let column = column();
    let shown = column.as_ref().map_or(Shown::Nothing, wanted);
    let generation = with(|s| s.generation);
    let on_page = column.as_ref().is_some_and(|column| match shown {
        Shown::Margin => column
            .query_selector(".margin-notes")
            .ok()
            .flatten()
            .is_some(),
        Shown::Markers => column
            .query_selector(".margin-marker")
            .ok()
            .flatten()
            .is_some(),
        Shown::Nothing => true,
    });
    if with(|s| s.drawn) != Some((generation, shown)) || !on_page {
        clear();
        if let Some(column) = &column {
            draw(column, shown);
        }
        with(|s| s.drawn = Some((generation, shown)));
    }
    if let (Shown::Margin, Some(column)) = (shown, &column) {
        place_margin(column);
    }
    let took = now() - started;
    with(|s| s.last_ms = took);
}

/// Takes the margin and the markers off the page.
fn clear() {
    let Some(document) = bridge::document() else {
        return;
    };
    let found = |selector: &str| {
        document
            .query_selector_all(selector)
            .map(|found| dom::elements(&found))
            .unwrap_or_default()
    };
    for aside in found(".margin-notes") {
        aside.remove();
    }
    let markers = found(".margin-marker");
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
    with(|s| s.placed.clear());
}

/// The chapter's notes in the order they go down the margin (by the verse
/// each sits beside, then as the backend ordered them), each with where
/// it sits among `numbered`.
fn anchored(numbered: &[i64]) -> Vec<Placed> {
    let mut placed: Vec<Placed> = with(|s| {
        s.notes
            .iter()
            .filter_map(|(note, raw)| {
                Some(Placed {
                    anchor: text::anchor(note.verse_start, note.verse_end, numbered)?,
                    note: note.clone(),
                    raw: raw.clone(),
                })
            })
            .collect()
    });
    placed.sort_by_key(|p| p.anchor.verse);
    placed
}

fn draw(column: &HtmlElement, shown: Shown) {
    let Some(code) = with(|s| s.chapter.as_ref().map(|c| c.2.clone())) else {
        return;
    };
    let verses = verses(column, &code);
    let numbered: Vec<i64> = verses.iter().filter_map(|v| number(v)).collect();
    let verses = by_number(verses);
    let placed = anchored(&numbered);
    match shown {
        Shown::Nothing => {}
        Shown::Margin => {
            let pairs: Vec<(&SelectionNote, Anchor)> =
                placed.iter().map(|p| (&p.note, p.anchor)).collect();
            let _ = column.insert_adjacent_html("beforeend", &text::margin_html(&pairs));
            if let Some(aside) = column.query_selector(".margin-notes").ok().flatten() {
                listen(&aside);
            }
        }
        Shown::Markers => {
            surfaces::keep_reading_place(
                || {
                    for p in &placed {
                        if let Some(verse) = verses.get(&p.anchor.verse) {
                            let _ = verse
                                .insert_adjacent_html("beforeend", &text::marker_html(&p.note));
                        }
                    }
                },
                None,
            );
            for marker in column
                .query_selector_all(".margin-marker")
                .map(|found| dom::elements(&found))
                .unwrap_or_default()
            {
                listen(&marker);
            }
        }
    }
    with(|s| s.placed = placed);
}

/// A click on a note, its reference or "more" (or a marker; Enter and
/// Space click them) opens it; the click goes no further, so it neither
/// selects the verse a marker is in nor clears the selection.
fn listen(target: &Element) {
    dom::on(target, "click", |event| {
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
            open(note_id);
        }
    });
}

/// Opens note `note_id`: its verses selected and the card with them, the
/// note among their notes (`margin_text::selection`). A note no selection
/// of this translation's verses can reach (past the chapter's end here)
/// opens in the note editor, as the Library's Edit opens it.
fn open(note_id: i64) {
    let found = with(|s| {
        let code = s.chapter.as_ref()?.2.clone();
        let p = s.placed.iter().find(|p| p.note.id == note_id)?;
        Some((code, p.note.clone(), p.raw.clone(), p.anchor))
    });
    let Some((code, note, raw, anchor)) = found else {
        return;
    };
    let numbered: Vec<i64> = column()
        .map(|column| verses(&column, &code))
        .unwrap_or_default()
        .iter()
        .filter_map(|v| number(v))
        .collect();
    match text::selection(anchor, note.verse_start, note.verse_end, &numbered) {
        Some((start, end)) => card::select_for_note(&code, start, end),
        None => card::edit_note(&raw),
    }
}

/// Sets the margin's width, then puts each note beside its verse (or,
/// where they'd overlap, below the one above), folded if it's long, and
/// each range's rule beside its verses: every box read in one pass, then
/// every top written.
fn place_margin(column: &HtmlElement) {
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
    let flowing = root()
        .and_then(|html| html.get_attribute("data-layout"))
        .as_deref()
        == Some("flowing");
    let Some(code) = with(|s| s.chapter.as_ref().map(|c| c.2.clone())) else {
        return;
    };
    let verses = by_number(verses(column, &code));
    let ids: Vec<(i64, Anchor)> =
        with(|s| s.placed.iter().map(|p| (p.note.id, p.anchor)).collect());
    let mut found = by_note_id(&aside, ".margin-note");
    let rules = by_note_id(&aside, ".margin-rule");
    let notes: Vec<Option<HtmlElement>> = ids.iter().map(|(id, _)| found.remove(id)).collect();
    let part = |note: &HtmlElement, class: &str| {
        note.query_selector(class)
            .ok()
            .flatten()
            .and_then(|el| el.dyn_into::<HtmlElement>().ok())
    };

    // Every box read in one pass (the page laid out once): which notes are
    // cut short (their words past the fold; "more" overlays the faded foot,
    // so folding changes no note's height), where each would go, and how
    // tall each is. Then every write.
    let folded: Vec<bool> = notes
        .iter()
        .map(|note| {
            note.as_ref()
                .and_then(|n| part(n, ".margin-note-body"))
                .is_some_and(|body| body.scroll_height() > body.client_height() + 1)
        })
        .collect();
    let top_of = column.get_bounding_client_rect().top();
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
        .map(|&(_, anchor)| {
            mark(anchor.verse).map_or(0.0, |el| el.get_bounding_client_rect().top() - top_of)
        })
        .collect();
    let spans: Vec<Option<(f64, f64)>> = ids
        .iter()
        .map(|&(_, anchor)| {
            anchor.is_range().then(|| {
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

    for ((((note, top), (id, _)), span), folded) in
        notes.iter().zip(&tops).zip(&ids).zip(&spans).zip(&folded)
    {
        if let Some(note) = note {
            let _ = note.style().set_property("top", &format!("{top}px"));
            let _ = note.class_list().toggle_with_force("is-folded", *folded);
            if let Some(more) = part(note, ".margin-note-more") {
                more.set_hidden(!folded);
            }
        }
        if let (Some((span_top, span_height)), Some(rule)) = (span, rules.get(id)) {
            let _ = rule.style().set_property("top", &format!("{span_top}px"));
            let _ = rule
                .style()
                .set_property("height", &format!("{span_height}px"));
        }
    }
}

/// `window.gospelGetterMargin`: `shown()`, what the margin shows now
/// ("margin", "markers" or "nothing"), and `layoutMs()`, how long its last
/// placing took. For the tests.
fn export_margin() {
    let Some(window) = web_sys::window() else {
        return;
    };
    let global = js_sys::Object::new();
    let shown = Closure::<dyn Fn() -> JsValue>::new(|| {
        let shown = with(|s| s.drawn.map_or(Shown::Nothing, |(_, shown)| shown));
        JsValue::from_str(shown.name())
    });
    let layout_ms = Closure::<dyn Fn() -> JsValue>::new(|| JsValue::from_f64(with(|s| s.last_ms)));
    let _ = js_sys::Reflect::set(&global, &JsValue::from_str("shown"), shown.as_ref());
    let _ = js_sys::Reflect::set(&global, &JsValue::from_str("layoutMs"), layout_ms.as_ref());
    let _ = js_sys::Reflect::set(&window, &JsValue::from_str("gospelGetterMargin"), &global);
    shown.forget();
    layout_ms.forget();
}

/// Places the notes again whenever the page may have changed shape: the
/// window resized, the reading pane resized, or a setting on `<html>`
/// changed.
pub fn mount() {
    let Some(window) = web_sys::window() else {
        return;
    };
    dom::on(&window, "resize", |_| schedule());
    let changed = Closure::<dyn Fn()>::new(schedule);
    if js_sys::Reflect::has(&window, &JsValue::from_str("ResizeObserver")).unwrap_or(false)
        && let (Some(pane), Ok(observer)) = (
            bridge::document().and_then(|d| d.get_element_by_id("reading-pane")),
            web_sys::ResizeObserver::new(changed.as_ref().unchecked_ref()),
        )
    {
        observer.observe(&pane);
    }
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
