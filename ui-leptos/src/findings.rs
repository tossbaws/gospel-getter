//! AI-flagged findings on the page (see the governing doc's C1-C10): the
//! chapter's, read from the backend each time a chapter is shown (and after
//! any change to the notes, as "Keep as mine" is), so what an agent saved
//! with `gospel-getter-cli` while the app was open shows on the next chapter
//! shown. Nothing pushes them meanwhile.
//!
//! - Each flagged verse gets a dotted violet underline (C8), `.has-finding`
//!   on it, with "(AI finding)" for a screen reader, alongside the reader's
//!   own highlight and note marks.
//! - The margin (`margin`) shows each verse's findings, collapsed as the
//!   violet badge and its verse (C7); its toggle (a click, Enter or Space)
//!   opens it to the findings, each with "Keep as mine" and "Remove", and
//!   folds it again. Nothing remembers which were open.
//! - "Keep as mine" opens the note editor on the verse, the finding's words
//!   in it, all of them selected (C10, the card's `keep_finding`); saving a
//!   note there makes it the reader's and removes the finding.
//! - "Remove" removes that one finding, with Undo in the toast.
//!
//! And the reader's switch in Aa, "Let AI agents save findings" (C6), read
//! as Aa opens and saved as it changes. It shows only the newest confirmed
//! state (GG-07): every read and every change claims the switch's
//! generation (`intent`), and an answer shows only while its claim holds.
//! Changes are saved one at a time, in order: a change made while a save
//! is on its way waits for it (the newest waiting replaces any before it),
//! so the backend sees them in the reader's order and the switch stays
//! focusable throughout. A save that fails is undone, and said, only if
//! nothing newer has happened since; a read asked for while a save is on
//! its way (Aa opened again, an import) is made once the saves are done.

use std::cell::RefCell;

use serde::Serialize;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;
use web_sys::{Element, HtmlElement, HtmlInputElement, KeyboardEvent};

use crate::bridge;
use crate::card;
use crate::dom;
use crate::dto::Finding;
use crate::findings_text as text;
use crate::intent::{Claim, Generation};
use crate::ipc;
use crate::margin;
use crate::surfaces;

/// How long a removal's Undo is offered, in milliseconds: as a note's.
const UNDO_MS: i32 = crate::card_text::UNDO_MS;

/// The switch in Aa.
const SWITCH_ID: &str = "ai-findings-toggle";

#[derive(Default)]
struct State {
    /// The chapter on screen (book, chapter).
    chapter: Option<(i64, i64)>,
    /// Its findings, once read, and the chapter they were read for.
    findings: Vec<Finding>,
    of: Option<(i64, i64)>,
    /// Bumped by each read: only the latest is shown.
    token: u64,
    /// The switch's reads and changes: only the newest shows.
    switch: Generation,
    /// What the backend last said the switch is (read or saved).
    switch_stored: Option<bool>,
    /// A change of the switch is being saved; the newest change made
    /// meanwhile waits, with its claim.
    switch_saving: bool,
    switch_waiting: Option<(bool, Claim)>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with_borrow_mut(f)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ChapterArgs {
    book_id: i64,
    chapter: i64,
}

#[derive(Serialize)]
struct IdArgs {
    id: i64,
}

#[derive(Serialize)]
struct RequestArgs<'a> {
    request: &'a str,
}

#[derive(Serialize)]
struct RestoreArgs<'a> {
    findings: &'a [Finding],
}

#[derive(Serialize)]
struct SwitchArgs {
    enabled: bool,
}

/// The reader has a chapter on screen (`book_id` `chapter`, its verses new):
/// its findings are marked at once if they're known, and read again.
pub fn chapter_shown(book_id: i64, chapter: i64) {
    let known = with(|s| {
        s.chapter = Some((book_id, chapter));
        s.of == Some((book_id, chapter))
    });
    if known {
        show();
    } else {
        with(|s| s.findings.clear());
        margin::findings(book_id, chapter, Vec::new());
    }
    spawn_local(refresh());
}

