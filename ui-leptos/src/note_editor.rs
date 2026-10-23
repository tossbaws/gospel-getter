//! The note editor: write a note on the selected verses, or edit or delete
//! one, with the passage quoted as the reader is reading it. The page opens
//! it (the selection card's + Add note and Edit, N, and the Library's
//! Edit) with a snapshot of the passage and, when adding, the other notes
//! on it; the editor saves and deletes through the backend, then has the
//! page refresh its notes (markers, the card's Notes section, the
//! Library) before closing, so focus can go back to the note's own Edit.
//! The status afterwards, and a deleted note's Undo, are the page's (the
//! selection card's status line); see `bridge`.
//!
//! A modal, as the page's others: while open it has the keyboard (a
//! capturing `keydown` on `window`, stopping every other handler). Escape
//! closes it at once if nothing changed, and otherwise asks first (and,
//! while asking, keeps editing); Ctrl+Enter (Cmd+Enter) saves; Tab stays
//! inside. Closing gives focus back to what opened it, or the note's Edit,
//! or + Add note, or the chapter heading. No focus it moves scrolls the
//! page.
//!
//! This is the page's note panel script (`ui/index.html` at fa5969d,
//! `charCount` to its keydown trap) as a Leptos component: the same ids,
//! classes, ARIA, `hidden` and `disabled`, text and focus order. The panel
//! answers within the click or key that asks, as the page's did: opening
//! and closing, the controls that follow the text as it's typed, and the
//! discard prompt are written straight to their elements; the words it
//! shows are signals.

use leptos::html;
use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::JsValue;
use web_sys::{CustomEvent, HtmlElement, KeyboardEvent, MouseEvent};

use crate::bridge::{self, NoteRequest, OtherNote};
use crate::dom::{self, focus};
use crate::dto::{AddNoteArgs, DeleteNoteArgs, UpdateNoteArgs};
use crate::ipc;
use crate::note_text;

/// The note being written or edited, while the editor is open.
#[derive(Clone)]
struct Session {
    /// Each opening's own number: an answer for an editor since closed and
    /// opened again is not for this one.
    id: u64,
    request: NoteRequest,
    /// The note as the page sent it, handed back for its Undo.
    note: JsValue,
    /// What the editor opened with (the note's text, or nothing).
    initial: String,
    return_to: Option<HtmlElement>,
}

#[derive(Clone, Copy)]
struct Editor {
    session: StoredValue<Option<Session>, LocalStorage>,
    next_id: StoredValue<u64>,
    busy: StoredValue<bool>,
    title: RwSignal<String>,
    passage_html: RwSignal<String>,
    text_shown: RwSignal<String>,
    others: RwSignal<Vec<OtherNote>>,
    error: RwSignal<String>,
    overlay: NodeRef<html::Div>,
    panel: NodeRef<html::Div>,
    body: NodeRef<html::Div>,
    others_box: NodeRef<html::Details>,
    input: NodeRef<html::Textarea>,
    length: NodeRef<html::P>,
    buttons: NodeRef<html::Div>,
    save: NodeRef<html::Button>,
    delete: NodeRef<html::Button>,
    discard: NodeRef<html::Div>,
    keep: NodeRef<html::Button>,
}

impl Editor {
    fn is_open(self) -> bool {
        self.overlay.get().is_some_and(|o| !o.hidden())
    }

    fn text(self) -> String {
        self.input.get().map(|i| i.value()).unwrap_or_default()
    }

    /// Save and Delete, and the length line, for the text as it is now.
    fn update_controls(self) {
        let text = self.text();
        let busy = self.busy.get_value();
        if let Some(save) = self.save.get() {
            save.set_disabled(!note_text::can_save(&text, busy));
        }
        if let Some(delete) = self.delete.get() {
            delete.set_disabled(busy);
        }
        if let Some(length) = self.length.get() {
            length.set_text_content(Some(&note_text::length_line(&text)));
        }
    }

    /// While a save or delete is on its way, through the refresh after it,
    /// the text is read-only (still focusable, and read out), so nothing
    /// typed meanwhile is lost when the editor closes, and Save stays off,
    /// so one save is one note.
    fn set_busy(self, busy: bool) {
        self.busy.set_value(busy);
        if let Some(input) = self.input.get() {
            input.set_read_only(busy);
        }
        self.update_controls();
    }

    /// When adding, the other notes on the passage, read-only.
    fn set_others(self, others: Vec<OtherNote>) {
        let adding = self
            .session
            .with_value(|s| s.as_ref().is_some_and(|s| s.request.mode == "add"));
        let others = if adding { others } else { Vec::new() };
        if let Some(others_box) = self.others_box.get() {
            others_box.set_hidden(others.is_empty());
        }
        self.others.set(others);
    }

