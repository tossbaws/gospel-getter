//! Import data…: choose a file (the backend's native open dialog), see
//! what importing it would do, and confirm or cancel; or, for a file that
//! can't be imported, why, with each problem found in it. The backend
//! validates the file, holds it under a token until it's confirmed or
//! cancelled, and imports all or nothing; this only shows it and asks.
//!
//! A modal, as the page's others: while open it has the keyboard (a
//! capturing `keydown` on `window`, stopping every other handler), Escape
//! cancels, Tab stays inside, and closing gives focus to the page's focus
//! home (☰, where Import data… was chosen). After an import the page reloads
//! what it shows of the reader's data (see `bridge`).
//!
//! This is the page's import script (`ui/index.html` at fa5969d,
//! `describePreferences` to its keydown handler) as a Leptos component:
//! the same ids, classes, ARIA, `hidden` and `disabled`, text and focus
//! order. The radio buttons and checkboxes are the reader's (and the
//! tests'), so their state is read from them when it matters; the warning
//! and the confirm button's label follow the add-or-replace choice within
//! the change that made it.

use leptos::html;
use leptos::prelude::*;
use leptos::task::{spawn_local, tick};
use wasm_bindgen::JsValue;
use web_sys::{CustomEvent, KeyboardEvent, MouseEvent};

use crate::bridge;
use crate::dom;
use crate::dto::{ApplyImportArgs, CancelImportArgs, ImportChoice, ImportPreview, ImportResult};
use crate::import_text;
use crate::ipc;

/// What the dialog shows: a preview to confirm, or why a file can't be
/// imported.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Shown {
    Preview,
    Invalid {
        message: String,
        problems: Vec<String>,
    },
}

#[derive(Clone, Copy)]
struct Import {
    open: RwSignal<bool>,
    shown: RwSignal<Shown>,
    file_line: RwSignal<String>,
    summary: RwSignal<Vec<String>>,
    choice: RwSignal<import_text::Choice>,
    position_label: RwSignal<String>,
    prefs_label: RwSignal<String>,
    error: RwSignal<String>,
    confirming: RwSignal<bool>,
    /// The preview being shown, for the view.
    shown_preview: RwSignal<Option<ImportPreview>>,
    /// The preview being shown, and its display settings as the backend
    /// sent them (handed to the page's preferences unchanged).
    preview: StoredValue<Option<(ImportPreview, JsValue)>, LocalStorage>,
    /// Choosing a file is under way.
    choosing: StoredValue<bool>,
    panel: NodeRef<html::Div>,
    merge: NodeRef<html::Input>,
    replace: NodeRef<html::Input>,
    position: NodeRef<html::Input>,
    prefs: NodeRef<html::Input>,
    warning: NodeRef<html::P>,
    confirm: NodeRef<html::Button>,
    cancel: NodeRef<html::Button>,
}

/// An Aa `<select>` option's text: `label(select, value)`.
fn option_text(select: &str, value: &str) -> String {
    use wasm_bindgen::JsCast;
    let options = bridge::document()
        .and_then(|d| d.get_element_by_id(select))
        .and_then(|el| el.dyn_into::<web_sys::HtmlSelectElement>().ok())
        .map(|select| select.options());
    let Some(options) = options else {
        return value.to_string();
    };
    (0..options.length())
        .filter_map(|i| options.item(i))
        .filter_map(|el| el.dyn_into::<web_sys::HtmlOptionElement>().ok())
        .find(|option| option.value() == value)
        .map(|option| option.text_content().unwrap_or_default())
        .unwrap_or_else(|| value.to_string())
}

impl Import {
    fn preferences_described(preview: &ImportPreview) -> Vec<String> {
        import_text::describe_preferences(&preview.preferences, option_text)
    }

