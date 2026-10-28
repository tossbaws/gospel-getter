//! Bookmarks (GH-11): listed, added and removed, each previewed in the
//! translation being read.

use anyhow::Context;
use serde::Serialize;

use super::{AppState, book_name, code_of, failed, resolve_translation};
use crate::db::{BookmarkError, BookmarkInTranslation};
use crate::domain::kjv_markup::VerseText;
use crate::passage_reference::passage_reference;

// ---- Bookmarks (GH-11)

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BookmarkDto {
    pub id: i64,
    pub book_id: i64,
    pub book_name: String,
    pub chapter: i64,
    pub verse_start: i64,
    pub verse_end: i64,
    /// e.g. "John 3:16–18".
    pub reference: String,
    /// The first verse's plain text in the requested translation (see
    /// `VerseText`), or `None` if that translation doesn't number it.
    pub preview: Option<String>,
    /// Whether the requested translation numbers every verse bookmarked.
    pub complete: bool,
    pub created_at: String,
}

/// A preview's plain text (see `VerseText`).
pub(super) fn preview_text(translation_code: &str, stored: Option<String>) -> Option<String> {
    stored.map(|text| VerseText::new(translation_code, &text).text)
}

fn bookmark_dto(state: &AppState, code: &str, b: BookmarkInTranslation) -> BookmarkDto {
    let BookmarkInTranslation {
        bookmark,
        first_verse_text,
        complete,
    } = b;
    let book_name = book_name(state, bookmark.book_id)
        .unwrap_or("?")
        .to_string();
    BookmarkDto {
        reference: passage_reference(
            &book_name,
            bookmark.chapter,
            Some((bookmark.verse_start, bookmark.verse_end)),
        ),
        id: bookmark.id,
        book_id: bookmark.book_id,
        book_name,
        chapter: bookmark.chapter,
        verse_start: bookmark.verse_start,
        verse_end: bookmark.verse_end,
        preview: preview_text(code, first_verse_text),
        complete,
        created_at: bookmark.created_at,
    }
}

/// Every bookmark, newest first, previewed in one translation.
pub async fn bookmarks_in(
    state: &AppState,
    translation_code: Option<&str>,
) -> anyhow::Result<Vec<BookmarkDto>> {
    let translation_id = resolve_translation(state, translation_code);
    let code = code_of(state, translation_id);
    Ok(state
        .store
        .bookmarks(translation_id)
        .await?
        .into_iter()
        .map(|b| bookmark_dto(state, code, b))
        .collect())
}

/// A passage that couldn't be bookmarked or highlighted, in words for the
/// reader. `action` is "bookmarked" or "highlighted"; `failed` is the
/// message for a database failure.
pub(super) fn passage_error_message(
    state: &AppState,
    error: BookmarkError,
    action: &str,
    failed: &str,
) -> String {
    match error {
        BookmarkError::InvalidRange {
            verse_start,
            verse_end,
        } => format!("{verse_start}\u{2013}{verse_end} isn't a verse range that can be {action}."),
        BookmarkError::NoSuchChapter { book_id, chapter } => {
            let name = book_name(state, book_id).unwrap_or("that book");
            format!("There's no {name} {chapter}.")
        }
        BookmarkError::NoSuchVerse {
            book_id,
            chapter,
            verse,
            last_verse,
        } => {
            let name = book_name(state, book_id).unwrap_or("that book");
            format!("There's no {name} {chapter}:{verse}; the chapter ends at verse {last_verse}.")
        }
        BookmarkError::Database(e) => {
            tracing::error!("{failed} {e:#}");
            failed.to_string()
        }
    }
}

/// Bookmark a passage, returning its id (the existing one if it was
/// already bookmarked), or a message for the reader.
pub async fn bookmark_passage(
    state: &AppState,
    book_id: i64,
    chapter: i64,
    verse_start: i64,
    verse_end: i64,
) -> Result<i64, String> {
    state
        .store
        .add_bookmark(book_id, chapter, verse_start, verse_end)
        .await
        .map(|bookmark| bookmark.id)
        .map_err(|e| {
            passage_error_message(state, e, "bookmarked", "The bookmark couldn't be saved.")
        })
}

#[tauri::command]
pub async fn list_bookmarks(
    state: tauri::State<'_, AppState>,
    translation_code: Option<String>,
) -> Result<Vec<BookmarkDto>, String> {
    bookmarks_in(&state, translation_code.as_deref())
        .await
        .context("Failed to list bookmarks")
        .map_err(failed("Bookmarks couldn't be loaded."))
}

#[tauri::command]
pub async fn add_bookmark(
    state: tauri::State<'_, AppState>,
    book_id: i64,
    chapter: i64,
    verse_start: i64,
    verse_end: i64,
) -> Result<i64, String> {
    bookmark_passage(&state, book_id, chapter, verse_start, verse_end).await
}

/// Remove a bookmark; `false` if it was already gone.
#[tauri::command]
pub async fn remove_bookmark(state: tauri::State<'_, AppState>, id: i64) -> Result<bool, String> {
    state
        .store
        .remove_bookmark(id)
        .await
        .with_context(|| format!("Failed to remove bookmark {id}"))
        .map_err(failed("The bookmark couldn't be removed."))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::tests::app_state;
    use crate::db::test_support::bundled;

    #[tokio::test]
    async fn bookmark_commands_add_list_and_explain_failures() {
        let (_db, state) = app_state("cmd_bookmarks").await;

        let id = bookmark_passage(&state, 43, 3, 16, 18).await.unwrap();
        assert_eq!(bookmark_passage(&state, 43, 3, 16, 18).await, Ok(id));
        assert_eq!(
            bookmark_passage(&state, 43, 22, 1, 1).await,
            Err("There's no John 22.".to_string())
        );
        assert_eq!(
            bookmark_passage(&state, 43, 3, 36, 40).await,
            Err("There's no John 3:40; the chapter ends at verse 36.".to_string())
        );
        assert!(bookmark_passage(&state, 43, 3, 4, 2).await.is_err());

        let listed =
            serde_json::to_value(bookmarks_in(&state, Some("web")).await.unwrap()).unwrap();
        let first = &listed[0];
        assert_eq!(first["reference"], "John 3:16\u{2013}18");
        assert_eq!(first["bookName"], "John");
        assert_eq!(first["complete"], true);
        assert_eq!(
            first["preview"].as_str().unwrap(),
            bundled("web")[42].chapters[2][15]
        );
    }
}
