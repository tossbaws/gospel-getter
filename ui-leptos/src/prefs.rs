//! The saved settings' store: each setting is saved in local storage and
//! shown as a data attribute on `<html>`, which the CSS keys off (no
//! attribute for its default, so with nothing saved the page looks as it
//! did before the settings existed). What the settings are, their keys and
//! values, is the one table in `preferences` (shared with the backend).
//!
//! This is the head script's preferences store (`ui/index.html` at
//! 4b2d15e: `store`, `load`, `isValue` and `apply`) in Rust. The page's
//! pre-paint script applies what's saved before the first frame; from then
//! on the settings are kept here. Local storage can throw in the webview,
//! so every use is guarded, as the page's were.

use wasm_bindgen::JsCast;
use web_sys::HtmlElement;

use crate::preferences::{self, Preference};

fn storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok()?
}

fn root() -> Option<HtmlElement> {
    web_sys::window()?
        .document()?
        .document_element()?
        .dyn_into::<HtmlElement>()
        .ok()
}

/// Saves `value`; quietly not, if local storage can't be written.
pub fn store(pref: &Preference, value: &str) {
    if let Some(storage) = storage() {
        let _ = storage.set_item(pref.key, value);
    }
}

/// What's saved, as it was read back: `None` if nothing is, or it can't
/// be read.
pub fn stored(pref: &Preference) -> Option<String> {
    storage()?.get_item(pref.key).ok()?
}

/// What's saved for `pref`, or its default if nothing is, or what is
/// isn't a value it may have (so a stale or hand-edited value can't leave
/// the page in a state no control can show). A renamed value is saved
/// again under its new name.
pub fn load(pref: &Preference) -> String {
    let mut value = stored(pref);
    if let Some(new) = value.as_deref().and_then(|v| pref.renamed_to(v)) {
        store(pref, new);
        value = Some(new.to_string());
    }
    value
        .filter(|v| pref.allows(v))
        .unwrap_or_else(|| pref.fallback.to_string())
}

/// Shows `value` as `pref`'s data attribute on `<html>`: none for the
/// default.
pub fn apply(pref: &Preference, value: &str) {
    let Some(data) = root().map(|html| html.dataset()) else {
        return;
    };
    if value == pref.fallback {
        data.delete(pref.name);
    } else {
        let _ = data.set(pref.name, value);
    }
}

/// Applies and saves `value`, the way every control changes a setting.
pub fn set(pref: &Preference, value: &str) {
    apply(pref, value);
    store(pref, value);
}

/// `pref`'s data attribute as it is, if it has one (an empty one counts as
/// none).
pub fn shown(pref: &Preference) -> Option<String> {
    root()?.dataset().get(pref.name).filter(|v| !v.is_empty())
}

/// `pref`'s value now: its data attribute, or its default.
pub fn value(pref: &Preference) -> String {
    shown(pref).unwrap_or_else(|| pref.fallback.to_string())
}

/// Whether an on/off setting is on now.
pub fn on(pref: &Preference) -> bool {
    value(pref) == "on"
}

/// A value from an export file, as the page received it: `true` or
/// `false`, text, or anything else (shown as JavaScript's `String` shows
/// it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Raw {
    Bool(bool),
    Text(String),
    Other(String),
}

impl Raw {
    /// As JavaScript's `String(raw)`.
    fn shown(&self) -> &str {
        match self {
            Self::Bool(true) => "true",
            Self::Bool(false) => "false",
            Self::Text(text) | Self::Other(text) => text,
        }
    }
}