/// Reads the chapter's findings again and shows them; only the latest read
/// is shown.
pub async fn refresh() {
    let Some((book_id, chapter)) = with(|s| s.chapter) else {
        return;
    };
    let token = with(|s| {
        s.token += 1;
        s.token
    });
    let answer = match ipc::args(&ChapterArgs { book_id, chapter }) {
        Ok(args) => ipc::invoke::<Vec<Finding>>("list_findings", args).await,
        Err(e) => Err(e),
    };
    if with(|s| s.token) != token {
        return;
    }
    match answer {
        Ok(findings) => {
            with(|s| {
                s.findings = findings;
                s.of = Some((book_id, chapter));
            });
            show();
        }
        Err(e) => ipc::log_error("Failed to load the chapter\u{2019}s AI findings", &e),
    }
}

/// The chapter's findings, again, and the Library's: after any change to
/// them.
pub async fn refresh_everywhere() {
    let library = bridge::refresh_library(&["findings"]);
    refresh().await;
    if let Some(library) = library {
        let _ = wasm_bindgen_futures::JsFuture::from(library).await;
    }
}

/// Marks the flagged verses and hands the findings to the margin.
fn show() {
    let Some((book_id, chapter, findings)) = with(|s| {
        let (book_id, chapter) = s.chapter?;
        (s.of == Some((book_id, chapter))).then(|| (book_id, chapter, s.findings.clone()))
    }) else {
        return;
    };
    mark(&findings);
    margin::findings(book_id, chapter, text::groups(findings));
}

/// Gives every verse of the chapter on screen a finding covers, in every
/// column, the dotted underline (C8) and, for a screen reader, one hidden
/// "(AI finding)".
fn mark(findings: &[Finding]) {
    let Some(document) = bridge::document() else {
        return;
    };
    let verses = document
        .query_selector_all(card::SELECTABLE)
        .map(|found| dom::elements(&found))
        .unwrap_or_default();
    for verse in verses {
        let number = verse
            .get_attribute("data-verse")
            .and_then(|n| n.parse::<i64>().ok());
        let flagged = number.is_some_and(|n| {
            findings
                .iter()
                .any(|f| n >= f.verse_start && n <= f.verse_end)
        });
        let _ = verse.class_list().toggle_with_force("has-finding", flagged);
        let cue = verse.query_selector(":scope > .finding-cue").ok().flatten();
        match cue {
            None if flagged => {
                if let (Ok(added), Some(number)) = (
                    document.create_element("span"),
                    verse.query_selector(".verse-num").ok().flatten(),
                ) {
                    added.set_class_name("finding-cue visually-hidden");
                    added.set_text_content(Some("(AI finding) "));
                    let _ = number.after_with_node_1(&added);
                }
            }
            Some(cue) if !flagged => cue.remove(),
            _ => {}
        }
    }
}

/// The finding with this id, of the chapter on screen.
fn finding(id: i64) -> Option<Finding> {
    with(|s| s.findings.iter().find(|f| f.id == id).cloned())
}

fn target(event: &web_sys::Event) -> Option<Element> {
    event.target()?.dyn_into::<Element>().ok()
}

fn closest(el: &Element, selector: &str) -> Option<Element> {
    el.closest(selector).ok().flatten()
}

fn id_of(el: &Element) -> Option<i64> {
    el.get_attribute("data-finding-id")?.parse().ok()
}

