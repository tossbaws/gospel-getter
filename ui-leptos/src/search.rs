//! Search and go-to-reference, and the command palette, in one box
//! (`#search-panel`, opened with ⌕, `/` or Ctrl+K). As the reader types,
//! the results come in three groups: "Go to" (the reference typed, as the
//! backend reads it, or a book's first chapter when its name is typed
//! alone), "Commands" (the app's own actions, `palette`) and
//! "Verses" (the search's matches, the words found marked). A leading ">"
//! lists commands only; the empty box suggests a few. Focus stays in the
//! box, a combobox: ↓ and ↑ move through every group's options, Enter runs
//! the one that's active, Escape closes it.
//!
//! The panel is the page's (`#search-panel`, whose `hidden` the page reads);
//! this draws what's inside it and opens and closes it, from its button,
//! from the page's keys (`bridge::SEARCH_EVENT`), and when a surface opens.
//! Opening it closes the surfaces. A result opens as a jump of the reader's
//! (`reader::open`), a command through its own control or the
//! page's code (`palette::Run`). Only the latest search's answer shows.
//!
//! This is the page's search script (`ui/index.html` at 201ab8f,
//! `setSearchOpen` to the panel's keydown) in Rust: the same ids, classes,
//! ARIA, `hidden`, text and focus order. Everything a test or a reader sees
//! at once happens within the key or click, as it did: the list is drawn
//! (as Leptos views, built and put in place there and then) and the active
//! option set before the event is over.

use leptos::html;
use leptos::prelude::*;
use leptos::tachys::view::{Mountable, Render};
use leptos::task::spawn_local;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{Element, HtmlElement, KeyboardEvent};

use crate::aa;
use crate::bridge;
use crate::dom;
use crate::dto::{GoTo, SearchAnswer, SearchArgs, SearchVerse};
use crate::ipc;
use crate::palette::{self, Command, Query, Run};
use crate::search_text::{self as text};
use crate::shortcuts::drawn_arrow;
use crate::surfaces::{self, Surface};

/// How long typing pauses before the words are searched.
const DEBOUNCE_MS: i32 = 150;

/// What the Go to and Verses groups show: the last search's answer (a
/// page of verses keeps the pages before it, for Show more), kept while the
/// next one is on its way.
#[derive(Clone)]
enum Shown {
    Passage {
        book_id: i64,
        chapter: i64,
        verse_start: Option<i64>,
        verse_end: Option<i64>,
        reference: String,
    },
    Text {
        total: i64,
        offset: i64,
        /// How many the last page had.
        page: usize,
        /// How many a page holds.
        page_size: i64,
        verses: Vec<SearchVerse>,
        /// A book typed by name alone: its first chapter, offered first.
        go_to: Option<GoTo>,
    },
}

/// What was searched last, and in which translation.
struct LastSearch {
    query: String,
    translation_code: String,
}

#[derive(Default)]
struct State {
    /// The latest search's number: only its answer shows.
    token: u64,
    /// The search waiting for typing to pause.
    timer: Option<i32>,
    last: Option<LastSearch>,
    shown: Option<Shown>,
    /// The active option's key ("go", "cmd:compare", "verse:43:3:16").
    active: Option<String>,
    /// Whether the reader chose it with the arrows (otherwise it follows
    /// what's typed).
    moved: bool,
    /// Where focus was when the box opened, for a command to give it back
    /// to.
    return_focus: Option<HtmlElement>,
}

#[derive(Clone, Copy)]
struct Search {
    state: StoredValue<State, LocalStorage>,
    /// The debounced search, as a function `setTimeout` can call.
    later: StoredValue<Option<js_sys::Function>, LocalStorage>,
    input: NodeRef<html::Input>,
    status: NodeRef<html::P>,
    results: NodeRef<html::Div>,
    more: NodeRef<html::Button>,
}

/// The page's element `id`.
fn by_id(id: &str) -> Option<HtmlElement> {
    bridge::document()?
        .get_element_by_id(id)?
        .dyn_into::<HtmlElement>()
        .ok()
}

fn panel() -> Option<HtmlElement> {
    by_id("search-panel")
}

fn toggle() -> Option<HtmlElement> {
    by_id("search-toggle")
}

fn target_of(event: &web_sys::Event) -> Option<Element> {
    event.target()?.dyn_into::<Element>().ok()
}

