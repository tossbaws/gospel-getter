//! The selection card: the reader's verse selection and everything about
//! it, in one card that floats over the text just under the selected verses
//! (or just above, when there's no room below), so selecting never moves
//! the text. It sits in the DOM straight after the last selected verse, so
//! Tab reaches it right after the selection, but it's out of the flow
//! (absolutely placed in the reading column). In order: the actions (Copy,
//! Copy as…, Bookmark, + Add note, the highlighter) with the citation and
//! the status line, the selection's footnotes, its notes, and its
//! cross-references as chips, one of which can be read in place (the
//! preview, with Go to).
//!
//! Also here: Copy and Copy as… for the selection and for the chapter
//! (Copy chapter…), the verses' bookmark, highlight and note marks, and the
//! Shift ghost (holding Shift makes the card faint, and clicks go through
//! it, so a Shift-click reaches a verse under it).
//!
//! The reader (`reader`) tells this what it shows: a chapter loading
//! ([`loading`]) and shown ([`shown`]), a passage to select
//! ([`select_passage`]) and a Library note to edit ([`edit_note`]). The
//! page's click and keyboard dispatchers, its boot, its import and its
//! palette commands ask this for what they need with `bridge::CARD_EVENT`
//! (see `bridge`): select, extend or clear the selection, copy, bookmark,
//! highlight, the notes' controls, and what the selection is. The Library
//! hands over its bookmarks (`bridge::LIBRARY_LOADED_EVENT`) and has the
//! highlights refreshed ([`refresh_highlights`]); the note editor opens
//! from here (`bridge::NOTE_EVENT`), has the notes refreshed
//! ([`refresh_notes`]) and says what it did (`bridge::NOTE_DONE_EVENT`).
//!
//! A stale answer never overwrites a newer one: the highlights, the
//! chapter's notes, the selection's notes and a preview each have their
//! token, and every action's status line its attempt (a newer action's
//! status, or a new selection, wins). Copying writes to the clipboard within
//! the click or key that asked (WebKitGTK refuses it afterwards), so each
//! passage's text is asked for as soon as it's selected or shown.
//!
//! This is the page's selection card (`ui/index.html` at 5af9b54:
//! `setActionStatus` to `placeCard`, `selectVerses`, Copy and Copy as…,
//! the bookmarks' and highlights' card halves, the notes' card half, and
//! the Shift ghost) in Rust: the same commands with the same arguments, in
//! the same order, the same markup (`card_text`), ids, classes, ARIA, text
//! and focus. Its elements are made once and moved in and out of the
//! reading pane as before, which the reader rebuilds whole per chapter.

use std::cell::RefCell;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::Closure;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::{JsFuture, spawn_local};
use web_sys::{CustomEvent, Element, HtmlElement, KeyboardEvent};

use crate::bridge;
use crate::card_text::{self as text, Change, Room, Selection, Side, Slot};
use crate::dom;
use crate::dto::{
    Bookmark, Compare, CopyText, CopyTextArgs, Highlight, PassageArgs, PassageText,
    PassageTextArgs, Reading, RemoveBookmarkArgs, SelectionNote, SetHighlightArgs, Verse,
};
use crate::ipc;
use crate::palette::PaletteState;

/// Every verse the reader can select: the chapter being read, in each
/// translation column it's shown in. Neither the faded neighbors nor
/// cross-reference text carry `data-translation`.
const SELECTABLE: &str = ".chapter-main .verse[data-translation]";

/// A future this runs to its end, its commands already sent.
type Task = Pin<Box<dyn Future<Output = ()>>>;

// ---- The card's elements, made once.

struct Elements {
    card: HtmlElement,
    actions: HtmlElement,
    footnotes: HtmlElement,
    xrefs: HtmlElement,
    preview: HtmlElement,
    notes: HtmlElement,
    menu: HtmlElement,
    citation: HtmlElement,
    status: HtmlElement,
    bookmark: HtmlElement,
    swatches: Vec<HtmlElement>,
    remove: HtmlElement,
    note_add: HtmlElement,
}

thread_local! {
    static ELEMENTS: RefCell<Option<Rc<Elements>>> = const { RefCell::new(None) };
    static STATE: RefCell<State> = RefCell::new(State::default());
}

fn elements() -> Option<Rc<Elements>> {
    ELEMENTS.with_borrow(Clone::clone)
}

/// The card's state, never held while the page is touched (focus moving,
/// or anything else that sends an event, may come back here).
fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with_borrow_mut(f)
}

// ---- State.

/// The chapter on screen, as the reader showed it: which one, and its
/// verses in each translation on screen, exactly as the backend returned
/// them. Anything acting on the selection reads verse text from here, never
/// from the rendered DOM, so verse numbers, labels and markup can't leak
/// into it.
struct Chapter {
    book_id: i64,
    book_name: String,
    chapter: i64,
    /// The translation being read (the left column while comparing).
    translation_code: String,
    reading: Rc<Reading>,
    /// The comparison on screen, if comparing.
    compare: Option<Rc<Compare>>,
    /// The first column of the last comparison fetched: a cross-reference
    /// previews in it while comparing.
    compare_first: Option<String>,
}

impl Chapter {
    /// A column's verses, as the backend sent them (none if it isn't on
    /// screen). The comparison's column replaces the reading's where they're
    /// the same translation, as on screen.
    fn column(&self, code: &str) -> &[Verse] {
        if let Some(column) = self
            .compare
            .as_ref()
            .and_then(|c| c.columns.iter().find(|c| c.code == code))
        {
            return &column.verses;
        }
        if code == self.translation_code {
            &self.reading.current_verses
        } else {
            &[]
        }
    }

    /// Every verse number on screen, in any column.
    fn all_numbers(&self) -> Vec<i64> {
        let mut numbers: Vec<i64> = self
            .column(&self.translation_code)
            .iter()
            .map(|v| v.number)
            .collect();
        if let Some(compare) = &self.compare {
            numbers.extend(
                compare
                    .columns
                    .iter()
                    .flat_map(|c| c.verses.iter().map(|v| v.number)),
            );
        }
        numbers
    }
}

/// The text of a passage to copy, as `get_copy_text` answered (or failed),
/// asked for once per passage: `done` settles when it has.
#[derive(Clone)]
struct CopyEntry {
    args: CopyTextArgs,
    answer: Rc<RefCell<Option<Result<CopyText, JsValue>>>>,
    done: js_sys::Promise,
}

/// A note on the selection, read, and as it came (for the editor).
#[derive(Clone)]
struct NoteRow {
    note: SelectionNote,
    raw: JsValue,
}

/// A note's verses (for the markers).
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NoteSpan {
    book_id: i64,
    chapter: i64,
    verse_start: i64,
    verse_end: i64,
}

#[derive(Default)]
struct State {
    chapter: Option<Chapter>,
    selection: Option<Selection>,
    /// The verse the card belongs to (the last selected one), where focus
    /// goes back to when it closes.
    card_verse: Option<HtmlElement>,
    /// The chip whose preview shows (pressed while it does).
    preview_chip: Option<HtmlElement>,
    preview_token: u64,
    /// Bumped by every action and selection: a status line for an older
    /// one is dropped.
    action_attempt: u64,
    status_timer: Option<i32>,
    /// What the status line's Undo restores (a deleted note), while it
    /// shows one.
    undo: Option<JsValue>,
    copy_selection: Option<CopyEntry>,
    copy_chapter: Option<CopyEntry>,
    /// Copy as… is open for this, from that button.
    menu_for: Option<Slot>,
    menu_opener: Option<HtmlElement>,
    /// The Library's bookmarks (for the markers and the Bookmark button).
    bookmarks: Vec<Bookmark>,
    bookmarks_loaded: bool,
    /// Every highlight, by book, chapter and verse. Highlights belong to no
    /// one translation, like bookmarks.
    highlights: HashMap<(i64, i64, i64), String>,
    highlights_loaded: bool,
    highlights_token: u64,
    chapter_notes: Vec<NoteSpan>,
    chapter_notes_token: u64,
    selection_notes: Vec<NoteRow>,
    selection_notes_error: String,
    selection_notes_token: u64,
    show_all_notes: bool,
    /// Shift is held (the ghost).
    shift_held: bool,
    /// The card's height when last placed by its observer.
    last_height: i32,
    /// Library lists asked for before the Library was there.
    library_waiting: Vec<String>,
}

impl State {
    fn copy(&mut self, slot: Slot) -> &mut Option<CopyEntry> {
        match slot {
            Slot::Selection => &mut self.copy_selection,
            Slot::Chapter => &mut self.copy_chapter,
        }
    }

    /// The selected verses, sorted, from the chapter (never from the DOM).
    fn selection_verses(&self) -> Vec<Verse> {
        let (Some(chapter), Some(selection)) = (&self.chapter, &self.selection) else {
            return Vec::new();
        };
        let mut verses: Vec<Verse> = chapter
            .column(&selection.translation_code)
            .iter()
            .filter(|v| v.number >= selection.start && v.number <= selection.end)
            .cloned()
            .collect();
        verses.sort_by_key(|v| v.number);
        verses
    }

    fn citation(&self) -> Option<String> {
        let (chapter, selection) = (self.chapter.as_ref()?, self.selection.as_ref()?);
        Some(text::citation(
            &chapter.book_name,
            chapter.chapter,
            selection.start,
            selection.end,
            &selection.translation_code,
        ))
    }

    /// The bookmark of exactly the selected verses, if there is one.
    fn selected_bookmark(&self) -> Option<Bookmark> {
        let (chapter, selection) = (self.chapter.as_ref()?, self.selection.as_ref()?);
        self.bookmarks
            .iter()
            .find(|b| {
                b.book_id == chapter.book_id
                    && b.chapter == chapter.chapter
                    && b.verse_start == selection.start
                    && b.verse_end == selection.end
            })
            .cloned()
    }