    /// Import data…: the native open dialog, then the preview, or why not.
    fn choose(self) {
        if self.choosing.get_value() || self.open.get_untracked() {
            return;
        }
        self.choosing.set_value(true);
        bridge::data_busy(true);
        bridge::toast("Choose a file to import…", false);
        spawn_local(async move {
            let answer = ipc::invoke_value("choose_import_file", JsValue::UNDEFINED).await;
            let choice = answer.and_then(|answer| {
                let raw_prefs = js_sys::Reflect::get(&answer, &JsValue::from_str("preview"))
                    .and_then(|preview| {
                        js_sys::Reflect::get(&preview, &JsValue::from_str("preferences"))
                    })
                    .unwrap_or(JsValue::UNDEFINED);
                serde_wasm_bindgen::from_value::<ImportChoice>(answer)
                    .map(|choice| (choice, raw_prefs))
                    .map_err(|e| JsValue::from(e.to_string()))
            });
            match choice {
                Ok((ImportChoice::Cancelled, _)) => {
                    bridge::toast("Import cancelled. Nothing was changed.", false);
                }
                Ok((
                    ImportChoice::Invalid {
                        file_name,
                        message,
                        problems,
                    },
                    _,
                )) => {
                    bridge::toast("", false);
                    self.show_problems(file_name, message, problems);
                }
                Ok((ImportChoice::Ready { preview }, raw_prefs)) => {
                    bridge::toast("", false);
                    self.show_preview(*preview, raw_prefs);
                }
                Err(e) => {
                    ipc::log_error("Import failed", &e);
                    bridge::toast(
                        &format!(
                            "Import failed: {} Nothing was changed.",
                            ipc::error_text(&e)
                        ),
                        true,
                    );
                }
            }
            self.choosing.set_value(false);
            bridge::data_busy(false);
        });
    }

    fn show_problems(self, file_name: String, message: String, problems: Vec<String>) {
        self.preview.set_value(None);
        self.shown_preview.set(None);
        self.file_line.set(file_name);
        self.error.set(String::new());
        self.shown.set(Shown::Invalid {
            message: format!("{message} Nothing was changed."),
            problems,
        });
        self.set_warning(None);
        self.reveal(self.cancel);
    }

    fn show_preview(self, preview: ImportPreview, raw_prefs: JsValue) {
        let prefs = Self::preferences_described(&preview);
        self.file_line.set(import_text::file_line(&preview));
        self.summary
            .set(import_text::summary(&preview, !prefs.is_empty()));
        self.choice.set(import_text::choice(&preview));
        self.position_label.set(
            preview
                .position
                .as_ref()
                .map(|p| format!("Go to the file’s reading position: {}", p.reference))
                .unwrap_or_default(),
        );
        self.prefs_label.set(if prefs.is_empty() {
            String::new()
        } else {
            format!("Use the file’s display settings: {}", prefs.join(", "))
        });
        self.error.set(String::new());
        self.confirming.set(false);
        if let Some(merge) = self.merge.get() {
            merge.set_checked(true);
        }
        if let Some(position) = self.position.get() {
            position.set_checked(preview.position.is_some());
        }
        if let Some(box_) = self.prefs.get() {
            box_.set_checked(!prefs.is_empty());
        }
        self.shown_preview.set(Some(preview.clone()));
        self.preview.set_value(Some((preview, raw_prefs)));
        self.shown.set(Shown::Preview);
        self.update_mode();
        self.reveal(self.confirm);
    }

    /// Shows the dialog, focus on `button` once it's there.
    fn reveal(self, button: NodeRef<html::Button>) {
        bridge::modal_opening(false);
        self.open.set(true);
        spawn_local(async move {
            tick().await;
            if let Some(button) = button.get() {
                let _ = button.focus();
            }
        });
    }

    /// The warning and the confirm button's label, for the choice as the
    /// radio buttons have it now.
    fn update_mode(self) {
        let replace = self.replace.get().is_some_and(|r| r.checked());
        let preview = self.preview.get_value();
        let Some((preview, _)) = preview else {
            return;
        };
        let (warning, label) = import_text::mode(&preview, replace);
        self.set_warning(replace.then_some(warning));
        if let Some(confirm) = self.confirm.get() {
            confirm.set_text_content(Some(label));
        }
    }

    fn set_warning(self, text: Option<String>) {
        if let Some(warning) = self.warning.get() {
            warning.set_hidden(text.is_none());
            if let Some(text) = text {
                warning.set_text_content(Some(&text));
            }
        }
    }

    fn close(self) {
        self.open.set(false);
        self.preview.set_value(None);
    }

    /// Cancel (or Close, for a file that can't be imported). Focus goes
    /// back to ☰, where Import data… was chosen.
    fn cancel(self) {
        if !self.open.get_untracked() {
            return;
        }
        if let Some((preview, _)) = self.preview.get_value() {
            let token = preview.token;
            spawn_local(async move {
                let result = match ipc::args(&CancelImportArgs { token }) {
                    Ok(args) => ipc::invoke::<()>("cancel_import", args).await,
                    Err(e) => Err(e),
                };
                if let Err(e) = result {
                    ipc::log_error("Failed to discard import", &e);
                }
            });
            bridge::toast("Import cancelled. Nothing was changed.", false);
        }
        self.close();
        bridge::focus_home();
    }