fn closest(el: &Element, selector: &str) -> Option<Element> {
    el.closest(selector).ok().flatten()
}

/// A `data-*` number of `el`.
fn data_number(el: &Element, name: &str) -> Option<i64> {
    el.get_attribute(name)?.parse().ok()
}

/// `el.scrollIntoView({ block: 'nearest' })`, where there is one (jsdom has
/// none).
fn scroll_into_view(el: &Element) {
    let Ok(scroll) = js_sys::Reflect::get(el, &JsValue::from_str("scrollIntoView")) else {
        return;
    };
    let Ok(scroll) = scroll.dyn_into::<js_sys::Function>() else {
        return;
    };
    let options = js_sys::Object::new();
    let _ = js_sys::Reflect::set(
        &options,
        &JsValue::from_str("block"),
        &JsValue::from_str("nearest"),
    );
    let _ = scroll.call1(el, &options);
}

impl Search {
    fn new() -> Self {
        Self {
            state: StoredValue::new_local(State::default()),
            later: StoredValue::new_local(None),
            input: NodeRef::new(),
            status: NodeRef::new(),
            results: NodeRef::new(),
            more: NodeRef::new(),
        }
    }

    fn value(self) -> String {
        self.input
            .get_untracked()
            .map(|input| input.value())
            .unwrap_or_default()
    }

    fn query(self) -> Query {
        Query::parse(&self.value())
    }

    fn is_open() -> bool {
        panel().is_some_and(|panel| !panel.hidden())
    }

    // ---- Opening and closing.

    /// Opens (remembering where focus was, closing the surfaces, drawing
    /// the list and putting focus in the box, its text selected) or closes.
    fn set_open(self, open: bool) {
        let Some(panel) = panel() else {
            return;
        };
        if open && panel.hidden() {
            let body = bridge::document().and_then(|d| d.body());
            let active = dom::active_element()
                .filter(|el| !body.as_ref().is_some_and(|body| dom::same(Some(el), body)))
                .and_then(|el| el.dyn_into::<HtmlElement>().ok());
            self.state.update_value(|s| s.return_focus = active);
        }
        panel.set_hidden(!open);
        if let Some(toggle) = toggle() {
            let _ = toggle.set_attribute("aria-expanded", if open { "true" } else { "false" });
        }
        if open {
            surfaces::close(false);
            self.state.update_value(|s| s.moved = false);
            self.render();
            if let Some(input) = self.input.get_untracked() {
                let _ = input.focus();
                input.select();
            }
        }
    }

    /// Closes, and focus goes to the search button.
    fn close_to_toggle(self) {
        self.set_open(false);
        if let Some(toggle) = toggle() {
            let _ = toggle.focus();
        }
    }

    fn set_status(self, message: &str, is_error: bool) {
        if let Some(status) = self.status.get_untracked() {
            status.set_text_content(Some(message));
            let _ = status.class_list().toggle_with_force("is-error", is_error);
        }
    }

    // ---- Searching: only the latest answer shows.

