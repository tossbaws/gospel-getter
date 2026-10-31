//! The page's one click dispatcher, on the body: a click away closes the
//! search box; the header, the picker, the Library, the dialogs and the
//! toast are left to their own; the selection card's controls, Copy as…
//! and Copy chapter… are the card's (`card::ask`, within the click: copying
//! needs that), and any other click closes Copy as…; the reader's own
//! controls in the pane are left to the reader (`reader`, whose listener on
//! the body runs after this); clicking a verse selects it (Shift-click, a
//! range), a footnote marker selects its verse, and anything else is a
//! click away from the selection. And the Shift `mousedown` that keeps a
//! Shift-click on a verse from extending the browser's own text selection,
//! and makes its click a Shift-click even if Shift is let go before the
//! button comes up.
//!
//! This is the main script's (`ui/index.html` at 2a9d85c: its body `click`
//! and `mousedown` listeners and `jumpTo`) in Rust, in the same order. Its
//! listeners are registered before any piece's (`page::mount`), as the main
//! script's were.

use std::cell::Cell;

use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;
use web_sys::{Element, HtmlElement, MouseEvent};

use crate::bridge;
use crate::card::{self, Ask};
use crate::card_text::Slot;
use crate::dom;
use crate::keyboard::SELECTABLE;
use crate::page;
use crate::reader::{self, Request};

/// Everything a click on which is never navigation, nor a click away from
/// a verse selection: the top bar (the translation, Aa, the ☰ menu, the
/// reading-mode exit), the picker, the Library, About, the search panel,
/// the dialogs and the toast, which answer their own.
const NOT_THE_READERS: &str = "#top-bar, #picker, #library-panel, #about-overlay, #search-panel, #import-overlay, #welcome-overlay, #note-overlay, #export-notes-overlay, #shortcuts-overlay, #data-status";

/// The reader's own controls in the pane (the chapter's Cross references,
/// its steps, a neighbor's heading, the lists' disclosures), which it
/// answers after this: none is a click away from the selection.
const THE_READERS: &str =
    ".xref-summary-chip, .chapter-step, .disclosure-toggle, .chapter-heading-link";

fn closest(el: &Element, selector: &str) -> Option<Element> {
    el.closest(selector).ok().flatten()
}

fn closest_html(el: &Element, selector: &str) -> Option<HtmlElement> {
    closest(el, selector)?.dyn_into::<HtmlElement>().ok()
}

fn data(el: &Element, name: &str) -> Option<String> {
    el.get_attribute(&format!("data-{name}"))
}

fn number(el: &Element, name: &str) -> Option<i64> {
    data(el, name)?.trim().parse().ok()
}

/// A selectable verse's column and number.
fn verse_of(verse: &Element) -> Option<(String, i64)> {
    Some((data(verse, "translation")?, number(verse, "verse")?))
}

/// Text is selected on the page: a click that ends a drag-highlight is the
/// reader selecting text, not choosing a verse.
fn text_selected() -> bool {
    web_sys::window()
        .and_then(|w| w.get_selection().ok().flatten())
        .is_some_and(|selection| !selection.is_collapsed())
}

/// The selection card's Go to: a jump, recorded in the history (Back
/// returns), the passage's verses selected.
fn jump_to(go: &Element) {
    let (Some(book_id), Some(chapter)) = (number(go, "book"), number(go, "chapter")) else {
        return;
    };
    let load = reader::ask(Request::Jump {
        book_id,
        chapter,
        start: number(go, "verse"),
        end: number(go, "end-verse"),
    });
    spawn_local(async move {
        load.await;
    });
}

thread_local! {
    /// Shift was held when the last mouse button went down.
    static PRESSED_WITH_SHIFT: Cell<bool> = const { Cell::new(false) };
}

