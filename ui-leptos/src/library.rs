//! The Library's tabs and its three lists: Bookmarks, Highlights (every
//! highlighted passage, with a color filter) and Notes, each row one link
//! to its passage with its own × or Edit beside it.
//!
//! The Library is one of the page's surfaces: `surfaces` opens and closes
//! `#library-panel` (not in reading mode), keeps one surface open at a
//! time and focuses the chosen tab when it opens. This draws what's
//! inside it (put there by [`place`]), switches the tabs, loads the lists
//! and acts on their rows: opening a row closes the Library and has the
//! reader jump to the passage (a note's Edit then has the page open the
//! note editor);
//! removing a bookmark or a highlight keeps focus in the list.
//!
//! The page asks for the lists to be reloaded with
//! `bridge::LIBRARY_REFRESH_EVENT`, and gets back what the reader's own
//! marks need with `bridge::LIBRARY_LOADED_EVENT`: each list is asked of
//! the backend here, once per reload, never there too. Only the latest of
//! overlapping reloads of a list is shown, as before (each list has its own
//! token).
//!
//! This is the page's Library script (`ui/index.html` at 6d8d9a5: the tabs,
//! `renderBookmarkList`, `renderHighlightList`, `renderNoteList`, their
//! refreshes and their clicks) as a Leptos component: the same ids,
//! classes, ARIA, `hidden` and `disabled`, text and focus order. The tabs
//! answer within the click or key, as they did; the lists are signals.

use leptos::html;
use leptos::prelude::*;
use leptos::task::{spawn_local, tick};
use serde::de::DeserializeOwned;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{CustomEvent, Element, HtmlElement, KeyboardEvent, MouseEvent};

use crate::bridge;
use crate::card;
use crate::dom;
use crate::dto::{
    Bookmark, HighlightPassage, Note, PassageArgs, RemoveBookmarkArgs, TranslationArgs,
};
use crate::ipc;
use crate::library_text::{self as text, List};
use crate::surfaces;

/// A list as last shown: its rows, or why it couldn't be loaded (or a row
/// removed) in place of them. The rows stay meanwhile, as the page kept
/// them: the color filter shows them again.
#[derive(Clone)]
struct Shown<T> {
    items: Vec<T>,
    error: Option<String>,
}

impl<T> Shown<T> {
    fn rows(items: Vec<T>) -> Self {
        Self { items, error: None }
    }
}

/// The count beside a tab: how many rows, or nothing before the first
/// answer or after a failure.
fn count<T>(shown: Option<&Shown<T>>) -> String {
    match shown {
        Some(Shown { items, error: None }) => items.len().to_string(),
        _ => String::new(),
    }
}

/// A note in the list, with the note as the backend gave it, which the
/// page's note editor is opened on.
#[derive(Clone)]
struct NoteRow {
    note: Note,
    raw: JsValue,
}

#[derive(Clone, Copy)]
struct Library {
    /// `None` until the list's first answer: the list is empty, its count
    /// blank (and Export notes disabled).
    bookmarks: RwSignal<Option<Shown<Bookmark>>>,
    highlights: RwSignal<Option<Shown<HighlightPassage>>>,
    notes: RwSignal<Option<Shown<NoteRow>>, LocalStorage>,
    /// The color filter: `all` or a color, for this session only.
    filter: RwSignal<String>,
    /// Each list's latest reload, by `List::index`.
    tokens: StoredValue<[u64; 3]>,
    tabs: [NodeRef<html::Button>; 3],
    panels: [NodeRef<html::Div>; 3],
    bookmark_list: NodeRef<html::Ul>,
    highlight_filter: NodeRef<html::Div>,
    highlight_list: NodeRef<html::Ul>,
    note_list: NodeRef<html::Ul>,
}

/// The answer to `cmd`, as the rows and as it came.
async fn fetch<T: DeserializeOwned>(cmd: &str, args: JsValue) -> Result<(T, JsValue), JsValue> {
    let raw = ipc::invoke_value(cmd, args).await?;
    let items = serde_wasm_bindgen::from_value(raw.clone()).map_err(JsValue::from)?;
    Ok((items, raw))
}