    /// Searches what the box holds in the translation being read, from
    /// `offset`. The empty box and commands alone (">") search nothing, and
    /// that's done at once (`None`); a search is a future that's done once
    /// its answer is shown, or dropped because a newer search replaced it.
    fn run_search(self, offset: i64) -> Option<impl Future<Output = ()> + 'static> {
        self.cancel_timer();
        let token = self.state.with_value(|s| s.token) + 1;
        self.state.update_value(|s| s.token = token);
        let query = self.value();
        let translation_code = bridge::translation_code();
        let parsed = Query::parse(&query);
        if !parsed.searches() {
            self.state.update_value(|s| {
                s.last = None;
                s.shown = None;
            });
            if !parsed.commands_only {
                self.set_status("", false);
            }
            self.render();
            return None;
        }
        // Sent now, within the key or event, as the page sent it.
        let sent = ipc::args(&SearchArgs {
            query: &query,
            translation_code: &translation_code,
            offset,
        })
        .map(|args| ipc::invoke_now::<SearchAnswer>("search", args));
        Some(async move {
            let answer = match sent {
                Ok(sent) => sent.await,
                Err(e) => Err(e),
            };
            if self.state.with_value(|s| s.token) != token {
                return;
            }
            match answer {
                Ok(answer) => {
                    self.state.update_value(|s| {
                        s.last = Some(LastSearch {
                            query,
                            translation_code,
                        });
                    });
                    self.show_answer(answer, offset > 0);
                }
                Err(e) => {
                    ipc::log_error("Search failed", &e);
                    self.state.update_value(|s| s.shown = None);
                    self.set_status(&text::failed(&ipc::error_text(&e)), true);
                    self.render();
                }
            }
        })
    }

    /// Searches now, without waiting for it.
    fn search_now(self, offset: i64) {
        if let Some(search) = self.run_search(offset) {
            spawn_local(search);
        }
    }

    fn cancel_timer(self) {
        if let Some(handle) = self.state.with_value(|s| s.timer)
            && let Some(window) = web_sys::window()
        {
            window.clear_timeout_with_handle(handle);
        }
        self.state.update_value(|s| s.timer = None);
    }

    /// Searches once typing has paused.
    fn search_soon(self) {
        let (Some(window), Some(later)) = (web_sys::window(), self.later.get_value()) else {
            return;
        };
        let handle = window
            .set_timeout_with_callback_and_timeout_and_arguments_0(&later, DEBOUNCE_MS)
            .ok();
        self.state.update_value(|s| s.timer = handle);
    }

    /// The search's answer (added to what's shown, for Show more), and its
    /// status line.
    fn show_answer(self, answer: SearchAnswer, append: bool) {
        match answer {
            SearchAnswer::Passage {
                book_id,
                chapter,
                verse_start,
                verse_end,
                reference,
            } => {
                self.state.update_value(|s| {
                    s.shown = Some(Shown::Passage {
                        book_id,
                        chapter,
                        verse_start,
                        verse_end,
                        reference,
                    });
                });
                self.set_status(text::GO_THERE, false);
            }
            SearchAnswer::Text {
                terms,
                total,
                offset,
                page_size,
                results,
                go_to,
            } => {
                let page = results.len();
                let code = self.state.with_value(|s| {
                    s.last
                        .as_ref()
                        .map(|l| l.translation_code.clone())
                        .filter(|c| !c.is_empty())
                });
                let code = code.unwrap_or_else(bridge::translation_code);
                // A book's name alone offers its first chapter first.
                let status = text::verses_status(
                    total,
                    offset + page as i64,
                    &code,
                    &terms,
                    go_to.as_ref().map(|g| g.reference.as_str()),
                );
                self.state.update_value(|s| {
                    let mut verses = match (append, s.shown.take()) {
                        (true, Some(Shown::Text { verses, .. })) => verses,
                        _ => Vec::new(),
                    };
                    verses.extend(results);
                    s.shown = Some(Shown::Text {
                        total,
                        offset,
                        page,
                        page_size,
                        verses,
                        go_to,
                    });
                });
                self.set_status(&status, false);
            }
            SearchAnswer::Empty => {
                self.state.update_value(|s| s.shown = None);
                self.set_status("", false);
            }
            SearchAnswer::Invalid { message } | SearchAnswer::Unavailable { message } => {
                self.state.update_value(|s| s.shown = None);
                self.set_status(&message, true);
            }
        }
        self.render();
    }

    // ---- The listbox.

    /// Draws the listbox: each group a `role="group"` named by its heading,
    /// its options in order, then the box's ARIA, Show more and the status
    /// for the empty box and ">", and the active option.
    fn render(self) {
        let Some(results) = self.results.get_untracked() else {
            return;
        };
        let query = self.query();
        let commands = palette::shown(&query, palette::commands(&bridge::palette_state()));
        let shown = if query.searches() {
            self.state.with_value(|s| s.shown.clone())
        } else {
            None
        };

        // Each option's key, in order; an option's id is its place.
        let mut keys = Vec::new();
        let mut groups = Vec::new();

        // The reference typed, or a book's first chapter (its name typed
        // alone, offered before its verses).
        let go_to = match &shown {
            Some(Shown::Passage {
                book_id,
                chapter,
                verse_start,
                verse_end,
                reference,
            }) => Some((*book_id, *chapter, *verse_start, *verse_end, reference)),
            Some(Shown::Text {
                go_to: Some(go_to), ..
            }) => Some((go_to.book_id, go_to.chapter, None, None, &go_to.reference)),
            _ => None,
        };
        if let Some((book_id, chapter, verse_start, verse_end, reference)) = go_to {
            let option = go_to_view(
                keys.len(),
                book_id,
                chapter,
                verse_start,
                verse_end,
                reference.clone(),
            );
            keys.push("go".to_string());
            groups.push(group_view("goto", "Go to", vec![option]));
        }
        if !commands.is_empty() {
            let options = commands
                .iter()
                .map(|c| {
                    let option = command_view(keys.len(), c);
                    keys.push(format!("cmd:{}", c.id));
                    option
                })
                .collect();
            groups.push(group_view("commands", "Commands", options));
        }
        let mut more = None;
        if let Some(Shown::Text {
            total,
            offset,
            page,
            page_size,
            verses,
            ..
        }) = &shown
        {
            if !verses.is_empty() {
                let options = verses
                    .iter()
                    .map(|v| {
                        let option = verse_view(keys.len(), v);
                        keys.push(format!("verse:{}:{}:{}", v.book_id, v.chapter, v.verse));
                        option
                    })
                    .collect();
                groups.push(group_view("verses", "Verses", options));
            }
            more = text::more_from(*offset, *page, *total)
                .map(|next| (next, text::more_label(next, *page_size, *total)));
        }

        while let Some(child) = results.first_child() {
            let _ = results.remove_child(&child);
        }
        // Static views: built and put in place now, and left there (they
        // hold nothing reactive to drop).
        let mut drawn = groups.into_iter().collect_view().build();
        drawn.mount(&results, None);

        if let Some(input) = self.input.get_untracked() {
            let _ = input.set_attribute(
                "aria-expanded",
                if keys.is_empty() { "false" } else { "true" },
            );
        }
        if let Some(button) = self.more.get_untracked() {
            button.set_hidden(more.is_none());
            // Show more says how many the next page holds, of how many.
            if let Some((next, label)) = more {
                let _ = button.set_attribute("data-offset", &next.to_string());
                button.set_text_content(Some(&label));
            }
        }
        if query.empty {
            self.set_status(text::HINT, false);
        } else if query.commands_only {
            self.set_status(
                if commands.is_empty() {
                    text::NO_COMMANDS
                } else {
                    ""
                },
                false,
            );
        }
        let (moved, chosen) = self.state.with_value(|s| (s.moved, s.active.clone()));
        let active = text::active_after_drawing(&keys, moved, chosen.as_deref(), query.empty);
        self.set_active(active);
    }

    /// The options, in order.
    fn options(self) -> Vec<HtmlElement> {
        self.results
            .get_untracked()
            .and_then(|list| list.query_selector_all("[role=\"option\"]").ok())
            .map(|found| dom::elements(&found))
            .unwrap_or_default()
    }

    /// Makes option `index` the active one (none: the box itself).
    fn set_active(self, index: Option<usize>) {
        let options = self.options();
        let option = index.and_then(|i| options.get(i));
        for (i, o) in options.iter().enumerate() {
            let selected = Some(i) == index;
            let _ = o.set_attribute("aria-selected", if selected { "true" } else { "false" });
            let _ = o.class_list().toggle_with_force("is-active", selected);
        }
        let key = option.and_then(|o| o.get_attribute("data-key"));
        self.state.update_value(|s| s.active = key);
        let Some(input) = self.input.get_untracked() else {
            return;
        };
        match option {
            Some(option) => {
                let _ = input.set_attribute("aria-activedescendant", &option.id());
                scroll_into_view(option);
            }
            None => {
                let _ = input.remove_attribute("aria-activedescendant");
            }
        }
    }

    /// The active option's place among the options.
    fn active_index(self, options: &[HtmlElement]) -> Option<usize> {
        let active = self.state.with_value(|s| s.active.clone())?;
        options
            .iter()
            .position(|o| o.get_attribute("data-key").as_deref() == Some(active.as_str()))
    }

    // ---- Running an option.

    /// Goes to the option's passage: the box closes, and the reader jumps.
    fn open_result(self, option: &Element) {
        let (Some(book_id), Some(chapter)) = (
            data_number(option, "data-book"),
            data_number(option, "data-chapter"),
        ) else {
            return;
        };
        let passage = bridge::OpenPassage {
            book_id,
            chapter,
            verse_start: data_number(option, "data-verse-start"),
            verse_end: data_number(option, "data-verse-end"),
        };
        self.set_open(false);
        crate::reader::open(&passage, JsValue::NULL);
    }

    /// Runs an option: goes to its passage, or runs its command once the
    /// box has closed and focus is back where it was. A command that can't
    /// run now says why, and the box stays.
    fn run_option(self, option: &Element) {
        let Some(id) = option.get_attribute("data-command") else {
            self.open_result(option);
            return;
        };
        let command = palette::commands(&bridge::palette_state())
            .into_iter()
            .find(|c| c.id == id);
        let command = match command {
            Some(command) if command.reason.is_none() => command,
            other => {
                let said = other
                    .and_then(|c| c.reason.map(|reason| text::cannot_run(&c.label, reason)))
                    .unwrap_or_default();
                self.set_status(&said, false);
                return;
            }
        };
        self.set_open(false);
        let back = self.state.with_value(|s| s.return_focus.clone());
        self.state.update_value(|s| s.return_focus = None);
        match back.filter(|back| dom::usable(back)) {
            Some(back) => dom::focus(&back),
            None => {
                if let Some(toggle) = toggle() {
                    let _ = toggle.focus();
                }
            }
        }
        run(&command.run);
    }

    // ---- The reader's keys and clicks.

    fn on_input(self) {
        self.cancel_timer();
        self.state.update_value(|s| s.moved = false);
        self.render();
        if self.query().searches() {
            self.search_soon();
        } else {
            self.search_now(0);
        }
    }

    fn on_keydown(self, event: &KeyboardEvent) {
        match event.key().as_str() {
            "Escape" => {
                event.prevent_default();
                self.close_to_toggle();
            }
            key @ ("ArrowDown" | "ArrowUp") => {
                event.prevent_default();
                let options = self.options();
                if options.is_empty() {
                    return;
                }
                let next = text::step(
                    self.active_index(&options),
                    options.len(),
                    key == "ArrowDown",
                );
                self.state.update_value(|s| s.moved = true);
                self.set_active(next);
            }
            "Enter" => {
                event.prevent_default();
                // A search still on its way for what's typed (Enter pressed
                // straight after typing a reference) is waited for, so
                // Enter goes there, as it always has. Otherwise the option
                // runs within the key (copying needs that).
                let value = self.value();
                let code = bridge::translation_code();
                let pending = Query::parse(&value).searches()
                    && !self.state.with_value(|s| {
                        s.last
                            .as_ref()
                            .is_some_and(|l| l.query == value && l.translation_code == code)
                    });
                let search = if pending { self.run_search(0) } else { None };
                match search {
                    Some(search) => spawn_local(async move {
                        search.await;
                        self.run_active();
                    }),
                    None => self.run_active(),
                }
            }
            _ => {}
        }
    }

    fn run_active(self) {
        let options = self.options();
        if let Some(option) = self.active_index(&options).map(|i| options[i].clone()) {
            self.run_option(&option);
        }
    }

    /// A click on an option runs it; on Show more, the next page.
    fn on_panel_click(self, event: &web_sys::Event) {
        let Some(target) = target_of(event) else {
            return;
        };
        if let Some(option) = closest(&target, "#search-results [role=\"option\"]") {
            self.run_option(&option);
            return;
        }
        if closest(&target, "#search-more").is_some() {
            self.state.update_value(|s| s.moved = true);
            let offset = self
                .more
                .get_untracked()
                .and_then(|more| data_number(&more, "data-offset"))
                .unwrap_or(0);
            self.search_now(offset);
        }
    }

    fn listen(self, root: &HtmlElement) {
        if let Some(input) = self.input.get_untracked() {
            dom::on(&input, "input", move |_| self.on_input());
            dom::on(&input, "keydown", move |event| {
                if let Some(event) = event.dyn_ref::<KeyboardEvent>() {
                    self.on_keydown(event);
                }
            });
        }
        // Options are chosen by clicking too; the box keeps focus.
        if let Some(results) = self.results.get_untracked() {
            dom::on(&results, "mousedown", |event| {
                if target_of(&event).is_some_and(|t| closest(&t, "[role=\"option\"]").is_some()) {
                    event.prevent_default();
                }
            });
        }
        dom::on(root, "click", move |event| self.on_panel_click(&event));
        // Escape closes the panel from anywhere in it (Show more).
        dom::on(root, "keydown", move |event| {
            let is_input = self.input.get_untracked().is_some_and(|input| {
                event.target().is_some_and(|t| {
                    let t: &JsValue = t.as_ref();
                    let input: &JsValue = input.as_ref();
                    t == input
                })
            });
            if is_input {
                return;
            }
            if event
                .dyn_ref::<KeyboardEvent>()
                .is_some_and(|e| e.key() == "Escape")
            {
                event.prevent_default();
                self.close_to_toggle();
            }
        });
        if let Some(toggle) = toggle() {
            dom::on(&toggle, "click", move |_| {
                self.set_open(panel().is_some_and(|p| p.hidden()));
            });
        }
        dom::on_document_event(bridge::SEARCH_EVENT, move |event| {
            event.prevent_default();
            match bridge::search_request(&event) {
                Ok(open) => self.set_open(open),
                Err(e) => ipc::log_error("The search request couldn't be read", &e),
            }
        });
        // One popover or panel at a time: opening one closes search.
        dom::on_document_event(bridge::SURFACE_OPEN_EVENT, move |_| {
            if Self::is_open() {
                self.set_open(false);
            }
        });
        // A new translation: what's typed is searched again in it.
        if let Some(select) = by_id("translation-select") {
            dom::on(&select, "change", move |_| {
                if !palette::js_trim(&self.value()).is_empty() {
                    self.search_now(0);
                }
            });
        }
        let later = wasm_bindgen::closure::Closure::<dyn Fn()>::new(move || self.search_now(0));
        self.later.set_value(Some(
            later.as_ref().unchecked_ref::<js_sys::Function>().clone(),
        ));
        later.forget();
    }
}