    /// Import: all or nothing in the backend; then the file's display
    /// settings, if chosen; then the page reloads what it shows.
    fn confirm(self) {
        let Some((preview, raw_prefs)) = self.preview.get_value() else {
            return;
        };
        if self.confirming.get_untracked() {
            return;
        }
        let choosing_shown = preview.bookmarks_in_file > 0
            || preview.highlights_in_file > 0
            || preview.notes_in_file > 0;
        let replace = choosing_shown && self.replace.get().is_some_and(|r| r.checked());
        let restore_position =
            preview.position.is_some() && self.position.get().is_some_and(|p| p.checked());
        let has_prefs = !Self::preferences_described(&preview).is_empty();
        let use_prefs = has_prefs && self.prefs.get().is_some_and(|p| p.checked());
        self.confirming.set(true);
        self.error.set(String::new());
        spawn_local(async move {
            let args = ApplyImportArgs {
                token: preview.token,
                replace,
                restore_position,
            };
            let result = match ipc::args(&args) {
                Ok(args) => ipc::invoke::<ImportResult>("apply_import", args).await,
                Err(e) => Err(e),
            };
            let result = match result {
                Ok(result) => result,
                Err(e) => {
                    // All or nothing: nothing changed; the preview stays
                    // open to try again or cancel.
                    ipc::log_error("Import failed", &e);
                    self.error.set(format!(
                        "{} You can try again, or cancel.",
                        ipc::error_text(&e)
                    ));
                    self.confirming.set(false);
                    return;
                }
            };
            self.close();
            bridge::focus_home();
            let mut parts = import_text::result_parts(&preview, replace, &result);
            // Display settings live in the webview's own storage, not the
            // database, so they're applied after the database part has
            // succeeded; a failure here is reported, not hidden.
            if use_prefs {
                let applied = bridge::apply_preferences(&raw_prefs).map_err(|e| {
                    ipc::log_error("Failed to apply display settings", &e);
                    ipc::error_text(&e)
                });
                parts.push(import_text::preferences_part(applied));
            }
            bridge::imported(result.position.as_ref(), &import_text::complete(&parts));
        });
    }

    fn on_keydown(self, event: KeyboardEvent) {
        if !self.open.get_untracked() {
            return;
        }
        event.stop_propagation();
        match event.key().as_str() {
            "Escape" => {
                event.prevent_default();
                self.cancel();
            }
            "Tab" => {
                if let Some(panel) = self.panel.get() {
                    let stops = dom::focusable(&panel, "input, button");
                    dom::trap_tab(&event, &panel, &stops);
                }
            }
            _ => {}
        }
    }

    fn listen(self) {
        dom::on_document_event(bridge::IMPORT_EVENT, move |event: CustomEvent| {
            event.prevent_default();
            self.choose();
        });
        dom::on_window_keydown(move |event: KeyboardEvent| self.on_keydown(event));
    }
}

