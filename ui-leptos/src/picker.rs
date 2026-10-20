//! The book and chapter picker: Random chapter, then the Old and New
//! Testament's books; choosing a book shows its chapters under it, and
//! choosing a chapter reads it. The chapter being read, and its book, are
//! marked. The arrows, Home and End move through it (and never turn the
//! chapter).
//!
//! The picker itself (`#picker`, with Random chapter and the testaments'
//! headings and lists) is the page's, and so are its two shapes: the head
//! script shows it as the sidebar on a wide window or as the popover from
//! the reference button, keeps the reader's place as the sidebar comes and
//! goes, and moves focus into it (the chapter being read, marked here). This
//! draws the books into its lists from what boot hands over
//! (`bridge::PICKER_BOOKS_EVENT`), a book's chapters, the marks for the
//! chapter the page puts on screen (`bridge::PICKER_CURRENT_EVENT`), and
//! scrolls the picker (never the page) to it when the picker appears
//! (`bridge::PICKER_SHOWN_EVENT`). A chapter chosen here, or Random
//! chapter's, closes the popover and is read through the page's jump
//! (`bridge::jump`): from its top, unless it's the one on screen.
//!
//! This is the page's picker script (`ui/index.html` at 13911c4, from
//! `pickRandomBookAndChapter` to the picker's keydown) in Rust: the same
//! ids, classes, ARIA, text and focus order. Everything happens within the
//! click or key, as it did: the lists and the chapters are drawn (as Leptos
//! views, built and put in place at once) and the marks set there and then.
//! Random chapter's numbers are the webview's `Math.random`.

use leptos::prelude::*;
use leptos::tachys::view::{Mountable, Render};
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{Element, HtmlElement, KeyboardEvent};

use crate::bridge;
use crate::dom;
use crate::ipc;
use crate::picker_text::{self as text, Book, FromChapter, FromItem, Key};

/// The grid of chapters' id: one book's chapters show at a time.
const CHAPTERS_ID: &str = "picker-chapters";

#[derive(Default)]
struct State {
    /// Every book, the Old Testament's then the New's, as the lists show
    /// them.
    books: Vec<Book>,
    /// The chapter on screen (book id, chapter), once there is one.
    reading: Option<(i64, i64)>,
    /// The book whose chapters show.
    open: Option<i64>,
}

#[derive(Clone, Copy)]
struct Picker {
    state: StoredValue<State, LocalStorage>,
}

/// The page's element `id`.
fn by_id(id: &str) -> Option<HtmlElement> {
    bridge::document()?
        .get_element_by_id(id)?
        .dyn_into::<HtmlElement>()
        .ok()
}

fn picker() -> Option<HtmlElement> {
    by_id("picker")
}

fn closest(el: &Element, selector: &str) -> Option<Element> {
    el.closest(selector).ok().flatten()
}

/// A `data-*` number of `el`.
fn data_number(el: &Element, name: &str) -> Option<i64> {
    el.get_attribute(name)?.parse().ok()
}

/// The elements `selector` finds in `root`, in order.
fn all(root: &Element, selector: &str) -> Vec<HtmlElement> {
    root.query_selector_all(selector)
        .map(|found| dom::elements(&found))
        .unwrap_or_default()
}

/// The book's button, by its id.
fn book_button(book_id: i64) -> Option<HtmlElement> {
    picker()?
        .query_selector(&format!(".book-item[data-book-id=\"{book_id}\"]"))
        .ok()??
        .dyn_into::<HtmlElement>()
        .ok()
}

/// The chapters under `book`, when they show.
fn grid_after(book: &Element) -> Option<HtmlElement> {
    if book.get_attribute("aria-expanded").as_deref() != Some("true") {
        return None;
    }
    book.next_element_sibling()?.dyn_into::<HtmlElement>().ok()
}

/// The picker's `scrollTop`, as the page reads it (a fraction, on some
/// screens).
fn scroll_top(picker: &Element) -> f64 {
    js_sys::Reflect::get(picker, &JsValue::from_str("scrollTop"))
        .ok()
        .and_then(|top| top.as_f64())
        .unwrap_or(0.0)
}

