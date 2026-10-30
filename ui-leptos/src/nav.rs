//! The top bar's way-finding controls: Back, ▾ (Recent places) and
//! Forward left of the reference, and the chapter arrows ‹ and › either
//! side of it. Back and Forward do what Alt+← and Alt+→ do
//! (`keyboard::go_history`, the reader's one path for a step), and ‹ and ›
//! what ← and → do (`keyboard::turn_chapter`); each says where it goes, or
//! that there's nowhere, as the reader tells this after each chapter, jump
//! and step ([`show`]). ▾ opens Recent places, a menu
//! (`surfaces::Surface::Recent`, which opens and closes it and moves through
//! it with the keys): the places jumped to and left, newest first, from
//! `nav_text`; choosing one jumps there as the picker or search does
//! (`reader::Request::Jump`), in the translation it was read in. On a
//! narrow window Forward folds into the menu, first.
//!
//! The list is kept on this computer (local storage, `nav_text::RECENT_KEY`)
//! and read back once the books and translations are known, every entry
//! checked against them; nothing in it is a display preference, and it's in
//! no backup or export. Nothing here asks the backend for anything: names
//! come from the books boot hands the picker (`bridge::PICKER_BOOKS_EVENT`).

use std::cell::RefCell;

use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::spawn_local;
use web_sys::{Element, HtmlElement, HtmlOptionElement, HtmlSelectElement};

use crate::aa;
use crate::bridge;
use crate::dom;
use crate::ipc;
use crate::keyboard;
use crate::nav_text::{self as text, RecentPlace, RecentPlaces, Stored};
use crate::picker_text::Book;
use crate::reader::{self, Request};
use crate::reader_text::{Place, Which};
use crate::surfaces::{self, Surface};

/// What the reader says after each chapter, jump and step: the chapter on
/// screen, the places Back and Forward would go to, and the chapters
/// before and after, by name ("John 2"), as ← and → turn to them.
#[derive(Debug, Clone, Default)]
pub struct Facts {
    pub here: Option<(i64, i64)>,
    pub back: Option<Place>,
    pub forward: Option<Place>,
    pub prev: Option<String>,
    pub next: Option<String>,
}

#[derive(Default)]
struct State {
    /// Every book, from boot.
    books: Vec<Book>,
    recent: RecentPlaces,
    /// Whether the kept list has been read back (once the books and
    /// translations are known); places recorded before then wait.
    loaded: bool,
    waiting: Vec<RecentPlace>,
    facts: Facts,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with_borrow_mut(f)
}

fn by_id(id: &str) -> Option<HtmlElement> {
    bridge::document()?
        .get_element_by_id(id)?
        .dyn_into::<HtmlElement>()
        .ok()
}

fn storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok()?
}

/// The bundled translations' codes, from the translation button's record.
fn translation_codes() -> Vec<String> {
    let Some(select) =
        by_id("translation-select").and_then(|el| el.dyn_into::<HtmlSelectElement>().ok())
    else {
        return Vec::new();
    };
    let options = select.options();
    (0..options.length())
        .filter_map(|i| options.item(i))
        .filter_map(|o| o.dyn_into::<HtmlOptionElement>().ok())
        .map(|o| o.value())
        .collect()
}

fn book_name(books: &[Book], book_id: i64) -> Option<String> {
    books
        .iter()
        .find(|b| b.id == book_id)
        .map(|b| b.name.clone())
}

fn narrow() -> bool {
    web_sys::window()
        .and_then(|w| w.match_media(text::NARROW_QUERY).ok().flatten())
        .is_some_and(|query| query.matches())
}

// ---- Recent places, as kept.

/// The kept list, read back and checked, once the books and translations
/// are known; then any place recorded before, on top.
fn load() {
    let codes = translation_codes();
    if with(|s| s.loaded || s.books.is_empty()) || codes.is_empty() {
        return;
    }
    let stored = storage()
        .and_then(|s| s.get_item(text::RECENT_KEY).ok().flatten())
        .filter(|value| {
            text::readable(u32::try_from(value.encode_utf16().count()).unwrap_or(u32::MAX))
        })
        .and_then(|value| js_sys::JSON::parse(&value).ok())
        .and_then(|value| serde_wasm_bindgen::from_value::<Vec<Stored>>(value).ok())
        .unwrap_or_default();
    let waited = with(|s| {
        s.recent = RecentPlaces::from_stored(stored, &s.books, &codes);
        s.loaded = true;
        let waiting = std::mem::take(&mut s.waiting);
        for place in &waiting {
            s.recent.record(place.clone());
        }
        !waiting.is_empty()
    });
    if waited {
        save();
    }
}

/// Keeps the list; quietly not, if local storage can't be written.
fn save() {
    let Some(storage) = storage() else {
        return;
    };
    let places = with(|s| s.recent.places().to_vec());
    let serializer = serde_wasm_bindgen::Serializer::json_compatible();
    let written = serde::Serialize::serialize(&places, &serializer)
        .map_err(JsValue::from)
        .and_then(|value| js_sys::JSON::stringify(&value))
        .and_then(|json| storage.set_item(text::RECENT_KEY, &String::from(json)));
    if let Err(e) = written {
        ipc::log_error("Recent places couldn't be kept", &e);
    }
}

/// A place jumped to, or left by a jump or a step (the reader's): the
/// newest recent place.
pub fn record(place: RecentPlace) {
    load();
    let loaded = with(|s| {
        if s.loaded {
            s.recent.record(place);
        } else {
            s.waiting.push(place);
        }
        s.loaded
    });
    if loaded {
        save();
    }
}