/// The translation previewed: its code's arguments.
fn translation_args(code: &str) -> Result<JsValue, JsValue> {
    ipc::args(&TranslationArgs {
        translation_code: code,
    })
}

/// The element an event happened on.
fn target_of(event: &web_sys::Event) -> Option<Element> {
    event.target()?.dyn_into::<Element>().ok()
}

/// `el.closest(selector)`, if any.
fn closest(el: &Element, selector: &str) -> Option<Element> {
    el.closest(selector).ok().flatten()
}

/// A plain `focus()`, as the page's list handlers did.
fn focus_plain(el: &HtmlElement) {
    let _ = el.focus();
}

/// A `data-*` id or index on a row's button.
fn data_number<T: std::str::FromStr>(el: &Element, name: &str) -> Option<T> {
    el.get_attribute(name)?.parse().ok()
}

impl Library {
    fn new() -> Self {
        Self {
            bookmarks: RwSignal::new(None),
            highlights: RwSignal::new(None),
            notes: RwSignal::new_local(None),
            filter: RwSignal::new(text::ALL.to_string()),
            tokens: StoredValue::new([0; 3]),
            tabs: [NodeRef::new(), NodeRef::new(), NodeRef::new()],
            panels: [NodeRef::new(), NodeRef::new(), NodeRef::new()],
            bookmark_list: NodeRef::new(),
            highlight_filter: NodeRef::new(),
            highlight_list: NodeRef::new(),
            note_list: NodeRef::new(),
        }
    }

    // ---- The tabs: the one chosen is kept for this session only.

    fn select_tab(self, index: usize, focus: bool) {
        for (i, (tab, panel)) in self.tabs.iter().zip(&self.panels).enumerate() {
            let selected = i == index;
            if let Some(tab) = tab.get_untracked() {
                let _ = tab.set_attribute("aria-selected", if selected { "true" } else { "false" });
                tab.set_tab_index(if selected { 0 } else { -1 });
                if selected && focus {
                    focus_plain(&tab);
                }
            }
            if let Some(panel) = panel.get_untracked() {
                panel.set_hidden(!selected);
            }
        }
    }

    /// Which tab `el` is (or is in).
    fn tab_index(self, el: &Element) -> Option<usize> {
        let tab = closest(el, "[role=\"tab\"]")?;
        self.tabs.iter().position(|t| {
            t.get_untracked()
                .is_some_and(|t| dom::same(Some(&tab), t.as_ref()))
        })
    }

    fn on_tab_click(self, event: &MouseEvent) {
        if let Some(index) = target_of(event).and_then(|el| self.tab_index(&el)) {
            self.select_tab(index, false);
        }
    }

    /// Arrow keys, Home and End move between the tabs, and are no chapter
    /// turn: they stop here, before the page's keyboard handler.
    fn on_tab_key(self, event: &KeyboardEvent) {
        let Some(index) = target_of(event).and_then(|el| self.tab_index(&el)) else {
            return;
        };
        let Some(next) = text::tab_after(index, self.tabs.len(), &event.key()) else {
            return;
        };
        event.prevent_default();
        event.stop_propagation();
        self.select_tab(next, true);
    }

    // ---- Loading: only the latest reload of a list is shown.

