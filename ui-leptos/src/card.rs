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
//! With two panes open (`pane`), the card is the active pane's: one
//! selection at a time, made there, and gone as another pane becomes
//! active (D3); Copy chapter copies the active pane's chapter. The marks
//! are every pane's: a bookmark or highlight shows on its verse in each
//! pane showing that chapter, and each pane's notes are its chapter's.
//!
//! The reader (`reader`) tells this what it shows: a chapter loading
//! ([`loading`]) and shown ([`shown`]), a passage to select
//! ([`select_passage`]) and a Library note to edit ([`edit_note`]). The
//! click and keyboard dispatchers (`clicks`, `keyboard`) and the palette's
//! page commands (`page`) ask this for what they need ([`ask`]): select,
//! extend or clear the selection, copy, bookmark, highlight and the notes'
//! controls; they, boot (`boot`) and an import (`page`) read what the
//! selection is ([`facts`]) and have the highlights and notes reloaded
//! ([`refresh_highlights`], [`refresh_notes`]). The Library
//! hands over its bookmarks ([`library_loaded`]) and has the
//! highlights refreshed; the note editor opens
//! from here (`note_editor::open`), has the notes refreshed
//! ([`refresh_notes`]) and says what it did ([`note_done`]).
//!
//! A stale answer never overwrites a newer one: the highlights, the
//! chapter's notes, the selection's notes and a preview each have their
//! token, and every action's status line its attempt (a newer action's
//! status, or a new selection, wins). Copying writes to the clipboard within
//! the click or key that asked (WebKitGTK refuses it afterwards), so each
//! passage's text is asked for as soon as it's selected or shown.
//!
//! Its elements are made once and moved in and out of the reading pane,
//! which the reader rebuilds whole per chapter.

use std::cell::RefCell;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::Closure;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::{JsFuture, spawn_local};
use web_sys::{Element, HtmlElement, KeyboardEvent};

use crate::bridge;
use crate::card_text::{self as text, Change, Room, Selection, Side, Slot};
use crate::dom::{self, by_id, closest, data_number, query_all, scroll_into_view};
use crate::dto::{
    Bookmark, Compare, CopyText, CopyTextArgs, Finding, Highlight, PassageArgs, PassageText,
    PassageTextArgs, Reading, RemoveBookmarkArgs, SelectionNote, SetHighlightArgs, Verse,
};
use crate::ipc;
use crate::library_text::List;
use crate::margin;
use crate::note_editor::{self, KeepRef, NoteRef, NoteRequest, OtherNote};
use crate::note_text::Passage;
use crate::palette::PaletteState;
use crate::pane::{self, PaneId, PerPane};

/// Every verse the reader can select: the chapter being read, in each
/// translation column it's shown in. Neither the faded neighbors nor
/// cross-reference text carry `data-translation`.
pub(crate) const SELECTABLE: &str = ".chapter-main .verse[data-translation]";

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
    /// What the status line's timer calls ([`clear_status`]): one callback,
    /// made once (`dom::kept_callback`).
    static CLEAR_STATUS: js_sys::Function = dom::kept_callback(clear_status);
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

/// What the card knows of one pane's chapter.
#[derive(Default)]
struct InPane {
    chapter: Option<Chapter>,
    copy_chapter: Option<CopyEntry>,
    /// The chapter's notes, each with the backend's own object for it (for
    /// the margin, which may open one in the note editor).
    chapter_notes: Vec<(SelectionNote, JsValue)>,
    chapter_notes_token: u64,
    /// The chapter (book, chapter) `chapter_notes` were last asked for,
    /// and whether they could be read (if not, there are none).
    chapter_notes_of: Option<(i64, i64)>,
    chapter_notes_read: bool,
}

#[derive(Default)]
struct State {
    panes: PerPane<InPane>,
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
    /// Copy as… is open for this, from that button.
    menu_for: Option<Slot>,
    menu_opener: Option<HtmlElement>,
    /// The Library's bookmarks (for the markers and the Bookmark button).
    bookmarks: Vec<Bookmark>,
    bookmarks_loaded: bool,
    /// A bookmark is being added or removed, until the bookmarks it leaves
    /// are back: Bookmark (and B) do nothing meanwhile, so a second press
    /// never sends the first one's change again (C-03).
    bookmark_pending: bool,
    /// Every highlight, by book, chapter and verse. Highlights belong to no
    /// one translation, like bookmarks.
    highlights: HashMap<(i64, i64, i64), String>,
    highlights_loaded: bool,
    highlights_token: u64,
    /// Reloads of the highlights still on their way.
    highlights_reloading: u32,
    selection_notes: Vec<NoteRow>,
    selection_notes_error: String,
    selection_notes_token: u64,
    show_all_notes: bool,
    /// Which Shift keys are held (the ghost).
    shift: text::ShiftHeld,
    /// A button went down on the ghost and hasn't come up yet: the card
    /// stays one until it does, so the button-up lands where the press did.
    ghost_press: bool,
    /// The click of a press that found the card a ghost no keyup had ended:
    /// it was aimed at the card but went through it, so it's dropped.
    swallow_click: bool,
    /// The card's height when last placed by its observer.
    last_height: i32,
    /// Library lists asked for before the Library was there.
    library_waiting: Vec<String>,
}

impl State {
    /// What the card knows of the pane it acts on, the active one (a
    /// selection is made in it).
    fn here(&self) -> &InPane {
        &self.panes[pane::active()]
    }

    /// The selection's text to copy, or `pane`'s chapter's.
    fn copy(&mut self, slot: Slot, pane: PaneId) -> &mut Option<CopyEntry> {
        match slot {
            Slot::Selection => &mut self.copy_selection,
            Slot::Chapter => &mut self.panes[pane].copy_chapter,
        }
    }