fn set_scroll_top(picker: &Element, top: f64) {
    let _ = js_sys::Reflect::set(
        picker,
        &JsValue::from_str("scrollTop"),
        &JsValue::from_f64(top),
    );
}

/// Scrolls the picker (not the page) just enough to show `el`.
fn reveal(el: &HtmlElement) {
    let Some(picker) = picker().filter(|p| !p.hidden()) else {
        return;
    };
    if let Some(top) = text::reveal(
        f64::from(el.offset_top()),
        f64::from(el.offset_height()),
        scroll_top(&picker),
        f64::from(picker.client_height()),
    ) {
        set_scroll_top(&picker, top);
    }
}

impl Picker {
    fn new() -> Self {
        Self {
            state: StoredValue::new_local(State::default()),
        }
    }

    fn reading(self) -> Option<(i64, i64)> {
        self.state.with_value(|s| s.reading)
    }

    // ---- Drawing.

    /// Fills the two lists with the books: each a button, its id and
    /// chapter count on it, a disclosure for its chapters.
    fn draw_books(self, old: Vec<Book>, new: Vec<Book>) {
        for (id, books) in [("ot-book-list", &old), ("nt-book-list", &new)] {
            let Some(list) = by_id(id) else {
                continue;
            };
            while let Some(child) = list.first_child() {
                let _ = list.remove_child(&child);
            }
            // Static views: built and put in place now, and left there.
            let mut drawn = books.iter().map(book_view).collect_view().build();
            drawn.mount(&list, None);
        }
        self.state.update_value(|s| {
            s.books = old.into_iter().chain(new).collect();
            s.open = None;
        });
        if let Some((book_id, chapter)) = self.reading() {
            self.mark_current(book_id, chapter);
        }
    }

    /// Shows `book_id`'s chapters under it (and no other book's), or no
    /// book's for `None`. Built from the book's chapter count, so it needs
    /// nothing from the backend.
    fn show_chapters(self, book_id: Option<i64>) {
        let Some(picker) = picker() else {
            return;
        };
        if let Some(shown) = bridge::document().and_then(|d| d.get_element_by_id(CHAPTERS_ID)) {
            shown.remove();
        }
        for book in all(&picker, ".book-item[aria-expanded=\"true\"]") {
            let _ = book.set_attribute("aria-expanded", "false");
            let _ = book.remove_attribute("aria-controls");
        }
        self.state.update_value(|s| s.open = None);
        let Some(book_id) = book_id else {
            return;
        };
        let Some(button) = book_button(book_id) else {
            return;
        };
        let Some(parent) = button.parent_element() else {
            return;
        };
        let count = self.state.with_value(|s| {
            s.books
                .iter()
                .find(|b| b.id == book_id)
                .map_or(0, |b| b.chapter_count)
        });
        let current = self
            .reading()
            .filter(|(reading, _)| *reading == book_id)
            .map(|(_, chapter)| chapter);
        let name = button.text_content().unwrap_or_default();
        let mut drawn = grid_view(book_id, &name, count, current).build();
        drawn.mount(&parent, button.next_sibling().as_ref());
        let _ = button.set_attribute("aria-expanded", "true");
        let _ = button.set_attribute("aria-controls", CHAPTERS_ID);
        self.state.update_value(|s| s.open = Some(book_id));
    }

    /// Scrolls the picker so the book being read, and as much of its
    /// chapters as fit, are in view: unless they already are, its name
    /// goes a little below the top.
    fn reveal_current(self) {
        let Some((book_id, _)) = self.reading() else {
            return;
        };
        let (Some(picker), Some(book)) = (picker(), book_button(book_id)) else {
            return;
        };
        if picker.hidden() {
            return;
        }
        let end = grid_after(&book).unwrap_or_else(|| book.clone());
        if let Some(top) = text::reveal_current(
            f64::from(book.offset_top()),
            f64::from(end.offset_top() + end.offset_height()),
            scroll_top(&picker),
            f64::from(picker.client_height()),
        ) {
            set_scroll_top(&picker, top);
        }
    }