    /// Starts reloading `list` (its token taken, and the translation read,
    /// now); the future finishes once the answer is shown, or dropped
    /// because a newer reload was asked for.
    fn reload(self, list: List) -> impl Future<Output = ()> + 'static {
        let index = list.index();
        self.tokens.update_value(|t| t[index] += 1);
        let token = self.tokens.with_value(|t| t[index]);
        let code = bridge::translation_code();
        async move {
            match list {
                List::Bookmarks => self.load_bookmarks(token, &code).await,
                List::Highlights => self.load_highlights(token, &code).await,
                List::Notes => self.load_notes(token).await,
            }
            tick().await;
        }
    }

    fn current(self, list: List, token: u64) -> bool {
        self.tokens.with_value(|t| t[list.index()] == token)
    }

    async fn load_bookmarks(self, token: u64, code: &str) {
        let answer = match translation_args(code) {
            Ok(args) => fetch::<Vec<Bookmark>>("list_bookmarks", args).await,
            Err(e) => Err(e),
        };
        if !self.current(List::Bookmarks, token) {
            return;
        }
        match answer {
            Ok((items, raw)) => {
                self.bookmarks.set(Some(Shown::rows(items)));
                bridge::library_loaded(List::Bookmarks.name(), &raw);
            }
            Err(e) => {
                ipc::log_error("Failed to load bookmarks", &e);
                self.show_error(
                    List::Bookmarks,
                    List::Bookmarks.load_failed(&ipc::error_text(&e)),
                );
                bridge::library_loaded(List::Bookmarks.name(), &JsValue::NULL);
            }
        }
    }

    async fn load_highlights(self, token: u64, code: &str) {
        let answer = match translation_args(code) {
            Ok(args) => fetch::<Vec<HighlightPassage>>("list_highlight_passages", args).await,
            Err(e) => Err(e),
        };
        if !self.current(List::Highlights, token) {
            return;
        }
        match answer {
            Ok((items, _)) => self.highlights.set(Some(Shown::rows(items))),
            Err(e) => {
                ipc::log_error("Failed to load highlighted passages", &e);
                self.show_error(
                    List::Highlights,
                    List::Highlights.load_failed(&ipc::error_text(&e)),
                );
            }
        }
    }

    async fn load_notes(self, token: u64) {
        let answer = fetch::<Vec<Note>>("list_all_notes", JsValue::UNDEFINED).await;
        if !self.current(List::Notes, token) {
            return;
        }
        match answer {
            Ok((items, raw)) => {
                let raw = js_sys::Array::from(&raw);
                let rows = items
                    .into_iter()
                    .zip(raw.iter())
                    .map(|(note, raw)| NoteRow { note, raw })
                    .collect();
                self.notes.set(Some(Shown::rows(rows)));
                bridge::library_loaded(List::Notes.name(), &raw);
            }
            Err(e) => {
                ipc::log_error("Failed to load notes", &e);
                self.show_error(List::Notes, List::Notes.load_failed(&ipc::error_text(&e)));
                bridge::library_loaded(List::Notes.name(), &JsValue::NULL);
            }
        }
    }

    /// Shows `message` in place of `list`'s rows (which it keeps).
    fn show_error(self, list: List, message: String) {
        fn set<T>(shown: &mut Option<Shown<T>>, message: String) {
            match shown {
                Some(shown) => shown.error = Some(message),
                None => {
                    *shown = Some(Shown {
                        items: Vec::new(),
                        error: Some(message),
                    })
                }
            }
        }
        match list {
            List::Bookmarks => self.bookmarks.update(|s| set(s, message)),
            List::Highlights => self.highlights.update(|s| set(s, message)),
            List::Notes => self.notes.update(|s| set(s, message)),
        }
    }

    /// The page asks for lists: each named starts reloading now, and
    /// `detail.done` settles once all have.
    fn on_refresh(self, event: CustomEvent) {
        event.prevent_default();
        let lists = match bridge::library_refresh(&event) {
            Ok(lists) => lists,
            Err(e) => {
                ipc::log_error("A Library reload the Library can't read", &e);
                Vec::new()
            }
        };
        let reloads = js_sys::Array::new();
        for list in lists.iter().filter_map(|name| List::named(name)) {
            let reload = self.reload(list);
            reloads.push(&wasm_bindgen_futures::future_to_promise(async move {
                reload.await;
                Ok(JsValue::UNDEFINED)
            }));
        }
        bridge::library_refresh_done(&event, &js_sys::Promise::all(&reloads));
    }

    // ---- The rows' clicks.

    /// A row is one link to its passage: a click anywhere on it outside its
    /// own buttons is a click on its `.bookmark-open`. Any other click goes
    /// to `on_button`, with the element clicked.
    fn on_list_click(event: &MouseEvent, on_button: impl FnOnce(Element)) {
        let Some(target) = target_of(event) else {
            return;
        };
        if closest(&target, "button, a, input").is_none() {
            let open = closest(&target, ".bookmark-item")
                .and_then(|row| row.query_selector(".bookmark-open").ok().flatten())
                .and_then(|open| open.dyn_into::<HtmlElement>().ok());
            if let Some(open) = open {
                open.click();
            }
            return;
        }
        on_button(target);
    }

    /// Opens a passage: the Library closes (focus back to its button, or
    /// left for the editor), and the reader jumps there.
    fn open(passage: PassageArgs, note: &JsValue) {
        surfaces::close(note.is_null());
        crate::reader::open(
            &bridge::OpenPassage {
                book_id: passage.book_id,
                chapter: passage.chapter,
                verse_start: Some(passage.verse_start),
                verse_end: Some(passage.verse_end),
            },
            note.clone(),
        );
    }

    fn on_bookmark_click(self, target: Element) {
        let remove = closest(&target, ".bookmark-remove");
        let open = closest(&target, ".bookmark-open");
        let Some(button) = remove.as_ref().or(open.as_ref()) else {
            return;
        };
        let Some(id) = data_number::<i64>(button, "data-bookmark-id") else {
            return;
        };
        let found = self.bookmarks.with_untracked(|s| {
            let items = &s.as_ref()?.items;
            let index = items.iter().position(|b| b.id == id)?;
            Some((index, items[index].clone()))
        });
        let Some((index, bookmark)) = found else {
            return;
        };
        if remove.is_none() {
            Self::open(
                PassageArgs {
                    book_id: bookmark.book_id,
                    chapter: bookmark.chapter,
                    verse_start: bookmark.verse_start,
                    verse_end: bookmark.verse_end,
                },
                &JsValue::NULL,
            );
            return;
        }
        spawn_local(async move {
            let removed = match ipc::args(&RemoveBookmarkArgs { id }) {
                Ok(args) => ipc::invoke_value("remove_bookmark", args).await,
                Err(e) => Err(e),
            };
            if let Err(e) = removed {
                ipc::log_error("Failed to remove bookmark", &e);
                self.show_error(
                    List::Bookmarks,
                    text::remove_failed(&bookmark.reference, &ipc::error_text(&e)),
                );
                return;
            }
            self.reload(List::Bookmarks).await;
            // Keep keyboard focus in the list, on the next bookmark.
            let next = self
                .bookmark_list
                .get_untracked()
                .and_then(|list| list.query_selector_all(".bookmark-remove").ok())
                .map(|found| dom::elements(&found))
                .and_then(|buttons| {
                    text::next_after_removing(index, buttons.len()).map(|i| buttons[i].clone())
                })
                .or_else(|| self.tabs[0].get_untracked().map(Into::into));
            if let Some(next) = next {
                focus_plain(&next);
            }
        });
    }

    fn on_highlight_click(self, target: Element) {
        let remove = closest(&target, ".bookmark-remove");
        let open = closest(&target, ".bookmark-open");
        let Some(button) = remove.as_ref().or(open.as_ref()) else {
            return;
        };
        let Some(index) = data_number::<usize>(button, "data-index") else {
            return;
        };
        let Some(passage) = self
            .highlights
            .with_untracked(|s| s.as_ref()?.items.get(index).cloned())
        else {
            return;
        };
        let args = PassageArgs {
            book_id: passage.book_id,
            chapter: passage.chapter,
            verse_start: passage.verse_start,
            verse_end: passage.verse_end,
        };
        let Some(remove) = remove else {
            Self::open(args, &JsValue::NULL);
            return;
        };
        let list = self.highlight_list;
        let buttons = move || {
            list.get_untracked()
                .and_then(|list| list.query_selector_all(".bookmark-remove").ok())
                .map(|found| dom::elements(&found))
                .unwrap_or_default()
        };
        let position = buttons()
            .iter()
            .position(|b| dom::same(Some(&remove), b.as_ref()));
        spawn_local(async move {
            let removed = match ipc::args(&args) {
                Ok(args) => ipc::invoke_value("remove_highlight", args).await,
                Err(e) => Err(e),
            };
            if let Err(e) = removed {
                ipc::log_error("Failed to remove highlight", &e);
                self.show_error(
                    List::Highlights,
                    text::remove_failed(&passage.reference, &ipc::error_text(&e)),
                );
                return;
            }
            // The reader's highlights, then this list (as the page's
            // `refreshHighlights` did).
            card::refresh_highlights().await;
            self.reload(List::Highlights).await;
            // Keep keyboard focus in the list, on the next passage, or on
            // the filter once the list is empty.
            let buttons = buttons();
            let next = position
                .and_then(|p| text::next_after_removing(p, buttons.len()))
                .map(|i| buttons[i].clone())
                .or_else(|| {
                    self.highlight_filter
                        .get_untracked()
                        .and_then(|f| f.query_selector("[aria-pressed=\"true\"]").ok().flatten())
                        .and_then(|b| b.dyn_into::<HtmlElement>().ok())
                });
            if let Some(next) = next {
                focus_plain(&next);
            }
        });
    }

    /// Opening a note's row works as opening a bookmark: its chapter, with
    /// its passage selected, so its Notes section shows. Edit does the
    /// same, then opens the note.
    fn on_note_click(self, target: Element) {
        let edit = closest(&target, ".note-list-edit");
        let Some(button) = edit.clone().or_else(|| closest(&target, ".bookmark-open")) else {
            return;
        };
        let Some(id) = data_number::<i64>(&button, "data-note-id") else {
            return;
        };
        let Some(row) = self.notes.with_untracked(|s| {
            s.as_ref()?
                .items
                .iter()
                .find(|row| row.note.id == id)
                .cloned()
        }) else {
            return;
        };
        let note = &row.note;
        let none = JsValue::NULL;
        Self::open(
            PassageArgs {
                book_id: note.book_id,
                chapter: note.chapter,
                verse_start: note.verse_start,
                verse_end: note.verse_end,
            },
            if edit.is_some() { &row.raw } else { &none },
        );
    }

    fn on_filter_click(self, event: &MouseEvent) {
        let Some(button) = target_of(event).and_then(|el| closest(&el, ".highlight-filter-btn"))
        else {
            return;
        };
        let Some(filter) = button.get_attribute("data-filter") else {
            return;
        };
        self.filter.set(filter);
        // Shown again as a list, as the page re-rendered it.
        self.highlights.update(|s| match s {
            Some(shown) => shown.error = None,
            None => *s = Some(Shown::rows(Vec::new())),
        });
    }
}