/// Runs a command's action: its control, clicked as a reader would, or the
/// page's code for it.
fn run(action: &Run) {
    let click = |id: &str| {
        if let Some(control) = by_id(id) {
            control.click();
        }
    };
    match action {
        Run::Click(id) => click(id),
        Run::Choice { group, value } => {
            let choice = bridge::document()
                .and_then(|d| {
                    d.query_selector(&format!("#{group} [data-value=\"{value}\"]"))
                        .ok()
                        .flatten()
                })
                .and_then(|el| el.dyn_into::<HtmlElement>().ok());
            if let Some(choice) = choice {
                choice.click();
            }
        }
        Run::Library(tab) => {
            if surfaces::current() != Some(Surface::Library) {
                click("library-toggle");
            }
            if let Some(tab) = by_id(&format!("library-tab-{tab}")) {
                tab.click();
                let _ = tab.focus();
            }
        }
        // About, then its Check now (both this crate's).
        Run::CheckUpdates => {
            click("about-btn");
            click("update-check-btn");
        }
        Run::TextSize(step) => aa::text_size_shortcut(*step),
        Run::Translation(code) => aa::choose_translation(code),
        Run::CompareWith(code) => aa::compare_with(code),
        Run::Page(command) => bridge::page_command(command),
    }
}