    fn show_discard_prompt(self, show: bool) {
        if let Some(buttons) = self.buttons.get() {
            buttons.set_hidden(show);
        }
        if let Some(discard) = self.discard.get() {
            discard.set_hidden(!show);
        }
        if show && let Some(keep) = self.keep.get() {
            focus(&keep);
        }
    }

    fn open(self, request: NoteRequest, note: JsValue, return_to: Option<HtmlElement>) {
        let id = self.next_id.get_value() + 1;
        self.next_id.set_value(id);
        let initial = request
            .note
            .as_ref()
            .map(|n| n.body.clone())
            .unwrap_or_default();
        let editing = request.mode == "edit";
        let code = request.code.to_uppercase();
        self.title.set(request.reference.clone());
        self.passage_html.set(request.passage_html.clone());
        self.text_shown.set(format!("Text shown: {code}"));
        let others = request.others.clone();
        self.session.set_value(Some(Session {
            id,
            request,
            note,
            initial: initial.clone(),
            return_to,
        }));
        if let Some(others_box) = self.others_box.get() {
            others_box.set_open(false);
        }
        self.set_others(others);
        self.error.set(String::new());
        if let Some(input) = self.input.get() {
            input.set_value(&initial);
        }
        if let Some(delete) = self.delete.get() {
            delete.set_hidden(!editing);
        }
        self.show_discard_prompt(false);
        self.set_busy(false);
        if let Some(overlay) = self.overlay.get() {
            overlay.set_hidden(false);
        }
        let body = self.body.get();
        if let Some(body) = &body {
            body.set_scroll_top(0);
        }
        // Focus never scrolls the page behind; in a short window the
        // panel's own body scrolls to show the whole of the note.
        if let Some(input) = self.input.get() {
            focus(&input);
            let end = input.value().encode_utf16().count() as u32;
            let _ = input.set_selection_range(end, end);
            if let Some(body) = &body {
                let below = input.get_bounding_client_rect().bottom()
                    - body.get_bounding_client_rect().bottom();
                if below > 0.0 {
                    body.set_scroll_top(body.scroll_top() + (below + 8.0) as i32);
                }
            }
        }
    }

    fn close(self) {
        if let Some(overlay) = self.overlay.get() {
            overlay.set_hidden(true);
        }
        self.show_discard_prompt(false);
        let session = self.session.get_value();
        self.session.set_value(None);
        let note_id = session
            .as_ref()
            .and_then(|s| s.request.note.as_ref().map(|n| n.id));
        let mut target = session
            .and_then(|s| s.return_to)
            .filter(|el| dom::usable(el));
        if target.is_none()
            && let Some(id) = note_id
        {
            target = dom::query(&format!(".verse-notes .note-edit[data-note-id=\"{id}\"]"))
                .filter(|el| dom::usable(el));
        }
        if target.is_none() {
            target = dom::query(".verse-actions .verse-note-btn").filter(|el| dom::usable(el));
        }
        if target.is_none() {
            target = dom::query(".chapter-main .chapter-heading");
            if let Some(heading) = &target {
                heading.set_tab_index(-1);
            }
        }
        if let Some(target) = target {
            focus(&target);
        }
    }

    /// Cancel, or Escape: closes at once if nothing changed, and otherwise
    /// asks first.
    fn request_close(self) {
        if self.busy.get_value() {
            return;
        }
        let changed = self.session.with_value(|s| {
            s.as_ref()
                .is_some_and(|s| note_text::changed(&self.text(), &s.initial))
        });
        if changed {
            self.show_discard_prompt(true);
        } else {
            self.close();
        }
    }

    fn same_session(self, id: u64) -> bool {
        self.session
            .with_value(|s| s.as_ref().is_some_and(|s| s.id == id))
    }

    /// A note that can't be saved stays in the editor, with why.
    fn save(self) {
        let Some(session) = self.session.get_value() else {
            return;
        };
        if self.busy.get_value() || self.save.get().is_none_or(|b| b.disabled()) {
            return;
        }
        let body = self.text();
        self.error.set(String::new());
        self.set_busy(true);
        spawn_local(async move {
            let result = match (&session.request.note, session.request.mode.as_str()) {
                (Some(note), "edit") => match ipc::args(&UpdateNoteArgs {
                    id: note.id,
                    body: &body,
                }) {
                    Ok(args) => ipc::invoke::<serde::de::IgnoredAny>("update_note", args)
                        .await
                        .map(drop),
                    Err(e) => Err(e),
                },
                _ => {
                    let passage = &session.request.passage;
                    let args = AddNoteArgs {
                        book_id: passage.book_id,
                        chapter: passage.chapter,
                        verse_start: passage.start,
                        verse_end: passage.end,
                        body: &body,
                    };
                    match ipc::args(&args) {
                        Ok(args) => ipc::invoke::<serde::de::IgnoredAny>("add_note", args)
                            .await
                            .map(drop),
                        Err(e) => Err(e),
                    }
                }
            };
            if let Err(e) = result {
                ipc::log_error("Failed to save the note", &e);
                if self.same_session(session.id) {
                    self.set_busy(false);
                    self.error.set(format!(
                        "The note wasn’t saved: {} Your text is still here.",
                        ipc::error_text(&e)
                    ));
                    if let Some(input) = self.input.get() {
                        focus(&input);
                    }
                }
                return;
            }
            bridge::refresh_notes().await;
            if self.same_session(session.id) {
                self.close();
                self.set_busy(false);
            }
            let action = if session.request.mode == "edit" {
                "saved"
            } else {
                "added"
            };
            bridge::note_done(action, &JsValue::NULL);
        });
    }

