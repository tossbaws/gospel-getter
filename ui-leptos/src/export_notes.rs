//! Export notes…: the notes chosen by book, chapters and when they were
//! written, as a readable Markdown file with each note's verses quoted in
//! the translation being read. The backend works out the choices when the
//! dialog opens (the books with notes, and date presets in local days),
//! counts the notes the choices match as they change, builds the file, asks
//! where to save it and writes it, all by one filter; this only shows them.
//!
//! A small modal, as the page's note panel: it has the keyboard while open
//! (a capturing `keydown` on `window`, stopping every other handler), Escape
//! closes it, Tab stays inside, and closing gives focus back to the button.
//! It opens fresh every time: nothing is kept.
//!
//! This is the page's export-notes script (`ui/index.html` at v2.16.1,
//! 6960–7217) as a Leptos component: the same ids, classes, ARIA, `hidden`
//! and `disabled`, text and focus order, so the CSS, the page's own
//! handlers and the frontend tests see what they saw.

use std::collections::BTreeSet;

use leptos::html;
use leptos::prelude::*;
use leptos::task::{spawn_local, tick};
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use web_sys::{CustomEvent, HtmlElement, HtmlInputElement, KeyboardEvent, MouseEvent, NodeList};

use crate::bridge::{self, OpenRequest};
use crate::dom::{self, focus, inside_hidden};
use crate::dto::{CountArgs, ExportArgs, ExportOptions, ExportOutcome, Filter, NotedBook};
use crate::ipc;
use crate::text::{Fields, build_filter, chapter_value, count_line, plural, thousands};

/// What is under way, outside the reactive graph (nothing renders from it).
#[derive(Debug, Clone, Copy, Default)]
struct Busy {
    /// An export is being written (the dialog is closed meanwhile).
    exporting: bool,
    /// The choices are being fetched to open the dialog.
    opening: bool,
    /// Only the latest count gets to show: each request takes the next
    /// number, and an answer whose number is stale is dropped.
    count_token: u64,
}

/// The dialog's state and the elements it reaches for. Everything is
/// `Copy`, so the handlers below take the whole thing by value.
#[derive(Clone, Copy)]
struct Dialog {
    open: RwSignal<bool>,
    /// What the page said when it asked the dialog to open.
    request: RwSignal<Option<OpenRequest>>,
    /// What the backend offered when the dialog opened.
    options: RwSignal<Option<ExportOptions>>,
    /// The ids of the ticked books.
    ticked: RwSignal<BTreeSet<i64>>,
    /// The ticked books, in Bible order (the checklist's).
    ticked_books: Memo<Vec<NotedBook>>,
    /// The book the chapter fields were last set for.
    chapter_book: RwSignal<Option<i64>>,
    chapter_max: RwSignal<String>,
    chapter_from: RwSignal<String>,
    chapter_to: RwSignal<String>,
    /// The Written `<select>`'s value: `all`, a preset, or `custom`.
    when: RwSignal<String>,
    date_from: RwSignal<String>,
    date_to: RwSignal<String>,
    error: RwSignal<String>,
    count: RwSignal<String>,
    /// How many notes the choices match, once the backend has said; `None`
    /// while it hasn't, or when the choices are wrong. Export needs some.
    matches: RwSignal<Option<u64>>,
    busy: StoredValue<Busy>,
    overlay: NodeRef<html::Div>,
    panel: NodeRef<html::Div>,
    books: NodeRef<html::Div>,
}

impl Dialog {
    fn new() -> Self {
        let options = RwSignal::new(None::<ExportOptions>);
        let ticked = RwSignal::new(BTreeSet::new());
        let ticked_books = Memo::new(move |_| {
            options.with(|options| {
                let Some(options) = options else {
                    return Vec::new();
                };
                ticked.with(|ticked| {
                    options
                        .books
                        .iter()
                        .filter(|b| ticked.contains(&b.id))
                        .cloned()
                        .collect()
                })
            })
        });
        Self {
            open: RwSignal::new(false),
            request: RwSignal::new(None),
            options,
            ticked,
            ticked_books,
            chapter_book: RwSignal::new(None),
            chapter_max: RwSignal::new(String::new()),
            chapter_from: RwSignal::new(String::new()),
            chapter_to: RwSignal::new(String::new()),
            when: RwSignal::new("all".to_string()),
            date_from: RwSignal::new(String::new()),
            date_to: RwSignal::new(String::new()),
            error: RwSignal::new(String::new()),
            count: RwSignal::new(String::new()),
            matches: RwSignal::new(None),
            busy: StoredValue::new(Busy::default()),
            overlay: NodeRef::new(),
            panel: NodeRef::new(),
            books: NodeRef::new(),
        }
    }

