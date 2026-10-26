//! Boot: the translations, the chapter the reader was on, the history from
//! there, then the Library's lists, the card's highlights, the welcome's
//! startup question and the launch check for updates. One code path for
//! putting a chapter on screen, at startup or afterward: the reader's.
//!
//! Boot's first two commands don't wait for this module: the page's
//! prefetch script, in `<head>`, sends `get_home` as the page loads and,
//! with its answer, the chapter's `get_reading` (unless comparing, when the
//! reader asks for it beside the comparison), as the main script's boot did
//! from the end of the page. Boot takes them over here ([`start`],
//! `bridge::boot_prefetch`): an answer already in is drawn at once, within
//! the module's start; one on its way is adopted, never asked again. Without
//! the prefetch (it isn't there), boot asks `get_home` itself.
//!
//! This is the main script's boot (`ui/index.html` at 2a9d85c: `boot`,
//! `startReader` and the launch check's `askUpdateCheck` after it) in Rust:
//! the same commands, in the same order, with the same arguments.

use wasm_bindgen::JsValue;
use wasm_bindgen_futures::{JsFuture, spawn_local};

use crate::bridge::{self, Prefetch};
use crate::card;
use crate::dto::Home;
use crate::ipc;
use crate::page;
use crate::reader::{self, Load, Sent};

/// What boot has to go on: `get_home`'s answer, and the chapter's
/// `get_reading` if it was sent.
struct Answered {
    home: JsValue,
    reading: Option<Sent>,
}

/// `get_home`'s answer as the prefetch has it, or its failure, or `None`
/// if it hasn't answered yet.
fn answered(prefetch: &Prefetch) -> Option<Result<Answered, JsValue>> {
    if let Some(failure) = prefetch.failure() {
        return Some(Err(failure));
    }
    let home = prefetch.home()?;
    Some(Ok(Answered {
        home,
        reading: prefetch.reading(),
    }))
}

/// Boot's books for the picker, its places to start for the welcome, then
/// the reader's start: the translations and the chapter (drawn now, within
/// this call, if its answer is in).
fn start_reader(answered: &Answered, reading: Option<Sent>) -> Result<Load, JsValue> {
    let home = &answered.home;
    let get = |key: &str| js_sys::Reflect::get(home, &JsValue::from_str(key));
    let books = js_sys::Object::new();
    for key in ["otBooks", "ntBooks"] {
        let _ = js_sys::Reflect::set(&books, &JsValue::from_str(key), &get(key)?);
    }
    page::set_picker_books(books.into());
    page::set_start_here(get("startHere")?);
    let parsed: Home = serde_wasm_bindgen::from_value(home.clone())
        .map_err(|e| JsValue::from(js_sys::Error::new(&e.to_string())))?;
    Ok(reader::start(parsed, reading))
}

/// The rest of boot, after the reader's start: the Library's bookmarks, the
/// card's highlights then the Library's, the Library's notes, any lists
/// asked again once the Library mounted, then the welcome's question.
async fn after_reader(home: JsValue, load: Load) -> Result<(), JsValue> {
    load.await;
    JsFuture::from(page::refresh_library(&["bookmarks"])).await?;
    card::refresh_highlights().await;
    JsFuture::from(page::refresh_library(&["highlights"])).await?;
    JsFuture::from(page::refresh_library(&["notes"])).await?;
    // Lists asked for before the Library mounted, asked again.
    JsFuture::from(page::library_resent()).await?;
    page::ask_welcome_at_startup(&home);
    Ok(())
}

/// Boot from `get_home`'s answer (or failure): the reader, the rest, then
/// the launch check, whatever happened.
fn boot_from(answered: Result<Answered, JsValue>) {
    let started = answered.and_then(|mut answered| {
        let reading = answered.reading.take();
        let load = start_reader(&answered, reading)?;
        Ok((answered.home, load))
    });
    spawn_local(async move {
        let booted = match started {
            Ok((home, load)) => after_reader(home, load).await,
            Err(e) => Err(e),
        };
        if let Err(e) = booted {
            ipc::log_error("Failed to load Gospel Getter", &e);
        }
        page::ask_update_check();
    });
}

/// Starts boot. Called once the reader has mounted, before the rest of the
/// pieces (`lib::start`): if `get_home` has answered, the reader starts
/// within this call, as it did when it said it was ready.
pub fn start() {
    let Some(prefetch) = bridge::boot_prefetch() else {
        // No prefetch: boot asks itself, and the reader asks for the chapter.
        let sent = ipc::send("get_home", JsValue::UNDEFINED);
        spawn_local(async move {
            let home = match sent {
                Ok(promise) => JsFuture::from(promise).await,
                Err(e) => Err(e),
            };
            boot_from(home.map(|home| Answered {
                home,
                reading: None,
            }));
        });
        return;
    };
    if let Some(answered) = answered(&prefetch) {
        boot_from(answered);
        return;
    }
    let asked = prefetch.asked();
    spawn_local(async move {
        if let Some(asked) = asked {
            let _ = JsFuture::from(asked).await;
        }
        boot_from(answered(&prefetch).unwrap_or_else(|| {
            Err(JsValue::from(js_sys::Error::new(
                "boot's prefetch settled without get_home's answer",
            )))
        }));
    });
}