    /// The selected verses' colors (`None` where there's none).
    fn selected_colors(&self) -> Vec<Option<String>> {
        let Some(chapter) = &self.chapter else {
            return Vec::new();
        };
        if self.selection.is_none() {
            return Vec::new();
        }
        self.selection_verses()
            .iter()
            .map(|v| {
                self.highlights
                    .get(&(chapter.book_id, chapter.chapter, v.number))
                    .cloned()
            })
            .collect()
    }

    fn attempt(&mut self) -> u64 {
        self.action_attempt += 1;
        self.action_attempt
    }
}

// ---- The page's elements.

fn by_id(id: &str) -> Option<HtmlElement> {
    bridge::document()?
        .get_element_by_id(id)?
        .dyn_into::<HtmlElement>()
        .ok()
}

fn query_all(root: &Element, selector: &str) -> Vec<HtmlElement> {
    root.query_selector_all(selector)
        .map(|found| dom::elements(&found))
        .unwrap_or_default()
}

fn selectable() -> Vec<HtmlElement> {
    bridge::document()
        .and_then(|d| d.document_element())
        .map(|root| query_all(&root, SELECTABLE))
        .unwrap_or_default()
}

fn closest(el: &Element, selector: &str) -> Option<Element> {
    el.closest(selector).ok().flatten()
}

fn data_number(el: &Element, name: &str) -> Option<i64> {
    el.get_attribute(name)?.parse().ok()
}

fn toggle_class(el: &Element, name: &str, on: bool) {
    let _ = el.class_list().toggle_with_force(name, on);
}

fn has_class(el: &Element, name: &str) -> bool {
    el.class_list().contains(name)
}

/// A selectable verse, by column and number.
fn verse_element(code: &str, number: i64) -> Option<HtmlElement> {
    dom::query(&format!(
        r#"{SELECTABLE}[data-translation="{code}"][data-verse="{number}"]"#
    ))
}

fn active_element() -> Option<Element> {
    dom::active_element()
}

fn contains_focus(el: &Element) -> bool {
    active_element().is_some_and(|active| el.contains(Some(&active)))
}

/// `el.focus()`, which may scroll the page to it (as the page's plain
/// `focus()` calls did).
fn focus_scrolling(el: &HtmlElement) {
    let _ = el.focus();
}

/// A number property of an element (`scrollTop`, a double), read and
/// written as the page did, without rounding.
fn number_property(el: &Element, name: &str) -> f64 {
    js_sys::Reflect::get(el, &JsValue::from_str(name))
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0)
}

fn set_number_property(el: &Element, name: &str, value: f64) {
    let _ = js_sys::Reflect::set(el, &JsValue::from_str(name), &JsValue::from_f64(value));
}

/// `el.scrollIntoView({ block })`, where there is one (jsdom has none).
fn scroll_into_view(el: &Element, block: &str) {
    let Ok(scroll) = js_sys::Reflect::get(el, &JsValue::from_str("scrollIntoView")) else {
        return;
    };
    let Some(scroll) = scroll.dyn_ref::<js_sys::Function>() else {
        return;
    };
    let options = js_sys::Object::new();
    let _ = js_sys::Reflect::set(
        &options,
        &JsValue::from_str("block"),
        &JsValue::from_str(block),
    );
    let _ = scroll.call1(el, &options);
}

fn rect(el: &Element) -> text::Rect {
    let r = el.get_bounding_client_rect();
    text::Rect {
        left: r.left(),
        top: r.top(),
        bottom: r.bottom(),
        width: r.width(),
    }
}

/// The box of a verse's first or last line (its whole box when it's a
/// block, a verse to a line).
fn line_rect(verse: &Element, first: bool) -> text::Rect {
    let rects = verse.get_client_rects();
    let lines: Vec<web_sys::DomRect> = (0..rects.length())
        .filter_map(|i| rects.get(i))
        .filter(|r| r.width() > 0.0 || r.height() > 0.0)
        .collect();
    let line = if lines.len() < 2 {
        None
    } else if first {
        lines.first()
    } else {
        lines.last()
    };
    match line {
        Some(r) => text::Rect {
            left: r.left(),
            top: r.top(),
            bottom: r.bottom(),
            width: r.width(),
        },
        None => rect(verse),
    }
}

/// A computed style's length, as `parseFloat` reads it (NaN when it isn't
/// one).
fn computed_length(el: &Element, property: &str) -> f64 {
    web_sys::window()
        .and_then(|w| w.get_computed_style(el).ok().flatten())
        .and_then(|style| style.get_property_value(property).ok())
        .map_or(f64::NAN, |value| js_sys::parse_float(&value))
}

fn set_style(el: &HtmlElement, property: &str, value: &str) {
    let _ = el.style().set_property(property, value);
}

fn window_height() -> f64 {
    web_sys::window()
        .and_then(|w| w.inner_height().ok())
        .and_then(|h| h.as_f64())
        .unwrap_or(0.0)
}

fn later(ms: i32, f: impl FnOnce() + 'static) -> Option<i32> {
    let f = Closure::once_into_js(f);
    web_sys::window()?
        .set_timeout_with_callback_and_timeout_and_arguments_0(f.unchecked_ref(), ms)
        .ok()
}

fn cancel(timer: Option<i32>) {
    if let (Some(timer), Some(window)) = (timer, web_sys::window()) {
        window.clear_timeout_with_handle(timer);
    }
}

/// A value as a JavaScript template would write it: a string as itself,
/// `undefined` otherwise (the backend always sends the strings read so).
fn js_text(value: &JsValue) -> String {
    value.as_string().unwrap_or_else(|| "undefined".to_string())
}

fn get(value: &JsValue, key: &str) -> JsValue {
    js_sys::Reflect::get(value, &JsValue::from_str(key)).unwrap_or(JsValue::UNDEFINED)
}

fn send(cmd: &str, args: &impl Serialize) -> Result<js_sys::Promise, JsValue> {
    ipc::args(args).and_then(|args| ipc::send(cmd, args))
}

async fn answer(sent: Result<js_sys::Promise, JsValue>) -> Result<JsValue, JsValue> {
    JsFuture::from(sent?).await
}

fn read<T: serde::de::DeserializeOwned>(value: JsValue) -> Result<T, JsValue> {
    serde_wasm_bindgen::from_value(value).map_err(|e| JsValue::from(e.to_string()))
}

// ---- The status line.

/// Says `message` in the card's status line, for 2.5 s unless `duration`
/// says otherwise (an error stays). With `undo` (a deleted note), it ends
/// with an Undo button that restores it.
fn set_action_status(message: &str, is_error: bool, undo: Option<JsValue>, duration: i32) {
    let Some(els) = elements() else {
        return;
    };
    let has_undo = undo.is_some();
    let timer = with(|s| {
        s.undo = undo;
        s.status_timer.take()
    });
    cancel(timer);
    els.status.set_text_content(Some(message));
    toggle_class(&els.status, "is-error", is_error);
    if has_undo
        && let Some(document) = bridge::document()
        && let Ok(button) = document.create_element("button")
    {
        let _ = button.set_attribute("type", "button");
        button.set_class_name("note-link status-undo");
        button.set_text_content(Some("Undo"));
        let _ = els.status.append_with_node_1(&button);
    }
    if !message.is_empty() && !is_error {
        let timer = later(duration, || {
            let Some(els) = elements() else {
                return;
            };
            let had_focus = contains_focus(&els.status);
            status(String::new(), false);
            if had_focus && els.note_add.is_connected() {
                dom::focus(&els.note_add);
            }
        });
        with(|s| s.status_timer = timer);
    }
}

fn status(message: impl AsRef<str>, is_error: bool) {
    set_action_status(message.as_ref(), is_error, None, text::STATUS_MS);
}

/// Says `message` if `attempt` is still the latest action.
fn status_for(attempt: u64, message: impl AsRef<str>, is_error: bool) {
    if with(|s| s.action_attempt) == attempt {
        status(message, is_error);
    }
}

// ---- The selection.

/// Reflects the selection in the reading column: highlights the selected
/// verses and opens the card on them (or closes it, returning focus to the
/// verse if it was in the card). Nothing is added to the column's flow.
fn render_selection() {
    let Some(els) = elements() else {
        return;
    };
    with(|s| s.action_attempt += 1);
    status("", false);
    let had_focus = contains_focus(&els.card);
    let selection = with(|s| s.selection.clone());
    let mut last = None;
    for el in selectable() {
        let selected = selection.as_ref().is_some_and(|sel| {
            let code = el.get_attribute("data-translation").unwrap_or_default();
            data_number(&el, "data-verse").is_some_and(|n| sel.covers(&code, n))
        });
        toggle_class(&el, "verse-selected", selected);
        if selected && data_number(&el, "data-verse") == selection.as_ref().map(|s| s.end) {
            last = Some(el);
        }
    }
    close_menu(false);
    with(|s| {
        s.copy_selection = None;
        s.selection_notes_token += 1;
        s.selection_notes = Vec::new();
        s.selection_notes_error = String::new();
        s.show_all_notes = false;
    });
    els.notes.remove();
    let Some(last) = last else {
        close_preview(false);
        let verse = with(|s| s.card_verse.take());
        els.card.remove();
        if had_focus && let Some(verse) = verse.filter(|v| v.is_connected()) {
            let _ = verse.set_attribute("tabindex", "-1");
            dom::focus(&verse);
        }
        return;
    };
    with(|s| s.card_verse = Some(last.clone()));
    let copy = with(|s| {
        let (chapter, selection) = (s.chapter.as_ref()?, s.selection.as_ref()?);
        Some(CopyTextArgs {
            translation_code: selection.translation_code.clone(),
            book_id: chapter.book_id,
            chapter: chapter.chapter,
            start: Some(selection.start),
            end: Some(selection.end),
        })
    });
    if let Some(copy) = copy {
        fetch_copy(Slot::Selection, copy);
    }
    let citation = with(|s| s.citation()).unwrap_or_default();
    let _ = els
        .card
        .set_attribute("aria-label", &format!("Selection: {citation}"));
    els.citation.set_text_content(Some(&citation));
    render_card_references(&els);
    let _ = last.after_with_node_1(&els.card);
    update_bookmark_button();
    update_highlight_controls();
    place_card();
    spawn_local(refresh_selection_notes());
}