/// The import dialog, mounted once, hidden until a file is chosen.
#[component]
pub fn ImportDialog() -> impl IntoView {
    let import = Import {
        open: RwSignal::new(false),
        shown: RwSignal::new(Shown::Preview),
        file_line: RwSignal::new(String::new()),
        summary: RwSignal::new(Vec::new()),
        choice: RwSignal::new(import_text::Choice {
            legend: "Bookmarks".to_string(),
            merge: "Add them to my bookmarks".to_string(),
            replace: "Replace all my bookmarks with these".to_string(),
        }),
        position_label: RwSignal::new(String::new()),
        prefs_label: RwSignal::new(String::new()),
        error: RwSignal::new(String::new()),
        confirming: RwSignal::new(false),
        shown_preview: RwSignal::new(None),
        preview: StoredValue::new_local(None),
        choosing: StoredValue::new(false),
        panel: NodeRef::new(),
        merge: NodeRef::new(),
        replace: NodeRef::new(),
        position: NodeRef::new(),
        prefs: NodeRef::new(),
        warning: NodeRef::new(),
        confirm: NodeRef::new(),
        cancel: NodeRef::new(),
    };
    import.listen();

    let invalid = move || matches!(import.shown.get(), Shown::Invalid { .. });
    let preview_flag = move |f: fn(&ImportPreview) -> bool| {
        move || !invalid() && import.shown_preview.with(|p| p.as_ref().is_some_and(f))
    };
    let choice_shown = preview_flag(|p| {
        p.bookmarks_in_file > 0 || p.highlights_in_file > 0 || p.notes_in_file > 0
    });
    let position_shown = preview_flag(|p| p.position.is_some());
    let prefs_shown = move || !invalid() && !import.prefs_label.get().is_empty();
    let problems = move || match import.shown.get() {
        Shown::Invalid { problems, .. } => problems
            .into_iter()
            .map(|p| view! { <li>{p}</li> })
            .collect_view()
            .into_any(),
        Shown::Preview => ().into_any(),
    };
    let invalid_text = move || match import.shown.get() {
        Shown::Invalid { message, .. } => message,
        Shown::Preview => String::new(),
    };

    view! {
        <div id="import-overlay" class="import-overlay" hidden=move || !import.open.get()>
            <div
                id="import-panel"
                class="import-panel"
                role="dialog"
                aria-modal="true"
                aria-labelledby="import-title"
                aria-describedby=move || {
                    if invalid() { "import-invalid data-problems" } else { "import-summary" }
                }
                node_ref=import.panel
                on:change=move |ev: web_sys::Event| {
                    let mode = ev
                        .target()
                        .and_then(|t| wasm_bindgen::JsCast::dyn_into::<web_sys::HtmlInputElement>(t).ok())
                        .is_some_and(|input| input.name() == "import-mode");
                    if mode {
                        import.update_mode();
                    }
                }
            >
                <h2 id="import-title" class="import-title">
                    {move || if invalid() { import_text::INVALID_TITLE } else { import_text::TITLE }}
                </h2>
                <p id="import-file" class="import-file">{move || import.file_line.get()}</p>
                <p id="import-invalid" class="import-invalid" hidden=move || !invalid()>
                    {invalid_text}
                </p>
                <ul id="data-problems" class="data-problems">{problems}</ul>
                <ul id="import-summary" class="import-summary" hidden=invalid>
                    {move || {
                        import
                            .summary
                            .get()
                            .into_iter()
                            .map(|line| view! { <li>{line}</li> })
                            .collect_view()
                    }}
                </ul>
                <fieldset
                    id="import-bookmark-choice"
                    class="import-choices"
                    hidden=move || !choice_shown()
                >
                    <legend id="import-choice-legend">{move || import.choice.get().legend}</legend>
                    <label>
                        <input type="radio" name="import-mode" value="merge" checked node_ref=import.merge />
                        " "
                        <span id="import-merge-label">{move || import.choice.get().merge}</span>
                    </label>
                    <label>
                        <input type="radio" name="import-mode" value="replace" node_ref=import.replace />
                        " "
                        <span id="import-replace-label">{move || import.choice.get().replace}</span>
                    </label>
                </fieldset>
                <p id="import-replace-warning" class="import-warning" hidden node_ref=import.warning></p>
                <label
                    id="import-position-option"
                    class="import-option"
                    hidden=move || !position_shown()
                >
                    <input type="checkbox" id="import-position" checked node_ref=import.position />
                    " "
                    <span id="import-position-label">{move || import.position_label.get()}</span>
                </label>
                <label id="import-prefs-option" class="import-option" hidden=move || !prefs_shown()>
                    <input type="checkbox" id="import-prefs" checked node_ref=import.prefs />
                    " "
                    <span id="import-prefs-label">{move || import.prefs_label.get()}</span>
                </label>
                <p id="import-error" class="import-error" role="alert">{move || import.error.get()}</p>
                <div class="import-buttons">
                    <button
                        type="button"
                        id="import-confirm"
                        class="import-confirm"
                        hidden=invalid
                        disabled=move || import.confirming.get()
                        node_ref=import.confirm
                        on:click=move |_: MouseEvent| import.confirm()
                    >
                        "Import"
                    </button>
                    <button
                        type="button"
                        id="import-cancel"
                        class="reader-mode-btn"
                        node_ref=import.cancel
                        on:click=move |_: MouseEvent| import.cancel()
                    >
                        {move || if invalid() { "Close" } else { "Cancel" }}
                    </button>
                </div>
            </div>
        </div>
    }
}
