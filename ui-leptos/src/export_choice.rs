//! ☰ → Export data…: a small chooser of what to export. "Everything
//! (backup)" is the backup Export data… has always saved (`page`'s
//! `export_data`: the same command, file and toasts), and "My notes" opens
//! the Export notes dialog (`export_notes`) with what it has always been
//! given. My notes is greyed out, saying why, while there are no notes, and
//! Everything while an export or import runs: the page's own facts
//! ([`bridge::ExportFacts`]), as it said them when the chooser was asked for
//! and whenever they change ([`bridge::EXPORT_FACTS_EVENT`]). The command
//! palette's Export notes… and Export data… don't come here: they run the
//! two directly, as they always have.
//!
//! A small modal, as the page's others: while open it has the keyboard (a
//! capturing `keydown` on `window`, stopping every other handler), Escape
//! closes it, as does Cancel or a click on the dimmed page, Tab and
//! Shift+Tab stay inside, and ↑ and ↓ move between the two choices. Closing
//! it, or choosing, gives focus back to ☰ (or the page's focus home); My
//! notes' dialog gives it back there too when it closes.

use leptos::html;
use leptos::prelude::*;
use leptos::task::{spawn_local, tick};
use wasm_bindgen::JsCast;
use web_sys::{CustomEvent, Element, HtmlElement, KeyboardEvent, MouseEvent};

use crate::bridge::{self, ExportFacts};
use crate::dom::{self, focus};
use crate::page;

/// The chooser's title.
pub const TITLE: &str = "Export";
/// The backup: what Export data… has always saved.
pub const EVERYTHING: &str = "Everything (backup)";
/// What the backup holds, and what it's for.
pub const EVERYTHING_HINT: &str =
    "Bookmarks, highlights, notes, findings and settings, to restore later";
/// The Export notes dialog.
pub const NOTES: &str = "My notes";
/// What the Export notes dialog does.
pub const NOTES_HINT: &str = "Choose notes and a layout, then save or print";
/// Why My notes can't be chosen: the palette's Export notes… says the same.
pub const NO_NOTES: &str = "There are no notes yet";
/// Why Everything can't be chosen: the palette's Export data… says the same.
pub const BUSY: &str = "Busy with an export or import";

/// What a choice's second line says: its hint, or, while it can't be
/// chosen, why not.
pub fn line(reason: Option<&'static str>, hint: &'static str) -> &'static str {
    reason.unwrap_or(hint)
}

/// Why Everything can't be chosen now, if it can't.
pub fn everything_reason(facts: ExportFacts) -> Option<&'static str> {
    facts.busy.then_some(BUSY)
}

/// Why My notes can't be chosen now, if it can't.
pub fn notes_reason(facts: ExportFacts) -> Option<&'static str> {
    (!facts.notes).then_some(NO_NOTES)
}

/// Where ↓ (`down`) or ↑ moves among `len` choices from the one at `at`
/// (none: focus isn't on one), wrapping round.
pub fn step(at: Option<usize>, len: usize, down: bool) -> Option<usize> {
    if len == 0 {
        return None;
    }
    Some(match (at, down) {
        (None, true) => 0,
        (None, false) => len - 1,
        (Some(i), true) => (i + 1) % len,
        (Some(i), false) => (i + len - 1) % len,
    })
}

/// The chooser's state and the elements it reaches for.
#[derive(Clone, Copy)]
struct Chooser {
    open: RwSignal<bool>,
    facts: RwSignal<ExportFacts>,
    /// Where focus goes back to on closing (☰, opened from there).
    return_to: StoredValue<Option<HtmlElement>, LocalStorage>,
    overlay: NodeRef<html::Div>,
    panel: NodeRef<html::Div>,
}

impl Chooser {
    fn open(self, facts: ExportFacts, return_to: Option<HtmlElement>) {
        if self.open.get_untracked() {
            return;
        }
        bridge::modal_opening(true);
        self.return_to.set_value(return_to);
        self.facts.set(facts);
        self.open.set(true);
        spawn_local(async move {
            tick().await;
            // Closed again before it was on screen: focus stays put.
            if !self.open.get_untracked() {
                return;
            }
            // The first choice that can be chosen, else Cancel.
            if let Some(first) = self.panel.get().and_then(|panel| {
                dom::focusable(&panel, ".export-choice, .reader-mode-btn")
                    .into_iter()
                    .next()
            }) {
                focus(&first);
            }
        });
    }