// ---- Back and Forward.

/// Back (-1) or Forward (1) as the facts have it: where it goes, by name.
fn history_button(step: i32) -> text::BarButton {
    with(|s| {
        let to = if step < 0 {
            &s.facts.back
        } else {
            &s.facts.forward
        };
        let name = to.as_ref().map(|place| {
            book_name(&s.books, place.book_id)
                .map(|book| text::place_name(&book, place.chapter, place.start, place.end))
        });
        text::history_button(step, name.is_some(), name.flatten().as_deref())
    })
}

/// A button, enabled or not: `aria-disabled` (it stays in the tab order,
/// and says so), its name and its tooltip.
fn set_button(id: &str, label: &str, title: &str, disabled: bool) {
    let Some(button) = by_id(id) else {
        return;
    };
    let _ = button.set_attribute("aria-label", label);
    let _ = button.set_attribute("title", title);
    if disabled {
        let _ = button.set_attribute("aria-disabled", "true");
    } else {
        let _ = button.remove_attribute("aria-disabled");
    }
}

/// The reader's facts, after a chapter, a jump or a step: Back, Forward, ‹
/// and › say where they go, and the menu, if open, is drawn again.
pub fn show(facts: Facts) {
    let chapters = [
        (
            "chapter-prev",
            text::chapter_button(Which::Prev, facts.prev.as_deref()),
        ),
        (
            "chapter-next",
            text::chapter_button(Which::Next, facts.next.as_deref()),
        ),
    ];
    with(|s| s.facts = facts);
    for (id, step) in [("history-back", -1), ("history-forward", 1)] {
        let button = history_button(step);
        set_button(id, &button.label, &button.title, button.disabled);
    }
    for (id, button) in chapters {
        set_button(id, &button.label, &button.title, button.disabled);
    }
    if surfaces::current() == Some(Surface::Recent) {
        draw_menu();
    }
}

fn enabled(el: &HtmlElement) -> bool {
    el.get_attribute("aria-disabled").as_deref() != Some("true")
}

// ---- The menu.

/// Draws Recent places as it opens (`surfaces::open`).
pub fn draw(surface: Surface) {
    if surface == Surface::Recent {
        load();
        draw_menu();
    }
}

fn draw_menu() {
    let Some(menu) = by_id("recent-places-menu") else {
        return;
    };
    let markup = with(|s| {
        let rows: Vec<text::MenuRow> = s
            .recent
            .places()
            .iter()
            .map(|p| {
                let book = book_name(&s.books, p.book_id).unwrap_or_default();
                let name = text::place_name(&book, p.chapter, p.start, p.end);
                let here = s.facts.here == Some((p.book_id, p.chapter));
                text::menu_row(&name, &p.translation, here)
            })
            .collect();
        let forward = narrow().then_some(s.facts.forward.is_some());
        text::menu_markup(&rows, forward)
    });
    menu.set_inner_html(&markup);
}

/// Jumps to the recent place at `index`, in the translation it was read in.
fn go_recent(index: usize) {
    let Some(place) = with(|s| s.recent.places().get(index).cloned()) else {
        return;
    };
    if place.translation != bridge::translation_code() {
        aa::choose_translation(&place.translation);
    }
    let load = reader::ask(Request::Jump {
        book_id: place.book_id,
        chapter: place.chapter,
        start: place.start,
        end: place.end,
    });
    spawn_local(async move {
        let _ = load.await;
    });
}

fn on_menu_click(event: &web_sys::Event) {
    let Some(item) = event
        .target()
        .and_then(|t| t.dyn_into::<Element>().ok())
        .and_then(|t| t.closest(r#"[role="menuitem"]"#).ok().flatten())
    else {
        return;
    };
    if item.has_attribute("disabled") {
        return;
    }
    surfaces::close(true);
    if item.id() == "recent-forward" {
        keyboard::go_history(1);
    } else if let Some(index) = item
        .get_attribute("data-index")
        .and_then(|i| i.parse::<usize>().ok())
    {
        go_recent(index);
    }
}

/// Listens to Back, Forward, the menu's items, ‹ and ›, and for boot's
/// books.
/// Called before boot (`boot`), which hands the books over.
pub fn mount() {
    dom::on_document_event(bridge::PICKER_BOOKS_EVENT, |event| {
        match bridge::picker_books(&event) {
            Ok((old, new)) => {
                with(|s| s.books = old.into_iter().chain(new).collect());
                load();
                // Back and Forward can name their places now.
                let facts = with(|s| s.facts.clone());
                show(facts);
            }
            Err(e) => ipc::log_error("The books couldn't be read", &e),
        }
    });
    for (id, step) in [("history-back", -1), ("history-forward", 1)] {
        if let Some(button) = by_id(id) {
            let target = button.clone();
            dom::on_click(&button, move |_| {
                if enabled(&target) {
                    keyboard::go_history(step);
                }
            });
        }
    }
    if let Some(menu) = by_id("recent-places-menu") {
        dom::on(&menu, "click", |event| on_menu_click(&event));
    }
    for (id, which) in [("chapter-prev", Which::Prev), ("chapter-next", Which::Next)] {
        if let Some(button) = by_id(id) {
            let target = button.clone();
            dom::on_click(&button, move |_| {
                if enabled(&target) {
                    keyboard::turn_chapter(which);
                }
            });
        }
    }
}