    fn delete(self) {
        let Some(session) = self.session.get_value() else {
            return;
        };
        let Some(id) = session.request.note.as_ref().map(|n| n.id) else {
            return;
        };
        if session.request.mode != "edit" || self.busy.get_value() {
            return;
        }
        self.error.set(String::new());
        self.set_busy(true);
        spawn_local(async move {
            let result = match ipc::args(&DeleteNoteArgs { id }) {
                Ok(args) => ipc::invoke_value("delete_note", args).await,
                Err(e) => Err(e),
            };
            let deleted = match result {
                Ok(deleted) => deleted,
                Err(e) => {
                    ipc::log_error("Failed to delete the note", &e);
                    if self.same_session(session.id) {
                        self.set_busy(false);
                        self.error
                            .set(format!("The note wasn’t deleted: {}", ipc::error_text(&e)));
                    }
                    return;
                }
            };
            bridge::refresh_notes().await;
            if self.same_session(session.id) {
                self.close();
                self.set_busy(false);
            }
            // The note as it was, for the page's Undo.
            let gone = if deleted.is_null() || deleted.is_undefined() {
                session.note.clone()
            } else {
                deleted
            };
            bridge::note_done("deleted", &gone);
        });
    }

    fn on_keydown(self, event: KeyboardEvent) {
        if !self.is_open() {
            return;
        }
        event.stop_propagation();
        let discarding = self.discard.get().is_some_and(|d| !d.hidden());
        match event.key().as_str() {
            "Escape" => {
                event.prevent_default();
                if discarding {
                    self.show_discard_prompt(false);
                    if let Some(input) = self.input.get() {
                        focus(&input);
                    }
                } else {
                    self.request_close();
                }
            }
            "Enter" if (event.ctrl_key() || event.meta_key()) && !discarding => {
                event.prevent_default();
                self.save();
            }
            "Tab" => self.trap_tab(&event),
            _ => {}
        }
    }

    /// Tab and Shift+Tab wrap among the textarea, the buttons and the
    /// other notes' summary; focus that got out comes back in.
    fn trap_tab(self, event: &KeyboardEvent) {
        let Some(panel) = self.panel.get() else {
            return;
        };
        let stops = dom::focusable(&panel, "textarea, button, summary");
        let (Some(first), Some(last)) = (stops.first(), stops.last()) else {
            return;
        };
        let active = dom::active_element();
        let inside = active
            .as_ref()
            .is_some_and(|active| panel.contains(Some(active)));
        if !inside {
            event.prevent_default();
            let _ = if event.shift_key() { last } else { first }.focus();
        } else if event.shift_key() && dom::same(active.as_ref(), first) {
            event.prevent_default();
            let _ = last.focus();
        } else if !event.shift_key() && dom::same(active.as_ref(), last) {
            event.prevent_default();
            let _ = first.focus();
        }
    }

    fn listen(self) {
        dom::on_document_event(bridge::NOTE_EVENT, move |event: CustomEvent| {
            event.prevent_default();
            match bridge::note_request(&event) {
                Ok(request) => {
                    let note = js_sys::Reflect::get(&event.detail(), &JsValue::from_str("note"))
                        .unwrap_or(JsValue::NULL);
                    self.open(request, note, bridge::return_focus(&event));
                }
                Err(e) => ipc::log_error("The note request couldn't be read", &e),
            }
        });
        dom::on_document_event(bridge::NOTE_OTHERS_EVENT, move |event: CustomEvent| {
            if !self.is_open() {
                return;
            }
            match bridge::note_others(&event) {
                Ok(others) => self.set_others(others),
                Err(e) => ipc::log_error("The other notes couldn't be read", &e),
            }
        });
        dom::on_window_keydown(move |event: KeyboardEvent| self.on_keydown(event));
    }
}