/// A preview of a passage's first verse in the translation `code`, or that
/// it isn't numbered there (wholly or in part).
fn preview(preview: Option<String>, complete: bool, code: &str) -> AnyView {
    match preview {
        None => view! { <span class="bookmark-preview is-missing">{text::not_numbered(code)}</span> }
            .into_any(),
        Some(preview) => view! {
            <span class="bookmark-preview">{preview}</span>
            {(!complete).then(|| view! { <span class="bookmark-partial">{text::partly_numbered(code)}</span> })}
        }
        .into_any(),
    }
}

/// A list's single row: empty, or what went wrong.
fn message_row(message: String, error: bool) -> AnyView {
    view! { <li class="bookmark-empty" class:is-error=error>{message}</li> }.into_any()
}

fn bookmark_rows(shown: Option<Shown<Bookmark>>) -> AnyView {
    let Some(shown) = shown else {
        return ().into_any();
    };
    if let Some(error) = shown.error {
        return message_row(error, true);
    }
    if shown.items.is_empty() {
        return message_row(List::Bookmarks.empty().to_string(), false);
    }
    let code = bridge::translation_code();
    shown
        .items
        .into_iter()
        .map(|b| {
            let id = b.id.to_string();
            let label = format!("Remove bookmark {}", b.reference);
            view! {
                <li class="bookmark-item">
                    <button type="button" class="bookmark-open" data-bookmark-id=id.clone()>
                        <span class="bookmark-ref">{b.reference}</span>
                        " "
                        {preview(b.preview, b.complete, &code)}
                    </button>
                    " "
                    <button type="button" class="bookmark-remove" data-bookmark-id=id aria-label=label title="Remove bookmark">
                        "\u{d7}"
                    </button>
                </li>
            }
        })
        .collect_view()
        .into_any()
}

