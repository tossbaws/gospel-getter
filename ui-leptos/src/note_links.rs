//! The links in a note's text (R17), wherever it's shown (the card's
//! notes, the Library's Notes list, the margin): a reference typed in it
//! ("see Rom 5:8", found by the backend) goes to its passage as Go to
//! does: a step in the history, its verses selected, or its chapter from
//! the top for a chapter alone; a verse this translation doesn't number
//! opens its chapter, saying so (`reader::open`). From the Library, it
//! closes first. A web link opens in the browser through the backend
//! (`open_web_link`, which opens only `http` and `https`, and says why
//! not otherwise); the page never follows one itself, as the webview
//! would leave the app.
//!
//! One capturing `click` listener on the document sees a link's click
//! before anything under it does (a margin note opening the card, a
//! Library row opening its passage, the click dispatcher), and keeps it
//! from them; a middle click (`auxclick`) on one does nothing. A key
//! press on a link arrives as a click too.

use serde::Serialize;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use wasm_bindgen_futures::{JsFuture, spawn_local};
use web_sys::{Element, MouseEvent};

use crate::bridge;
use crate::ipc;
use crate::surfaces::{self, Surface};

/// The links note text draws (`note_markdown`).
const LINKS: &str = "a.note-ref-link, a.note-web-link";

#[derive(Serialize)]
struct WebLinkArgs<'a> {
    url: &'a str,
}

/// Starts listening for clicks on notes' links.
pub fn mount() {
    let Some(document) = bridge::document() else {
        return;
    };
    for name in ["click", "auxclick"] {
        let handler = Closure::<dyn Fn(MouseEvent)>::new(move |event: MouseEvent| {
            on_click(&event, name == "click");
        });
        let _ = document.add_event_listener_with_callback_and_bool(
            name,
            handler.as_ref().unchecked_ref(),
            true,
        );
        handler.forget();
    }
}

fn on_click(event: &MouseEvent, follow: bool) {
    let Some(link) = event
        .target()
        .and_then(|t| t.dyn_into::<Element>().ok())
        .and_then(|t| t.closest(LINKS).ok().flatten())
    else {
        return;
    };
    event.prevent_default();
    event.stop_propagation();
    if !follow || event.button() != 0 {
        return;
    }
    if link.class_list().contains("note-web-link") {
        open_web_link(&link.get_attribute("href").unwrap_or_default());
    } else {
        go_to(&link);
    }
}

fn number(link: &Element, name: &str) -> Option<i64> {
    link.get_attribute(name)?.parse().ok()
}

/// A reference's passage, as Go to opens it.
fn go_to(link: &Element) {
    let (Some(book_id), Some(chapter)) = (number(link, "data-book"), number(link, "data-chapter"))
    else {
        return;
    };
    if surfaces::current() == Some(Surface::Library) {
        surfaces::close(true);
    }
    match (number(link, "data-start"), number(link, "data-end")) {
        (Some(start), Some(end)) => crate::reader::open(
            &bridge::OpenPassage {
                book_id,
                chapter,
                verse_start: Some(start),
                verse_end: Some(end),
            },
            wasm_bindgen::JsValue::NULL,
        ),
        _ => crate::reader::jump(book_id, chapter, true),
    }
}

/// Has the backend open `url` in the browser; a refusal or failure is
/// said in the toast.
fn open_web_link(url: &str) {
    let sent = ipc::args(&WebLinkArgs { url }).and_then(|args| ipc::send("open_web_link", args));
    spawn_local(async move {
        let answer = match sent {
            Ok(promise) => JsFuture::from(promise).await,
            Err(e) => Err(e),
        };
        if let Err(e) = answer {
            ipc::log_error("Failed to open a note's link", &e);
            bridge::toast(&ipc::error_text(&e), true);
        }
    });
}
