//! Notes on passages: listed (each with the references typed in it, for
//! the page to link), written, changed, deleted and restored (to undo a
//! delete), and copied as Markdown.

use anyhow::Context;
use serde::Serialize;

use super::{AppState, book_name, failed, passage_error_message};
use crate::db::{Note, NoteError, Passage};
use crate::domain::query::references_in;
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
    /// The references typed in it ("see Rom 5:8"), in order, which the
    /// page shows as links to their passages.
    pub links: Vec<NoteLinkDto>,
    pub created_at: String,
    /// The day it was written, as the app shows it ("Oct 9, 2026", in the
    /// reader's time zone; see `notes_export::stored_date`).
    pub created_at_display: String,
    pub updated_at: String,
}

/// A reference typed in a note: where it is in the note's `body` (byte
/// offsets, `start..end`) and the passage it names (see
/// `domain::query::references_in`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteLinkDto {
    pub start: usize,
    pub end: usize,
    pub book_id: i64,
    pub chapter: i64,
    /// Its verses, or none for the whole chapter.
    pub verse_start: Option<i64>,
    pub verse_end: Option<i64>,
    /// "Romans 5:8": the passage, as the app names it.
    pub reference: String,
}

/// The references typed in `body`, as the page links them.
fn note_links(state: &AppState, body: &str) -> Vec<NoteLinkDto> {
    references_in(body, &state.books)
        .into_iter()
        .map(|found| {
            let p = found.passage;
            NoteLinkDto {
                start: found.start,
                end: found.end,
                book_id: p.book_id,
                chapter: p.chapter,
                verse_start: p.verses.map(|(first, _)| first),
                verse_end: p.verses.map(|(_, last)| last),
                reference: passage_reference(
                    book_name(state, p.book_id).unwrap_or("?"),
                    p.chapter,
                    p.verses,
                ),
            }
        })
        .collect()
}

fn note_dto(state: &AppState, note: Note) -> NoteDto {
    let book_name = book_name(state, note.book_id).unwrap_or("?").to_string();
    NoteDto {
        links: note_links(state, &note.body),
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

/// Note `id` as Markdown, for Copy as Markdown, its verses quoted in the
/// translation `translation_code` (see `notes_export::note_markdown`).
pub async fn note_markdown_text(
    state: &AppState,
    id: i64,
    translation_code: &str,
) -> Result<String, String> {
    let translation = state
        .translations
        .iter()
        .find(|t| t.code == translation_code)
        .ok_or_else(|| format!("There's no translation \u{201c}{translation_code}\u{201d}."))?;
    let note = state
        .store
        .note(id)
        .await
        .with_context(|| format!("Failed to read note {id}"))
        .map_err(failed("The note couldn't be read."))?
        .ok_or_else(|| "That note no longer exists; it may have been deleted.".to_string())?;
    let verses = state
        .store
        .chapter_verses(translation.id, note.book_id, note.chapter)
        .await
        .with_context(|| format!("Failed to read the verses of note {id}"))
        .map_err(failed("The note's verses couldn't be read."))?;
    Ok(crate::notes_export::note_markdown(
        &note,
        book_name(state, note.book_id).unwrap_or("?"),
        &verses,
        &translation.code,
    ))
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

/// Note `id` as Markdown (Copy as Markdown), quoting its verses in
/// `translation_code`.
#[tauri::command]
pub async fn get_note_markdown(
    state: tauri::State<'_, AppState>,
    id: i64,
    translation_code: String,
) -> Result<String, String> {
    note_markdown_text(&state, id, &translation_code).await
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

    #[tokio::test]
    async fn every_note_comes_with_the_references_typed_in_it() {
        let (_db, state) = app_state("cmd_note_links").await;
        let at = Passage {
            book_id: 43,
            chapter: 3,
            verse_start: 16,
            verse_end: 16,
        };
        let body = "Éphèse first, then see Rom 5:8 and 1 Cor 13; not Job well done.";
        let note = note_add(&state, at, body).await.unwrap();
        let typed: Vec<&str> = note
            .links
            .iter()
            .map(|l| &note.body[l.start..l.end])
            .collect();
        assert_eq!(typed, ["Rom 5:8", "1 Cor 13"]);
        let link = |start, end, book_id, chapter, verses: Option<(i64, i64)>, reference: &str| {
            NoteLinkDto {
                start,
                end,
                book_id,
                chapter,
                verse_start: verses.map(|v| v.0),
                verse_end: verses.map(|v| v.1),
                reference: reference.to_string(),
            }
        };
        assert_eq!(
            note.links,
            [
                link(25, 32, 45, 5, Some((8, 8)), "Romans 5:8"),
                link(37, 45, 46, 13, None, "1 Corinthians 13"),
            ]
        );
        // The same in every list, and as JSON the page reads.
        let all = notes_list(&state).await.unwrap();
        assert_eq!(all[0].links, note.links);
        let json = serde_json::to_value(&all[0]).unwrap();
        assert_eq!(
            json["links"][0],
            serde_json::json!({ "start": 25, "end": 32, "bookId": 45, "chapter": 5,
                                "verseStart": 8, "verseEnd": 8, "reference": "Romans 5:8" })
        );
        assert_eq!(json["links"][1]["verseStart"], serde_json::Value::Null);
        assert!(notes_overlapping(&state, at).await.unwrap()[0].links == note.links);
    }

    #[tokio::test]
    async fn copy_as_markdown_quotes_the_note_s_verses_in_the_translation_read() {
        let (_db, state) = app_state("cmd_note_markdown").await;
        let at = Passage {
            book_id: 43,
            chapter: 3,
            verse_start: 16,
            verse_end: 17,
        };
        let note = note_add(&state, at, "**Loved**, see Rom 5:8.\n- the world")
            .await
            .unwrap();
        let verse = |code: &str, n: usize| {
            crate::passage_copy::verse_markdown(
                code,
                &crate::db::test_support::bundled(code)[42].chapters[2][n - 1],
            )
        };
        for code in ["kjv", "web"] {
            let upper = code.to_uppercase();
            assert_eq!(
                note_markdown_text(&state, note.id, code).await.unwrap(),
                format!(
                    "## John 3:16\u{2013}17\n\n> **16** {} **17** {}\n>\n> \u{2014} John 3:16\u{2013}17 ({upper})\n\n\
                     **Loved**, see Rom 5:8.\n- the world",
                    verse(code, 16),
                    verse(code, 17)
                ),
                "{code}"
            );
        }
        assert_eq!(
            note_markdown_text(&state, note.id, "niv")
                .await
                .unwrap_err(),
            "There's no translation \u{201c}niv\u{201d}."
        );
        note_delete(&state, note.id).await.unwrap();
        assert_eq!(
            note_markdown_text(&state, note.id, "kjv")
                .await
                .unwrap_err(),
            "That note no longer exists; it may have been deleted."
        );
    }
}