/// The selection's footnotes and cross-references, from the chapter as the
/// backend returned it. Cross-references are by verse number (the same in
/// every column); footnotes are the column's own. A range labels each with
/// its verse.
fn render_card_references(els: &Elements) {
    let (verses, range, chapter, xrefs) = with(|s| {
        let range = s.selection.as_ref().is_some_and(|sel| sel.start != sel.end);
        let chapter = s.chapter.as_ref().map_or(0, |c| c.chapter);
        let verses = s.selection_verses();
        let xrefs: Vec<(i64, Vec<crate::dto::Xref>)> = verses
            .iter()
            .filter_map(|v| {
                let found = s
                    .chapter
                    .as_ref()?
                    .reading
                    .current_verses
                    .iter()
                    .find(|c| c.number == v.number)
                    .map(|c| c.xrefs.clone())
                    .unwrap_or_default();
                (!found.is_empty()).then_some((v.number, found))
            })
            .collect();
        (verses, range, chapter, xrefs)
    });
    let notes: Vec<_> = verses
        .iter()
        .flat_map(|v| v.footnotes.iter().map(move |f| (v, f)))
        .collect();
    els.footnotes.set_hidden(notes.is_empty());
    els.footnotes
        .set_inner_html(&text::footnotes_html(&notes, range));
    let rows: Vec<(i64, &[crate::dto::Xref])> =
        xrefs.iter().map(|(n, x)| (*n, x.as_slice())).collect();
    els.xrefs.set_hidden(rows.is_empty());
    els.xrefs
        .set_inner_html(&text::xrefs_html(&rows, chapter, range));
    close_preview(false);
    let _ = els.xrefs.append_with_node_1(&els.preview);
}

fn select_verses(code: &str, anchor: i64, focus: i64) {
    with(|s| s.selection = Some(Selection::new(code, anchor, focus)));
    render_selection();
}

fn clear_selection() {
    if with(|s| s.selection.take()).is_none() {
        return;
    }
    render_selection();
}

fn apply(change: Change) {
    match change {
        Change::Select(selection) => {
            with(|s| s.selection = Some(selection));
            render_selection();
        }
        Change::Clear => clear_selection(),
        Change::Keep => {}
    }
}

// ---- A cross-reference read in place.

/// The passage's text comes from the translation being read; while
/// comparing, from the first (left) column's.
fn preview_translation() -> String {
    let comparing = bridge::document()
        .and_then(|d| d.document_element())
        .is_some_and(|html| html.get_attribute("data-compare").as_deref() == Some("on"));
    with(|s| {
        let chapter = s.chapter.as_ref();
        chapter
            .and_then(|c| c.compare_first.clone())
            .filter(|_| comparing)
            .or_else(|| chapter.map(|c| c.translation_code.clone()))
            .unwrap_or_default()
    })
}

fn close_preview(focus_chip: bool) {
    let Some(els) = elements() else {
        return;
    };
    let chip = with(|s| {
        s.preview_token += 1;
        s.preview_chip.take()
    });
    els.preview.set_hidden(true);
    els.preview.set_inner_html("");
    let Some(chip) = chip else {
        return;
    };
    let _ = chip.set_attribute("aria-expanded", "false");
    if focus_chip && chip.is_connected() {
        dom::focus(&chip);
    }
}

/// A chip shows its passage under its row, replacing any other preview;
/// the same chip again closes it. Nothing navigates.
fn toggle_preview(chip: HtmlElement) {
    let Some(els) = elements() else {
        return;
    };
    let same = with(|s| {
        s.preview_chip
            .as_ref()
            .is_some_and(|c| dom::same(Some(c), &chip))
    });
    if same {
        close_preview(false);
        return;
    }
    close_preview(false);
    with(|s| s.preview_chip = Some(chip.clone()));
    let _ = chip.set_attribute("aria-expanded", "true");
    if let Some(row) = closest(&chip, ".selection-xref-row") {
        let _ = row.append_with_node_1(&els.preview);
    }
    let book_id = data_number(&chip, "data-book").unwrap_or(0);
    let chapter = data_number(&chip, "data-chapter").unwrap_or(0);
    let start = data_number(&chip, "data-verse").unwrap_or(0);
    let end = data_number(&chip, "data-end-verse").unwrap_or(start);
    let code = preview_translation();
    let token = with(|s| s.preview_token);
    let sent = send(
        "get_passage",
        &PassageTextArgs {
            translation_code: &code,
            book_id,
            chapter,
            start,
            end,
        },
    );
    spawn_local(async move {
        let passage = answer(sent).await.and_then(read::<PassageText>);
        let failure = match &passage {
            Ok(_) => String::new(),
            Err(e) => {
                ipc::log_error("Failed to read a cross-reference", e);
                ipc::error_text(e)
            }
        };
        if with(|s| s.preview_token) != token {
            return;
        }
        // "1 John 4:9–10", as the backend names it (or, if it couldn't, the
        // chip's citation with its dash).
        let passage = passage.ok();
        let reference = match &passage {
            Some(p) => p.reference.clone(),
            None => text::chip_reference(&chip.text_content().unwrap_or_default()),
        };
        let notice = match &passage {
            Some(p) => p.notice.clone().unwrap_or_default(),
            None => text::preview_failed(&reference, &failure),
        };
        let Some(els) = elements() else {
            return;
        };
        els.preview
            .set_inner_html(&text::preview_html(&text::Preview {
                reference: &reference,
                code: &code,
                verses: passage.as_ref().map_or(&[], |p| p.verses.as_slice()),
                notice: &notice,
                book_id,
                chapter,
                start,
                end,
            }));
        els.preview.set_hidden(false);
        place_card();
        reveal_preview(&els);
    });
}

/// Scrolls the card, not the page, so the whole preview shows (or its
/// top, when it's taller than the card).
fn reveal_preview(els: &Elements) {
    let card = rect(&els.card);
    let r = els.preview.get_bounding_client_rect();
    let inset = js_sys::parse_float(
        &web_sys::window()
            .and_then(|w| w.get_computed_style(&els.card).ok().flatten())
            .and_then(|s| s.get_property_value("padding-bottom").ok())
            .unwrap_or_default(),
    );
    let inset = if inset.is_nan() { 0.0 } else { inset };
    let top = number_property(&els.card, "scrollTop");
    if r.bottom() + inset > card.bottom {
        let by = (r.bottom() + inset - card.bottom).min(r.top() - card.top);
        set_number_property(&els.card, "scrollTop", top + by);
    } else if r.top() < card.top {
        set_number_property(&els.card, "scrollTop", top - (card.top - r.top()));
    }
}

// ---- Where the card goes (`card_text`'s rules).

fn place_card() {
    let Some(els) = elements() else {
        return;
    };
    let card = &els.card;
    let (card_verse, preview_open) = with(|s| (s.card_verse.clone(), s.preview_chip.is_some()));
    let Some(card_verse) = card_verse.filter(|_| card.is_connected()) else {
        return;
    };
    let Some(column) = closest(card, ".chapter-main") else {
        return;
    };
    // A preview opening or closing doesn't move the card to the other side:
    // it grows (or scrolls) where it is.
    let keep = preview_open.then(|| {
        if has_class(card, "is-docked") {
            Side::Docked
        } else if has_class(card, "is-above") {
            Side::Above
        } else {
            Side::Below
        }
    });
    let scrolled = number_property(card, "scrollTop");
    let first: Element = dom::query(&format!("{SELECTABLE}.verse-selected"))
        .map_or_else(|| card_verse.clone().into(), Into::into);
    let flowing = bridge::document()
        .and_then(|d| d.document_element())
        .is_some_and(|html| html.get_attribute("data-layout").as_deref() == Some("flowing"))
        && card_verse
            .parent_element()
            .is_some_and(|p| has_class(&p, "chapter-text"));
    let column_rect = rect(&column);
    let first_rect = line_rect(&first, true);
    let last_rect = line_rect(&card_verse, false);
    let bar = by_id("top-bar").map_or(0.0, |b| b.get_bounding_client_rect().bottom());
    let (width, left) = text::card_box(column_rect, first_rect, flowing);
    let _ = card.class_list().remove_2("is-docked", "is-above");
    set_style(card, "max-height", "");
    set_style(
        card,
        "width",
        &if width != 0.0 && !width.is_nan() {
            text::px(width)
        } else {
            String::new()
        },
    );
    set_style(card, "left", &text::px(left));
    let cap = computed_length(card, "max-height");
    let cap = if cap.is_nan() || cap == 0.0 {
        f64::INFINITY
    } else {
        cap
    };
    let room = Room {
        wanted: f64::from(card.scroll_height()).min(cap),
        window: window_height(),
        bar,
        first: first_rect,
        last: last_rect,
    };
    let placement = text::placement(&room, keep);
    if placement.side == Side::Docked {
        let _ = card.class_list().add_1("is-docked");
        set_style(card, "left", &text::px(column_rect.left));
        set_style(card, "width", &text::px(column_rect.width));
        set_style(card, "top", "");
        set_number_property(card, "scrollTop", scrolled);
        return;
    }
    if let Some(max_height) = placement.max_height {
        set_style(card, "max-height", &text::px(max_height));
    }
    // Above, by its height once capped.
    let height = if placement.side == Side::Above {
        let _ = card.class_list().add_1("is-above");
        f64::from(card.offset_height())
    } else {
        0.0
    };
    let top = text::card_top(placement.side, column_rect, &room, height);
    set_style(card, "top", &text::px(top));
    set_number_property(card, "scrollTop", scrolled);
}