    /// The one ticked book, when exactly one is (tracked).
    fn single_book(self) -> Option<NotedBook> {
        self.ticked_books.with(|books| match books.as_slice() {
            [book] => Some(book.clone()),
            _ => None,
        })
    }

    /// The chapter fields show only while exactly one book is ticked, and
    /// only when it has two chapters with notes to choose between.
    fn chapters_hidden(self) -> bool {
        self.single_book()
            .is_none_or(|book| book.noted_chapters < 2)
    }

    fn fields(self) -> Fields {
        Fields {
            chapter_from: self.chapter_from.get_untracked(),
            chapter_to: self.chapter_to.get_untracked(),
            when: self.when.get_untracked(),
            date_from: self.date_from.get_untracked(),
            date_to: self.date_to.get_untracked(),
        }
    }

    /// The filter the backend reads for the choices as they are.
    fn filter(self) -> Option<Filter> {
        let options = self.options.get_untracked()?;
        let ticked = self.ticked_books.get_untracked();
        let ticked: Vec<&NotedBook> = ticked.iter().collect();
        Some(build_filter(
            &ticked,
            options.books.len(),
            &self.fields(),
            &options.dates,
        ))
    }

    /// Shows the backend's count, or why the choices are wrong.
    fn show_count(self, count: Option<u64>, error: Option<String>) {
        match error {
            Some(error) => {
                self.matches.set(None);
                self.error.set(error);
                self.count.set(String::new());
            }
            None => {
                self.matches.set(count);
                self.error.set(String::new());
                self.count.set(count.map(count_line).unwrap_or_default());
            }
        }
    }

    /// Asks the backend how many notes the choices match. No book ticked
    /// matches nothing, without asking (an empty book list would be every
    /// book).
    fn refresh_count(self) {
        let token = self.busy.get_value().count_token + 1;
        self.busy.update_value(|b| b.count_token = token);
        self.matches.set(None);
        if self.ticked_books.get_untracked().is_empty() {
            self.show_count(Some(0), None);
            return;
        }
        let Some(filter) = self.filter() else {
            return;
        };
        spawn_local(async move {
            let result = match ipc::args(&CountArgs { filter: &filter }) {
                Ok(args) => ipc::invoke::<u64>("count_notes_export", args).await,
                Err(e) => Err(e),
            };
            if self.busy.get_value().count_token != token {
                return;
            }
            match result {
                Ok(count) => self.show_count(Some(count), None),
                Err(e) => self.show_count(None, Some(ipc::error_text(&e))),
            }
        });
    }

    /// The chapter fields follow the one ticked book: starting on its
    /// chapters with notes when it changes, held to its chapter count.
    fn sync_chapters(self) {
        let book = self
            .ticked_books
            .with_untracked(|books| match books.as_slice() {
                [book] => Some(book.clone()),
                _ => None,
            });
        if let Some(book) = &book
            && self.chapter_book.get_untracked() != Some(book.id)
        {
            self.chapter_max.set(book.chapter_count.to_string());
            self.chapter_from.set(book.first_chapter.to_string());
            self.chapter_to.set(book.last_chapter.to_string());
        }
        self.chapter_book.set(book.map(|b| b.id));
    }

    fn choices_changed(self) {
        self.sync_chapters();
        self.refresh_count();
    }

    /// Opens the dialog on the book being read if it has notes, otherwise
    /// on every book, and on All time.
    fn open(self, request: OpenRequest) {
        let busy = self.busy.get_value();
        if busy.exporting || busy.opening || request.note_count == 0 {
            return;
        }
        self.busy.update_value(|b| b.opening = true);
        spawn_local(async move {
            let result =
                ipc::invoke::<ExportOptions>("get_notes_export_options", JsValue::UNDEFINED).await;
            self.busy.update_value(|b| b.opening = false);
            let options = match result {
                Ok(options) => options,
                Err(e) => {
                    ipc::log_error("Failed to prepare the notes export", &e);
                    bridge::toast(&format!("Export failed: {}", ipc::error_text(&e)), true);
                    return;
                }
            };
            if options.books.is_empty() {
                return;
            }
            let current = request
                .book_id
                .filter(|id| options.books.iter().any(|b| b.id == *id));
            let ticked: BTreeSet<i64> = match current {
                Some(id) => BTreeSet::from([id]),
                None => options.books.iter().map(|b| b.id).collect(),
            };
            self.request.set(Some(request));
            self.options.set(Some(options));
            self.ticked.set(ticked);
            self.chapter_book.set(None);
            self.when.set("all".to_string());
            self.date_from.set(String::new());
            self.date_to.set(String::new());
            self.error.set(String::new());
            self.count.set(String::new());
            self.sync_chapters();
            self.open.set(true);
            // The checklist is on screen once the reactive system has
            // caught up with those changes.
            tick().await;
            self.focus_first_box();
            self.refresh_count();
        });
    }