    /// After a chapter is put on screen: its book and chapter are the ones
    /// marked, and its book's chapters show (unless they already do, so a
    /// reader choosing chapters keeps their place).
    fn mark_current(self, book_id: i64, chapter: i64) {
        self.state
            .update_value(|s| s.reading = Some((book_id, chapter)));
        let Some(picker) = picker() else {
            return;
        };
        for book in all(&picker, ".book-item[aria-current]") {
            let _ = book.remove_attribute("aria-current");
        }
        if let Some(book) = book_button(book_id) {
            let _ = book.set_attribute("aria-current", "true");
        }
        if self.state.with_value(|s| s.open) != Some(book_id) {
            self.show_chapters(Some(book_id));
        } else {
            for button in all(&picker, ".chapter-btn") {
                if data_number(&button, "data-chapter") == Some(chapter) {
                    let _ = button.set_attribute("aria-current", "page");
                } else {
                    let _ = button.remove_attribute("aria-current");
                }
            }
        }
        if !dom::active_element().is_some_and(|active| picker.contains(Some(&active))) {
            self.reveal_current();
        }
    }

    // ---- Choosing.

    /// Reads a chapter chosen here (or Random chapter's), from its top, as
    /// the end-of-chapter buttons do. The chapter already on screen stays
    /// where the reader is in it. The popover closes (focus going back to
    /// the reference button); the sidebar stays.
    fn choose(self, book_id: i64, chapter: i64) {
        if bridge::current_surface().as_deref() == Some("picker") {
            bridge::close_surfaces_with(true);
        }
        let shown = self.reading() == Some((book_id, chapter));
        bridge::jump(book_id, chapter, !shown);
    }

    fn random(self) {
        let pick = self
            .state
            .with_value(|s| text::random_pick(&s.books, js_sys::Math::random));
        if let Some((book_id, chapter)) = pick {
            self.choose(book_id, chapter);
        }
    }

    // ---- The reader's clicks and keys.

    /// A book shows or hides its chapters; a chapter is read; Random
    /// chapter reads one it picks.
    fn on_click(self, event: &web_sys::Event) {
        let Some(target) = event.target().and_then(|t| t.dyn_into::<Element>().ok()) else {
            return;
        };
        if let Some(book) = closest(&target, ".book-item") {
            let open = book.get_attribute("aria-expanded").as_deref() == Some("true");
            self.show_chapters(if open {
                None
            } else {
                data_number(&book, "data-book-id")
            });
            return;
        }
        if let Some(chapter) = closest(&target, ".chapter-btn") {
            let book_id = closest(&chapter, ".chapter-grid")
                .and_then(|grid| data_number(&grid, "data-book-id"));
            if let (Some(book_id), Some(chapter)) = (book_id, data_number(&chapter, "data-chapter"))
            {
                self.choose(book_id, chapter);
            }
            return;
        }
        if closest(&target, "#random-chapter-btn").is_some() {
            self.random();
        }
    }

