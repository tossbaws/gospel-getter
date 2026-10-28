//! Notes on passages: listed, written, changed, deleted and restored (to
//! undo a delete).

use anyhow::Context;
use serde::Serialize;

use super::{AppState, book_name, failed, passage_error_message};
use crate::db::{Note, NoteError, Passage};
use crate::passage_reference::passage_reference;
use crate::reader_data;

// ---- Notes

/// One note.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteDto {
    pub id: i64,
    pub book_id: i64,
    pub book_name: String,
    pub chapter: i64,
    pub verse_start: i64,
    pub verse_end: i64,
    /// e.g. "John 3:16–18", with no translation: a note belongs to none.
    pub reference: String,
    /// Plain text, as the reader wrote it (trimmed).
    pub body: String,
    pub created_at: String,
    /// The day it was written, as the app shows it ("Oct 9, 2026", in the
    /// reader's time zone; see `notes_export::stored_date`).
    pub created_at_display: String,
    pub updated_at: String,
}

fn note_dto(state: &AppState, note: Note) -> NoteDto {
    let book_name = book_name(state, note.book_id).unwrap_or("?").to_string();
    NoteDto {
        reference: passage_reference(
            &book_name,
            note.chapter,
            Some((note.verse_start, note.verse_end)),
        ),
        id: note.id,
        book_id: note.book_id,
        book_name,
        chapter: note.chapter,
        verse_start: note.verse_start,
        verse_end: note.verse_end,
        body: note.body,
        created_at_display: crate::notes_export::stored_date(&note.created_at, &chrono::Local),
        created_at: note.created_at,
        updated_at: note.updated_at,
    }
}

/// A note that couldn't be saved, in words for the reader.
fn note_error_message(state: &AppState, error: NoteError) -> String {
    const FAILED: &str = "The note couldn't be saved.";
    match error {
        NoteError::Passage(e) => passage_error_message(state, e, "noted", FAILED),
        NoteError::Body(e) => e.to_string(),
        NoteError::Limit(limit) => limit.to_string(),
        NoteError::NotFound { .. } => {
            "That note no longer exists; it may have been deleted.".to_string()
        }
        NoteError::Database(e) => {
            tracing::error!("{FAILED} {e:#}");
            FAILED.to_string()
        }
    }
}

/// Every note overlapping a passage, in the order they're shown: by first
/// verse, then last verse, then oldest first.
pub async fn notes_overlapping(state: &AppState, passage: Passage) -> Result<Vec<NoteDto>, String> {
    if passage.verse_start < 1 || passage.verse_end < passage.verse_start {
        return Err(format!(
            "{}\u{2013}{} isn't a verse range.",
            passage.verse_start, passage.verse_end
        ));
    }
    let notes = state
        .store
        .notes_overlapping(&passage)
        .await
        .context("Failed to list notes")
        .map_err(failed("Notes couldn't be loaded."))?;
    Ok(notes.into_iter().map(|n| note_dto(state, n)).collect())
}

/// Every note, in Bible order.
pub async fn notes_list(state: &AppState) -> anyhow::Result<Vec<NoteDto>> {
    Ok(state
        .store
        .notes()
        .await?
        .into_iter()
        .map(|n| note_dto(state, n))
        .collect())
}

/// Write a new note on a passage, or say why not.
pub async fn note_add(state: &AppState, passage: Passage, body: &str) -> Result<NoteDto, String> {
    state
        .store
        .add_note(&passage, body, None)
        .await
        .map(|n| note_dto(state, n))
        .map_err(|e| note_error_message(state, e))
}

/// Change a note's text, or say why not.
pub async fn note_update(state: &AppState, id: i64, body: &str) -> Result<NoteDto, String> {
    state
        .store
        .update_note(id, body)
        .await
        .map(|n| note_dto(state, n))
        .map_err(|e| note_error_message(state, e))
}

/// Delete a note, returning it as it was (to undo with `note_restore`), or
/// `None` if it was already gone.
pub async fn note_delete(state: &AppState, id: i64) -> Result<Option<NoteDto>, String> {
    state
        .store
        .delete_note(id)
        .await
        .map(|n| n.map(|n| note_dto(state, n)))
        .with_context(|| format!("Failed to delete note {id}"))
        .map_err(failed("The note couldn't be deleted."))
}

/// Bring back a deleted note: the same passage, text and dates (a new id).
pub async fn note_restore(
    state: &AppState,
    passage: Passage,
    body: &str,
    created_at: &str,
    updated_at: &str,
) -> Result<NoteDto, String> {
    let (Some(created_at), Some(updated_at)) = (
        reader_data::normalize_timestamp(created_at),
        reader_data::normalize_timestamp(updated_at),
    ) else {
        return Err("That note's date isn't valid, so it couldn't be restored.".to_string());
    };
    state
        .store
        .add_note(&passage, body, Some((&created_at, &updated_at)))
        .await
        .map(|n| note_dto(state, n))
        .map_err(|e| note_error_message(state, e))
}

fn passage(book_id: i64, chapter: i64, verse_start: i64, verse_end: i64) -> Passage {
    Passage {
        book_id,
        chapter,
        verse_start,
        verse_end,
    }
}

