//! Highlights: verses marked in a color, listed one by one (for the
//! chapter being read) or as passages (for the Library).

use anyhow::Context;
use serde::Serialize;

use super::{
    AppState, book_name, code_of, failed, passage_error_message, preview_text, resolve_translation,
};
use crate::db::{HighlightColor, HighlightPassageInTranslation};
use crate::passage_reference::passage_reference;

// ---- Highlights

/// One highlighted verse.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HighlightDto {
    pub book_id: i64,
    pub chapter: i64,
    pub verse: i64,
    pub color: HighlightColor,
}

/// Every highlight, in Bible order.
pub async fn highlights_list(state: &AppState) -> anyhow::Result<Vec<HighlightDto>> {
    Ok(state
        .store
        .highlights()
        .await?
        .into_iter()
        .map(|h| HighlightDto {
            book_id: h.book_id,
            chapter: h.chapter,
            verse: h.verse,
            color: h.color,
        })
        .collect())
}

/// A highlighted passage: consecutive verses of one chapter in one color.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HighlightPassageDto {
    pub book_id: i64,
    pub book_name: String,
    pub chapter: i64,
    pub verse_start: i64,
    pub verse_end: i64,
    pub color: HighlightColor,
    /// e.g. "John 3:16–18".
    pub reference: String,
    /// The first verse's plain text in the requested translation (see
    /// `VerseText`), or `None` if that translation doesn't number it.
    pub preview: Option<String>,
    /// Whether the requested translation numbers every verse in it.
    pub complete: bool,
}

fn highlight_passage_dto(
    state: &AppState,
    code: &str,
    h: HighlightPassageInTranslation,
) -> HighlightPassageDto {
    let HighlightPassageInTranslation {
        passage,
        first_verse_text,
        complete,
    } = h;
    let book_name = book_name(state, passage.book_id).unwrap_or("?").to_string();
    HighlightPassageDto {
        reference: passage_reference(
            &book_name,
            passage.chapter,
            Some((passage.verse_start, passage.verse_end)),
        ),
        book_id: passage.book_id,
        book_name,
        chapter: passage.chapter,
        verse_start: passage.verse_start,
        verse_end: passage.verse_end,
        color: passage.color,
        preview: preview_text(code, first_verse_text),
        complete,
    }
}

/// Every highlighted passage, in Bible order, previewed in one
/// translation.
pub async fn highlight_passages_in(
    state: &AppState,
    translation_code: Option<&str>,
) -> anyhow::Result<Vec<HighlightPassageDto>> {
    let translation_id = resolve_translation(state, translation_code);
    let code = code_of(state, translation_id);
    Ok(state
        .store
        .highlight_passages(translation_id)
        .await?
        .into_iter()
        .map(|h| highlight_passage_dto(state, code, h))
        .collect())
}

/// Highlight every verse of a passage in `color` (by name), replacing any
/// color they had, or say why not.
pub async fn highlight_passage(
    state: &AppState,
    book_id: i64,
    chapter: i64,
    verse_start: i64,
    verse_end: i64,
    color: &str,
) -> Result<(), String> {
    let color: HighlightColor = color.parse().map_err(|e| format!("{e}."))?;
    state
        .store
        .set_highlights(book_id, chapter, verse_start, verse_end, color)
        .await
        .map_err(|e| {
            passage_error_message(state, e, "highlighted", "The highlight couldn't be saved.")
        })
}

/// Remove the highlight from every verse of a passage; returns how many
/// verses had one.
pub async fn unhighlight_passage(
    state: &AppState,
    book_id: i64,
    chapter: i64,
    verse_start: i64,
    verse_end: i64,
) -> Result<usize, String> {
    state
        .store
        .remove_highlights(book_id, chapter, verse_start, verse_end)
        .await
        .context("Failed to remove highlights")
        .map_err(failed("The highlight couldn't be removed."))
}

#[tauri::command]
pub async fn list_highlights(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<HighlightDto>, String> {
    highlights_list(&state)
        .await
        .context("Failed to list highlights")
        .map_err(failed("Highlights couldn't be loaded."))
}

#[tauri::command]
pub async fn list_highlight_passages(
    state: tauri::State<'_, AppState>,
    translation_code: Option<String>,
) -> Result<Vec<HighlightPassageDto>, String> {
    highlight_passages_in(&state, translation_code.as_deref())
        .await
        .context("Failed to list highlighted passages")
        .map_err(failed("Highlights couldn't be loaded."))
}

#[tauri::command]
pub async fn set_highlight(
    state: tauri::State<'_, AppState>,
    book_id: i64,
    chapter: i64,
    verse_start: i64,
    verse_end: i64,
    color: String,
) -> Result<(), String> {
    highlight_passage(&state, book_id, chapter, verse_start, verse_end, &color).await
}

#[tauri::command]
pub async fn remove_highlight(
    state: tauri::State<'_, AppState>,
    book_id: i64,
    chapter: i64,
    verse_start: i64,
    verse_end: i64,
) -> Result<usize, String> {
    unhighlight_passage(&state, book_id, chapter, verse_start, verse_end).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::tests::app_state;
    use crate::db::test_support::bundled;

    #[tokio::test]
    async fn highlight_passages_command_groups_and_previews() {
        let (_db, state) = app_state("cmd_highlight_passages").await;
        assert_eq!(
            serde_json::to_value(highlight_passages_in(&state, Some("kjv")).await.unwrap())
                .unwrap(),
            serde_json::json!([])
        );

        highlight_passage(&state, 43, 3, 16, 18, "yellow")
            .await
            .unwrap();
        highlight_passage(&state, 43, 3, 20, 20, "yellow")
            .await
            .unwrap();
        highlight_passage(&state, 19, 23, 1, 2, "blue")
            .await
            .unwrap();

        let listed =
            serde_json::to_value(highlight_passages_in(&state, Some("web")).await.unwrap())
                .unwrap();
        assert_eq!(
            listed,
            serde_json::json!([
                {
                    "bookId": 19, "bookName": "Psalms", "chapter": 23,
                    "verseStart": 1, "verseEnd": 2, "color": "blue",
                    "reference": "Psalms 23:1\u{2013}2",
                    "preview": bundled("web")[18].chapters[22][0], "complete": true
                },
                {
                    "bookId": 43, "bookName": "John", "chapter": 3,
                    "verseStart": 16, "verseEnd": 18, "color": "yellow",
                    "reference": "John 3:16\u{2013}18",
                    "preview": bundled("web")[42].chapters[2][15], "complete": true
                },
                {
                    "bookId": 43, "bookName": "John", "chapter": 3,
                    "verseStart": 20, "verseEnd": 20, "color": "yellow",
                    "reference": "John 3:20",
                    "preview": bundled("web")[42].chapters[2][19], "complete": true
                },
            ])
        );

        // Removing one passage, through the unhighlight path, leaves the rest.
        assert_eq!(unhighlight_passage(&state, 43, 3, 16, 18).await, Ok(3));
        let left = highlight_passages_in(&state, None).await.unwrap();
        let references: Vec<&str> = left.iter().map(|h| h.reference.as_str()).collect();
        assert_eq!(references, ["Psalms 23:1\u{2013}2", "John 3:20"]);
    }
}