// ---- Copying (GH-10), Copy as… and Copy chapter. The text comes from the
// backend (`get_copy_text`, one formatter for every format, and for the
// notes export's quotes), asked for as soon as verses are selected or a
// chapter is shown: WebKitGTK only allows `writeText` during the click or
// keydown that asked for it, so the text must already be here by then.
// Should it not be yet, the copy waits for it and tries anyway, and says so
// if that fails.

fn fetch_copy(slot: Slot, args: CopyTextArgs) {
    if with(|s| s.copy(slot).as_ref().is_some_and(|e| e.args == args)) {
        return;
    }
    let answer_cell = Rc::new(RefCell::new(None));
    let sent = send("get_copy_text", &args);
    let store = answer_cell.clone();
    let done = wasm_bindgen_futures::future_to_promise(async move {
        let got = answer(sent).await.and_then(read::<CopyText>);
        // A failure is reported when a copy is asked for.
        if let Err(e) = &got {
            ipc::log_error("Failed to read the text to copy", e);
        }
        *store.borrow_mut() = Some(got);
        Ok(JsValue::UNDEFINED)
    });
    let entry = CopyEntry {
        args,
        answer: answer_cell,
        done,
    };
    with(|s| *s.copy(slot) = Some(entry));
}

/// `navigator.clipboard` and its `writeText`, or why not.
fn clipboard() -> Result<(JsValue, js_sys::Function), JsValue> {
    let unavailable = || JsValue::from(js_sys::Error::new("clipboard access is not available"));
    let window = web_sys::window().ok_or_else(unavailable)?;
    let clipboard = get(&get(&window, "navigator"), "clipboard");
    if clipboard.is_null() || clipboard.is_undefined() {
        return Err(unavailable());
    }
    let write = get(&clipboard, "writeText")
        .dyn_into::<js_sys::Function>()
        .map_err(|_| unavailable())?;
    Ok((clipboard, write))
}

/// The text in `format`, once the answer is in.
fn copy_text_in(
    answer: &Option<Result<CopyText, JsValue>>,
    format: &str,
) -> Option<Result<String, JsValue>> {
    match answer.as_ref()? {
        Ok(copy) => Some(Ok(copy.formats.get(format).unwrap_or_default().to_string())),
        Err(e) => Some(Err(e.clone())),
    }
}

/// Copies the selection or the chapter in `format`, and says so: in the
/// card's status line for the selection, in the toast for the chapter.
/// Must be called within the click or keydown that asked for it.
fn copy_passage(slot: Slot, format: &str) {
    let ready = with(|s| s.chapter.is_some() && (slot == Slot::Chapter || s.selection.is_some()));
    if !ready {
        return;
    }
    let Some(entry) = with(|s| s.copy(slot).clone()) else {
        return;
    };
    let Some(copy_format) = text::copy_format(format) else {
        return;
    };
    let attempt = with(State::attempt);
    let format = copy_format.format;
    let write: Pin<Box<dyn Future<Output = Result<JsValue, JsValue>>>> = match clipboard() {
        Err(e) => Box::pin(async move { Err(e) }),
        Ok((clipboard, write_text)) => match copy_text_in(&entry.answer.borrow(), format) {
            // In: written now, within the gesture.
            Some(Ok(copy)) => {
                let written = write_text.call1(&clipboard, &JsValue::from_str(&copy));
                Box::pin(async move { JsFuture::from(js_sys::Promise::resolve(&written?)).await })
            }
            _ => {
                let (done, cell) = (entry.done.clone(), entry.answer.clone());
                Box::pin(async move {
                    JsFuture::from(done).await?;
                    let copy = copy_text_in(&cell.borrow(), format)
                        .unwrap_or_else(|| Ok(String::new()))?;
                    let written = write_text.call1(&clipboard, &JsValue::from_str(&copy))?;
                    JsFuture::from(js_sys::Promise::resolve(&written)).await
                })
            }
        },
    };
    let report = move |message: String, is_error: bool| match slot {
        Slot::Chapter => bridge::toast(&message, is_error),
        Slot::Selection => status_for(attempt, message, is_error),
    };
    spawn_local(async move {
        match write.await {
            Ok(_) => {
                let reference = entry
                    .answer
                    .borrow()
                    .as_ref()
                    .and_then(|a| a.as_ref().ok())
                    .map(|c| c.reference.clone())
                    .unwrap_or_default();
                report(text::copied(&reference, copy_format), false);
            }
            Err(e) => {
                ipc::log_error("Failed to copy verses", &e);
                report(text::copy_failed(&ipc::error_text(&e)), true);
            }
        }
    });
}

/// Copy as…: a menu of the formats, under the card's actions for the
/// selection, or under the chapter's Copy chapter… control. One at a time;
/// ↑ and ↓ (and Home and End) move, Enter or a click copies, Escape or Tab
/// closes it, focus going back to the button that opened it.
fn open_menu(slot: Slot, opener: Option<HtmlElement>) {
    close_menu(false);
    let (Some(els), Some(opener)) = (elements(), opener) else {
        return;
    };
    let what = with(|s| {
        let chapter = s.chapter.as_ref()?;
        match slot {
            Slot::Chapter => Some(format!("{} {}", chapter.book_name, chapter.chapter)),
            Slot::Selection => s.citation(),
        }
    });
    let Some(what) = what else {
        return;
    };
    with(|s| {
        s.menu_for = Some(slot);
        s.menu_opener = Some(opener.clone());
    });
    let _ = els
        .menu
        .set_attribute("aria-label", &text::copy_menu_label(&what));
    match slot {
        Slot::Chapter => {
            if let Some(place) = closest(&opener, ".chapter-copy") {
                let _ = place.after_with_node_1(&els.menu);
            }
        }
        Slot::Selection => {
            let _ = els.actions.after_with_node_1(&els.menu);
        }
    }
    els.menu.set_hidden(false);
    let _ = opener.set_attribute("aria-expanded", "true");
    if let Some(item) = query_all(&els.menu, ".copy-as-item").first() {
        let options = web_sys::FocusOptions::new();
        options.set_prevent_scroll(slot == Slot::Selection);
        let _ = item.focus_with_options(&options);
    }
    if slot == Slot::Chapter {
        scroll_into_view(&els.menu, "nearest");
    }
}

fn close_menu(focus_opener: bool) {
    let Some(els) = elements() else {
        return;
    };
    let Some(opener) = with(|s| {
        s.menu_for.take()?;
        s.menu_opener.take()
    }) else {
        return;
    };
    els.menu.set_hidden(true);
    els.menu.remove();
    let _ = opener.set_attribute("aria-expanded", "false");
    if focus_opener && opener.is_connected() {
        dom::focus(&opener);
    }
}

fn choose_format(format: &str) {
    let slot = with(|s| s.menu_for);
    close_menu(true);
    if let Some(slot) = slot {
        copy_passage(slot, format);
    }
}

/// Enter and Space choose an item by clicking it, so the copy happens
/// within that click (the page's click handler asks for it).
fn on_menu_keydown(menu: &HtmlElement, event: &KeyboardEvent) {
    let items = query_all(menu, ".copy-as-item");
    let len = items.len() as i64;
    let active = active_element();
    let index = items
        .iter()
        .position(|item| dom::same(active.as_ref(), item.as_ref()))
        .map_or(-1, |i| i as i64);
    let at = |i: i64| items.get(i.rem_euclid(len.max(1)) as usize).cloned();
    let next = match event.key().as_str() {
        "ArrowDown" => at(index + 1),
        "ArrowUp" => at(index - 1 + len),
        "Home" => items.first().cloned(),
        "End" => items.last().cloned(),
        "Escape" => {
            event.prevent_default();
            event.stop_propagation();
            close_menu(true);
            return;
        }
        "Tab" => {
            event.prevent_default();
            close_menu(true);
            return;
        }
        key => {
            // Not a chapter turn while in the menu.
            if key == "ArrowLeft" || key == "ArrowRight" {
                event.stop_propagation();
            }
            return;
        }
    };
    event.prevent_default();
    event.stop_propagation();
    if let Some(next) = next {
        focus_scrolling(&next);
    }
}

// ---- Bookmarks (GH-11).

fn update_bookmark_button() {
    let Some(els) = elements() else {
        return;
    };
    let bookmarked = with(|s| s.selected_bookmark().is_some());
    els.bookmark
        .set_inner_html(text::bookmark_label(bookmarked));
}

/// Marks every bookmarked verse of the chapter on screen, in every column
/// (bookmarks belong to no one translation).
fn apply_bookmark_markers() {
    let spans = with(|s| {
        s.chapter.as_ref().map(|c| {
            s.bookmarks
                .iter()
                .filter(|b| b.book_id == c.book_id && b.chapter == c.chapter)
                .map(|b| (b.verse_start, b.verse_end))
                .collect::<Vec<_>>()
        })
    });
    for el in selectable() {
        let number = data_number(&el, "data-verse");
        let marked = spans.as_ref().is_some_and(|spans| {
            number.is_some_and(|n| spans.iter().any(|&(start, end)| n >= start && n <= end))
        });
        toggle_class(&el, "is-bookmarked", marked);
    }
}

/// The Library's bookmarks have loaded (or failed to: `items` is `null`).
fn on_library_loaded(event: &CustomEvent) {
    let detail = event.detail();
    if get(&detail, "list").as_string().as_deref() != Some("bookmarks") {
        return;
    }
    let items = get(&detail, "items");
    if !items.is_null() && !items.is_undefined() {
        match read::<Vec<Bookmark>>(items) {
            Ok(bookmarks) => with(|s| {
                s.bookmarks = bookmarks;
                s.bookmarks_loaded = true;
            }),
            Err(e) => ipc::log_error("The bookmarks can't be read", &e),
        }
    }
    apply_bookmark_markers();
    if with(|s| s.selection.is_some()) {
        update_bookmark_button();
    }
}

