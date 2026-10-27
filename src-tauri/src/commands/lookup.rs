//! What the commands look up the same way: a book's name, a translation's
//! id or code, and `failed`, which turns a failure into the sentence the
//! reader is shown.

use super::AppState;

pub(super) fn book_name(state: &AppState, book_id: i64) -> Option<&str> {
    state
        .books
        .iter()
        .find(|b| b.id == book_id)
        .map(|b| b.name.as_str())
}

/// The code of the translation `translation_id` ("kjv", "web"), which
/// decides how its verses are shown (see `VerseText`).
pub(super) fn code_of(state: &AppState, translation_id: i64) -> &str {
    state
        .translations
        .iter()
        .find(|t| t.id == translation_id)
        .map_or("", |t| t.code.as_str())
}

/// Resolve a translation code to a translation id, falling back to the
/// first bundled translation (the default) when the code is absent or
/// doesn't match anything we have.
pub(super) fn resolve_translation(state: &AppState, code: Option<&str>) -> i64 {
    code.and_then(|c| state.translations.iter().find(|t| t.code == c))
        .or(state.translations.first())
        .map(|t| t.id)
        .unwrap_or(1)
}

/// For `map_err` on a command's failure: logs the error with its causes
/// (give it its context first, with `anyhow::Context`) and answers with
/// `sentence`, which is what the reader is shown.
pub(super) fn failed(sentence: &'static str) -> impl FnOnce(anyhow::Error) -> String {
    move |e| {
        tracing::error!("{e:#}");
        sentence.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A failure answers with its sentence alone; what's logged is its
    /// context and causes, as each command logged them by hand before.
    #[test]
    fn a_failure_is_its_sentence_and_logs_its_context() {
        let e = anyhow::anyhow!("database is locked").context("Failed to list bookmarks");
        assert_eq!(
            format!("{e:#}"),
            "Failed to list bookmarks: database is locked"
        );
        assert_eq!(
            failed("Bookmarks couldn't be loaded.")(e),
            "Bookmarks couldn't be loaded."
        );
    }
}