#[tauri::command]
pub async fn list_notes(
    state: tauri::State<'_, AppState>,
    book_id: i64,
    chapter: i64,
    verse_start: i64,
    verse_end: i64,
) -> Result<Vec<NoteDto>, String> {
    notes_overlapping(&state, passage(book_id, chapter, verse_start, verse_end)).await
}

#[tauri::command]
pub async fn list_all_notes(state: tauri::State<'_, AppState>) -> Result<Vec<NoteDto>, String> {
    notes_list(&state)
        .await
        .context("Failed to list notes")
        .map_err(failed("Notes couldn't be loaded."))
}

#[tauri::command]
pub async fn add_note(
    state: tauri::State<'_, AppState>,
    book_id: i64,
    chapter: i64,
    verse_start: i64,
    verse_end: i64,
    body: String,
) -> Result<NoteDto, String> {
    note_add(
        &state,
        passage(book_id, chapter, verse_start, verse_end),
        &body,
    )
    .await
}

#[tauri::command]
pub async fn update_note(
    state: tauri::State<'_, AppState>,
    id: i64,
    body: String,
) -> Result<NoteDto, String> {
    note_update(&state, id, &body).await
}

#[tauri::command]
pub async fn delete_note(
    state: tauri::State<'_, AppState>,
    id: i64,
) -> Result<Option<NoteDto>, String> {
    note_delete(&state, id).await
}

#[tauri::command]
#[expect(
    clippy::too_many_arguments,
    reason = "a Tauri command's arguments are the fields the frontend passes"
)]
pub async fn restore_note(
    state: tauri::State<'_, AppState>,
    book_id: i64,
    chapter: i64,
    verse_start: i64,
    verse_end: i64,
    body: String,
    created_at: String,
    updated_at: String,
) -> Result<NoteDto, String> {
    note_restore(
        &state,
        passage(book_id, chapter, verse_start, verse_end),
        &body,
        &created_at,
        &updated_at,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::tests::app_state;
    use crate::db::Passage;

    #[tokio::test]
    async fn note_commands_save_list_and_explain_failures() {
        let (_db, state) = app_state("cmd_notes").await;
        let at = |verse_start, verse_end| Passage {
            book_id: 43,
            chapter: 3,
            verse_start,
            verse_end,
        };

        let note = note_add(&state, at(16, 18), " <i>Not</i> markup.\n")
            .await
            .unwrap();
        assert_eq!(
            (
                note.reference.as_str(),
                note.book_name.as_str(),
                note.body.as_str()
            ),
            ("John 3:16\u{2013}18", "John", "<i>Not</i> markup.")
        );
        // The day it was written, as the app shows it, from the stored
        // timestamp in the reader's time zone.
        assert_eq!(
            note.created_at_display,
            crate::notes_export::display_date(crate::notes_export::local_date(
                chrono::DateTime::parse_from_rfc3339(&note.created_at)
                    .unwrap()
                    .to_utc(),
                &chrono::Local,
            ))
        );
        let listed = notes_overlapping(&state, at(17, 17)).await.unwrap();
        assert_eq!(listed[0].id, note.id);
        assert_eq!(
            notes_list(&state).await.unwrap()[0].reference,
            "John 3:16\u{2013}18"
        );

        for (passage, body, expected) in [
            (at(16, 16), " ", "A note can't be empty."),
            (
                Passage {
                    chapter: 22,
                    ..at(1, 1)
                },
                "x",
                "There's no John 22.",
            ),
            (
                at(36, 40),
                "x",
                "There's no John 3:40; the chapter ends at verse 36.",
            ),
            (
                at(4, 2),
                "x",
                "4\u{2013}2 isn't a verse range that can be noted.",
            ),
        ] {
            assert_eq!(
                note_add(&state, passage, body).await.unwrap_err(),
                expected,
                "{passage:?}"
            );
        }
        assert_eq!(
            notes_overlapping(&state, at(4, 2)).await.unwrap_err(),
            "4\u{2013}2 isn't a verse range."
        );

        let edited = note_update(&state, note.id, "Edited.").await.unwrap();
        assert_eq!(edited.body, "Edited.");
        let deleted = note_delete(&state, note.id).await.unwrap().unwrap();
        assert_eq!(
            note_delete(&state, note.id).await.unwrap().map(|n| n.id),
            None
        );
        assert_eq!(
            note_update(&state, note.id, "Too late.").await.unwrap_err(),
            "That note no longer exists; it may have been deleted."
        );
        let restored = note_restore(
            &state,
            at(deleted.verse_start, deleted.verse_end),
            &deleted.body,
            &deleted.created_at,
            &deleted.updated_at,
        )
        .await
        .unwrap();
        assert_eq!(
            (
                &restored.reference,
                &restored.body,
                &restored.created_at,
                &restored.updated_at
            ),
            (
                &deleted.reference,
                &deleted.body,
                &deleted.created_at,
                &deleted.updated_at
            )
        );
        assert_eq!(
            note_restore(&state, at(16, 16), "x", "yesterday", "today")
                .await
                .unwrap_err(),
            "That note's date isn't valid, so it couldn't be restored."
        );
    }
}