/// One `role="group"`, named by its heading.
fn group_view(id: &str, label: &'static str, options: Vec<AnyView>) -> AnyView {
    let label_id = format!("palette-group-{id}");
    let labelled_by = label_id.clone();
    view! {
        <div class="palette-group" role="group" aria-labelledby=labelled_by>
            <div class="palette-group-label" id=label_id role="presentation">{label}</div>
            {options}
        </div>
    }
    .into_any()
}

/// The reference typed, to go to.
fn go_to_view(
    n: usize,
    book_id: i64,
    chapter: i64,
    verse_start: Option<i64>,
    verse_end: Option<i64>,
    reference: String,
) -> AnyView {
    // A whole chapter has no verses to select.
    let (start, end) = match verse_start {
        Some(start) => (Some(start), verse_end),
        None => (None, None),
    };
    view! {
        <div
            role="option"
            id=format!("palette-option-{n}")
            class="search-result search-go"
            data-key="go"
            aria-selected="false"
            aria-describedby="palette-group-goto"
            data-book=book_id.to_string()
            data-chapter=chapter.to_string()
            data-verse-start=start.map(|v| v.to_string())
            data-verse-end=end.map(|v| v.to_string())
        >
            "Go to "
            <strong>{reference}</strong>
        </div>
    }
    .into_any()
}

