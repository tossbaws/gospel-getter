//! Copy as Markdown on a note (R17), in the card's notes and the Library's
//! Notes list: the note's reference as a heading, its verses quoted in the
//! translation being read, as Copy as… quotes them in Markdown, then the
//! note's text as written (`get_note_markdown`, `notes_export::note_markdown`).
//!
//! WebKitGTK only lets the page write to the clipboard during the click or
//! key press that asked, so a note's Markdown is asked for before then:
//! when the card shows the note, and when the pointer or focus comes to its
//! row in the Library. A copy asked for before its answer is in waits for
//! it and tries anyway, as the card's own Copy does, and says so if that
//! fails. Each answer is kept for its note and translation until the
//! note's text changes, for the [`KEPT`] notes asked for most recently
//! (AUD-08): one that's gone is asked for again when it's next shown or
//! pointed at, before its Copy as Markdown can be pressed.

use std::cell::RefCell;
use std::rc::Rc;

use serde::Serialize;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::{JsFuture, spawn_local};

use crate::bridge;
use crate::ipc;

/// A note's Markdown in one translation: asked for, and in once answered.
struct Entry {
    /// The note's text when it was asked for: a different text asks again.
    body: String,
    answer: Rc<RefCell<Option<Result<String, JsValue>>>>,
    done: js_sys::Promise,
}

/// How many notes' Markdown is kept at once, by note and translation: more
/// than the card's notes on one passage and the Library rows pointed at on
/// the way to a Copy as Markdown, and few enough that a long session's
/// notes, deleted or not, aren't all kept.
const KEPT: usize = 32;

/// Values by key, the most recently used last, never more than `cap` of
/// them: the least recently used goes first.
struct Recent<K, V> {
    entries: Vec<(K, V)>,
    cap: usize,
}

impl<K: PartialEq, V> Recent<K, V> {
    const fn new(cap: usize) -> Self {
        Self {
            entries: Vec::new(),
            cap,
        }
    }

    /// The value of `key`, now the most recently used.
    fn get(&mut self, key: &K) -> Option<&V> {
        let at = self.entries.iter().position(|(k, _)| k == key)?;
        let entry = self.entries.remove(at);
        self.entries.push(entry);
        self.entries.last().map(|(_, v)| v)
    }

    /// Keeps `value` as `key`'s, the most recently used, in place of any
    /// before; the least recently used go until there are `cap`.
    fn insert(&mut self, key: K, value: V) {
        self.entries.retain(|(k, _)| *k != key);
        self.entries.push((key, value));
        let over = self.entries.len().saturating_sub(self.cap);
        self.entries.drain(..over);
    }
}

thread_local! {
    /// By note and translation.
    static ASKED: RefCell<Recent<(i64, String), Entry>> = const { RefCell::new(Recent::new(KEPT)) };
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NoteMarkdownArgs<'a> {
    id: i64,
    translation_code: &'a str,
}

/// Asks for note `id`'s Markdown in the translation being read, unless
/// it's been asked for with this `body` (on its way, or in) and didn't
/// fail.
pub fn prefetch(id: i64, body: &str) {
    let code = bridge::translation_code();
    let asked = ASKED.with_borrow_mut(|asked| {
        asked
            .get(&(id, code.clone()))
            .is_some_and(|e| e.body == body && !matches!(*e.answer.borrow(), Some(Err(_))))
    });
    if asked {
        return;
    }
    let sent = ipc::args(&NoteMarkdownArgs {
        id,
        translation_code: &code,
    })
    .and_then(|args| ipc::send("get_note_markdown", args));
    let answer = Rc::new(RefCell::new(None));
    let store = answer.clone();
    let done = wasm_bindgen_futures::future_to_promise(async move {
        let got = match sent {
            Ok(promise) => JsFuture::from(promise).await.and_then(|value| {
                value
                    .as_string()
                    .ok_or_else(|| JsValue::from_str("the note's Markdown wasn't text"))
            }),
            Err(e) => Err(e),
        };
        if let Err(e) = &got {
            ipc::log_error("Failed to read a note as Markdown", e);
        }
        *store.borrow_mut() = Some(got);
        Ok(JsValue::UNDEFINED)
    });
    ASKED.with_borrow_mut(|asked| {
        asked.insert(
            (id, code),
            Entry {
                body: body.to_string(),
                answer,
                done,
            },
        );
    });
}