    /// The keyboard, besides Tab: ↑ and ↓ move through Random chapter and
    /// the books (into and out of a book's chapters), → shows a book's
    /// chapters and ← hides them; in the chapters, the arrows move around
    /// the grid, and Home and End go to its first and last. Arrows never
    /// turn the chapter here.
    fn on_keydown(self, event: &KeyboardEvent) {
        let Some(key) = Key::named(&event.key()) else {
            return;
        };
        event.stop_propagation();
        event.prevent_default();
        let (Some(picker), Some(target)) = (
            picker(),
            event.target().and_then(|t| t.dyn_into::<Element>().ok()),
        ) else {
            return;
        };
        let mut items: Vec<HtmlElement> = by_id("random-chapter-btn").into_iter().collect();
        items.extend(all(&picker, ".book-item"));
        let place = |el: &Element| items.iter().position(|item| dom::same(Some(item), el));

        let next = if let Some(chapter) = closest(&target, ".chapter-btn") {
            let Some(grid) = closest(&chapter, ".chapter-grid") else {
                return;
            };
            let buttons = all(&grid, ".chapter-btn");
            let Some(i) = buttons.iter().position(|b| dom::same(Some(b), &chapter)) else {
                return;
            };
            let columns = grid
                .get_attribute("data-columns")
                .and_then(|c| c.parse().ok())
                .unwrap_or(0);
            let book = grid.previous_element_sibling();
            match text::from_chapter(key, i, buttons.len(), columns) {
                FromChapter::Chapter(j) => buttons.get(j).cloned(),
                FromChapter::Book => book.and_then(|b| b.dyn_into::<HtmlElement>().ok()),
                // What follows the book (Random chapter, if the book isn't
                // one of the items, as the page's indexOf had it).
                FromChapter::AfterBook => {
                    let after = book.as_ref().and_then(place).map_or(0, |i| i + 1);
                    items.get(after).cloned()
                }
                FromChapter::Stay => None,
            }
        } else {
            let Some(i) = place(&target) else {
                return;
            };
            let open = items
                .iter()
                .position(|item| item.get_attribute("aria-expanded").as_deref() == Some("true"));
            let is_book = target.class_list().contains("book-item");
            let chapters_of = |j: usize| {
                items
                    .get(j)
                    .and_then(|book| grid_after(book))
                    .map(|grid| all(&grid, ".chapter-btn"))
                    .unwrap_or_default()
            };
            match text::from_item(key, i, items.len(), open, is_book) {
                FromItem::Item(j) => items.get(j).cloned(),
                FromItem::FirstChapter(j) => chapters_of(j).into_iter().next(),
                FromItem::LastChapter(j) => chapters_of(j).pop(),
                FromItem::ShowChapters => {
                    self.show_chapters(data_number(&target, "data-book-id"));
                    None
                }
                FromItem::HideChapters => {
                    self.show_chapters(None);
                    None
                }
                FromItem::Stay => None,
            }
        };
        if let Some(next) = next {
            let _ = next.focus();
            reveal(&next);
        }
    }

    fn listen(self, picker: &HtmlElement) {
        dom::on(picker, "click", move |event| self.on_click(&event));
        dom::on(picker, "keydown", move |event| {
            if let Some(event) = event.dyn_ref::<KeyboardEvent>() {
                self.on_keydown(event);
            }
        });
        dom::on_document_event(
            bridge::PICKER_BOOKS_EVENT,
            move |event| match bridge::picker_books(&event) {
                Ok((old, new)) => self.draw_books(old, new),
                Err(e) => ipc::log_error("The books couldn't be read", &e),
            },
        );
        dom::on_document_event(bridge::PICKER_CURRENT_EVENT, move |event| {
            match bridge::picker_current(&event) {
                Ok((book_id, chapter)) => self.mark_current(book_id, chapter),
                Err(e) => ipc::log_error("The chapter on screen couldn't be read", &e),
            }
        });
        // The head script shows the picker: scroll it to the book being read.
        dom::on_document_event(bridge::PICKER_SHOWN_EVENT, move |_| self.reveal_current());
    }
}

/// One book in a testament's list.
fn book_view(book: &Book) -> impl IntoView + use<> {
    view! {
        <li>
            <button
                type="button"
                class="book-item"
                data-book-id=book.id.to_string()
                data-chapter-count=book.chapter_count.to_string()
                aria-expanded="false"
            >
                {book.name.clone()}
            </button>
        </li>
    }
}

/// A book's chapters: a compact grid for a long book, else pills; the
/// chapter being read (`current`) marked.
fn grid_view(book_id: i64, name: &str, count: i64, current: Option<i64>) -> impl IntoView + use<> {
    let class = if text::compact(count) {
        "chapter-grid is-compact"
    } else {
        "chapter-grid"
    };
    let chapters = (1..=count)
        .map(|n| {
            view! {
                <button
                    type="button"
                    class="chapter-btn"
                    data-chapter=n.to_string()
                    aria-current=(current == Some(n)).then_some("page")
                >
                    {n.to_string()}
                </button>
            }
        })
        .collect_view();
    view! {
        <div
            id=CHAPTERS_ID
            class=class
            role="group"
            aria-label=format!("{name} chapters")
            data-book-id=book_id.to_string()
            data-columns=text::columns(count).to_string()
        >
            {chapters}
        </div>
    }
}

/// Starts drawing into the page's `#picker` and listening.
pub fn mount() {
    let Some(picker) = picker() else {
        return;
    };
    Picker::new().listen(&picker);
}