/// An export file's display setting `file_name`, with the value `raw`, as
/// the setting and the value to apply and save, or why not: an unknown
/// setting, or a value it may not have. `true` and `false` are "on" and
/// "off" for an on/off setting.
pub fn checked(file_name: &str, raw: &Raw) -> Result<(&'static Preference, String), String> {
    let pref = preferences::by_file_name(file_name)
        .ok_or_else(|| format!("Unknown display setting “{file_name}”."))?;
    let value = match raw {
        Raw::Bool(on) if pref.on_off() => Some(if *on { "on" } else { "off" }.to_string()),
        Raw::Text(text) => Some(text.clone()),
        _ => None,
    };
    match value {
        Some(value) if pref.allows(&value) => Ok((pref, value)),
        _ => Err(format!(
            "“{}” isn’t a valid {}.",
            raw.shown(),
            file_name.replacen('_', " ", 1)
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preferences::{PREFERENCES, Values};

    fn text(s: &str) -> Raw {
        Raw::Text(s.to_string())
    }

    #[test]
    fn a_file_s_settings_are_checked_against_the_one_table() {
        let (pref, value) = checked("layout", &text("flowing")).unwrap();
        assert_eq!(
            (pref.key, value.as_str()),
            ("gospel-getter-layout", "flowing")
        );
        let (pref, value) = checked("reading_mode", &Raw::Bool(true)).unwrap();
        assert_eq!((pref.name, value.as_str()), ("readerMode", "on"));
        let (_, value) = checked("compare", &Raw::Bool(false)).unwrap();
        assert_eq!(value, "off");
        // "on" and "off" as text are fine too, as they were.
        assert_eq!(checked("compare", &text("on")).unwrap().1, "on");
        let (pref, value) = checked("compare_with", &text("bsb")).unwrap();
        assert_eq!((pref.name, value.as_str()), ("compareWith", "bsb"));
    }

    #[test]
    fn a_bad_setting_is_refused_in_the_page_s_words() {
        assert_eq!(
            checked("layout", &text("columns")).unwrap_err(),
            "“columns” isn’t a valid layout."
        );
        assert_eq!(
            checked("text_size", &text("huge")).unwrap_err(),
            "“huge” isn’t a valid text size."
        );
        assert_eq!(
            checked("theme", &Raw::Bool(true)).unwrap_err(),
            "“true” isn’t a valid theme."
        );
        assert_eq!(
            checked("compare_with", &text("KJV!")).unwrap_err(),
            "“KJV!” isn’t a valid compare with."
        );
        assert_eq!(
            checked("reading_mode", &Raw::Other("1".into())).unwrap_err(),
            "“1” isn’t a valid reading mode."
        );
        // The sidebar and the launch check are this computer's, not a
        // file's.
        assert_eq!(
            checked("sidebar", &text("open")).unwrap_err(),
            "Unknown display setting “sidebar”."
        );
        assert_eq!(
            checked("font", &text("serif")).unwrap_err(),
            "Unknown display setting “font”."
        );
    }

    /// The table as the page's script had it: ten settings, their keys,
    /// defaults and values, and the renamed theme.
    #[test]
    fn the_table_is_the_page_s() {
        let rows: Vec<(&str, &str, &str, Vec<&str>)> = PREFERENCES
            .iter()
            .map(|p| (p.name, p.key, p.fallback, p.values.list()))
            .collect();
        assert_eq!(
            rows,
            [
                (
                    "theme",
                    "gospel-getter-theme",
                    "chi-rho",
                    vec![
                        "chi-rho",
                        "vellum",
                        "system",
                        "vaporwave",
                        "classic-dark",
                        "classic-light",
                        "matrix",
                        "beast-slayer",
                        "hot-pink"
                    ]
                ),
                (
                    "textSize",
                    "gospel-getter-text-size",
                    "medium",
                    vec!["small", "medium", "large", "x-large"]
                ),
                (
                    "lineSpacing",
                    "gospel-getter-line-spacing",
                    "normal",
                    vec!["compact", "normal", "relaxed"]
                ),
                (
                    "width",
                    "gospel-getter-width",
                    "normal",
                    vec!["narrow", "normal", "wide"]
                ),
                (
                    "layout",
                    "gospel-getter-layout",
                    "verses",
                    vec!["verses", "flowing"]
                ),
                (
                    "readerMode",
                    "gospel-getter-reader-mode",
                    "off",
                    vec!["off", "on"]
                ),
                ("compare", "gospel-getter-compare", "off", vec!["off", "on"]),
                ("compareWith", "gospel-getter-compare-with", "", vec![]),
                (
                    "sidebar",
                    "gospel-getter-sidebar",
                    "open",
                    vec!["open", "closed"]
                ),
                (
                    "updateCheck",
                    "gospel-getter-update-check",
                    "on",
                    vec!["on", "off"]
                ),
            ]
        );
        assert_eq!(
            preferences::THEME.renamed_to("monster-hunter"),
            Some("beast-slayer")
        );
        assert!(!preferences::THEME.allows("monster-hunter"));
        assert_eq!(preferences::COMPARE_WITH.values, Values::Code);
        for code in ["kjv", "bsb", "web", "a", "abcdefghijklmnop", "x1"] {
            assert!(preferences::COMPARE_WITH.allows(code), "{code}");
        }
        for code in ["", "KJV", "k-j", "abcdefghijklmnopq", "é"] {
            assert!(!preferences::COMPARE_WITH.allows(code), "{code}");
        }
        // Every default is one of its values (a code's is none).
        for p in PREFERENCES.iter().filter(|p| p.values != Values::Code) {
            assert!(p.allows(p.fallback), "{}", p.name);
        }
    }
}