fn on_click(event: &MouseEvent) {
    // A click is a Shift-click if Shift was down when its button went down,
    // as the browser's own Shift-click selection goes by: letting go of
    // Shift a moment before the button doesn't make it a plain click. A
    // click from the keyboard (no press, `detail` 0) goes by its own.
    let pressed_with_shift = PRESSED_WITH_SHIFT.take() && event.detail() > 0;
    let Some(target) = event.target().and_then(|t| t.dyn_into::<Element>().ok()) else {
        return;
    };
    let ask = |what: Ask| {
        card::ask(what);
    };
    // The cross references' credit links are the reader's, which has the
    // backend open them.
    if closest(&target, "a.credit-link").is_some() {
        return;
    }
    // A click anywhere outside the search panel closes it.
    if page::search_shown() && closest(&target, "#search-panel, #search-toggle").is_none() {
        page::set_search_open(false);
    }
    if closest(&target, NOT_THE_READERS).is_some() {
        return;
    }

    // The selection card's controls, Copy as… and Copy chapter….
    if closest(&target, ".verse-copy-btn").is_some() {
        return ask(Ask::Copy);
    }
    if let Some(item) = closest(&target, ".copy-as-item") {
        return ask(Ask::CopyFormat(data(&item, "format").unwrap_or_default()));
    }
    if let Some(copy_as) = closest_html(&target, ".verse-copy-as-btn, .chapter-copy-btn") {
        let slot = if copy_as.class_list().contains("chapter-copy-btn") {
            Slot::Chapter
        } else {
            Slot::Selection
        };
        return ask(Ask::CopyAs {
            slot,
            opener: Some(copy_as),
        });
    }
    // Any other click closes Copy as….
    ask(Ask::CloseCopyMenu);
    if closest(&target, ".verse-bookmark-btn").is_some() {
        return ask(Ask::Bookmark);
    }
    if let Some(swatch) = closest(&target, ".highlight-swatch") {
        return ask(Ask::Highlight(data(&swatch, "color")));
    }
    if closest(&target, ".highlight-remove").is_some() {
        return ask(Ask::Highlight(None));
    }
    if let Some(note_add) = closest_html(&target, ".verse-note-btn") {
        return ask(Ask::Note(Some(note_add)));
    }
    if closest(&target, ".status-undo").is_some() {
        return ask(Ask::Undo);
    }
    if closest(&target, ".verse-actions").is_some() {
        return;
    }

    // The selection's notes: Edit opens one; "more" shows all of one's
    // text; Copy as Markdown copies one; "Show all" shows every one.
    if let Some(edit) = closest_html(&target, ".note-edit") {
        return ask(Ask::NoteEdit {
            note_id: data(&edit, "note-id").unwrap_or_default(),
            opener: Some(edit),
        });
    }
    if let Some(more) = closest_html(&target, ".note-more") {
        return ask(Ask::NoteMore(more));
    }
    if let Some(copy) = closest_html(&target, "#selection-card .note-copy") {
        return ask(Ask::NoteCopy(number(&copy, "note-id").unwrap_or_default()));
    }
    if closest(&target, ".note-show-all").is_some() {
        return ask(Ask::ShowAll);
    }
    // A cross-reference chip in the card shows that passage in the card;
    // its Go to button reads it, selected (Back returns).
    if let Some(chip) = closest_html(&target, "#selection-card .xref-citation") {
        return ask(Ask::Xref(chip));
    }
    if let Some(go) = closest(&target, "#selection-card .xref-preview-go") {
        return jump_to(&go);
    }
    if closest(&target, THE_READERS).is_some() {
        return;
    }
    // The rest of the card (its footnotes and notes, to read and select) is
    // never a click away from the selection.
    if closest(&target, "#selection-card").is_some() {
        return;
    }

    // A selection never extends outside the chapter being read: Shift-click
    // in a faded neighbor leaves it as is.
    let shift = event.shift_key() || pressed_with_shift;
    if shift && closest(&target, ".chapter-side").is_some() {
        return;
    }

    // Clicking a footnote marker selects its verse, showing its footnotes;
    // unlike the rest of the verse, it never clears the selection.
    let marked = closest(&target, ".fn-marker").and_then(|marker| closest(&marker, SELECTABLE));
    if let Some(marked) = marked.filter(|_| !shift) {
        if let Some((translation_code, n)) = verse_of(&marked) {
            ask(Ask::Select {
                translation_code,
                anchor: n,
                focus: n,
            });
        }
        return;
    }

    // Clicking a verse selects it (clicking it again clears it), showing
    // its cross-references, if any, and its actions underneath until the
    // reader clicks away. Shift-click selects the whole range from the
    // verse first clicked through this one, within the same column.
    if let Some(verse) = closest(&target, SELECTABLE) {
        let Some((translation_code, verse)) = verse_of(&verse) else {
            return;
        };
        if shift {
            return ask(Ask::Extend {
                translation_code,
                verse,
            });
        }
        if text_selected() {
            return;
        }
        return ask(Ask::Toggle {
            translation_code,
            verse,
        });
    }

    // Anything else is a click away from the current selection.
    ask(Ask::Clear);
}

/// Listens on the body for clicks, and for Shift's `mousedown` on a verse:
/// Shift-click is for verse ranges in the chapter being read, so it mustn't
/// also extend the browser's own text selection there. Called by
/// `page::mount`, before the reader's own listener on the body.
pub fn mount() {
    let Some(body) = bridge::document().and_then(|d| d.body()) else {
        return;
    };
    dom::on(&body, "click", |event| {
        if let Some(event) = event.dyn_ref::<MouseEvent>() {
            on_click(event);
        }
    });
    dom::on(&body, "mousedown", |event| {
        let Some(event) = event.dyn_ref::<MouseEvent>() else {
            return;
        };
        PRESSED_WITH_SHIFT.set(event.button() == 0 && event.shift_key());
        let on_verse = event
            .target()
            .and_then(|t| t.dyn_into::<Element>().ok())
            .is_some_and(|t| closest(&t, SELECTABLE).is_some());
        if event.shift_key() && on_verse {
            event.prevent_default();
        }
    });
}