fn highlight_rows(shown: Option<Shown<HighlightPassage>>, filter: &str) -> AnyView {
    let Some(shown) = shown else {
        return ().into_any();
    };
    if let Some(error) = shown.error {
        return message_row(error, true);
    }
    if shown.items.is_empty() {
        return message_row(List::Highlights.empty().to_string(), false);
    }
    let indices = text::shown(shown.items.iter().map(|h| h.color.as_str()), filter);
    if indices.is_empty() {
        return message_row(text::no_highlights_in(filter), false);
    }
    let code = bridge::translation_code();
    let mut items: Vec<Option<HighlightPassage>> = shown.items.into_iter().map(Some).collect();
    indices
        .into_iter()
        .filter_map(|index| items[index].take().map(|h| (index, h)))
        .map(|(index, h)| {
            let index = index.to_string();
            let label = format!("Remove highlight {}", h.reference);
            let name = text::color_name(&h.color);
            let color = h.color.clone();
            view! {
                <li class="bookmark-item" data-color=color>
                    <button type="button" class="bookmark-open" data-index=index.clone()>
                        <span class="highlight-head">
                            <span class="bookmark-ref">{h.reference}</span>
                            " "
                            <span class="highlight-color">
                                <span class="highlight-dot" data-color=h.color aria-hidden="true"></span>
                                {name}
                            </span>
                        </span>
                        " "
                        {preview(h.preview, h.complete, &code)}
                    </button>
                    " "
                    <button type="button" class="bookmark-remove" data-index=index aria-label=label title="Remove highlight">
                        "\u{d7}"
                    </button>
                </li>
            }
        })
        .collect_view()
        .into_any()
}