    /// Closes it; focus goes back to where it came from, or to the page's
    /// focus home if that's gone. What was there to go back to.
    fn close(self) -> Option<HtmlElement> {
        let back = self.return_to.get_value();
        self.return_to.set_value(None);
        if !self.open.get_untracked() {
            return back;
        }
        self.open.set(false);
        match &back {
            Some(back) if dom::usable(back) && !back.has_attribute("disabled") => focus(back),
            _ => bridge::focus_home(),
        }
        back
    }

    /// Everything (backup): the chooser closes, and the backup runs as
    /// Export data… always ran it.
    fn everything(self) {
        if !self.open.get_untracked() || everything_reason(self.facts.get_untracked()).is_some() {
            return;
        }
        self.close();
        page::export_data();
    }

    /// My notes: the chooser closes, and the Export notes dialog opens,
    /// giving focus back where the chooser would have.
    fn notes(self) {
        if !self.open.get_untracked() || notes_reason(self.facts.get_untracked()).is_some() {
            return;
        }
        let back = self.close();
        page::open_export_notes(back);
    }

    /// The choices that can be chosen now, in order.
    fn choices(self) -> Vec<HtmlElement> {
        self.panel
            .get()
            .map(|panel| dom::focusable(&panel, ".export-choice"))
            .unwrap_or_default()
    }

    /// While open it has the keyboard: none of the reader's shortcuts run.
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
            "Tab" => {
                if let Some(panel) = self.panel.get() {
                    let stops = dom::focusable(&panel, "button");
                    dom::trap_tab(&event, &panel, &stops);
                }
            }
            key @ ("ArrowDown" | "ArrowUp") => {
                let on_choice = event
                    .target()
                    .and_then(|t| t.dyn_into::<Element>().ok())
                    .and_then(|t| t.closest(".export-choice").ok().flatten());
                let Some(on_choice) = on_choice else {
                    return;
                };
                event.prevent_default();
                let choices = self.choices();
                let at = choices.iter().position(|c| dom::same(Some(&on_choice), c));
                if let Some(next) =
                    step(at, choices.len(), key == "ArrowDown").and_then(|next| choices.get(next))
                {
                    focus(next);
                }
            }
            _ => {}
        }
    }

    fn listen(self) {
        dom::on_document_event(bridge::EXPORT_CHOICE_EVENT, move |event: CustomEvent| {
            // Handled, whatever happens next: the chooser is here.
            event.prevent_default();
            match bridge::detail::<ExportFacts>(&event) {
                Ok(facts) => self.open(facts, bridge::return_focus(&event)),
                Err(e) => crate::ipc::log_error("The export request couldn't be read", &e),
            }
        });
        dom::on_document_event(bridge::EXPORT_FACTS_EVENT, move |event: CustomEvent| {
            if let Ok(facts) = bridge::detail::<ExportFacts>(&event) {
                self.facts.set(facts);
            }
        });
        dom::on_window_keydown(move |event: KeyboardEvent| self.on_keydown(event));
    }
}

/// One choice: a button with its name and a second line, its hint or why
/// it can't be chosen now.
fn choice_view(
    id: &'static str,
    name: &'static str,
    hint: &'static str,
    reason: Signal<Option<&'static str>>,
    choose: impl Fn() + 'static,
) -> impl IntoView {
    let name_id = format!("{id}-name");
    let line_id = format!("{id}-line");
    let (labelled_by, described_by) = (name_id.clone(), line_id.clone());
    view! {
        <button
            type="button"
            id=id
            class="export-choice"
            aria-labelledby=labelled_by
            aria-describedby=described_by
            disabled=move || reason.get().is_some()
            on:click=move |_| choose()
        >
            <span id=name_id class="export-choice-name">{name}</span>
            <span id=line_id class="export-choice-line">{move || line(reason.get(), hint)}</span>
        </button>
    }
}