/// A click in a verse's findings (in the margin, the notes column, under a
/// verse or a marker's): its toggle opens or folds them, "Keep as mine" and
/// "Remove" act; any click there goes no further, so it never selects a
/// verse. Whether the click was one of them.
pub fn on_click(event: &web_sys::Event) -> bool {
    let Some(el) = target(event) else {
        return false;
    };
    if closest(&el, ".margin-finding, .finding-marker, .finding-inline").is_none() {
        return false;
    }
    event.stop_propagation();
    event.prevent_default();
    if let Some(toggle) = closest(&el, ".margin-finding-head, .finding-marker") {
        toggle_open(&toggle);
    } else if let Some(keep) = closest(&el, ".finding-keep") {
        if let Some(found) = id_of(&keep).and_then(finding) {
            card::keep_finding(&found, keep.dyn_into::<HtmlElement>().ok());
        }
    } else if let Some(remove) = closest(&el, ".finding-remove")
        && let Some(found) = id_of(&remove).and_then(finding)
    {
        spawn_local(remove_one(found));
    }
    true
}

/// Enter or Space on a verse's findings' toggle opens or folds them, as a
/// click does, and goes no further.
pub fn on_key(event: &KeyboardEvent) {
    let key = event.key();
    if key != "Enter" && key != " " {
        return;
    }
    let Some(toggle) =
        target(event).and_then(|el| closest(&el, ".margin-finding-head, .finding-marker"))
    else {
        return;
    };
    event.prevent_default();
    event.stop_propagation();
    toggle_open(&toggle);
}

/// Opens or folds the findings `toggle` controls. A marker's are in the
/// text, so the reading place is kept.
fn toggle_open(toggle: &Element) {
    let open = toggle.get_attribute("aria-expanded").as_deref() != Some("true");
    let body = toggle
        .get_attribute("aria-controls")
        .and_then(|id| bridge::document()?.get_element_by_id(&id))
        .and_then(|el| el.dyn_into::<HtmlElement>().ok());
    let change = || {
        let _ = toggle.set_attribute("aria-expanded", if open { "true" } else { "false" });
        if let Some(body) = &body {
            body.set_hidden(!open);
        }
        if let Some(entry) = closest(toggle, ".margin-finding") {
            let _ = entry.class_list().toggle_with_force("is-open", open);
        }
    };
    if toggle.class_list().contains("finding-marker") {
        surfaces::keep_reading_place(change, None);
    } else {
        change();
    }
    margin::relayout();
}

/// "Remove" on one finding: it's removed, and the toast offers Undo.
async fn remove_one(found: Finding) {
    let removed = match ipc::args(&IdArgs { id: found.id }) {
        Ok(args) => ipc::invoke::<Option<Finding>>("remove_finding", args).await,
        Err(e) => Err(e),
    };
    match removed {
        Ok(gone) => {
            refresh_everywhere().await;
            if let Some(gone) = gone {
                offer_undo(&text::removed(&gone), vec![gone]);
            }
        }
        Err(e) => {
            ipc::log_error("Failed to remove the finding", &e);
            bridge::toast(
                &format!("The finding wasn\u{2019}t removed: {}", ipc::error_text(&e)),
                true,
            );
        }
    }
}

/// The Library's "Remove all" on a request's findings (C4): they're
/// removed together, and the toast offers Undo. What happened, for the
/// Library to say if it failed.
pub async fn remove_request(request: String) -> Result<(), String> {
    let removed = match ipc::args(&RequestArgs { request: &request }) {
        Ok(args) => ipc::invoke::<Vec<Finding>>("remove_finding_request", args).await,
        Err(e) => Err(e),
    };
    match removed {
        Ok(gone) => {
            refresh_everywhere().await;
            if !gone.is_empty() {
                offer_undo(&text::removed_all(gone.len(), &request), gone);
            }
            Ok(())
        }
        Err(e) => {
            ipc::log_error("Failed to remove a request\u{2019}s findings", &e);
            Err(ipc::error_text(&e))
        }
    }
}