/// `navigator.clipboard.writeText(text)`, now.
fn write_text(text: &str) -> Result<js_sys::Promise, JsValue> {
    let unavailable = || JsValue::from(js_sys::Error::new("clipboard access is not available"));
    let window = web_sys::window().ok_or_else(unavailable)?;
    let navigator = js_sys::Reflect::get(&window, &JsValue::from_str("navigator"))?;
    let clipboard = js_sys::Reflect::get(&navigator, &JsValue::from_str("clipboard"))?;
    if clipboard.is_null() || clipboard.is_undefined() {
        return Err(unavailable());
    }
    let write = js_sys::Reflect::get(&clipboard, &JsValue::from_str("writeText"))?
        .dyn_into::<js_sys::Function>()
        .map_err(|_| unavailable())?;
    let written = write.call1(&clipboard, &JsValue::from_str(text))?;
    Ok(js_sys::Promise::resolve(&written))
}

/// What's said once a note on `reference` is copied.
pub fn copied(reference: &str) -> String {
    format!("Copied the note on {reference} as Markdown.")
}

/// What's said when it couldn't be.
pub fn copy_failed(error: &str) -> String {
    format!("Couldn\u{2019}t copy the note: {error}")
}

/// Copies note `id` (on `reference`, saying `body`) as Markdown, and has
/// `report` say how it went. Must be called within the click or key press
/// that asked for it.
pub fn copy(id: i64, reference: String, body: &str, report: impl Fn(String, bool) + 'static) {
    prefetch(id, body);
    let code = bridge::translation_code();
    let Some((answer, done)) = ASKED.with_borrow_mut(|asked| {
        asked
            .get(&(id, code))
            .map(|e| (e.answer.clone(), e.done.clone()))
    }) else {
        return;
    };
    // In: written now, within the gesture. Not yet: once it is.
    let ready = answer.borrow().clone();
    let written: Result<js_sys::Promise, JsValue> = match ready {
        Some(Ok(markdown)) => write_text(&markdown),
        Some(Err(e)) => Err(e),
        None => Ok(js_sys::Promise::resolve(&JsValue::UNDEFINED)),
    };
    let waiting = answer.borrow().is_none();
    spawn_local(async move {
        let result = async {
            let mut promise = written?;
            if waiting {
                JsFuture::from(done).await?;
                let markdown = answer
                    .borrow()
                    .clone()
                    .unwrap_or_else(|| Err(JsValue::from_str("no answer")))?;
                promise = write_text(&markdown)?;
            }
            JsFuture::from(promise).await
        }
        .await;
        match result {
            Ok(_) => report(copied(&reference), false),
            Err(e) => {
                ipc::log_error("Failed to copy a note", &e);
                report(copy_failed(&ipc::error_text(&e)), true);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(recent: &Recent<u32, &str>) -> Vec<u32> {
        recent.entries.iter().map(|(k, _)| *k).collect()
    }

    /// AUD-08: what's kept stays bounded, the least recently used going
    /// first; one used again is kept the longest.
    #[test]
    fn at_most_cap_are_kept_the_least_recently_used_going_first() {
        let mut recent = Recent::new(3);
        for key in 1..=5 {
            recent.insert(key, "markdown");
        }
        assert_eq!(keys(&recent), [3, 4, 5]);
        assert!(recent.get(&1).is_none(), "the oldest went");
        assert_eq!(recent.get(&3), Some(&"markdown"));
        recent.insert(6, "markdown");
        assert_eq!(keys(&recent), [5, 3, 6], "3 was used again: 4 went");
        // A key asked again replaces its value, not adding one.
        recent.insert(5, "changed");
        assert_eq!(keys(&recent), [3, 6, 5]);
        assert_eq!(recent.get(&5), Some(&"changed"));
        assert_eq!(recent.entries.len(), 3);
    }
}
