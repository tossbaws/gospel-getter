//! Export notes…: the notes chosen by book, chapters and when they were
//! written, as a readable Markdown file with each note's verses quoted in
//! the translation being read, or (Layout "Commentary") as a commentary,
//! one line per note by verse number and no Bible text; or (Save as "One
//! file per book") as a folder of files, one a book, for a notes app such
//! as Obsidian. The commentary can also be printed (Print…): the page draws
//! it into its print-only `#print-commentary` and asks the webview to
//! print (`window.print()`, the system's print dialog). The
//! backend works out the choices when the dialog opens (the books with
//! notes, and date presets in local days), counts the notes the choices
//! match as they change, builds the files, asks where to save them and
//! writes them, all by one filter; this only shows them. Files already in
//! the folder chosen are replaced only once the reader says so: the dialog
//! stays open for a folder, and asks (Replace or Cancel).
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
use crate::dto::{
    Commentary, CommentaryArgs, ConfirmFolderArgs, CountArgs, ExportArgs, ExportOptions,
    ExportOutcome, Filter, FolderOutcome, NotedBook,
};
use crate::ipc;
use crate::text::{
    Fields, build_filter, chapter_value, commentary_markup, count_line, plural, replace_question,
    saved_files, thousands,
};

/// Save as: one file (as always), or one file per book in a folder.
const ONE_FILE: &str = "one";
const BY_BOOK: &str = "books";

/// Layout: each note with its verses quoted (as always), or a commentary
/// (`notes_export::NotesLayout`, by these names).
const QUOTED: &str = "quoted";
const COMMENTARY: &str = "commentary";

/// `<html>`'s attribute while the commentary is being printed: the print
/// stylesheet then prints `#print-commentary` and nothing else.
const PRINTING: &str = "data-printing";