fn toggle_bookmark() -> Task {
    let ready = with(|s| s.selection.is_some() && s.chapter.is_some());
    if !ready {
        return Box::pin(async {});
    }
    let (attempt, existing, args) = with(|s| {
        let attempt = s.attempt();
        let existing = s.selected_bookmark();
        let args = s
            .chapter
            .as_ref()
            .zip(s.selection.as_ref())
            .map(|(c, sel)| PassageArgs {
                book_id: c.book_id,
                chapter: c.chapter,
                verse_start: sel.start,
                verse_end: sel.end,
            });
        (attempt, existing, args)
    });
    let removing = existing.is_some();
    let sent = match (&existing, args) {
        (Some(existing), _) => send("remove_bookmark", &RemoveBookmarkArgs { id: existing.id }),
        (None, Some(args)) => send("add_bookmark", &args),
        (None, None) => return Box::pin(async {}),
    };
    Box::pin(async move {
        if let Err(e) = answer(sent).await {
            ipc::log_error("Failed to change bookmark", &e);
            status_for(
                attempt,
                text::bookmark_failed(removing, &ipc::error_text(&e)),
                true,
            );
            return;
        }
        refresh_library(&["bookmarks"]).await;
        status_for(attempt, text::bookmark_done(removing), false);
    })
}

// ---- Highlights.

/// Reloads the highlights (only the latest reload is shown: an earlier one
/// answering after it is dropped), then marks them. The Library's list of
/// them is reloaded by whoever asked (as the page's `refreshHighlights`
/// went on to do).
pub fn refresh_highlights() -> impl Future<Output = ()> + 'static {
    let token = with(|s| {
        s.highlights_token += 1;
        s.highlights_token
    });
    let sent = ipc::send("list_highlights", JsValue::UNDEFINED);
    async move {
        let got = answer(sent).await.and_then(read::<Vec<Highlight>>);
        if with(|s| s.highlights_token) != token {
            return;
        }
        match got {
            Ok(list) => with(|s| {
                s.highlights = list
                    .into_iter()
                    .map(|h| ((h.book_id, h.chapter, h.verse), h.color))
                    .collect();
                s.highlights_loaded = true;
            }),
            Err(e) => ipc::log_error("Failed to load highlights", &e),
        }
        apply_highlight_markers();
        if with(|s| s.selection.is_some()) {
            update_highlight_controls();
        }
    }
}

fn set_highlight_marker(el: &Element, color: Option<&str>) {
    match color {
        Some(color) => {
            let _ = el.set_attribute("data-highlight", color);
        }
        None => {
            let _ = el.remove_attribute("data-highlight");
        }
    }
}

/// Shows every highlight on screen: in the chapter being read (every
/// column, while comparing) and the faded neighbors.
fn apply_highlight_markers() {
    let (here, highlights) = with(|s| {
        (
            s.chapter.as_ref().map(|c| (c.book_id, c.chapter)),
            s.highlights.clone(),
        )
    });
    for el in selectable() {
        let color = here
            .zip(data_number(&el, "data-verse"))
            .and_then(|((book, chapter), verse)| highlights.get(&(book, chapter, verse)));
        set_highlight_marker(&el, color.map(String::as_str));
    }
    let Some(root) = bridge::document().and_then(|d| d.document_element()) else {
        return;
    };
    for el in query_all(&root, ".chapter-side .verse[data-verse]") {
        let key = (
            data_number(&el, "data-book"),
            data_number(&el, "data-chapter"),
            data_number(&el, "data-verse"),
        );
        let color = match key {
            (Some(book), Some(chapter), Some(verse)) => highlights.get(&(book, chapter, verse)),
            _ => None,
        };
        set_highlight_marker(&el, color.map(String::as_str));
    }
}

/// A swatch is pressed when the whole selection has its color; Remove
/// highlight shows when any selected verse has one.
fn update_highlight_controls() {
    let Some(els) = elements() else {
        return;
    };
    let colors = with(|s| s.selected_colors());
    let colors: Vec<Option<&str>> = colors.iter().map(Option::as_deref).collect();
    let (pressed, any) = text::highlight_controls(&colors);
    for (swatch, pressed) in els.swatches.iter().zip(pressed) {
        let _ = swatch.set_attribute("aria-pressed", &pressed.to_string());
    }
    let had_focus = dom::same(active_element().as_ref(), els.remove.as_ref());
    els.remove.set_hidden(!any);
    if had_focus
        && !any
        && let Some(first) = els.swatches.first()
    {
        focus_scrolling(first);
    }
}

/// Highlights every selected verse in `color`, replacing any color they
/// had, or (with `None`) removes their highlights.
fn highlight_selection(color: Option<String>) -> Task {
    let found = with(|s| {
        let (chapter, selection) = (s.chapter.as_ref()?, s.selection.as_ref()?);
        let found = (
            chapter.book_id,
            chapter.chapter,
            selection.start,
            selection.end,
        );
        Some((s.attempt(), found))
    });
    let Some((attempt, (book_id, chapter, start, end))) = found else {
        return Box::pin(async {});
    };
    let sent = match &color {
        Some(color) => send(
            "set_highlight",
            &SetHighlightArgs {
                book_id,
                chapter,
                verse_start: start,
                verse_end: end,
                color,
            },
        ),
        None => send(
            "remove_highlight",
            &PassageArgs {
                book_id,
                chapter,
                verse_start: start,
                verse_end: end,
            },
        ),
    };
    Box::pin(async move {
        if let Err(e) = answer(sent).await {
            ipc::log_error("Failed to change the highlight", &e);
            status_for(
                attempt,
                text::highlight_failed(color.as_deref(), &ipc::error_text(&e)),
                true,
            );
            return;
        }
        with(|s| {
            for verse in start..=end {
                match &color {
                    Some(color) => {
                        s.highlights
                            .insert((book_id, chapter, verse), color.clone());
                    }
                    None => {
                        s.highlights.remove(&(book_id, chapter, verse));
                    }
                }
            }
        });
        apply_highlight_markers();
        if with(|s| s.selection.is_some()) {
            update_highlight_controls();
        }
        status_for(attempt, text::highlight_done(color.as_deref()), false);
        // The Library's list, not waited for (as before).
        spawn_local(refresh_library(&["highlights"]));
    })
}

// ---- Notes: the reader's own words on a passage. Like bookmarks they
// belong to no translation, and a passage can have any number of them.
// Which notes overlap a passage, and their order, is the backend's
// (`list_notes`); this shows them. Every change reloads all three views (the
// markers, the selection's Notes section and the Library), so none of them
// goes stale.

/// Marks every verse of the chapter on screen that a note covers, in every
/// column, with the pencil (CSS) and, for screen readers, one hidden "has
/// notes" per verse.
fn apply_note_markers() {
    let spans = with(|s| {
        s.chapter.as_ref().map(|c| {
            s.chapter_notes
                .iter()
                .filter(|n| n.book_id == c.book_id && n.chapter == c.chapter)
                .map(|n| (n.verse_start, n.verse_end))
                .collect::<Vec<_>>()
        })
    });
    let Some(document) = bridge::document() else {
        return;
    };
    for el in selectable() {
        let number = data_number(&el, "data-verse");
        let noted = spans.as_ref().is_some_and(|spans| {
            number.is_some_and(|n| spans.iter().any(|&(start, end)| n >= start && n <= end))
        });
        toggle_class(&el, "has-notes", noted);
        let cue = el.query_selector(":scope > .note-cue").ok().flatten();
        match cue {
            None if noted => {
                if let (Ok(added), Some(number)) = (
                    document.create_element("span"),
                    el.query_selector(".verse-num").ok().flatten(),
                ) {
                    added.set_class_name("note-cue visually-hidden");
                    added.set_text_content(Some("(has notes) "));
                    let _ = number.after_with_node_1(&added);
                }
            }
            Some(cue) if !noted => cue.remove(),
            _ => {}
        }
    }
}

fn refresh_chapter_notes() -> Task {
    let (token, args) = with(|s| {
        s.chapter_notes_token += 1;
        let args = s.chapter.as_ref().map(|c| PassageArgs {
            book_id: c.book_id,
            chapter: c.chapter,
            verse_start: 1,
            verse_end: c.all_numbers().into_iter().fold(1, i64::max),
        });
        (s.chapter_notes_token, args)
    });
    let Some(args) = args else {
        return Box::pin(async {});
    };
    let sent = send("list_notes", &args);
    Box::pin(async move {
        let got = answer(sent).await.and_then(read::<Vec<NoteSpan>>);
        if with(|s| s.chapter_notes_token) != token {
            return;
        }
        let notes = got.unwrap_or_else(|e| {
            ipc::log_error("Failed to load the chapter’s notes", &e);
            Vec::new()
        });
        with(|s| s.chapter_notes = notes);
        apply_note_markers();
    })
}

fn refresh_selection_notes() -> Task {
    let (token, args) = with(|s| {
        s.selection_notes_token += 1;
        let args = s
            .chapter
            .as_ref()
            .zip(s.selection.as_ref())
            .map(|(c, sel)| PassageArgs {
                book_id: c.book_id,
                chapter: c.chapter,
                verse_start: sel.start,
                verse_end: sel.end,
            });
        (s.selection_notes_token, args)
    });
    let Some(args) = args else {
        return Box::pin(async {});
    };
    let sent = send("list_notes", &args);
    Box::pin(async move {
        let got = answer(sent).await.and_then(|raw| {
            let notes: Vec<SelectionNote> = read(raw.clone())?;
            let raw = js_sys::Array::from(&raw);
            Ok(notes
                .into_iter()
                .zip(raw.iter())
                .map(|(note, raw)| NoteRow { note, raw })
                .collect::<Vec<_>>())
        });
        if with(|s| s.selection_notes_token) != token {
            return;
        }
        match got {
            Ok(notes) => with(|s| {
                s.selection_notes = notes;
                s.selection_notes_error = String::new();
            }),
            Err(e) => {
                ipc::log_error("Failed to load notes", &e);
                with(|s| {
                    s.selection_notes = Vec::new();
                    s.selection_notes_error = text::notes_failed(&ipc::error_text(&e));
                });
            }
        }
        render_notes_section();
        // The note editor, if it's open and adding, lists them too.
        dispatch_others();
    })
}

