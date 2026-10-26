//! The keyboard: the page's one dispatcher for the reader's keys, on
//! `document`. ← and → leaf through the whole Bible a chapter at a time, by
//! the chapters before and after that the reading pane says it was drawn
//! with (`#reading-pane-inner`'s `data-prev-*` and `data-next-*`, the
//! backend's, so books are crossed without this knowing the Bible's
//! shape); `[` and `]` go a book at a time; j and k (with Shift, extending)
//! step the selection a verse; Alt+← and Alt+→ go back and forward; Escape
//! backs out a step; Ctrl+C and Ctrl+Shift+C copy; N writes a note; `/` and
//! Ctrl+K search; `?` the shortcuts; Ctrl+B, G, T and C the Library, the
//! picker, the translations and Compare. Which key does what, and the
//! selection's steps, are `keyboard_text`'s, tested on the host.
//!
//! This is the main script's keyboard dispatcher (`ui/index.html` at
//! 2a9d85c: its `keydown` listener, `turnChapter`, `stepBook`,
//! `stepSelection`, `viewTop`, `verseInView`, `revealVerse`, `goHistory`) in
//! Rust: the same keys in the same order, each `preventDefault` within the
//! key, as before. It's registered first of the pieces' `document`
//! listeners (`page::mount`), as the main script's was, so the Escape it
//! uses is marked handled before the surfaces' Escape (`surfaces`) and Aa's
//! keys hear it; the modals' capturing listeners on `window` still come
//! before it, and stop the keys they take.

use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;
use web_sys::{Element, HtmlElement, KeyboardEvent};

use crate::bridge;
use crate::card::{self, Ask, SelectionFacts};
use crate::card_text::Slot;
use crate::dom;
use crate::keyboard_text::{self as text, Press, Rect, Route, Selected};
use crate::page;
use crate::reader::{self, Load, Request};
use crate::reader_text::Which;
use crate::surfaces::{self, Surface};

/// Every verse the reader can select: the chapter being read, in each
/// translation column it's shown in. Neither the faded neighbors nor
/// cross-reference text carry `data-translation`.
pub const SELECTABLE: &str = ".chapter-main .verse[data-translation]";

/// Runs `load` (a load's remainder, after what it sent within the key).
fn spawn(load: Load) {
    spawn_local(async move {
        load.await;
    });
}

/// The chapter before (`Prev`) or after (`Next`) the one on screen, as ←
/// and → (and the palette's commands) turn to it: by the pane's
/// `#reading-pane-inner`'s `data-prev-book` and `data-prev-chapter` (or
/// `data-next-*`), the backend's.
pub fn turn_chapter(which: Which) {
    let Some(inner) = bridge::document().and_then(|d| d.get_element_by_id("reading-pane-inner"))
    else {
        return;
    };
    let (book, chapter) = match which {
        Which::Prev => ("data-prev-book", "data-prev-chapter"),
        Which::Next => ("data-next-book", "data-next-chapter"),
    };
    let Some(book) = inner.get_attribute(book).filter(|b| !b.is_empty()) else {
        return;
    };
    let number = |value: Option<String>| value.and_then(|v| v.trim().parse::<i64>().ok());
    if let (Some(book_id), Some(chapter)) =
        (number(Some(book)), number(inner.get_attribute(chapter)))
    {
        spawn(reader::ask(Request::Load {
            book_id,
            chapter,
            translation_code: None,
            top: false,
        }));
    }
}

/// `[` and `]`: chapter 1 of the book before or after, from the top (the
/// backend names them), or a toast at the Bible's ends. Not a jump: like ←
/// and →, it moves on from where the reader is.
pub fn step_book(which: Which) {
    spawn(reader::ask(Request::Book { which }));
}

/// Back (-1) or forward (1) through the passages jumped to.
pub fn go_history(step: i32) {
    spawn(reader::ask(Request::History { step }));
}

/// The top bar's foot: the top of what's in view.
fn view_top() -> f64 {
    bridge::document()
        .and_then(|d| d.get_element_by_id("top-bar"))
        .map_or(0.0, |bar| bar.get_bounding_client_rect().bottom())
}

fn window_height() -> f64 {
    web_sys::window()
        .and_then(|w| w.inner_height().ok())
        .and_then(|h| h.as_f64())
        .unwrap_or(0.0)
}

fn rect_of(el: &Element) -> Rect {
    let r = el.get_bounding_client_rect();
    Rect {
        top: r.top(),
        bottom: r.bottom(),
        height: r.height(),
    }
}

fn data_verse(el: &Element) -> Option<i64> {
    el.get_attribute("data-verse")?.trim().parse().ok()
}