    /// The selected verses, sorted, from the chapter (never from the DOM).
    fn selection_verses(&self) -> Vec<Verse> {
        let (Some(chapter), Some(selection)) = (&self.here().chapter, &self.selection) else {
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
        let (chapter, selection) = (self.here().chapter.as_ref()?, self.selection.as_ref()?);
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
        let (chapter, selection) = (self.here().chapter.as_ref()?, self.selection.as_ref()?);
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
        let Some(chapter) = &self.here().chapter else {
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

/// Every selectable verse in `pane`.
fn selectable(pane: PaneId) -> Vec<HtmlElement> {
    pane.query_all(SELECTABLE)
}

fn toggle_class(el: &Element, name: &str, on: bool) {
    let _ = el.class_list().toggle_with_force(name, on);
}

fn has_class(el: &Element, name: &str) -> bool {
    el.class_list().contains(name)
}

/// A selectable verse of the active pane (where the selection is), by
/// column and number.
fn verse_element(code: &str, number: i64) -> Option<HtmlElement> {
    pane::active().query(&format!(
        r#"{SELECTABLE}[data-translation="{code}"][data-verse="{number}"]"#
    ))
}

fn active_element() -> Option<Element> {
    dom::active_element()
}

fn contains_focus(el: &Element) -> bool {
    active_element().is_some_and(|active| el.contains(Some(&active)))
}

/// `el.focus()`, which may scroll the page to it.
fn focus_scrolling(el: &HtmlElement) {
    let _ = el.focus();
}

/// A number property of an element (`scrollTop`, a double), read and
/// written without rounding.
fn number_property(el: &Element, name: &str) -> f64 {
    js_sys::Reflect::get(el, &JsValue::from_str(name))
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0)
}

fn set_number_property(el: &Element, name: &str, value: f64) {
    let _ = js_sys::Reflect::set(el, &JsValue::from_str(name), &JsValue::from_f64(value));
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

/// Runs `f` in `ms`, once. It's never cancelled (it gives no handle to
/// cancel it with), so its callback is always called, and so freed; a
/// timer that may be cancelled has a callback of its own, made once
/// (`dom::kept_callback`), as the status line's has.
fn later(ms: i32, f: impl FnOnce() + 'static) {
    let f = Closure::once_into_js(f);
    let _ = dom::set_timeout(f.unchecked_ref(), ms);
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
    dom::clear_timeout(timer);
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
        let timer = CLEAR_STATUS.with(|clear| dom::set_timeout(clear, duration));
        with(|s| s.status_timer = timer);
    }
}

/// The status line's time is up: it's cleared, focus in it (its Undo)
/// going to + Add note.
fn clear_status() {
    with(|s| s.status_timer = None);
    let Some(els) = elements() else {
        return;
    };
    let had_focus = contains_focus(&els.status);
    status(String::new(), false);
    if had_focus && els.note_add.is_connected() {
        dom::focus(&els.note_add);
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
    // The selection is the active pane's; the other has none (D3).
    for (pane, el) in
        pane::shown().flat_map(|pane| selectable(pane).into_iter().map(move |el| (pane, el)))
    {
        let selected = pane == pane::active()
            && selection.as_ref().is_some_and(|sel| {
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
    crate::note_copy::show_in_card(&[]);
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
        let (chapter, selection) = (s.here().chapter.as_ref()?, s.selection.as_ref()?);
        Some(CopyTextArgs {
            translation_code: selection.translation_code.clone(),
            book_id: chapter.book_id,
            chapter: chapter.chapter,
            start: Some(selection.start),
            end: Some(selection.end),
        })
    });
    if let Some(copy) = copy {
        fetch_copy(Slot::Selection, pane::active(), copy);
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
    // Opened (or moved) by a Shift-click with Shift still held: a ghost at
    // once, so the next Shift-click goes through it too.
    sync_ghost();
    spawn_local(refresh_selection_notes());
}

/// The selection's footnotes and cross-references, from the chapter as the
/// backend returned it. Cross-references are by verse number (the same in
/// every column); footnotes are the column's own. A range labels each with
/// its verse.
fn render_card_references(els: &Elements) {
    let (verses, range, chapter, xrefs) = with(|s| {
        let range = s.selection.as_ref().is_some_and(|sel| sel.start != sel.end);
        let chapter = s.here().chapter.as_ref().map_or(0, |c| c.chapter);
        let verses = s.selection_verses();
        let xrefs: Vec<(i64, Vec<crate::dto::Xref>)> = verses
            .iter()
            .filter_map(|v| {
                let found = s
                    .here()
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
/// comparing translations, from the first (left) column's.
fn preview_translation() -> String {
    let comparing = crate::aa::beside() == crate::aa_text::Beside::Translations;
    with(|s| {
        let chapter = s.here().chapter.as_ref();
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
        // "1 John 4:9–10", as the backend names it (or, if it couldn't, as
        // the chip's citation does: the same rule's name).
        let passage = passage.ok();
        let reference = match &passage {
            Some(p) => p.reference.clone(),
            None => chip.text_content().unwrap_or_default(),
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
    let pane = pane::active();
    let first: Element = pane
        .query(&format!("{SELECTABLE}.verse-selected"))
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
    let view = pane.scroller().view().unwrap_or_default();
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
        window: view.bottom,
        bar: view.top,
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
// if that fails. An answer that failed is asked for again, by the next
// render or copy.

/// Asks for `slot`'s text (in `pane`) with `args`, unless it's already been
/// asked for (on its way, or in) and didn't fail. Each answer goes to its
/// own entry, so a late one never replaces a newer entry's.
fn fetch_copy(slot: Slot, pane: PaneId, args: CopyTextArgs) {
    let asked = with(|s| {
        s.copy(slot, pane)
            .as_ref()
            .is_some_and(|e| e.args == args && !answer_failed(&e.answer.borrow()))
    });
    if asked {
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
    with(|s| *s.copy(slot, pane) = Some(entry));
}

/// Whether a copy's text was asked for and couldn't be had.
fn answer_failed(answer: &Option<Result<CopyText, JsValue>>) -> bool {
    matches!(answer, Some(Err(_)))
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
    let ready =
        with(|s| s.here().chapter.is_some() && (slot == Slot::Chapter || s.selection.is_some()));
    if !ready {
        return;
    }
    let pane = pane::active();
    let Some(mut entry) = with(|s| s.copy(slot, pane).clone()) else {
        return;
    };
    // Text that couldn't be had is asked for again, and the copy waits for
    // it, as for text not in yet.
    if answer_failed(&entry.answer.borrow()) {
        fetch_copy(slot, pane, entry.args.clone());
        let Some(again) = with(|s| s.copy(slot, pane).clone()) else {
            return;
        };
        entry = again;
    }
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
        Slot::Chapter => crate::page::set_data_status(&message, is_error),
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
        let chapter = s.here().chapter.as_ref()?;
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
/// within that click (the click dispatcher, `clicks`, asks for it).
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
/// and every pane (bookmarks belong to no one translation, nor pane).
fn apply_bookmark_markers() {
    for pane in pane::shown() {
        let spans = with(|s| {
            s.panes[pane].chapter.as_ref().map(|c| {
                s.bookmarks
                    .iter()
                    .filter(|b| b.book_id == c.book_id && b.chapter == c.chapter)
                    .map(|b| (b.verse_start, b.verse_end))
                    .collect::<Vec<_>>()
            })
        });
        for el in selectable(pane) {
            let number = data_number::<i64>(&el, "data-verse");
            let marked = spans.as_ref().is_some_and(|spans| {
                number.is_some_and(|n| spans.iter().any(|&(start, end)| n >= start && n <= end))
            });
            toggle_class(&el, "is-bookmarked", marked);
        }
    }
}

/// A Library list has loaded (`items`, or `null` if it failed): the
/// bookmarks are the card's.
pub fn library_loaded(list: List, items: &JsValue) {
    if list != List::Bookmarks {
        return;
    }
    if !items.is_null() && !items.is_undefined() {
        match read::<Vec<Bookmark>>(items.clone()) {
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
    let ready =
        with(|s| s.selection.is_some() && s.here().chapter.is_some() && !s.bookmark_pending);
    if !ready {
        return Box::pin(async {});
    }
    let (attempt, existing, args) = with(|s| {
        let attempt = s.attempt();
        let existing = s.selected_bookmark();
        let args = s
            .here()
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
        (Some(existing), _) => send(
            "remove_bookmark",
            &RemoveBookmarkArgs {
                id: existing.id,
                created_at: &existing.created_at,
            },
        ),
        (None, Some(args)) => send("add_bookmark", &args),
        (None, None) => return Box::pin(async {}),
    };
    with(|s| s.bookmark_pending = true);
    Box::pin(async move {
        let changed = answer(sent).await;
        if changed.is_ok() {
            refresh_library(&["bookmarks"]).await;
        }
        // The bookmarks are as this change left them: the next press is
        // the next change.
        with(|s| s.bookmark_pending = false);
        match changed {
            Ok(_) => status_for(attempt, text::bookmark_done(removing), false),
            Err(e) => {
                ipc::log_error("Failed to change bookmark", &e);
                status_for(
                    attempt,
                    text::bookmark_failed(removing, &ipc::error_text(&e)),
                    true,
                );
            }
        }
    })
}

// ---- Highlights.

/// Reloads the highlights (only the latest reload is shown: an earlier one
/// answering after it is dropped), then marks them. The Library's list of
/// them is reloaded by whoever asked.
pub fn refresh_highlights() -> impl Future<Output = ()> + 'static {
    let token = with(|s| {
        s.highlights_token += 1;
        s.highlights_reloading += 1;
        s.highlights_token
    });
    let sent = ipc::send("list_highlights", JsValue::UNDEFINED);
    async move {
        let got = answer(sent).await.and_then(read::<Vec<Highlight>>);
        let newest = with(|s| {
            s.highlights_reloading = s.highlights_reloading.saturating_sub(1);
            s.highlights_token == token
        });
        if !newest {
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

/// Shows every highlight on screen, in every pane: in the chapter being
/// read (every column, while comparing) and the faded neighbors.
fn apply_highlight_markers() {
    let highlights = with(|s| s.highlights.clone());
    for pane in pane::shown() {
        let here = with(|s| {
            s.panes[pane]
                .chapter
                .as_ref()
                .map(|c| (c.book_id, c.chapter))
        });
        for el in selectable(pane) {
            let color = here
                .zip(data_number(&el, "data-verse"))
                .and_then(|((book, chapter), verse)| highlights.get(&(book, chapter, verse)));
            set_highlight_marker(&el, color.map(String::as_str));
        }
    }
    for el in pane::shown().flat_map(|pane| pane.query_all(".chapter-side .verse[data-verse]")) {
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
        let (chapter, selection) = (s.here().chapter.as_ref()?, s.selection.as_ref()?);
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
    // Newer than any reload on its way, which may have been answered
    // before this change: its answer is dropped, not painted over this
    // one (C2-04), and the highlights are read again once this is in.
    let overtook = with(|s| {
        s.highlights_token += 1;
        s.highlights_reloading > 0
    });
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
            if overtook {
                spawn_local(refresh_highlights());
            }
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
        // What the reload this overtook would have shown (another change,
        // an import), read again with this one in it.
        if overtook {
            spawn_local(refresh_highlights());
        }
        // The Library's list, not waited for.
        spawn_local(refresh_library(&["highlights"]));
    })
}

// ---- Notes: the reader's own words on a passage. Like bookmarks they
// belong to no translation, and a passage can have any number of them.
// Which notes overlap a passage, and their order, is the backend's
// (`list_notes`); this shows them. Every change reloads all three views (the
// markers, the selection's Notes section and the Library), so none of them
// goes stale.

/// Marks every verse of `pane`'s chapter that a note covers, in every
/// column, with the pencil (CSS) and, for screen readers, one hidden "has
/// notes" per verse.
fn apply_note_markers(pane: PaneId) {
    let spans = with(|s| {
        let here = &s.panes[pane];
        here.chapter.as_ref().map(|c| {
            here.chapter_notes
                .iter()
                .filter(|(n, _)| n.book_id == c.book_id && n.chapter == c.chapter)
                .map(|(n, _)| (n.verse_start, n.verse_end))
                .collect::<Vec<_>>()
        })
    });
    let Some(document) = bridge::document() else {
        return;
    };
    for el in selectable(pane) {
        let number = data_number::<i64>(&el, "data-verse");
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

/// Reads the notes on `pane`'s chapter again.
fn refresh_chapter_notes(pane: PaneId) -> Task {
    let (token, args) = with(|s| {
        let s = &mut s.panes[pane];
        s.chapter_notes_token += 1;
        // Every note on the chapter, those past the last verse on screen
        // too (the margin shows them beside it).
        let args = s.chapter.as_ref().map(|c| PassageArgs {
            book_id: c.book_id,
            chapter: c.chapter,
            verse_start: 1,
            verse_end: c
                .all_numbers()
                .into_iter()
                .fold(PAST_EVERY_LAST_VERSE, i64::max),
        });
        (s.chapter_notes_token, args)
    });
    let Some(args) = args else {
        return Box::pin(async {});
    };
    let sent = send("list_notes", &args);
    let of = (args.book_id, args.chapter);
    Box::pin(async move {
        let got = answer(sent).await.and_then(|raw| {
            let notes: Vec<SelectionNote> = read(raw.clone())?;
            let raw = js_sys::Array::from(&raw);
            Ok(notes.into_iter().zip(raw.iter()).collect::<Vec<_>>())
        });
        if with(|s| s.panes[pane].chapter_notes_token) != token {
            return;
        }
        let (notes, read) = match got {
            Ok(notes) => (notes, true),
            Err(e) => {
                ipc::log_error("Failed to load the chapter’s notes", &e);
                (Vec::new(), false)
            }
        };
        with(|s| {
            let s = &mut s.panes[pane];
            s.chapter_notes = notes;
            s.chapter_notes_of = Some(of);
            s.chapter_notes_read = read;
        });
        apply_note_markers(pane);
        show_margin();
    })
}

/// A verse number past any chapter's last (Psalm 119 has 176): the end of
/// the chapter's notes asked for.
const PAST_EVERY_LAST_VERSE: i64 = 999;

/// The chapter's notes, to the margin (`margin`), which shows them in
/// reading mode and in Compare's notes column, and whether they're known:
/// read for this chapter (not still on their way, or unreadable), so the
/// column only says a chapter has none when it hasn't.
fn show_margin() {
    let found = with(|s| {
        let here = s.here();
        let c = here.chapter.as_ref()?;
        let known =
            here.chapter_notes_read && here.chapter_notes_of == Some((c.book_id, c.chapter));
        let notes = here
            .chapter_notes
            .iter()
            .filter(|(n, _)| n.book_id == c.book_id && n.chapter == c.chapter)
            .cloned()
            .collect::<Vec<_>>();
        Some((
            c.book_id,
            c.chapter,
            c.translation_code.clone(),
            notes,
            known,
        ))
    });
    if let Some((book_id, chapter, code, notes, known)) = found {
        margin::notes(book_id, chapter, &code, notes, known);
    }
}

fn refresh_selection_notes() -> Task {
    let (token, args) = with(|s| {
        s.selection_notes_token += 1;
        let args = s
            .here()
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
    let passage = Passage {
        book_id: args.book_id,
        chapter: args.chapter,
        start: args.verse_start,
        end: args.verse_end,
    };
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
        // The note editor, if it's open and adding on this passage, lists
        // them too.
        note_editor::show_others(&passage, others());
    })
}

/// The selection's notes, as the note editor lists the others.
fn others() -> Vec<OtherNote> {
    with(|s| {
        s.selection_notes
            .iter()
            .map(|n| OtherNote {
                reference: n.note.reference.clone(),
                date: n.note.created_at_display.clone(),
                body: n.note.body.clone(),
            })
            .collect()
    })
}

/// The selection's Notes section, under its actions: shown only when notes
/// overlap it (or they couldn't be loaded).
fn render_notes_section() {
    let Some(els) = elements() else {
        return;
    };
    let shown = with(|s| {
        let (chapter, selection) = (s.here().chapter.as_ref()?, s.selection.as_ref()?);
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
        crate::note_copy::show_in_card(&[]);
        place_card();
        return;
    };
    let had_focus = active_element().filter(|active| els.notes.contains(Some(active)));
    let _ = els
        .notes
        .set_attribute("aria-label", &format!("Notes for {reference}"));
    els.notes
        .set_inner_html(&text::notes_html(&notes, &error, show_all));
    // Copy as Markdown writes within its click or key press: the Markdown
    // of each note shown is asked for now, and kept while it's shown.
    let shown: Vec<crate::note_copy::Copyable> = text::notes_shown(&notes, show_all)
        .iter()
        .map(Into::into)
        .collect();
    crate::note_copy::show_in_card(&shown);
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
/// Library's, and the chapter's AI findings (a note kept from one removes
/// it), after any change to notes, wherever it came from; the commands are
/// sent now, in that order, and this settles once all four have.
pub fn refresh_notes() -> impl Future<Output = ()> + 'static {
    let chapters = js_sys::Array::new();
    for pane in pane::shown() {
        chapters.push(&to_promise(refresh_chapter_notes(pane)));
    }
    let chapter = js_sys::Promise::all(&chapters);
    let selection = to_promise(refresh_selection_notes());
    let library = to_promise(refresh_library(&["notes"]));
    let findings = to_promise(Box::pin(crate::findings::refresh()));
    let all = js_sys::Promise::all(&js_sys::Array::of4(
        &chapter, &selection, &library, &findings,
    ));
    async move {
        let _ = JsFuture::from(all).await;
    }
}

/// Runs `task` by itself, as a promise of its end.
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
    open_note_editor(mode, note, passage, reference, code, opener, None);
}

/// [`open_note_panel`], or, with `keep`, "Keep as mine" on that finding.
fn open_note_editor(
    mode: &str,
    note: Option<JsValue>,
    passage: (i64, i64, i64, i64),
    reference: String,
    code: &str,
    opener: Option<HtmlElement>,
    keep: Option<&Finding>,
) {
    let (book_id, chapter, start, end) = passage;
    let Some(verses) = with(|s| {
        let here = s.here().chapter.as_ref()?;
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
    crate::page::modal_opening(true);
    // The verses as this translation numbers and shows them (KJV supplied
    // words in italics, no note markers); any it doesn't number are said to
    // be missing.
    let missing = end - start + 1 - verses.len() as i64;
    let quoted: Vec<&Verse> = verses.iter().collect();
    let passage_html = text::note_passage_html(&quoted, missing, code);
    // Any note editor opening is newer than a note's Edit still waiting
    // for its jump.
    crate::reader::note_editor_opening();
    let edited = match note
        .as_ref()
        .map(|n| serde_wasm_bindgen::from_value::<NoteRef>(n.clone()))
    {
        Some(Err(e)) => {
            ipc::log_error("The note request couldn't be read", &JsValue::from(e));
            return;
        }
        Some(Ok(edited)) => Some(edited),
        None => None,
    };
    let request = NoteRequest {
        mode: mode.to_owned(),
        note: edited,
        passage: Passage {
            book_id,
            chapter,
            start,
            end,
        },
        reference,
        code: code.to_owned(),
        passage_html,
        others: match keep {
            Some(finding) => chapter_others(finding),
            None if mode == "add" => others(),
            None => Vec::new(),
        },
        keep: keep.map(|f| KeepRef {
            id: f.id,
            created_at: f.created_at.clone(),
            text: f.text.clone(),
        }),
    };
    if !note_editor::open(request, note.unwrap_or(JsValue::NULL), opener) {
        crate::page::set_data_status(text::EDITOR_UNAVAILABLE, true);
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
        let (chapter, selection) = (s.here().chapter.as_ref()?, s.selection.as_ref()?);
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

/// "Keep as mine" on an AI finding of the chapter on screen (C10): the same
/// editor as + Add note, on the finding's verses, in the translation being
/// read, opened with the finding's words, all of them selected. Saving a
/// note there makes it the reader's own note and removes the finding; the
/// reader's notes already on the verse are listed, and never changed.
pub fn keep_finding(finding: &Finding, opener: Option<HtmlElement>) {
    let Some(code) = with(|s| {
        let chapter = s.here().chapter.as_ref()?;
        (chapter.book_id == finding.book_id && chapter.chapter == finding.chapter)
            .then(|| chapter.translation_code.clone())
    }) else {
        return;
    };
    open_note_editor(
        "keep",
        None,
        (
            finding.book_id,
            finding.chapter,
            finding.verse_start,
            finding.verse_end,
        ),
        finding.reference.clone(),
        &code,
        opener,
        Some(finding),
    );
}

/// The reader's own notes on a finding's verses, from the chapter's, as the
/// editor lists the others.
fn chapter_others(finding: &Finding) -> Vec<OtherNote> {
    with(|s| {
        s.here()
            .chapter_notes
            .iter()
            .map(|(n, _)| n)
            .filter(|n| {
                n.book_id == finding.book_id
                    && n.chapter == finding.chapter
                    && n.verse_start <= finding.verse_end
                    && n.verse_end >= finding.verse_start
            })
            .map(|n| OtherNote {
                reference: n.reference.clone(),
                date: n.created_at_display.clone(),
                body: n.body.clone(),
            })
            .collect()
    })
}

/// A Library note's Edit has opened its passage (the reader jumped there,
/// the passage selected, so its Notes section shows): the note opens.
pub fn edit_note(note: &JsValue) {
    if note.is_null() || note.is_undefined() {
        return;
    }
    let code = with(|s| {
        let chapter = s.here().chapter.as_ref()?;
        Some(s.selection.as_ref().map_or_else(
            || chapter.translation_code.clone(),
            |sel| sel.translation_code.clone(),
        ))
    });
    if let Some(code) = code {
        edit(note.clone(), &code, None);
    }
}

/// What the note editor did.
pub enum NoteDone {
    Added,
    Saved,
    /// The note deleted, as the backend returned it.
    Deleted(JsValue),
}

/// The note editor added, saved or deleted a note, the notes already
/// refreshed in every pane ([`refresh_notes`]): the card says so in its
/// status line, a deletion with its Undo, or in the page's toast when the
/// card is gone.
pub fn note_done(done: NoteDone) {
    let Some(els) = elements() else {
        return;
    };
    let card_open = els.actions.is_connected();
    match done {
        NoteDone::Deleted(note) if card_open => {
            set_action_status(text::NOTE_DELETED, false, Some(note), text::UNDO_MS);
        }
        NoteDone::Deleted(note) => deleted_elsewhere(note),
        NoteDone::Saved if card_open => status(text::NOTE_SAVED, false),
        NoteDone::Added if card_open => status(text::NOTE_ADDED, false),
        _ => {}
    }
}

/// A note deleted with the card gone (a Library note's Edit on a verse
/// the translation doesn't number, say): the toast says so, with the card's
/// timed Undo, which restores it the same way and says how that went there.
fn deleted_elsewhere(note: JsValue) {
    let message = text::note_deleted_elsewhere(&js_text(&get(&note, "reference")));
    let Some(undo) = crate::page::set_data_status_with_action(&message, "Undo", text::UNDO_MS)
    else {
        return;
    };
    dom::on_click(&undo, move |_| {
        crate::page::set_data_status("", false);
        spawn_local(restore_note(note.clone(), |message, is_error| {
            crate::page::set_data_status(&message, is_error);
        }));
    });
}

/// The card's Undo: [`restore_note`], said in its status line if nothing
/// newer has been.
fn restore_note_in_card(note: JsValue) -> Task {
    let attempt = with(State::attempt);
    restore_note(note, move |message, is_error| {
        status_for(attempt, message, is_error);
    })
}

/// Undo: the same passage, text and dates (each as the note had it), and
/// `say` says how it went.
fn restore_note(note: JsValue, say: impl Fn(String, bool) + 'static) -> Task {
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
            say(text::restore_failed(&ipc::error_text(&e)), true);
            return;
        }
        refresh_notes().await;
        say(text::NOTE_RESTORED.to_string(), false);
    })
}

// ---- The Library's lists.

/// Has the Library reload `lists` (`library::refresh`), settling once it
/// has; asked before the Library is there, they're asked for again once
/// every piece is ([`on_ready`]).
fn refresh_library(lists: &[&str]) -> Task {
    match crate::library::refresh(lists) {
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

/// A new layout (`aa`) has moved the selected verses: the card follows.
pub fn layout_changed() {
    place_card();
}

/// Every piece has mounted (`lib`'s ready list): the Library lists asked
/// for before the Library was there, asked again.
pub fn on_ready() {
    let lists = with(|s| std::mem::take(&mut s.library_waiting));
    if !lists.is_empty() {
        let lists: Vec<&str> = lists.iter().map(String::as_str).collect();
        spawn_local(refresh_library(&lists));
    }
}

// ---- The Shift ghost. Holding Shift makes the card a ghost: faint, and
// clicks go through it, so a Shift-click reaches a verse it covers and
// extends the selection. Shift is followed from every key, pointer and
// wheel event's own modifier state (`card_text::shift_after`), so the first
// one without Shift ends the ghost even if Shift's keyup never came (as on
// Wayland with an input method), and let go of on blur, focus or when the
// window is hidden. It's only visual: the card stays in the accessibility
// tree and focus doesn't move.

/// Whether Shift may ghost the card now. Not while Shift is for something
/// else: typing in a field, the search box or palette, a dialog or sheet,
/// Copy as… or another menu or popover, or focus in the card itself
/// (Shift+Tab moves through it).
fn ghost_allowed(els: &Elements) -> bool {
    if !els.card.is_connected() {
        return false;
    }
    if crate::overlays::any_open() || !els.menu.hidden() {
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
    let (held, press) = with(|s| (s.shift.any(), s.ghost_press));
    let ghost = press || (held && ghost_allowed(&els));
    toggle_class(&els.card, "is-ghost", ghost);
}

/// Follows `event` for Shift; whether that changed what's held.
fn follow_shift(event: text::ShiftEvent) -> bool {
    with(|s| {
        let held = text::shift_after(s.shift, event);
        std::mem::replace(&mut s.shift, held) != held
    })
}

fn release_shift() {
    follow_shift(text::ShiftEvent::LetGo);
    with(|s| {
        s.ghost_press = false;
        s.swallow_click = false;
    });
    sync_ghost();
}

fn on_shift_key(event: &KeyboardEvent, down: bool) {
    let key = text::shift_key(&event.key(), &event.code());
    let shift = event.shift_key();
    follow_shift(if down {
        text::ShiftEvent::KeyDown { key, shift }
    } else {
        text::ShiftEvent::KeyUp { key, shift }
    });
    sync_ghost();
}

/// Whether `event` happened inside the card's box.
fn over_card(card: &Element, event: &web_sys::MouseEvent) -> bool {
    let rect = card.get_bounding_client_rect();
    let (x, y) = (f64::from(event.client_x()), f64::from(event.client_y()));
    rect.width() > 0.0
        && rect.height() > 0.0
        && x >= rect.left()
        && x <= rect.right()
        && y >= rect.top()
        && y <= rect.bottom()
}

/// A pointer or wheel event (`MouseEvent`'s `shiftKey`). These still reach
/// the window while the card lets clicks through, so moving the mouse after
/// letting go of Shift ends the ghost, keyup or not. A button going down on
/// the ghost keeps it one until that button comes up and its click is done,
/// so Shift let go first doesn't put the card under the button-up. And a
/// button going down without Shift, with the mouse still, on a ghost no
/// keyup ended: the browser has already aimed it through the card at the
/// verse beneath, but the reader meant the card, so its click is dropped
/// (the selection stays) and the card is itself for the next one.
fn on_shift_pointer(event: &web_sys::MouseEvent) {
    let kind = event.type_();
    // A cancelled pointer says nothing of the keys: it only ends the press.
    let changed = kind != "pointercancel"
        && follow_shift(text::ShiftEvent::Pointer {
            shift: event.shift_key(),
        });
    match kind.as_str() {
        "pointerdown" => {
            let Some(els) = elements() else {
                return;
            };
            let was_ghost = has_class(&els.card, "is-ghost");
            if changed {
                sync_ghost();
            }
            if event.button() != 0 {
                return;
            }
            if has_class(&els.card, "is-ghost") {
                with(|s| s.ghost_press = true);
            } else if was_ghost && !event.shift_key() && over_card(&els.card, event) {
                with(|s| s.swallow_click = true);
            }
        }
        "pointerup" | "pointercancel" => {
            sync_ghost();
            if with(|s| s.ghost_press || s.swallow_click) {
                // After the gesture's click, which comes straight after.
                later(0, || {
                    with(|s| {
                        s.ghost_press = false;
                        s.swallow_click = false;
                    });
                    sync_ghost();
                });
            }
        }
        _ if changed => sync_ghost(),
        _ => {}
    }
}

fn listen_capturing<E: JsCast + 'static>(
    target: &web_sys::EventTarget,
    name: &str,
    handler: impl Fn(E) + 'static,
) {
    let handler = Closure::<dyn Fn(web_sys::Event)>::new(move |event: web_sys::Event| {
        if let Ok(event) = event.dyn_into::<E>() {
            handler(event);
        }
    });
    let _ = target.add_event_listener_with_callback_and_bool(
        name,
        handler.as_ref().unchecked_ref(),
        true,
    );
    handler.forget();
}

// ---- What the reader tells this.

/// The reader has asked for another chapter or translation in `pane`: the
/// selection, made in the active pane, goes now if that's the one, not when
/// the new text arrives, so nothing (Ctrl+C included) acts on it in the
/// meantime.
pub fn loading(pane: PaneId) {
    if pane == pane::active() {
        clear_selection();
    }
}

/// The reader has put a chapter on screen in `pane` (its
/// `#reading-pane-inner` is new): the chapter's copy text, the selection and
/// the verses' marks follow. `compare` is the comparison on screen (while
/// comparing), `compare_first` the first column of the last one fetched.
pub fn shown(
    pane: PaneId,
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
    // The same answer drawn again (Compare on or off): see the end.
    let redrawn = with(|s| {
        s.panes[pane]
            .chapter
            .as_ref()
            .is_some_and(|c| Rc::ptr_eq(&c.reading, &chapter.reading))
    });
    with(|s| s.panes[pane].chapter = Some(chapter));
    // The selection, and Copy as…, are the active pane's: another pane's
    // chapter leaves them be.
    let active = pane == pane::active();
    if active {
        close_menu(false);
    }
    fetch_copy(Slot::Chapter, pane, copy);
    if active {
        with(|s| s.selection = None);
        render_selection();
    }
    apply_bookmark_markers();
    apply_highlight_markers();
    apply_note_markers(pane);
    show_margin();
    if let Some((book_id, chapter)) = with(|s| {
        s.panes[pane]
            .chapter
            .as_ref()
            .map(|c| (c.book_id, c.chapter))
    }) {
        crate::findings::chapter_shown(pane, book_id, chapter);
    }
    // The same reading drawn again (Compare turned on or off) keeps the
    // notes already read for its chapter: every change to them through the
    // app reads them again ([`refresh_notes`]). A chapter loaded, even the
    // one on screen, reads them afresh, as does a failed read. The findings
    // are asked again regardless: an agent may have saved some meanwhile.
    let read_already = redrawn
        && with(|s| {
            let s = &s.panes[pane];
            s.chapter_notes_read
                && s.chapter
                    .as_ref()
                    .is_some_and(|c| s.chapter_notes_of == Some((c.book_id, c.chapter)))
        });
    if !read_already {
        spawn_local(refresh_chapter_notes(pane));
    }
}

/// Another pane is about to be the active one (`pane::set_active`): the
/// selection, and Copy as…, go with the pane left (one selection at a
/// time, D3).
pub fn pane_switching() {
    close_menu(false);
    clear_selection();
}

/// The active pane's selection, to put back with [`reselect`] once its
/// chapter is drawn again as it was: the second pane closing redraws the
/// first (`pane::close_second`), and drawing a chapter clears the selection
/// ([`shown`]), but closing the other pane isn't the reader leaving these
/// verses (GG32-14).
pub fn held_selection() -> Option<Selection> {
    with(|s| s.selection.clone())
}

/// Puts back `held` ([`held_selection`]), with its card, if its verses are
/// on screen in the active pane.
pub fn reselect(held: Option<Selection>) {
    let Some(held) = held else {
        return;
    };
    if verse_element(&held.translation_code, held.start).is_none() {
        return;
    }
    with(|s| s.selection = Some(held));
    render_selection();
    place_card();
}

/// Another pane is the active one: the margin shows its notes.
pub fn pane_switched() {
    show_margin();
}

/// `pane` closed: what the card knew of its chapter goes, and its notes
/// still on their way are dropped as they come.
pub fn forget(pane: PaneId) {
    with(|s| {
        let token = s.panes[pane].chapter_notes_token + 1;
        s.panes[pane] = InPane {
            chapter_notes_token: token,
            ..InPane::default()
        };
    });
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

/// A note opened from the margin (`margin`): its verses `start`–`end`
/// selected, the card with their notes beside them, and the first verse
/// scrolled into view if it isn't.
pub fn select_for_note(code: &str, start: i64, end: i64) {
    select_verses(code, start, end);
    if let Some(el) = verse_element(code, start) {
        scroll_into_view(&el, "nearest");
    }
    place_card();
}

/// What the palette's commands need of this: the chapter's text is ready
/// to copy.
pub fn palette_facts(state: &mut PaletteState) {
    state.chapter_copy = with(|s| s.here().copy_chapter.is_some());
}

// ---- What the click and keyboard dispatchers, boot, an import and the
// palette ask ([`ask`], [`facts`]).

/// What the card says it is: the chapter it acts on, the selection, and
/// what the welcome's fresh-install rule counts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Facts {
    pub chapter: Option<ChapterFacts>,
    pub selection: Option<SelectionFacts>,
    pub bookmarks_loaded: bool,
    pub bookmarks: usize,
    pub highlights_loaded: bool,
    pub highlights: usize,
}

/// The chapter the card acts on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChapterFacts {
    pub book_id: i64,
    pub translation_code: String,
    /// The verse numbers of the column being read, in the backend's order.
    pub numbers: Vec<i64>,
}

/// The selection: its column, the verse it started from, and its ends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionFacts {
    pub translation_code: String,
    pub anchor: i64,
    pub start: i64,
    pub end: i64,
}

/// What the card is now: what j and k, Export notes and the welcome's
/// fresh-install rule need.
pub fn facts() -> Facts {
    with(|s| Facts {
        chapter: s.here().chapter.as_ref().map(|c| ChapterFacts {
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

fn has_selection() -> bool {
    with(|s| s.selection.is_some())
}

/// The selected verses' colors (`None` where there's none; empty with no
/// selection), for H's next one (`card_text::next_highlight`).
pub fn selection_colors() -> Vec<Option<String>> {
    with(|s| s.selected_colors())
}

/// What the click and keyboard dispatchers (`clicks`, `keyboard`) and the
/// palette's page commands (`page`) ask of the card, each within the click
/// or key it's for (copying needs that).
#[derive(Debug, Clone)]
pub enum Ask {
    /// Copy, and Ctrl+C: the selection, in the first format.
    Copy,
    /// The palette's Copy chapter.
    CopyChapter,
    /// Copy as…'s button (or Copy chapter…) clicked: opens it, or closes it
    /// if it's open for that.
    CopyAs {
        slot: Slot,
        opener: Option<HtmlElement>,
    },
    /// Ctrl+Shift+C, and the palette's Copy chapter as…: opens it.
    CopyAsOpen(Slot),
    /// One of Copy as…'s formats clicked (Enter and Space click it).
    CopyFormat(String),
    /// Any other click: Copy as… closes.
    CloseCopyMenu,
    /// ★ Bookmark, or B.
    Bookmark,
    /// A swatch (its colour), Remove highlight (`None`), or H.
    Highlight(Option<String>),
    /// + Add note, or N: the note editor, focus going back to `opener`.
    Note(Option<HtmlElement>),
    /// The status line's Undo.
    Undo,
    /// A note's Edit.
    NoteEdit {
        note_id: String,
        opener: Option<HtmlElement>,
    },
    /// A note's "more" (or "less").
    NoteMore(HtmlElement),
    /// A note's Copy as Markdown: note `note_id`.
    NoteCopy(i64),
    /// The notes' Show all.
    ShowAll,
    /// A cross-reference chip: its preview opens or closes in the card.
    Xref(HtmlElement),
    /// These verses: a footnote marker's, or j and k's.
    Select {
        translation_code: String,
        anchor: i64,
        focus: i64,
    },
    /// Shift-click on a verse.
    Extend {
        translation_code: String,
        verse: i64,
    },
    /// A click on a verse.
    Toggle {
        translation_code: String,
        verse: i64,
    },
    /// A click away.
    Clear,
    /// Escape with no surface open: the preview, then the selection.
    Escape,
}

/// Does what `ask` asks, within the caller's event. Whether there was
/// something to do it to, where that matters (`Copy`: a selection to copy;
/// `CopyAsOpen`: whether it could open; `Note`, `Bookmark`, `Highlight`: a
/// selection; `Escape`: a preview or a selection), else `true`.
pub fn ask(ask: Ask) -> bool {
    match ask {
        Ask::Copy => {
            let selected = has_selection();
            copy_passage(Slot::Selection, "reference");
            selected
        }
        Ask::CopyChapter => {
            copy_passage(Slot::Chapter, "reference");
            true
        }
        Ask::CopyAs { slot, opener } => {
            if with(|s| s.menu_for) == Some(slot) {
                close_menu(true);
            } else {
                open_menu(slot, opener);
            }
            true
        }
        Ask::CopyAsOpen(slot) => {
            let opener = match slot {
                Slot::Selection => elements().and_then(|els| {
                    els.actions
                        .query_selector(".verse-copy-as-btn")
                        .ok()
                        .flatten()
                        .and_then(|b| b.dyn_into::<HtmlElement>().ok())
                }),
                Slot::Chapter => pane::active().query(".chapter-copy-btn"),
            };
            let can = slot == Slot::Chapter || has_selection();
            if can {
                open_menu(slot, opener);
            }
            can
        }
        Ask::CopyFormat(format) => {
            choose_format(&format);
            true
        }
        Ask::CloseCopyMenu => {
            close_menu(false);
            true
        }
        // B and H, like N, go on to other keys without a selection.
        Ask::Bookmark => {
            let selected = has_selection();
            spawn_local(toggle_bookmark());
            selected
        }
        Ask::Highlight(color) => {
            let selected = has_selection();
            spawn_local(highlight_selection(color));
            selected
        }
        // N goes on to other keys without a selection.
        Ask::Note(opener) => {
            let selected = has_selection();
            if selected {
                open_add_note(opener);
            }
            selected
        }
        Ask::Undo => {
            let undo = with(|s| s.undo.clone());
            status("", false);
            if let Some(els) = elements() {
                dom::focus(&els.note_add);
            }
            if let Some(note) = undo {
                spawn_local(restore_note_in_card(note));
            }
            true
        }
        Ask::NoteEdit { note_id, opener } => {
            let found = with(|s| {
                let selection = s.selection.as_ref()?;
                let note = s
                    .selection_notes
                    .iter()
                    .find(|n| n.note.id.to_string() == note_id)?;
                Some((note.raw.clone(), selection.translation_code.clone()))
            });
            if let Some((note, code)) = found {
                edit(note, &code, opener);
            }
            true
        }
        Ask::NoteMore(more) => {
            let expanded = more.get_attribute("aria-expanded").as_deref() != Some("true");
            if let Some(body) = more
                .get_attribute("aria-controls")
                .and_then(|id| by_id(&id))
            {
                toggle_class(&body, "is-expanded", expanded);
            }
            let _ = more.set_attribute("aria-expanded", &expanded.to_string());
            more.set_text_content(Some(if expanded { "less" } else { "more" }));
            true
        }
        Ask::NoteCopy(id) => {
            let Some(note) = with(|s| {
                s.selection_notes
                    .iter()
                    .find(|n| n.note.id == id)
                    .map(|n| n.note.clone())
            }) else {
                return false;
            };
            let attempt = with(State::attempt);
            crate::note_copy::copy(&(&note).into(), note.reference, move |message, is_error| {
                status_for(attempt, message, is_error);
            });
            true
        }
        Ask::ShowAll => {
            with(|s| s.show_all_notes = true);
            render_notes_section();
            if let Some(next) = elements().and_then(|els| {
                query_all(&els.notes, ".note-edit")
                    .into_iter()
                    .nth(text::NOTES_SHOWN)
            }) {
                focus_scrolling(&next);
            }
            true
        }
        Ask::Xref(chip) => {
            toggle_preview(chip);
            true
        }
        Ask::Select {
            translation_code,
            anchor,
            focus,
        } => {
            select_verses(&translation_code, anchor, focus);
            true
        }
        Ask::Extend {
            translation_code,
            verse,
        } => {
            apply(with(|s| {
                text::extend(s.selection.as_ref(), &translation_code, verse)
            }));
            true
        }
        Ask::Toggle {
            translation_code,
            verse,
        } => {
            apply(with(|s| {
                text::toggle(s.selection.as_ref(), &translation_code, verse)
            }));
            true
        }
        Ask::Clear => {
            clear_selection();
            true
        }
        Ask::Escape => {
            let (preview, selected) = with(|s| (s.preview_chip.is_some(), s.selection.is_some()));
            if preview {
                let in_card = elements().is_some_and(|els| contains_focus(&els.card));
                close_preview(in_card);
            } else if selected {
                clear_selection();
            }
            preview || selected
        }
    }
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
        // The Shift ghost follows the keyboard and the pointer, before any
        // other handler.
        listen_capturing(&window, "keydown", |event: KeyboardEvent| {
            on_shift_key(&event, true);
        });
        listen_capturing(&window, "keyup", |event: KeyboardEvent| {
            on_shift_key(&event, false);
        });
        for name in [
            "pointermove",
            "pointerdown",
            "pointerup",
            "pointercancel",
            "wheel",
        ] {
            listen_capturing(&window, name, |event: web_sys::MouseEvent| {
                on_shift_pointer(&event);
            });
        }
        // Before the page's own click dispatcher (on the body).
        listen_capturing(&window, "click", |event: web_sys::MouseEvent| {
            if with(|s| std::mem::take(&mut s.swallow_click)) {
                event.prevent_default();
                event.stop_propagation();
            }
        });
        dom::on(&window, "blur", |_| release_shift());
        // Focus back: whatever was held may have been let go elsewhere; a
        // Shift still down is picked up by the next event that says so.
        dom::on(&window, "focus", |_| release_shift());
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
        later(0, sync_ghost);
    });
}