/// A command: its label, why it can't run now (part of its name), and its
/// key.
fn command_view(n: usize, command: &Command) -> AnyView {
    let reason = command.reason.map(|reason| {
        view! {
            <span class="visually-hidden">", "</span>
            <span class="palette-reason">{reason}</span>
        }
    });
    view! {
        <div
            role="option"
            id=format!("palette-option-{n}")
            class=if command.reason.is_some() { "search-command is-disabled" } else { "search-command" }
            data-key=format!("cmd:{}", command.id)
            aria-selected="false"
            aria-describedby="palette-group-commands"
            aria-disabled=command.reason.map(|_| "true")
            data-command=command.id.clone()
            aria-keyshortcuts=(!command.keys.is_empty()).then_some(command.keys)
        >
            <span class="palette-label">{command.label.clone()}</span>
            {reason}
            {key_view(&command.key)}
        </div>
    }
    .into_any()
}

/// A command's key hint. The arrows are drawn, as in the sheet (the UI
/// sans's are tiny); the arrow itself stays in the key's text, unseen.
fn key_view(key: &str) -> Option<AnyView> {
    if key.is_empty() {
        return None;
    }
    let mut parts: Vec<AnyView> = Vec::new();
    let mut plain = String::new();
    for c in key.chars() {
        let mut buf = [0; 4];
        match drawn_arrow(c.encode_utf8(&mut buf)) {
            Some((_, path)) => {
                if !plain.is_empty() {
                    parts.push(std::mem::take(&mut plain).into_any());
                }
                parts.push(
                    view! {
                        <span class="visually-hidden">{c.to_string()}</span>
                        <svg viewBox="0 0 16 16" aria-hidden="true" focusable="false">
                            <path d=path></path>
                        </svg>
                    }
                    .into_any(),
                );
            }
            None => plain.push(c),
        }
    }
    if !plain.is_empty() {
        parts.push(plain.into_any());
    }
    Some(
        view! {
            <span class="palette-key">
                <span class="visually-hidden">", key "</span>
                <kbd>{parts}</kbd>
            </span>
        }
        .into_any(),
    )
}