/// The selection's notes, as the note editor lists the others.
#[derive(Serialize)]
struct Other {
    reference: String,
    date: String,
    body: String,
}

fn others() -> Vec<Other> {
    with(|s| {
        s.selection_notes
            .iter()
            .map(|n| Other {
                reference: n.note.reference.clone(),
                date: n.note.created_at_display.clone(),
                body: n.note.body.clone(),
            })
            .collect()
    })
}

fn dispatch_others() {
    #[derive(Serialize)]
    struct Others {
        others: Vec<Other>,
    }
    bridge::dispatch(bridge::NOTE_OTHERS_EVENT, &Others { others: others() });
}

/// The selection's Notes section, under its actions: shown only when notes
/// overlap it (or they couldn't be loaded).
fn render_notes_section() {
    let Some(els) = elements() else {
        return;
    };
    let shown = with(|s| {
        let (chapter, selection) = (s.chapter.as_ref()?, s.selection.as_ref()?);
        if s.selection_notes.is_empty() && s.selection_notes_error.is_empty() {
            return None;
        }
        let notes: Vec<SelectionNote> = s.selection_notes.iter().map(|n| n.note.clone()).collect();
        Some((
            text::reference(
                &chapter.book_name,
                chapter.chapter,
                selection.start,
                selection.end,
            ),
            notes,
            s.selection_notes_error.clone(),
            s.show_all_notes,
        ))
    });
    let Some((reference, notes, error, show_all)) = shown.filter(|_| els.actions.is_connected())
    else {
        els.notes.remove();
        place_card();
        return;
    };
    let had_focus = active_element().filter(|active| els.notes.contains(Some(active)));
    let _ = els
        .notes
        .set_attribute("aria-label", &format!("Notes for {reference}"));
    els.notes
        .set_inner_html(&text::notes_html(&notes, &error, show_all));
    let _ = els.xrefs.before_with_node_1(&els.notes);
    // "more" only where the text is cut short.
    for body in query_all(&els.notes, ".note-card-body[id]") {
        if let Some(more) = body
            .parent_element()
            .and_then(|p| p.query_selector(".note-more").ok().flatten())
            .and_then(|m| m.dyn_into::<HtmlElement>().ok())
        {
            more.set_hidden(body.scroll_height() <= body.client_height() + 1);
        }
    }
    place_card();
    if let Some(had_focus) = had_focus.filter(|el| !el.is_connected()) {
        let same = had_focus.get_attribute("data-note-id").and_then(|id| {
            query_all(&els.notes, &format!(r#".note-edit[data-note-id="{id}"]"#))
                .into_iter()
                .next()
        });
        dom::focus(same.as_ref().unwrap_or(&els.note_add));
    }
}

/// Reloads the chapter's notes (the markers), the selection's and the
/// Library's, after any change to notes, wherever it came from; the
/// commands are sent now, in that order, and this settles once all three
/// have.
pub fn refresh_notes() -> impl Future<Output = ()> + 'static {
    let chapter = to_promise(refresh_chapter_notes());
    let selection = to_promise(refresh_selection_notes());
    let library = to_promise(refresh_library(&["notes"]));
    let all = js_sys::Promise::all(&js_sys::Array::of3(&chapter, &selection, &library));
    async move {
        let _ = JsFuture::from(all).await;
    }
}

/// Runs `task` by itself (as the page's `async` functions ran), as a
/// promise of its end.
fn to_promise(task: Task) -> js_sys::Promise {
    wasm_bindgen_futures::future_to_promise(async move {
        task.await;
        Ok(JsValue::UNDEFINED)
    })
}

/// A note's verses, for the editor.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NotePassage {
    book_id: i64,
    chapter: i64,
    verse_start: i64,
    verse_end: i64,
}

/// What the note editor is asked to open with (`bridge::NoteRequest`, plus
/// `note` and `returnFocus`, set beside it).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NoteOpen<'a> {
    mode: &'a str,
    passage: PassageOut,
    reference: &'a str,
    code: &'a str,
    passage_html: &'a str,
    others: Vec<Other>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PassageOut {
    book_id: i64,
    chapter: i64,
    start: i64,
    end: i64,
}

/// Opens the note editor: adding a note on `passage` (`note` is `None`), or
/// editing `note` (as the backend gave it), with `code` the translation whose
/// text is quoted, and focus going back to `opener` when it closes.
fn open_note_panel(
    mode: &str,
    note: Option<JsValue>,
    passage: (i64, i64, i64, i64),
    reference: String,
    code: &str,
    opener: Option<HtmlElement>,
) {
    let (book_id, chapter, start, end) = passage;
    let Some(verses) = with(|s| {
        let here = s.chapter.as_ref()?;
        let mut verses: Vec<Verse> = if here.book_id == book_id && here.chapter == chapter {
            here.column(code)
                .iter()
                .filter(|v| v.number >= start && v.number <= end)
                .cloned()
                .collect()
        } else {
            Vec::new()
        };
        verses.sort_by_key(|v| v.number);
        Some(verses)
    }) else {
        return;
    };
    bridge::modal_opening(true);
    // The verses as this translation numbers and shows them (KJV supplied
    // words in italics, no note markers); any it doesn't number are said to
    // be missing.
    let missing = end - start + 1 - verses.len() as i64;
    let quoted: Vec<&Verse> = verses.iter().collect();
    let passage_html = text::note_passage_html(&quoted, missing, code);
    let open = NoteOpen {
        mode,
        passage: PassageOut {
            book_id,
            chapter,
            start,
            end,
        },
        reference: &reference,
        code,
        passage_html: &passage_html,
        others: if mode == "add" { others() } else { Vec::new() },
    };
    let Ok(detail) = ipc::args(&open) else {
        return;
    };
    let set = |key: &str, value: &JsValue| {
        let _ = js_sys::Reflect::set(&detail, &JsValue::from_str(key), value);
    };
    set("note", &note.unwrap_or(JsValue::NULL));
    set("returnFocus", &opener.map_or(JsValue::NULL, JsValue::from));
    let init = web_sys::CustomEventInit::new();
    init.set_cancelable(true);
    init.set_detail(&detail);
    let (Some(document), Ok(request)) = (
        bridge::document(),
        CustomEvent::new_with_event_init_dict(bridge::NOTE_EVENT, &init),
    ) else {
        return;
    };
    if document.dispatch_event(&request).unwrap_or(true) {
        bridge::toast(text::EDITOR_UNAVAILABLE, true);
    }
}

/// `note` (as the backend gave it): its verses and name, if it can be read.
fn note_passage(note: &JsValue) -> Option<((i64, i64, i64, i64), String)> {
    let passage: NotePassage = serde_wasm_bindgen::from_value(note.clone()).ok()?;
    Some((
        (
            passage.book_id,
            passage.chapter,
            passage.verse_start,
            passage.verse_end,
        ),
        js_text(&get(note, "reference")),
    ))
}

fn edit(note: JsValue, code: &str, opener: Option<HtmlElement>) {
    let Some((passage, reference)) = note_passage(&note) else {
        ipc::log_error("The note to edit can't be read", &note);
        return;
    };
    open_note_panel("edit", Some(note), passage, reference, code, opener);
}

/// + Add note, or N: a note on the selected verses.
fn open_add_note(opener: Option<HtmlElement>) {
    let found = with(|s| {
        let (chapter, selection) = (s.chapter.as_ref()?, s.selection.as_ref()?);
        Some((
            (
                chapter.book_id,
                chapter.chapter,
                selection.start,
                selection.end,
            ),
            text::reference(
                &chapter.book_name,
                chapter.chapter,
                selection.start,
                selection.end,
            ),
            selection.translation_code.clone(),
        ))
    });
    if let Some((passage, reference, code)) = found {
        open_note_panel("add", None, passage, reference, &code, opener);
    }
}

/// A Library note's Edit has opened its passage (the reader jumped there,
/// the passage selected, so its Notes section shows): the note opens.
pub fn edit_note(note: &JsValue) {
    if note.is_null() || note.is_undefined() {
        return;
    }
    let code = with(|s| {
        let chapter = s.chapter.as_ref()?;
        Some(s.selection.as_ref().map_or_else(
            || chapter.translation_code.clone(),
            |sel| sel.translation_code.clone(),
        ))
    });
    if let Some(code) = code {
        edit(note.clone(), &code, None);
    }
}

/// What the editor did, its notes already refreshed.
fn on_note_done(event: &CustomEvent) {
    let Some(els) = elements() else {
        return;
    };
    let detail = event.detail();
    let action = js_text(&get(&detail, "action"));
    let note = get(&detail, "note");
    let card_open = els.actions.is_connected();
    match action.as_str() {
        "deleted" if card_open => {
            set_action_status(text::NOTE_DELETED, false, Some(note), text::UNDO_MS);
        }
        "deleted" => {
            bridge::toast(
                &text::note_deleted_elsewhere(&js_text(&get(&note, "reference"))),
                false,
            );
        }
        "saved" if card_open => status(text::NOTE_SAVED, false),
        _ if card_open => status(text::NOTE_ADDED, false),
        _ => {}
    }
}

