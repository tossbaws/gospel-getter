//! Tauri commands, called the way the page's JavaScript calls them: through
//! the global `window.__TAURI__.core.invoke` (`app.withGlobalTauri` is on),
//! with the same command names and camelCase argument objects. Going
//! through the global, not a bundled API, is what lets the frontend test
//! harness answer the calls from the real backend.

use serde::Serialize;
use serde::de::DeserializeOwned;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    /// `window.__TAURI__.core.invoke(cmd, args)`: resolves to the command's
    /// answer, or rejects with the command's error value (a string).
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], js_name = invoke, catch)]
    async fn invoke_js(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;
}

/// Calls `cmd` with `args` ([`JsValue::UNDEFINED`] for a command that takes
/// nothing, as the page passes no second argument) and reads its answer as
/// `T`. The error is the command's own error value, as `invoke` rejected
/// with it; see [`error_text`] for the reader's words.
pub async fn invoke<T: DeserializeOwned>(cmd: &str, args: JsValue) -> Result<T, JsValue> {
    let answer = invoke_js(cmd, args).await?;
    serde_wasm_bindgen::from_value(answer).map_err(|e| JsValue::from(e.to_string()))
}

#[wasm_bindgen]
extern "C" {
    /// The same `invoke`, called at once: its promise.
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], js_name = invoke, catch)]
    fn invoke_promise(cmd: &str, args: JsValue) -> Result<js_sys::Promise, JsValue>;
}

/// As [`invoke`], but the command is sent now, within the caller's event,
/// as the page's own `invoke` calls are, rather than when the future is
/// first awaited.
pub fn invoke_now<T: DeserializeOwned>(
    cmd: &str,
    args: JsValue,
) -> impl Future<Output = Result<T, JsValue>> + 'static {
    let sent = invoke_promise(cmd, args);
    async move {
        let answer = wasm_bindgen_futures::JsFuture::from(sent?).await?;
        serde_wasm_bindgen::from_value(answer).map_err(|e| JsValue::from(e.to_string()))
    }
}

/// Calls `cmd` with `args` and hands back its answer as it came, for an
/// answer part of which goes back to the page unchanged.
pub async fn invoke_value(cmd: &str, args: JsValue) -> Result<JsValue, JsValue> {
    invoke_js(cmd, args).await
}

/// A command's argument object as the page's JavaScript builds it: a plain
/// object, with `null` (not a missing key) where the page passed `null`.
pub fn args(value: &impl Serialize) -> Result<JsValue, JsValue> {
    let serializer = serde_wasm_bindgen::Serializer::new().serialize_missing_as_null(true);
    value
        .serialize(&serializer)
        .map_err(|e| JsValue::from(e.to_string()))
}

/// An error's text for the reader, as the page's `errorText` reads it: its
/// `message`, or what it was thrown as. Never empty (a command can fail
/// with `""`), so a failure always says so and is never taken for success.
pub fn error_text(error: &JsValue) -> String {
    let message = if error.is_object() {
        js_sys::Reflect::get(error, &JsValue::from_str("message"))
            .ok()
            .and_then(|m| m.as_string())
            .filter(|m| !m.is_empty())
    } else {
        None
    };
    let text = message
        .or_else(|| error.as_string())
        .or_else(|| error.as_f64().map(|n| n.to_string()))
        .or_else(|| {
            error
                .dyn_ref::<js_sys::Object>()
                .map(|o| String::from(o.to_string()))
        })
        .unwrap_or_default();
    let text = text.trim();
    if text.is_empty() {
        "Something went wrong.".to_string()
    } else {
        text.to_string()
    }
}

/// `console.error(context, error)`, as the page logs a failed command.
pub fn log_error(context: &str, error: &JsValue) {
    web_sys::console::error_2(&JsValue::from_str(context), error);
}