/// An export as one file per book waiting for the reader to say whether
/// to replace the files already in their folder.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ReplaceQuestion {
    token: u64,
    question: String,
}

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
    /// Save as: [`ONE_FILE`] or [`BY_BOOK`].
    layout: RwSignal<String>,
    /// Layout: [`QUOTED`] or [`COMMENTARY`].
    style: RwSignal<String>,
    /// An export to a folder is under way (the dialog stays open for it).
    working: RwSignal<bool>,
    /// The question while a folder already holds some of the files.
    replace: RwSignal<Option<ReplaceQuestion>>,
    replace_no: NodeRef<html::Button>,
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
            layout: RwSignal::new(ONE_FILE.to_string()),
            style: RwSignal::new(QUOTED.to_string()),
            working: RwSignal::new(false),
            replace: RwSignal::new(None),
            replace_no: NodeRef::new(),
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
            self.layout.set(ONE_FILE.to_string());
            self.style.set(QUOTED.to_string());
            self.working.set(false);
            self.replace.set(None);
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
        done_printing();
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
        let style = self.style.get_untracked();
        if self.layout.get_untracked() == BY_BOOK {
            self.export_by_book(translation_code, filter, style);
            return;
        }
        self.close();
        self.busy.update_value(|b| b.exporting = true);
        spawn_local(async move {
            let args = ExportArgs {
                translation_code: &translation_code,
                filter: &filter,
                layout: &style,
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

    /// One file per book: the backend builds the files, asks for a folder
    /// and writes them there, or asks first if some are there already.
    /// The dialog stays open meanwhile.
    fn export_by_book(self, translation_code: String, filter: Filter, style: String) {
        if self.working.get_untracked() {
            return;
        }
        self.working.set(true);
        self.busy.update_value(|b| b.exporting = true);
        spawn_local(async move {
            let args = ExportArgs {
                translation_code: &translation_code,
                filter: &filter,
                layout: &style,
            };
            let result = match ipc::args(&args) {
                Ok(args) => ipc::invoke::<FolderOutcome>("export_notes_by_book", args).await,
                Err(e) => Err(e),
            };
            self.folder_answer(result).await;
        });
    }

    /// Print…: the commentary of the notes the choices match, by the same
    /// filter, from the backend (the file's own words), printed. The
    /// dialog stays open.
    fn print(self) {
        if !self.open.get_untracked()
            || self.working.get_untracked()
            || self.matches.get_untracked().unwrap_or(0) == 0
        {
            return;
        }
        let Some(filter) = self.filter() else {
            return;
        };
        self.working.set(true);
        spawn_local(async move {
            let args = CommentaryArgs { filter: &filter };
            let result = match ipc::args(&args) {
                Ok(args) => ipc::invoke::<Commentary>("get_notes_commentary", args).await,
                Err(e) => Err(e),
            };
            self.working.set(false);
            match result {
                Ok(commentary) => print_commentary(&commentary).await,
                Err(e) => {
                    ipc::log_error("Printing the commentary failed", &e);
                    self.error.set(ipc::error_text(&e));
                }
            }
        });
    }

    /// Replace (`replace`) or Cancel, for the files already in the folder.
    fn answer_replace(self, replace: bool) {
        let Some(question) = self.replace.get_untracked() else {
            return;
        };
        self.replace.set(None);
        self.working.set(true);
        spawn_local(async move {
            let args = ConfirmFolderArgs {
                token: question.token,
                replace,
            };
            let result = match ipc::args(&args) {
                Ok(args) => {
                    ipc::invoke::<FolderOutcome>("confirm_notes_export_by_book", args).await
                }
                Err(e) => Err(e),
            };
            self.folder_answer(result).await;
        });
    }

    /// How the folder export went: written (said in the toast), cancelled
    /// (nothing said), a question (asked in the dialog), or why it failed.
    async fn folder_answer(self, result: Result<FolderOutcome, JsValue>) {
        self.working.set(false);
        match result {
            Ok(FolderOutcome::Exists {
                token,
                folder,
                existing,
            }) => {
                self.replace.set(Some(ReplaceQuestion {
                    token,
                    question: replace_question(&existing, &folder),
                }));
                tick().await;
                if let Some(no) = self.replace_no.get_untracked() {
                    focus(&no);
                }
                return;
            }
            Ok(FolderOutcome::Saved { folder, files, .. }) => {
                self.close();
                bridge::toast(&saved_files(files, &folder), false);
            }
            Ok(FolderOutcome::Cancelled) => self.close(),
            Err(e) => {
                ipc::log_error("Notes export by book failed", &e);
                self.close();
                bridge::toast(&format!("Export failed: {}", ipc::error_text(&e)), true);
            }
        }
        self.busy.update_value(|b| b.exporting = false);
    }

    /// While the dialog is open it has the keyboard, before any other
    /// handler sees a key: Escape closes it (and only it; with the
    /// question about files already in a folder, Escape is its Cancel),
    /// and Tab stays inside. Listens on `window`, capturing, as the page's
    /// modals do.
    fn on_keydown(self, event: KeyboardEvent) {
        if !self.open.get_untracked() {
            return;
        }
        event.stop_propagation();
        match event.key().as_str() {
            "Escape" if self.replace.with_untracked(Option::is_some) => {
                event.prevent_default();
                self.answer_replace(false);
            }
            "Escape" if self.working.get_untracked() => event.prevent_default(),
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
        if let Some(window) = web_sys::window() {
            dom::on(&window, "afterprint", |_| done_printing());
        }
    }
}

/// The faces the printed commentary uses that the page may not have
/// loaded yet: its verse numbers and chapter headings are bold, and a face
/// still loading prints as nothing (`font-display: block`).
const PRINT_FACES: [&str; 2] = ["400 16px Literata", "700 16px Literata"];

/// Draws `commentary` into the page's print-only container, marks the page
/// as printing it, waits for its faces, and asks the webview to print: the
/// system's print dialog (WebKitGTK's GTK dialog on Linux, WebView2's
/// print preview on Windows). Put back once printing is done
/// (`afterprint`), or when the dialog closes, whichever comes first.
async fn print_commentary(commentary: &Commentary) {
    let Some(document) = bridge::document() else {
        return;
    };
    let (Some(container), Some(html), Some(window)) = (
        document.get_element_by_id("print-commentary"),
        document.document_element(),
        web_sys::window(),
    ) else {
        return;
    };
    container.set_inner_html(&commentary_markup(commentary));
    // Each chapter's notes hang from its widest label (`data-hang`, in em),
    // set through the style object: the page's CSP allows no inline style.
    if let Ok(chapters) = container.query_selector_all(".print-notes[data-hang]") {
        for chapter in (0..chapters.length()).filter_map(|i| chapters.item(i)) {
            if let Ok(chapter) = chapter.dyn_into::<HtmlElement>()
                && let Some(hang) = chapter.get_attribute("data-hang")
            {
                let _ = chapter.style().set_property("--hang", &format!("{hang}em"));
            }
        }
    }
    let _ = html.set_attribute(PRINTING, "commentary");
    // Not every engine has `document.fonts` (the frontend tests' hasn't).
    let fonts = js_sys::Reflect::get(&document, &JsValue::from_str("fonts"))
        .ok()
        .and_then(|fonts| fonts.dyn_into::<web_sys::FontFaceSet>().ok());
    if let Some(fonts) = fonts {
        for face in PRINT_FACES {
            // A face that can't load prints in the fallback face instead.
            let _ = wasm_bindgen_futures::JsFuture::from(fonts.load(face)).await;
        }
    }
    if let Err(e) = window.print() {
        ipc::log_error("The print dialog couldn't be opened", &e);
        done_printing();
    }
}

/// The page is no longer printing the commentary: the container empty, and
/// the page prints as it always has.
fn done_printing() {
    let Some(document) = bridge::document() else {
        return;
    };
    if let Some(html) = document.document_element() {
        let _ = html.remove_attribute(PRINTING);
    }
    if let Some(container) = document.get_element_by_id("print-commentary") {
        container.set_inner_html("");
    }
}

fn inputs(list: &NodeList) -> Vec<HtmlInputElement> {
    (0..list.length())
        .filter_map(|i| list.item(i))
        .filter_map(|node| node.dyn_into::<HtmlInputElement>().ok())
        .collect()
}

/// The Export notes dialog, mounted once, hidden until it's asked for.
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
                         Notepad or any text editor. "
                        {move || {
                            if dialog.style.get() == COMMENTARY {
                                view! {
                                    "As a commentary: each note on a line of its own by its \
                                     verse number, under its book and chapter, with no Bible \
                                     text."
                                }
                                    .into_any()
                            } else {
                                view! {
                                    "Each note has its verses quoted in the "
                                    <span id="export-notes-translation">{translation_name}</span>
                                    "."
                                }
                                    .into_any()
                            }
                        }}
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
                    <fieldset class="export-notes-group">
                        <legend class="export-notes-legend">"Layout"</legend>
                        <div class="export-notes-layouts">
                            <label class="export-notes-layout-option">
                                <input
                                    type="radio"
                                    name="export-notes-style"
                                    id="export-notes-quoted"
                                    value=QUOTED
                                    prop:checked=move || dialog.style.get() == QUOTED
                                    on:change=move |_| dialog.style.set(QUOTED.to_string())
                                />
                                <span>"Notes with their verses"</span>
                            </label>
                            <label class="export-notes-layout-option">
                                <input
                                    type="radio"
                                    name="export-notes-style"
                                    id="export-notes-commentary"
                                    value=COMMENTARY
                                    prop:checked=move || dialog.style.get() == COMMENTARY
                                    on:change=move |_| dialog.style.set(COMMENTARY.to_string())
                                />
                                <span>"Commentary (notes by verse number)"</span>
                            </label>
                        </div>
                    </fieldset>
                    <fieldset class="export-notes-group">
                        <legend class="export-notes-legend">"Save as"</legend>
                        <div class="export-notes-layouts">
                            <label class="export-notes-layout-option">
                                <input
                                    type="radio"
                                    name="export-notes-layout"
                                    id="export-notes-one-file"
                                    value=ONE_FILE
                                    prop:checked=move || dialog.layout.get() == ONE_FILE
                                    on:change=move |_| dialog.layout.set(ONE_FILE.to_string())
                                />
                                <span>"One file"</span>
                            </label>
                            <label class="export-notes-layout-option">
                                <input
                                    type="radio"
                                    name="export-notes-layout"
                                    id="export-notes-by-book"
                                    value=BY_BOOK
                                    prop:checked=move || dialog.layout.get() == BY_BOOK
                                    on:change=move |_| dialog.layout.set(BY_BOOK.to_string())
                                />
                                <span>"One file per book (a folder)"</span>
                            </label>
                        </div>
                    </fieldset>
                    <p id="export-notes-error" class="note-error" role="alert">
                        {move || dialog.error.get()}
                    </p>
                    <p id="export-notes-count" class="export-notes-count" aria-live="polite">
                        {move || dialog.count.get()}
                    </p>
                </div>
                <div class="modal-buttons" hidden=move || dialog.replace.with(Option::is_some)>
                    <button
                        type="button"
                        id="export-notes-confirm"
                        class="import-confirm"
                        disabled=move || dialog.matches.get().unwrap_or(0) == 0 || dialog.working.get()
                        on:click=move |_| dialog.export()
                    >
                        "Export…"
                    </button>
                    <button
                        type="button"
                        id="export-notes-print"
                        class="reader-mode-btn"
                        title="Print the commentary of these notes"
                        hidden=move || dialog.style.get() != COMMENTARY
                        disabled=move || dialog.matches.get().unwrap_or(0) == 0 || dialog.working.get()
                        on:click=move |_| dialog.print()
                    >
                        "Print…"
                    </button>
                    <button
                        type="button"
                        id="export-notes-cancel"
                        class="reader-mode-btn"
                        disabled=move || dialog.working.get()
                        on:click=move |_| dialog.close()
                    >
                        "Cancel"
                    </button>
                </div>
                <div
                    id="export-notes-replace"
                    class="modal-buttons export-notes-replace"
                    hidden=move || dialog.replace.with(Option::is_none)
                >
                    <p id="export-notes-replace-question" class="note-discard-question" role="alert">
                        {move || dialog.replace.with(|r| r.as_ref().map(|r| r.question.clone()).unwrap_or_default())}
                    </p>
                    <button
                        type="button"
                        id="export-notes-replace-yes"
                        class="import-confirm"
                        aria-describedby="export-notes-replace-question"
                        on:click=move |_| dialog.answer_replace(true)
                    >
                        "Replace"
                    </button>
                    <button
                        type="button"
                        id="export-notes-replace-no"
                        class="reader-mode-btn"
                        aria-describedby="export-notes-replace-question"
                        node_ref=dialog.replace_no
                        on:click=move |_| dialog.answer_replace(false)
                    >
                        "Cancel"
                    </button>
                </div>
            </div>
        </div>
    }
}