/// Says `message` in the toast, with an Undo that brings `gone` back as
/// they were.
fn offer_undo(message: &str, gone: Vec<Finding>) {
    let Some(undo) = bridge::toast_with_action(message, "Undo", UNDO_MS) else {
        return;
    };
    dom::on_click(&undo, move |_| {
        bridge::toast("", false);
        let gone = gone.clone();
        spawn_local(async move {
            let restored = match ipc::args(&RestoreArgs { findings: &gone }) {
                Ok(args) => ipc::invoke::<usize>("restore_findings", args).await,
                Err(e) => Err(e),
            };
            match restored {
                Ok(count) => {
                    refresh_everywhere().await;
                    bridge::toast(&text::restored(count), false);
                }
                Err(e) => {
                    ipc::log_error("Failed to restore findings", &e);
                    bridge::toast(&ipc::error_text(&e), true);
                }
            }
        });
    });
}

// ---- The switch, in Aa.

fn switch() -> Option<HtmlInputElement> {
    bridge::document()?
        .get_element_by_id(SWITCH_ID)?
        .dyn_into::<HtmlInputElement>()
        .ok()
}

/// Aa is opening (or an import changed it): the switch shows what's saved,
/// unless the reader changes it before the answer is in. While a change is
/// being saved, the read is made once the saves are done.
pub fn draw_switch() {
    let (claim, saving) = with(|s| (s.switch.claim(), s.switch_saving));
    if saving {
        return;
    }
    spawn_local(async move {
        let read =
            ipc::invoke::<bool>("get_ai_findings_enabled", wasm_bindgen::JsValue::UNDEFINED).await;
        if !with(|s| s.switch.holds(claim)) {
            return;
        }
        match read {
            Ok(on) => {
                with(|s| s.switch_stored = Some(on));
                if let Some(switch) = switch() {
                    switch.set_checked(on);
                }
            }
            Err(e) => ipc::log_error("Failed to read the AI agents setting", &e),
        }
    });
}

/// The switch changed: saved, after any save already on its way.
fn on_switch() {
    let Some(input) = switch() else {
        return;
    };
    let on = input.checked();
    let first = with(|s| {
        let claim = s.switch.claim();
        s.switch_waiting = Some((on, claim));
        if s.switch_saving {
            None
        } else {
            s.switch_saving = true;
            s.switch_waiting.take()
        }
    });
    if let Some(first) = first {
        spawn_local(save_switch(first));
    }
}

/// Saves `first` (a change of the switch, with its claim), then each change
/// that waited meanwhile, the newest only, one at a time. A save that fails
/// is put back to what the backend last said only while its claim holds
/// (nothing newer since), and is said unless a newer change is on its way
/// to replace it. Once none waits, a read asked for meanwhile is made.
async fn save_switch(first: (bool, Claim)) {
    let mut next = Some(first);
    let mut last = first.1;
    while let Some((on, claim)) = next {
        last = claim;
        let saved = match ipc::args(&SwitchArgs { enabled: on }) {
            Ok(args) => ipc::invoke::<bool>("set_ai_findings_enabled", args).await,
            Err(e) => Err(e),
        };
        match saved {
            Ok(_) => with(|s| s.switch_stored = Some(on)),
            Err(e) => {
                ipc::log_error("Failed to save the AI agents setting", &e);
                let (current, stored, newer) = with(|s| {
                    (
                        s.switch.holds(claim),
                        s.switch_stored,
                        s.switch_waiting.is_some(),
                    )
                });
                if current && let Some(switch) = switch() {
                    switch.set_checked(stored.unwrap_or(!on));
                }
                if !newer {
                    bridge::toast(text::SWITCH_FAILED, true);
                }
            }
        }
        next = with(|s| s.switch_waiting.take());
    }
    let reread = with(|s| {
        s.switch_saving = false;
        !s.switch.holds(last)
    });
    if reread {
        draw_switch();
    }
}

/// Listens to the switch, and reads it once.
pub fn mount() {
    if let Some(input) = switch() {
        dom::on(&input, "change", |_| on_switch());
        draw_switch();
    }
}