/// The note editor, mounted once, hidden until the page opens it.
#[component]
pub fn NoteEditor() -> impl IntoView {
    let editor = Editor {
        session: StoredValue::new_local(None),
        next_id: StoredValue::new(0),
        busy: StoredValue::new(false),
        title: RwSignal::new(String::new()),
        passage_html: RwSignal::new(String::new()),
        text_shown: RwSignal::new(String::new()),
        others: RwSignal::new(Vec::new()),
        error: RwSignal::new(String::new()),
        overlay: NodeRef::new(),
        panel: NodeRef::new(),
        body: NodeRef::new(),
        others_box: NodeRef::new(),
        input: NodeRef::new(),
        length: NodeRef::new(),
        buttons: NodeRef::new(),
        save: NodeRef::new(),
        delete: NodeRef::new(),
        discard: NodeRef::new(),
        keep: NodeRef::new(),
    };
    editor.listen();

    let other_rows = move || {
        editor
            .others
            .get()
            .into_iter()
            .map(|n| {
                view! {
                    <li>
                        <span class="note-others-ref">{format!("{} · {}", n.reference, n.date)}</span>
                        <span class="note-others-body">{n.body}</span>
                    </li>
                }
            })
            .collect_view()
    };

    view! {
        <div
            id="note-overlay"
            class="modal-overlay"
            hidden
            node_ref=editor.overlay
            on:mousedown=move |ev: MouseEvent| {
                // Clicking the dimmed page behind the panel does nothing,
                // and leaves focus where it was.
                if let Some(overlay) = editor.overlay.get()
                    && dom::targets(&ev, &overlay)
                {
                    ev.prevent_default();
                }
            }
        >
            <div
                id="note-panel"
                class="modal-panel note-panel"
                role="dialog"
                aria-modal="true"
                aria-labelledby="note-title"
                aria-describedby="note-passage"
                node_ref=editor.panel
            >
                <div class="modal-body" node_ref=editor.body>
                    <h2 id="note-title" class="note-title">{move || editor.title.get()}</h2>
                    <blockquote
                        id="note-passage"
                        class="note-passage"
                        inner_html=move || editor.passage_html.get()
                    ></blockquote>
                    <p id="note-text-shown" class="note-text-shown">{move || editor.text_shown.get()}</p>
                    <details
                        id="note-others"
                        class="note-others"
                        hidden
                        node_ref=editor.others_box
                    >
                        <summary id="note-others-summary">
                            {move || format!("Other notes on these verses ({})", editor.others.with(Vec::len))}
                        </summary>
                        <ul id="note-others-list">{other_rows}</ul>
                    </details>
                    <label id="note-label" class="note-label" for="note-input">
                        {move || {
                            let title = editor.title.get();
                            if title.is_empty() { String::new() } else { format!("Your note on {title}") }
                        }}
                    </label>
                    <textarea
                        id="note-input"
                        class="note-input"
                        rows="5"
                        aria-describedby="note-length note-error"
                        spellcheck="true"
                        node_ref=editor.input
                        on:input=move |_| editor.update_controls()
                    ></textarea>
                    <p id="note-length" class="note-length" aria-live="polite" node_ref=editor.length></p>
                    <p id="note-error" class="note-error" role="alert">{move || editor.error.get()}</p>
                </div>
                <div id="note-buttons" class="modal-buttons note-buttons" node_ref=editor.buttons>
                    <button
                        type="button"
                        id="note-save"
                        class="import-confirm note-save"
                        disabled
                        node_ref=editor.save
                        on:click=move |_| editor.save()
                    >
                        "Save"
                    </button>
                    <button
                        type="button"
                        id="note-cancel"
                        class="reader-mode-btn"
                        on:click=move |_| editor.request_close()
                    >
                        "Cancel"
                    </button>
                    <button
                        type="button"
                        id="note-delete"
                        class="reader-mode-btn note-delete"
                        hidden
                        node_ref=editor.delete
                        on:click=move |_| editor.delete()
                    >
                        "Delete"
                    </button>
                </div>
                <div id="note-discard" class="modal-buttons note-buttons" hidden node_ref=editor.discard>
                    <p id="note-discard-question" class="note-discard-question" role="alert">
                        "Discard this note?"
                    </p>
                    <button
                        type="button"
                        id="note-discard-yes"
                        class="import-confirm"
                        on:click=move |_| editor.close()
                    >
                        "Discard"
                    </button>
                    <button
                        type="button"
                        id="note-keep"
                        class="reader-mode-btn"
                        aria-describedby="note-discard-question"
                        node_ref=editor.keep
                        on:click=move |_| {
                            editor.show_discard_prompt(false);
                            if let Some(input) = editor.input.get() {
                                focus(&input);
                            }
                        }
                    >
                        "Keep editing"
                    </button>
                </div>
            </div>
        </div>
    }
}