/// The first (`first`) or last verse of column `code` that's in view, or
/// none when none is (or there's no layout to tell).
fn verse_in_view(code: &str, first: bool) -> Option<i64> {
    let document = bridge::document()?;
    let found = document
        .query_selector_all(&format!(r#"{SELECTABLE}[data-translation="{code}"]"#))
        .ok()?;
    let top = view_top();
    let window = window_height();
    let seen: Vec<HtmlElement> = dom::elements(&found)
        .into_iter()
        .filter(|el| text::in_view(rect_of(el), top, window))
        .collect();
    let el = if first { seen.first() } else { seen.last() }?;
    data_verse(el)
}

/// Scrolls just enough to bring verse `number` of column `code` into view.
fn reveal_verse(code: &str, number: i64) {
    let Some(el) = dom::query(&format!(
        r#"{SELECTABLE}[data-translation="{code}"][data-verse="{number}"]"#
    )) else {
        return;
    };
    if let (Some(by), Some(window)) = (
        text::reveal(rect_of(&el), view_top(), window_height()),
        web_sys::window(),
    ) {
        window.scroll_by_with_x_and_y(0.0, by);
    }
}

/// j and k: the verse after (`step` 1) or before (-1) the selection, in the
/// reading column (the left one while comparing), skipping any the
/// translation doesn't number; with `extend` (Shift), the selection's
/// moving end instead. The chapter and the selection are the card's. The
/// page scrolls only as far as the verse needs.
fn step_selection(step: i64, extend: bool) {
    let facts = card::facts();
    let Some(chapter) = facts.chapter else {
        return;
    };
    let code = chapter.translation_code;
    let selection = facts.selection.as_ref().map(|s: &SelectionFacts| Selected {
        translation_code: &s.translation_code,
        anchor: s.anchor,
        start: s.start,
        end: s.end,
    });
    let Some((anchor, focus)) = text::step_selection(
        &chapter.numbers,
        selection.as_ref(),
        &code,
        step,
        extend,
        || verse_in_view(&code, step > 0),
    ) else {
        return;
    };
    card::ask(Ask::Select {
        translation_code: code.clone(),
        anchor,
        focus,
    });
    reveal_verse(&code, focus);
}

/// Whether the key belongs to something else: a form control (About's
/// checkbox, the search box, which handles its own), the search results or
/// Copy as…, or an open dialog (they handle their own keys).
fn not_ours(event: &KeyboardEvent) -> bool {
    if let Some(target) = event.target().and_then(|t| t.dyn_into::<Element>().ok()) {
        if matches!(target.tag_name().as_str(), "INPUT" | "TEXTAREA" | "SELECT") {
            return true;
        }
        if target
            .closest("#search-panel, #copy-as-menu")
            .ok()
            .flatten()
            .is_some()
        {
            return true;
        }
    }
    page::import_open()
        || page::about_open()
        || page::note_open()
        || page::export_notes_open()
        || page::shortcuts_open()
}

/// No text is selected on the page (a drag-highlight copies as usual).
fn no_text_selected() -> bool {
    web_sys::window()
        .and_then(|w| w.get_selection().ok().flatten())
        .is_none_or(|selection| selection.is_collapsed())
}

fn on_keydown(event: &KeyboardEvent) {
    if not_ours(event) {
        return;
    }
    let key = event.key();
    let press = Press {
        key: &key,
        ctrl: event.ctrl_key(),
        meta: event.meta_key(),
        alt: event.alt_key(),
        shift: event.shift_key(),
    };
    let Some(route) = text::route(&press) else {
        return;
    };
    if text::always_prevented(route) {
        event.prevent_default();
    }
    match route {
        // Escape backs out one step at a time: the search box, then an open
        // popover or panel (closed by `surfaces`, which skips an Escape
        // already handled here), then a cross-reference previewed in the
        // card, then a verse selection, then reading mode (`surfaces`).
        Route::Escape => {
            if page::search_shown() {
                page::set_search_open(false);
                event.prevent_default();
            } else if surfaces::current().is_none() && card::ask(Ask::Escape) {
                event.prevent_default();
            }
        }
        Route::History(step) => go_history(step),
        Route::Shortcuts => page::open_shortcuts(None),
        Route::Search => page::set_search_open(true),
        // Unless the reader has highlighted some text, which copies as
        // usual.
        Route::Copy => {
            if no_text_selected() && card::ask(Ask::Copy) {
                event.prevent_default();
            }
        }
        Route::CopyAs => {
            if card::ask(Ask::CopyAsOpen(Slot::Selection)) {
                event.prevent_default();
            }
        }
        Route::Library => surfaces::toggle_surface(Surface::Library, None),
        Route::Note => {
            if card::ask(Ask::Note(page::focused())) {
                event.prevent_default();
            }
        }
        Route::Picker => surfaces::open_picker(false),
        Route::Translation => surfaces::toggle_surface(Surface::Translation, None),
        Route::Step { step, extend } => step_selection(step, extend),
        Route::Book(which) => step_book(which),
        // Through its own toggle, in the translation button's popover.
        Route::Compare => {
            if let Some(compare) = bridge::document()
                .and_then(|d| d.get_element_by_id("compare-btn"))
                .and_then(|el| el.dyn_into::<HtmlElement>().ok())
            {
                compare.click();
            }
        }
        Route::Turn(which) => turn_chapter(which),
    }
}

/// Listens on `document` for the reader's keys. Called by `page::mount`,
/// before any piece's own `document` listeners.
pub fn mount() {
    let Some(document) = bridge::document() else {
        return;
    };
    dom::on(&document, "keydown", |event| {
        if let Some(event) = event.dyn_ref::<KeyboardEvent>() {
            on_keydown(event);
        }
    });
}