/// A verse that matched, the words found marked (as DOM nodes: the text is
/// never markup).
fn verse_view(n: usize, verse: &SearchVerse) -> AnyView {
    let pieces = text::marked(&verse.text, &verse.marks)
        .into_iter()
        .map(|(piece, marked)| {
            let piece = piece.to_string();
            if marked {
                view! { <mark>{piece}</mark> }.into_any()
            } else {
                piece.into_any()
            }
        })
        .collect_view();
    view! {
        <div
            role="option"
            id=format!("palette-option-{n}")
            class="search-result"
            data-key=format!("verse:{}:{}:{}", verse.book_id, verse.chapter, verse.verse)
            aria-selected="false"
            aria-describedby="palette-group-verses"
            data-book=verse.book_id.to_string()
            data-chapter=verse.chapter.to_string()
            data-verse-start=verse.verse.to_string()
            data-verse-end=verse.verse.to_string()
        >
            <span class="search-ref">{verse.reference.clone()}</span>
            " "
            <span class="search-text">{pieces}</span>
        </div>
    }
    .into_any()
}

/// Draws the box's contents inside the page's `#search-panel` and starts
/// listening.
pub fn mount() {
    let Some(panel) = panel() else {
        return;
    };
    let search = Search::new();
    leptos::mount::mount_to(panel.clone(), move || shell(search)).forget();
    search.listen(&panel);
}

fn shell(search: Search) -> impl IntoView {
    view! {
        <input
            node_ref=search.input
            type="search"
            id="search-input"
            class="search-input"
            role="combobox"
            aria-expanded="false"
            aria-controls="search-results"
            aria-autocomplete="list"
            placeholder="Words, a reference like John 3:16, or > for commands"
            aria-label="Search the text, go to a reference, or run a command"
            aria-describedby="search-status"
            autocomplete="off"
            spellcheck="false"
        />
        <p node_ref=search.status id="search-status" class="search-status" role="status" aria-live="polite"></p>
        <div node_ref=search.results id="search-results" class="search-results" role="listbox" aria-label="Results"></div>
        <button node_ref=search.more type="button" id="search-more" class="search-more" hidden>
            "Show more"
        </button>
    }
}