fn note_rows(shown: Option<Shown<NoteRow>>) -> AnyView {
    let Some(shown) = shown else {
        return ().into_any();
    };
    if let Some(error) = shown.error {
        return message_row(error, true);
    }
    if shown.items.is_empty() {
        return message_row(List::Notes.empty().to_string(), false);
    }
    shown
        .items
        .into_iter()
        .map(|NoteRow { note, .. }| {
            let id = note.id.to_string();
            let label = format!("Edit note on {}", note.reference);
            view! {
                <li class="bookmark-item">
                    <button type="button" class="bookmark-open" data-note-id=id.clone()>
                        <span class="bookmark-ref">{note.reference}</span>
                        " "
                        <span class="note-preview">{text::note_preview(&note.body)}</span>
                    </button>
                    " "
                    <button type="button" class="note-list-edit" data-note-id=id aria-label=label title="Edit note">
                        "Edit"
                    </button>
                </li>
            }
        })
        .collect_view()
        .into_any()
}

/// What a click on one of a list's buttons does, given the element clicked.
type RowClick = fn(Library, Element);

/// Draws the Library's tablist and its three tab panels, puts them inside
/// `#library-panel` (between its heading and its hint, where the page had
/// them), and starts listening for the page's reloads.
pub fn mount() {
    let Some(holder) = bridge::document()
        .and_then(|d| d.create_element("div").ok())
        .and_then(|el| el.dyn_into::<HtmlElement>().ok())
    else {
        return;
    };
    let library = Library::new();
    leptos::mount::mount_to(holder.clone(), move || lists(library)).forget();
    // The rows' clicks: a click on a row clicks its open button, which
    // comes back up to the same list.
    let lists: [(NodeRef<html::Ul>, RowClick); 3] = [
        (library.bookmark_list, Library::on_bookmark_click),
        (library.highlight_list, Library::on_highlight_click),
        (library.note_list, Library::on_note_click),
    ];
    for (list, on_button) in lists {
        if let Some(list) = list.get_untracked() {
            dom::on_click(&list, move |event| {
                Library::on_list_click(&event, |target| on_button(library, target));
            });
        }
    }
    dom::on_document_event(bridge::LIBRARY_REFRESH_EVENT, move |event| {
        library.on_refresh(event);
    });
    place(&holder);
}