/// The export chooser, mounted once, hidden until it's asked for.
#[component]
pub fn ExportChoice() -> impl IntoView {
    let chooser = Chooser {
        open: RwSignal::new(false),
        facts: RwSignal::new(ExportFacts::default()),
        return_to: StoredValue::new_local(None),
        overlay: NodeRef::new(),
        panel: NodeRef::new(),
    };
    chooser.listen();
    let everything = Signal::derive(move || everything_reason(chooser.facts.get()));
    let notes = Signal::derive(move || notes_reason(chooser.facts.get()));

    view! {
        <div
            id="export-choice-overlay"
            class="modal-overlay"
            hidden=move || !chooser.open.get()
            node_ref=chooser.overlay
            on:click=move |ev: MouseEvent| {
                // A click on the dimmed page behind it closes it.
                if let Some(overlay) = chooser.overlay.get()
                    && dom::targets(&ev, &overlay)
                {
                    chooser.close();
                }
            }
        >
            <div
                id="export-choice-panel"
                class="modal-panel export-choice-panel"
                role="dialog"
                aria-modal="true"
                aria-labelledby="export-choice-title"
                node_ref=chooser.panel
            >
                <div class="modal-body">
                    <h2 id="export-choice-title" class="note-title">{TITLE}</h2>
                    <div class="export-choices">
                        {choice_view(
                            "export-choice-everything",
                            EVERYTHING,
                            EVERYTHING_HINT,
                            everything,
                            move || chooser.everything(),
                        )}
                        {choice_view(
                            "export-choice-notes",
                            NOTES,
                            NOTES_HINT,
                            notes,
                            move || chooser.notes(),
                        )}
                    </div>
                </div>
                <div class="modal-buttons export-choice-buttons">
                    <button
                        type="button"
                        id="export-choice-cancel"
                        class="reader-mode-btn"
                        on:click=move |_| {
                            chooser.close();
                        }
                    >
                        "Cancel"
                    </button>
                </div>
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palette::{self, PaletteState};

    fn facts(notes: bool, busy: bool) -> ExportFacts {
        ExportFacts { notes, busy }
    }

    #[test]
    fn each_choice_says_its_hint_or_why_it_cannot_be_chosen() {
        let free = facts(true, false);
        assert_eq!(everything_reason(free), None);
        assert_eq!(notes_reason(free), None);
        assert_eq!(
            line(everything_reason(free), EVERYTHING_HINT),
            EVERYTHING_HINT
        );
        assert_eq!(line(notes_reason(free), NOTES_HINT), NOTES_HINT);
        let stuck = facts(false, true);
        assert_eq!(line(everything_reason(stuck), EVERYTHING_HINT), BUSY);
        assert_eq!(line(notes_reason(stuck), NOTES_HINT), NO_NOTES);
        // Each on its own.
        assert_eq!(everything_reason(facts(false, false)), None);
        assert_eq!(notes_reason(facts(true, true)), None);
    }

    /// The chooser's reasons are the palette's, word for word, for the
    /// same page.
    #[test]
    fn its_reasons_are_the_palettes() {
        let reason = |state: &PaletteState, id: &str| {
            palette::commands(state)
                .into_iter()
                .find(|c| c.id == id)
                .and_then(|c| c.reason)
        };
        for (notes, busy) in [(false, false), (true, false), (false, true), (true, true)] {
            let state = PaletteState {
                notes,
                export_busy: busy,
                ..PaletteState::default()
            };
            let facts = facts(notes, busy);
            assert_eq!(reason(&state, "export-notes"), notes_reason(facts));
            assert_eq!(reason(&state, "export-data"), everything_reason(facts));
        }
    }

    #[test]
    fn the_arrows_move_between_the_choices_and_wrap() {
        assert_eq!(step(None, 2, true), Some(0));
        assert_eq!(step(None, 2, false), Some(1));
        assert_eq!(step(Some(0), 2, true), Some(1));
        assert_eq!(step(Some(1), 2, true), Some(0));
        assert_eq!(step(Some(0), 2, false), Some(1));
        assert_eq!(step(Some(0), 1, true), Some(0));
        assert_eq!(step(None, 0, true), None);
    }

    #[test]
    fn the_words_are_these() {
        assert_eq!(TITLE, "Export");
        assert_eq!(EVERYTHING, "Everything (backup)");
        assert_eq!(
            EVERYTHING_HINT,
            "Bookmarks, highlights, notes, findings and settings, to restore later"
        );
        assert_eq!(NOTES, "My notes");
        assert_eq!(NOTES_HINT, "Choose notes and a layout, then save or print");
    }
}