    /// Focus goes to the first ticked book (or the first book), and the
    /// list scrolls to it; only the list, so the page behind keeps still.
    fn focus_first_box(self) {
        let Some(list) = self.books.get() else {
            return;
        };
        let Ok(boxes) = list.query_selector_all("input[type=\"checkbox\"]") else {
            return;
        };
        let boxes = inputs(&boxes);
        let Some(first) = boxes.iter().find(|b| b.checked()).or(boxes.first()) else {
            return;
        };
        list.set_scroll_top(0);
        if let Ok(Some(label)) = first.closest("label")
            && let Some(label) = label.dyn_ref::<HtmlElement>()
        {
            list.set_scroll_top(label.offset_top() - list.client_height() / 2);
        }
        focus(first);
    }

    /// Closes the dialog. Focus goes back to the button that opened it, or
    /// to the page's focus home if the button is gone, hidden or disabled.
    fn close(self) {
        self.open.set(false);
        self.busy.update_value(|b| b.count_token += 1);
        match bridge::export_button() {
            Some(button)
                if button.is_connected()
                    && !inside_hidden(&button)
                    && !button.has_attribute("disabled") =>
            {
                focus(&button);
            }
            _ => bridge::focus_home(),
        }
    }

    /// Export…: builds the file by the filter, asks where to save it and
    /// writes it, all in the backend. Cancelling the save dialog says
    /// nothing; nothing was saved.
    fn export(self) {
        if !self.open.get_untracked()
            || self.busy.get_value().exporting
            || self.matches.get_untracked().unwrap_or(0) == 0
        {
            return;
        }
        let Some(filter) = self.filter() else {
            return;
        };
        let book = self
            .ticked_books
            .with_untracked(|books| match books.as_slice() {
                [book] => Some(book.name.clone()),
                _ => None,
            });
        let translation_code = self
            .request
            .with_untracked(|r| r.as_ref().map(|r| r.translation_code.clone()))
            .unwrap_or_default();
        self.close();
        self.busy.update_value(|b| b.exporting = true);
        spawn_local(async move {
            let args = ExportArgs {
                translation_code: &translation_code,
                filter: &filter,
            };
            let result = match ipc::args(&args) {
                Ok(args) => ipc::invoke::<ExportOutcome>("export_notes", args).await,
                Err(e) => Err(e),
            };
            match result {
                Ok(ExportOutcome::Saved { path, notes }) => {
                    let on = book.map(|name| format!(" on {name}")).unwrap_or_default();
                    bridge::toast(
                        &format!("Exported {}{on} to {path}.", plural(notes, "note")),
                        false,
                    );
                }
                Ok(ExportOutcome::Cancelled) => {}
                Err(e) => {
                    ipc::log_error("Notes export failed", &e);
                    bridge::toast(&format!("Export failed: {}", ipc::error_text(&e)), true);
                }
            }
            self.busy.update_value(|b| b.exporting = false);
        });
    }

    /// While the dialog is open it has the keyboard, before any other
    /// handler sees a key: Escape closes it (and only it), and Tab stays
    /// inside. Listens on `window`, capturing, as the page's modals do.
    fn on_keydown(self, event: KeyboardEvent) {
        if !self.open.get_untracked() {
            return;
        }
        event.stop_propagation();
        match event.key().as_str() {
            "Escape" => {
                event.prevent_default();
                self.close();
            }
            "Tab" => self.trap_tab(&event),
            _ => {}
        }
    }

    /// Tab and Shift+Tab wrap among the panel's enabled, shown fields and
    /// buttons; from outside the panel they go to its first or last.
    fn trap_tab(self, event: &KeyboardEvent) {
        let Some(panel) = self.panel.get() else {
            return;
        };
        let stops = dom::focusable(&panel, "input, select, button");
        dom::trap_tab(event, &panel, &stops);
    }

    /// Clicking the dimmed page behind the panel does nothing, and leaves
    /// focus where it was.
    fn on_backdrop_mousedown(self, event: MouseEvent) {
        if let Some(overlay) = self.overlay.get()
            && dom::targets(&event, &overlay)
        {
            event.prevent_default();
        }
    }