fn lists(library: Library) -> impl IntoView {
    let [tab_bookmarks, tab_highlights, tab_notes] = library.tabs;
    let [panel_bookmarks, panel_highlights, panel_notes] = library.panels;
    // Export notes… needs notes to export (its click is the page's).
    let no_notes = move || {
        library.notes.with(|s| {
            s.as_ref()
                .is_none_or(|s| s.error.is_some() || s.items.is_empty())
        })
    };

    view! {
        <div
            class="library-tabs"
            role="tablist"
            aria-labelledby="library-title"
            on:click=move |event: MouseEvent| library.on_tab_click(&event)
            on:keydown=move |event: KeyboardEvent| library.on_tab_key(&event)
        >
            <button node_ref=tab_bookmarks type="button" id="library-tab-bookmarks" class="library-tab" role="tab" aria-controls="library-bookmarks" aria-selected="true">
                "Bookmarks "
                <span id="bookmark-count" class="tab-count">{move || library.bookmarks.with(|s| count(s.as_ref()))}</span>
            </button>
            <button node_ref=tab_highlights type="button" id="library-tab-highlights" class="library-tab" role="tab" aria-controls="library-highlights" aria-selected="false" tabindex="-1">
                "Highlights "
                <span id="highlight-count" class="tab-count">{move || library.highlights.with(|s| count(s.as_ref()))}</span>
            </button>
            <button node_ref=tab_notes type="button" id="library-tab-notes" class="library-tab" role="tab" aria-controls="library-notes" aria-selected="false" tabindex="-1">
                "Notes "
                <span id="note-count" class="tab-count">{move || library.notes.with(|s| count(s.as_ref()))}</span>
            </button>
        </div>
        <div node_ref=panel_bookmarks id="library-bookmarks" class="library-tabpanel" role="tabpanel" aria-labelledby="library-tab-bookmarks">
            <ul
                node_ref=library.bookmark_list
                id="bookmark-list"
                class="bookmark-list"
                aria-labelledby="library-tab-bookmarks"
            >
                {move || bookmark_rows(library.bookmarks.get())}
            </ul>
        </div>
        <div node_ref=panel_highlights id="library-highlights" class="library-tabpanel" role="tabpanel" aria-labelledby="library-tab-highlights" hidden>
            <div
                node_ref=library.highlight_filter
                id="highlight-filter"
                class="highlight-filter"
                role="group"
                aria-label="Show highlights"
                on:click=move |event: MouseEvent| library.on_filter_click(&event)
            >
                {text::filters()
                    .map(|f| {
                        view! {
                            <button
                                type="button"
                                class="highlight-filter-btn"
                                data-filter=f
                                aria-pressed=move || library.filter.with(|chosen| if chosen == f { "true" } else { "false" })
                            >
                                {(f != text::ALL).then(|| view! { <span class="highlight-dot" data-color=f aria-hidden="true"></span> })}
                                {text::filter_label(f)}
                            </button>
                        }
                    })
                    .collect_view()}
            </div>
            <ul
                node_ref=library.highlight_list
                id="highlight-list"
                class="bookmark-list"
                aria-labelledby="library-tab-highlights"
            >
                {move || highlight_rows(library.highlights.get(), &library.filter.get())}
            </ul>
        </div>
        <div node_ref=panel_notes id="library-notes" class="library-tabpanel" role="tabpanel" aria-labelledby="library-tab-notes" hidden>
            <button type="button" id="export-notes-btn" class="reader-mode-btn export-notes-btn" aria-haspopup="dialog" disabled=no_notes>
                "Export notes\u{2026}"
            </button>
            <ul
                node_ref=library.note_list
                id="note-list"
                class="bookmark-list"
                aria-labelledby="library-tab-notes"
            >
                {move || note_rows(library.notes.get())}
            </ul>
        </div>
    }
}

/// Moves the Library's tabs and panels from `holder` into `#library-panel`,
/// before its hint.
fn place(holder: &HtmlElement) {
    let Some(document) = bridge::document() else {
        return;
    };
    let Some(panel) = document.get_element_by_id("library-panel") else {
        return;
    };
    let hint = panel.query_selector(".library-hint").ok().flatten();
    while let Some(child) = holder.first_child() {
        let _ = panel.insert_before(&child, hint.as_ref().map(AsRef::as_ref));
    }
}