/// Undo: the same passage, text and dates (each as the note had it).
fn restore_note(note: JsValue) -> Task {
    let attempt = with(State::attempt);
    let args = js_sys::Object::new();
    for (key, from) in [
        ("bookId", "bookId"),
        ("chapter", "chapter"),
        ("verseStart", "verseStart"),
        ("verseEnd", "verseEnd"),
        ("body", "body"),
        ("createdAt", "createdAt"),
        ("updatedAt", "updatedAt"),
    ] {
        let _ = js_sys::Reflect::set(&args, &JsValue::from_str(key), &get(&note, from));
    }
    let sent = ipc::send("restore_note", args.into());
    Box::pin(async move {
        if let Err(e) = answer(sent).await {
            ipc::log_error("Failed to restore the note", &e);
            status_for(attempt, text::restore_failed(&ipc::error_text(&e)), true);
            return;
        }
        refresh_notes().await;
        status_for(attempt, text::NOTE_RESTORED, false);
    })
}

// ---- The Library's lists.

/// Has the Library reload `lists` (`bridge::LIBRARY_REFRESH_EVENT`),
/// settling once it has; asked before the Library is there, they're asked
/// for again once every piece is (`bridge::READY_EVENT`).
fn refresh_library(lists: &[&str]) -> Task {
    match bridge::refresh_library(lists) {
        Some(done) => Box::pin(async move {
            let _ = JsFuture::from(done).await;
        }),
        None => {
            with(|s| {
                for list in lists {
                    if !s.library_waiting.iter().any(|l| l == list) {
                        s.library_waiting.push((*list).to_string());
                    }
                }
            });
            Box::pin(async {})
        }
    }
}

// ---- The Shift ghost. Holding Shift makes the card a ghost: faint, and
// clicks go through it, so a Shift-click reaches a verse it covers and
// extends the selection. Shift is followed from the keyboard (keydown and
// keyup), and let go of on blur or when the window is hidden, so a Shift
// released elsewhere never leaves the card a ghost. It's only visual: the
// card stays in the accessibility tree and focus doesn't move.

/// Whether an overlay of the page's pieces is open (looked up by id).
fn overlay_open(id: &str) -> bool {
    by_id(id).is_some_and(|overlay| !overlay.hidden())
}