    /// Listens for the page's request to open, and for the keyboard.
    fn listen(self) {
        dom::on_document_event(bridge::OPEN_EVENT, move |event: CustomEvent| {
            // Handled, whatever happens next: the dialog is here.
            event.prevent_default();
            match bridge::open_request(&event) {
                Ok(request) => self.open(request),
                Err(e) => ipc::log_error("The export-notes request couldn't be read", &e),
            }
        });
        dom::on_window_keydown(move |event: KeyboardEvent| self.on_keydown(event));
    }
}

fn inputs(list: &NodeList) -> Vec<HtmlInputElement> {
    (0..list.length())
        .filter_map(|i| list.item(i))
        .filter_map(|node| node.dyn_into::<HtmlInputElement>().ok())
        .collect()
}

/// The Export notes dialog, mounted once, hidden until the page asks for it.
#[component]
pub fn ExportNotes() -> impl IntoView {
    let dialog = Dialog::new();
    dialog.listen();

    let translation_name = move || {
        dialog
            .request
            .with(|r| r.as_ref().map(|r| r.translation_name.clone()))
            .unwrap_or_default()
    };
    let book_rows = move || {
        dialog.options.with(|options| {
            options.as_ref().map(|options| {
                options
                    .books
                    .iter()
                    .map(|book| {
                        let id = book.id;
                        view! {
                            <label class="export-notes-book-option">
                                <input
                                    type="checkbox"
                                    value=id.to_string()
                                    prop:checked=move || dialog.ticked.with(|t| t.contains(&id))
                                    on:change=move |ev| {
                                        let checked = event_target_checked(&ev);
                                        dialog.ticked.update(|t| {
                                            if checked {
                                                t.insert(id);
                                            } else {
                                                t.remove(&id);
                                            }
                                        });
                                        dialog.choices_changed();
                                    }
                                />
                                <span>{book.name.clone()}</span>
                                " "
                                <span class="export-notes-book-count">
                                    {format!("({})", thousands(book.notes))}
                                </span>
                            </label>
                        }
                    })
                    .collect_view()
            })
        })
    };
    let tick_all = move |checked: bool| {
        dialog.ticked.set(if checked {
            dialog
                .options
                .with_untracked(|o| o.as_ref().map(|o| o.books.iter().map(|b| b.id).collect()))
                .unwrap_or_default()
        } else {
            BTreeSet::new()
        });
        dialog.choices_changed();
    };
    // Once a chapter field is left, it shows the chapter it counts as.
    let settle_chapter = move |field: RwSignal<String>, from: bool| {
        if dialog.chapters_hidden() {
            return;
        }
        let Some(book) = dialog.single_book() else {
            return;
        };
        let fallback = if from {
            book.first_chapter
        } else {
            book.last_chapter
        };
        let value = chapter_value(&field.get_untracked(), book.chapter_count, fallback);
        field.set(value.to_string());
        dialog.refresh_count();
    };

    view! {
        <div
            id="export-notes-overlay"
            class="modal-overlay"
            hidden=move || !dialog.open.get()
            node_ref=dialog.overlay
            on:mousedown=move |ev| dialog.on_backdrop_mousedown(ev)
        >
            <div
                id="export-notes-panel"
                class="modal-panel export-notes-panel"
                role="dialog"
                aria-modal="true"
                aria-labelledby="export-notes-title"
                aria-describedby="export-notes-text"
                node_ref=dialog.panel
            >
                <div class="modal-body">
                    <h2 id="export-notes-title" class="note-title">"Export notes"</h2>
                    <p id="export-notes-text" class="export-notes-text">
                        "Creates a Markdown (.md) file of your notes, plain text that opens in \
                         Notepad or any text editor. Each note has its verses quoted in the "
                        <span id="export-notes-translation">{translation_name}</span>
                        "."
                    </p>
                    <fieldset class="export-notes-group">
                        <legend class="export-notes-legend">"Books"</legend>
                        <div class="export-notes-tools">
                            <button
                                type="button"
                                id="export-notes-all"
                                class="reader-mode-btn"
                                aria-label="Tick every book"
                                on:click=move |_| tick_all(true)
                            >
                                "All"
                            </button>
                            <button
                                type="button"
                                id="export-notes-none"
                                class="reader-mode-btn"
                                aria-label="Untick every book"
                                on:click=move |_| tick_all(false)
                            >
                                "None"
                            </button>
                        </div>
                        <div id="export-notes-books" class="export-notes-books" node_ref=dialog.books>
                            {book_rows}
                        </div>
                    </fieldset>
                    <fieldset
                        id="export-notes-chapters"
                        class="export-notes-group"
                        hidden=move || dialog.chapters_hidden()
                    >
                        <legend class="export-notes-legend">"Chapters"</legend>
                        <div class="export-notes-range">
                            <label>
                                "From "
                                <input
                                    type="number"
                                    id="export-notes-chapter-from"
                                    class="export-notes-field"
                                    inputmode="numeric"
                                    min="1"
                                    step="1"
                                    max=move || dialog.chapter_max.get()
                                    aria-describedby="export-notes-error"
                                    prop:value=move || dialog.chapter_from.get()
                                    on:input=move |ev| {
                                        dialog.chapter_from.set(event_target_value(&ev));
                                        dialog.refresh_count();
                                    }
                                    on:change=move |_| settle_chapter(dialog.chapter_from, true)
                                />
                            </label>
                            <label>
                                "to "
                                <input
                                    type="number"
                                    id="export-notes-chapter-to"
                                    class="export-notes-field"
                                    inputmode="numeric"
                                    min="1"
                                    step="1"
                                    max=move || dialog.chapter_max.get()
                                    aria-describedby="export-notes-error"
                                    prop:value=move || dialog.chapter_to.get()
                                    on:input=move |ev| {
                                        dialog.chapter_to.set(event_target_value(&ev));
                                        dialog.refresh_count();
                                    }
                                    on:change=move |_| settle_chapter(dialog.chapter_to, false)
                                />
                            </label>
                        </div>
                    </fieldset>
                    <fieldset class="export-notes-group">
                        <legend id="export-notes-when-legend" class="export-notes-legend">
                            "Written"
                        </legend>
                        <div>
                            <select
                                id="export-notes-when"
                                class="export-notes-select"
                                aria-labelledby="export-notes-when-legend"
                                prop:value=move || dialog.when.get()
                                on:change=move |ev| {
                                    dialog.when.set(event_target_value(&ev));
                                    dialog.refresh_count();
                                }
                            >
                                <option value="all">"All time"</option>
                                <option value="last7Days">"Last 7 days"</option>
                                <option value="last30Days">"Last 30 days"</option>
                                <option value="thisYear">"This year"</option>
                                <option value="custom">"Custom"</option>
                            </select>
                            <div
                                id="export-notes-custom"
                                class="export-notes-range export-notes-dates"
                                hidden=move || dialog.when.get() != "custom"
                            >
                                <label>
                                    "From "
                                    <input
                                        type="date"
                                        id="export-notes-from"
                                        class="export-notes-field"
                                        aria-describedby="export-notes-error"
                                        prop:value=move || dialog.date_from.get()
                                        on:input=move |ev| {
                                            dialog.date_from.set(event_target_value(&ev));
                                            dialog.refresh_count();
                                        }
                                        on:change=move |ev| {
                                            dialog.date_from.set(event_target_value(&ev));
                                            dialog.refresh_count();
                                        }
                                    />
                                </label>
                                <label>
                                    "To "
                                    <input
                                        type="date"
                                        id="export-notes-to"
                                        class="export-notes-field"
                                        aria-describedby="export-notes-error"
                                        prop:value=move || dialog.date_to.get()
                                        on:input=move |ev| {
                                            dialog.date_to.set(event_target_value(&ev));
                                            dialog.refresh_count();
                                        }
                                        on:change=move |ev| {
                                            dialog.date_to.set(event_target_value(&ev));
                                            dialog.refresh_count();
                                        }
                                    />
                                </label>
                            </div>
                        </div>
                    </fieldset>
                    <p id="export-notes-error" class="note-error" role="alert">
                        {move || dialog.error.get()}
                    </p>
                    <p id="export-notes-count" class="export-notes-count" aria-live="polite">
                        {move || dialog.count.get()}
                    </p>
                </div>
                <div class="modal-buttons">
                    <button
                        type="button"
                        id="export-notes-confirm"
                        class="import-confirm"
                        disabled=move || dialog.matches.get().unwrap_or(0) == 0
                        on:click=move |_| dialog.export()
                    >
                        "Export…"
                    </button>
                    <button
                        type="button"
                        id="export-notes-cancel"
                        class="reader-mode-btn"
                        on:click=move |_| dialog.close()
                    >
                        "Cancel"
                    </button>
                </div>
            </div>
        </div>
    }
}