/// Whether Shift may ghost the card now. Not while Shift is for something
/// else: typing in a field, the search box or palette, a dialog or sheet,
/// Copy as… or another menu or popover, or focus in the card itself
/// (Shift+Tab moves through it).
fn ghost_allowed(els: &Elements) -> bool {
    if !els.card.is_connected() {
        return false;
    }
    let overlays = [
        "import-overlay",
        "about-overlay",
        "note-overlay",
        "export-notes-overlay",
        "shortcuts-overlay",
        "welcome-overlay",
    ];
    if overlays.iter().any(|id| overlay_open(id)) {
        return false;
    }
    if by_id("search-panel").is_some_and(|p| !p.hidden()) || !els.menu.hidden() {
        return false;
    }
    let body = bridge::document().and_then(|d| d.body());
    let Some(active) = active_element() else {
        return true;
    };
    if body.is_some_and(|body| dom::same(Some(&active), body.as_ref())) {
        return true;
    }
    if els.card.contains(Some(&active)) {
        return false;
    }
    let tag = active.tag_name();
    let editable = active
        .dyn_ref::<HtmlElement>()
        .is_some_and(HtmlElement::is_content_editable);
    if matches!(tag.as_str(), "INPUT" | "TEXTAREA" | "SELECT") || editable {
        return false;
    }
    closest(&active, r#"[role="dialog"], [role="menu"], .popover"#).is_none()
}

fn sync_ghost() {
    let Some(els) = elements() else {
        return;
    };
    let held = with(|s| s.shift_held);
    let ghost = held && ghost_allowed(&els);
    toggle_class(&els.card, "is-ghost", ghost);
}

fn release_shift() {
    with(|s| s.shift_held = false);
    sync_ghost();
}

fn listen_capturing(
    target: &web_sys::EventTarget,
    name: &str,
    handler: impl Fn(KeyboardEvent) + 'static,
) {
    let handler = Closure::<dyn Fn(KeyboardEvent)>::new(handler);
    let _ = target.add_event_listener_with_callback_and_bool(
        name,
        handler.as_ref().unchecked_ref(),
        true,
    );
    handler.forget();
}

// ---- What the reader tells this.

/// The reader has asked for another chapter or translation: the selection
/// goes now, not when the new text arrives, so nothing (Ctrl+C included)
/// acts on it in the meantime.
pub fn loading() {
    clear_selection();
}

/// The reader has put a chapter on screen (`#reading-pane-inner` is new):
/// the chapter's copy text, the selection and the verses' marks follow.
/// `compare` is the comparison on screen (while comparing), `compare_first`
/// the first column of the last one fetched.
pub fn shown(
    reading: Rc<Reading>,
    compare: Option<Rc<Compare>>,
    translation_code: &str,
    compare_first: Option<String>,
) {
    let chapter = Chapter {
        book_id: reading.current_book_id,
        book_name: reading.current_book_name.clone(),
        chapter: reading.current_chapter,
        translation_code: translation_code.to_string(),
        reading,
        compare,
        compare_first,
    };
    // Copy chapter copies the translation being read (while comparing, the
    // left column).
    let copy = CopyTextArgs {
        translation_code: chapter.translation_code.clone(),
        book_id: chapter.book_id,
        chapter: chapter.chapter,
        start: None,
        end: None,
    };
    with(|s| s.chapter = Some(chapter));
    close_menu(false);
    fetch_copy(Slot::Chapter, copy);
    with(|s| s.selection = None);
    render_selection();
    apply_bookmark_markers();
    apply_highlight_markers();
    apply_note_markers();
    spawn_local(refresh_chapter_notes());
}

/// A passage the reader opened (a jump, Back or Forward) whose verses are
/// numbered: selected, in the middle of the window. What's then selected,
/// for the reader's history.
pub fn select_passage(code: &str, start: i64, end: i64) -> Option<(i64, i64)> {
    select_verses(code, start, end);
    if let Some(el) = verse_element(code, start) {
        scroll_into_view(&el, "center");
    }
    place_card();
    with(|s| s.selection.as_ref().map(|sel| (sel.start, sel.end)))
}

/// What the palette's commands need of this: the chapter's text is ready
/// to copy.
pub fn palette_facts(state: &mut PaletteState) {
    state.chapter_copy = with(|s| s.copy_chapter.is_some());
}

// ---- The page's requests (`bridge::CARD_EVENT`).

/// What the card says it is: the chapter it acts on, the selection, and
/// what the welcome's fresh-install rule counts.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Facts {
    chapter: Option<ChapterFacts>,
    selection: Option<SelectionFacts>,
    bookmarks_loaded: bool,
    bookmarks: usize,
    highlights_loaded: bool,
    highlights: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ChapterFacts {
    book_id: i64,
    translation_code: String,
    /// The verse numbers of the column being read, in the backend's order.
    numbers: Vec<i64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SelectionFacts {
    translation_code: String,
    anchor: i64,
    start: i64,
    end: i64,
}

fn facts() -> Facts {
    with(|s| Facts {
        chapter: s.chapter.as_ref().map(|c| ChapterFacts {
            book_id: c.book_id,
            translation_code: c.translation_code.clone(),
            numbers: c
                .column(&c.translation_code)
                .iter()
                .map(|v| v.number)
                .collect(),
        }),
        selection: s.selection.as_ref().map(|sel| SelectionFacts {
            translation_code: sel.translation_code.clone(),
            anchor: sel.anchor,
            start: sel.start,
            end: sel.end,
        }),
        bookmarks_loaded: s.bookmarks_loaded,
        bookmarks: s.bookmarks.len(),
        highlights_loaded: s.highlights_loaded,
        highlights: s.highlights.len(),
    })
}

fn element(detail: &JsValue, key: &str) -> Option<HtmlElement> {
    get(detail, key).dyn_into::<HtmlElement>().ok()
}

fn number(detail: &JsValue, key: &str) -> Option<i64> {
    get(detail, key).as_f64().map(|n| n as i64)
}

fn has_selection() -> bool {
    with(|s| s.selection.is_some())
}

/// Answers one of the page's requests (`bridge::CARD_EVENT`), within it.
fn on_request(event: &CustomEvent) {
    let detail = event.detail();
    let action = js_text(&get(&detail, "action"));
    let slot = || {
        get(&detail, "slot")
            .as_string()
            .as_deref()
            .and_then(Slot::named)
    };
    let verse = || {
        get(&detail, "translationCode")
            .as_string()
            .zip(number(&detail, "verse"))
    };
    let answer = match action.as_str() {
        // Copy, and Ctrl+C: the selection, in the first format.
        "copy" => {
            let selected = has_selection();
            copy_passage(Slot::Selection, "reference");
            JsValue::from_bool(selected)
        }
        "copy-chapter" => {
            copy_passage(Slot::Chapter, "reference");
            JsValue::TRUE
        }
        // Copy as… clicked: opens, or closes if it's open for that.
        "copy-as" => {
            let Some(slot) = slot() else {
                return;
            };
            if with(|s| s.menu_for) == Some(slot) {
                close_menu(true);
            } else {
                open_menu(slot, element(&detail, "opener"));
            }
            JsValue::TRUE
        }
        // Ctrl+Shift+C, and the palette's Copy chapter as…: opens it.
        "copy-as-open" => {
            let Some(slot) = slot() else {
                return;
            };
            let opener = match slot {
                Slot::Selection => elements().and_then(|els| {
                    els.actions
                        .query_selector(".verse-copy-as-btn")
                        .ok()
                        .flatten()
                        .and_then(|b| b.dyn_into::<HtmlElement>().ok())
                }),
                Slot::Chapter => dom::query(".chapter-copy-btn"),
            };
            let can = slot == Slot::Chapter || has_selection();
            if can {
                open_menu(slot, opener);
            }
            JsValue::from_bool(can)
        }
        "copy-format" => {
            choose_format(&js_text(&get(&detail, "format")));
            JsValue::TRUE
        }
        "close-copy-menu" => {
            close_menu(false);
            JsValue::TRUE
        }
        "bookmark" => {
            spawn_local(toggle_bookmark());
            JsValue::TRUE
        }
        "highlight" => {
            spawn_local(highlight_selection(get(&detail, "color").as_string()));
            JsValue::TRUE
        }
        // + Add note, or N (which goes on to other keys without a
        // selection).
        "note" => {
            let selected = has_selection();
            if selected {
                open_add_note(element(&detail, "opener"));
            }
            JsValue::from_bool(selected)
        }
        "undo" => {
            let undo = with(|s| s.undo.clone());
            status("", false);
            if let Some(els) = elements() {
                dom::focus(&els.note_add);
            }
            if let Some(note) = undo {
                spawn_local(restore_note(note));
            }
            JsValue::TRUE
        }
        // The selection's notes: Edit opens one; "more" shows all of one's
        // text; "Show all" shows every one.
        "note-edit" => {
            let id = js_text(&get(&detail, "noteId"));
            let found = with(|s| {
                let selection = s.selection.as_ref()?;
                let note = s
                    .selection_notes
                    .iter()
                    .find(|n| n.note.id.to_string() == id)?;
                Some((note.raw.clone(), selection.translation_code.clone()))
            });
            if let Some((note, code)) = found {
                edit(note, &code, element(&detail, "opener"));
            }
            JsValue::TRUE
        }
        "note-more" => {
            if let Some(more) = element(&detail, "button") {
                let expanded = more.get_attribute("aria-expanded").as_deref() != Some("true");
                if let Some(body) = more
                    .get_attribute("aria-controls")
                    .and_then(|id| by_id(&id))
                {
                    toggle_class(&body, "is-expanded", expanded);
                }
                let _ = more.set_attribute("aria-expanded", &expanded.to_string());
                more.set_text_content(Some(if expanded { "less" } else { "more" }));
            }
            JsValue::TRUE
        }
        "show-all" => {
            with(|s| s.show_all_notes = true);
            render_notes_section();
            if let Some(next) = elements().and_then(|els| {
                query_all(&els.notes, ".note-edit")
                    .into_iter()
                    .nth(text::NOTES_SHOWN)
            }) {
                focus_scrolling(&next);
            }
            JsValue::TRUE
        }
        // A cross-reference chip in the card shows that passage in the card.
        "xref" => {
            if let Some(chip) = element(&detail, "chip") {
                toggle_preview(chip);
            }
            JsValue::TRUE
        }
        // Verse clicks: a footnote marker's verse, a Shift-click, a click.
        "select" => {
            if let (Some(code), Some(anchor), Some(focus)) = (
                get(&detail, "translationCode").as_string(),
                number(&detail, "anchor"),
                number(&detail, "focus"),
            ) {
                select_verses(&code, anchor, focus);
            }
            JsValue::TRUE
        }
        "extend" => {
            if let Some((code, n)) = verse() {
                apply(with(|s| text::extend(s.selection.as_ref(), &code, n)));
            }
            JsValue::TRUE
        }
        "toggle" => {
            if let Some((code, n)) = verse() {
                apply(with(|s| text::toggle(s.selection.as_ref(), &code, n)));
            }
            JsValue::TRUE
        }
        "clear" => {
            clear_selection();
            JsValue::TRUE
        }
        // Escape (no surface open): the preview, then the selection.
        "escape" => {
            let (preview, selected) = with(|s| (s.preview_chip.is_some(), s.selection.is_some()));
            if preview {
                let in_card = elements().is_some_and(|els| contains_focus(&els.card));
                close_preview(in_card);
            } else if selected {
                clear_selection();
            }
            JsValue::from_bool(preview || selected)
        }
        "state" => ipc::args(&facts()).unwrap_or(JsValue::NULL),
        // After an import, and at boot: reloaded, a promise of the end.
        "highlights" => to_promise(Box::pin(refresh_highlights())).into(),
        "notes" => to_promise(Box::pin(refresh_notes())).into(),
        _ => {
            ipc::log_error("The card's request can't be read", &detail);
            return;
        }
    };
    event.prevent_default();
    bridge::card_answer(event, &answer);
}

// ---- Mounting.

/// Makes the card's elements (kept for as long as the page lives, moved in
/// and out of the reading pane), and listens for the page's requests, the
/// Library's bookmarks, the note editor's news, the window and layout
/// changing (the card is placed again), and Shift. Called before the
/// reader's, which tells this about the first chapter.
pub fn mount() {
    let Some(document) = bridge::document() else {
        return;
    };
    let make = |tag: &str| {
        document
            .create_element(tag)
            .ok()
            .and_then(|el| el.dyn_into::<HtmlElement>().ok())
    };
    let (
        Some(actions),
        Some(card),
        Some(footnotes),
        Some(xrefs),
        Some(preview),
        Some(menu),
        Some(notes),
    ) = (
        make("span"),
        make("div"),
        make("div"),
        make("div"),
        make("div"),
        make("div"),
        make("div"),
    )
    else {
        return;
    };
    actions.set_class_name("verse-actions");
    actions.set_inner_html(&text::actions_html());
    card.set_id("selection-card");
    card.set_class_name("selection-card");
    let _ = card.set_attribute("role", "group");
    footnotes.set_class_name("popup-footnotes selection-footnotes");
    xrefs.set_class_name("selection-xrefs");
    let _ = card.append_with_node_3(&actions, &footnotes, &xrefs);
    // A cross-reference read in place: one at a time, under the row of the
    // chip that opened it (pressed while it shows).
    preview.set_id("xref-preview");
    preview.set_class_name("xref-preview");
    let _ = preview.set_attribute("role", "region");
    let _ = preview.set_attribute("aria-labelledby", "xref-preview-title");
    preview.set_hidden(true);
    menu.set_id("copy-as-menu");
    menu.set_class_name("copy-as-menu");
    let _ = menu.set_attribute("role", "menu");
    menu.set_hidden(true);
    menu.set_inner_html(&text::copy_menu_html());
    notes.set_class_name("verse-notes");
    let _ = notes.set_attribute("role", "region");
    let find = |selector: &str| {
        actions
            .query_selector(selector)
            .ok()
            .flatten()
            .and_then(|el| el.dyn_into::<HtmlElement>().ok())
    };
    let (Some(citation), Some(status), Some(bookmark), Some(remove), Some(note_add)) = (
        find(".verse-actions-ref"),
        find(".verse-actions-status"),
        find(".verse-bookmark-btn"),
        find(".highlight-remove"),
        find(".verse-note-btn"),
    ) else {
        return;
    };
    let swatches = query_all(&actions, ".highlight-swatch");
    let made = Rc::new(Elements {
        card: card.clone(),
        actions,
        footnotes,
        xrefs,
        preview,
        notes,
        menu: menu.clone(),
        citation,
        status,
        bookmark,
        swatches,
        remove,
        note_add,
    });
    ELEMENTS.with_borrow_mut(|e| *e = Some(made));

    dom::on_document_event(bridge::CARD_EVENT, |event| on_request(&event));
    dom::on_document_event(bridge::LIBRARY_LOADED_EVENT, |event| {
        on_library_loaded(&event)
    });
    dom::on_document_event(bridge::NOTE_DONE_EVENT, |event| on_note_done(&event));
    // Library lists asked for before the Library was there.
    dom::on_document_event(bridge::READY_EVENT, |_| {
        let lists = with(|s| std::mem::take(&mut s.library_waiting));
        if !lists.is_empty() {
            let lists: Vec<&str> = lists.iter().map(String::as_str).collect();
            spawn_local(refresh_library(&lists));
        }
    });
    {
        let menu_for_keys = menu.clone();
        dom::on(&menu, "keydown", move |event| {
            if let Some(event) = event.dyn_ref::<KeyboardEvent>() {
                on_menu_keydown(&menu_for_keys, event);
            }
        });
    }
    if let Some(window) = web_sys::window() {
        dom::on(&window, "resize", |_| place_card());
        // The Shift ghost follows the keyboard, before any other handler.
        listen_capturing(&window, "keydown", |event| {
            with(|s| s.shift_held = event.shift_key() || event.key() == "Shift");
            sync_ghost();
        });
        listen_capturing(&window, "keyup", |event| {
            with(|s| s.shift_held = event.key() != "Shift" && event.shift_key());
            sync_ghost();
        });
        dom::on(&window, "blur", |_| release_shift());
        // The card grows as its notes load or a status line shows.
        if js_sys::Reflect::has(&window, &JsValue::from_str("ResizeObserver")).unwrap_or(false) {
            let grown = Closure::<dyn Fn()>::new(|| {
                let Some(els) = elements() else {
                    return;
                };
                let height = els.card.scroll_height();
                if with(|s| std::mem::replace(&mut s.last_height, height)) == height {
                    return;
                }
                place_card();
            });
            if let Ok(observer) = web_sys::ResizeObserver::new(grown.as_ref().unchecked_ref()) {
                observer.observe(&card);
            }
            grown.forget();
        }
    }
    // A new layout moves the selected verses.
    dom::on_document_event(crate::aa::LAYOUT_CHANGE_EVENT, |_| place_card());
    dom::on(&document, "visibilitychange", |_| {
        if bridge::document().is_some_and(|d| d.hidden()) {
            release_shift();
        }
    });
    // Focus moving (into a sheet Shift+? opened, Copy as… from
    // Ctrl+Shift+C, a field) can end the ghost while Shift is still held, or
    // allow it again.
    dom::on(&document, "focusin", |_| sync_ghost());
    dom::on(&document, "focusout", |_| {
        let _ = later(0, sync_ghost);
    });
}
