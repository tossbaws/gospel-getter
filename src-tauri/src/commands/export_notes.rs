//! Exporting the reader's notes as Markdown (the file itself is
//! `crate::notes_export`'s).

use anyhow::Context;
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;

use super::{AppState, failed, run_dialog, write_atomically};
use crate::db::{self, Note};
use crate::notes_export::NotesFilter;

// ---- Export notes as Markdown: readable, for a notes app or printing,
// never imported back (see `notes_export`).

/// How exporting notes ended.
#[derive(Debug, Serialize)]
#[serde(
    tag = "status",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum NotesExportOutcome {
    /// The reader closed the save dialog; no file was written.
    Cancelled,
    /// The file was completely written.
    Saved { path: String, notes: usize },
}

/// A notes export, ready to save.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotesFile {
    /// The name the save dialog suggests.
    pub file_name: String,
    /// The Markdown.
    pub contents: String,
    /// How many notes it holds.
    pub notes: usize,
}

const NOTES_UNREADABLE: &str = "Your notes couldn't be read for export.";

/// Every note, for an export (or its count).
async fn notes_to_export(state: &AppState) -> Result<Vec<Note>, String> {
    state
        .store
        .notes()
        .await
        .context("Failed to read notes to export")
        .map_err(failed(NOTES_UNREADABLE))
}

/// What the export dialog offers, worked out when it opens.
#[derive(Debug, Serialize)]
pub struct NotesExportOptions {
    /// The books with notes, in Bible order.
    pub books: Vec<crate::notes_export::NotedBook>,
    /// The date choices, ending on today's local date.
    pub dates: crate::notes_export::DatePresets,
}

/// The export dialog's choices: the books with notes, and date presets
/// that end today in `tz`.
pub async fn notes_export_options<Tz: chrono::TimeZone>(
    state: &AppState,
    now: chrono::DateTime<chrono::Utc>,
    tz: &Tz,
) -> Result<NotesExportOptions, String> {
    let notes = notes_to_export(state).await?;
    Ok(NotesExportOptions {
        books: crate::notes_export::noted_books(&notes, &state.books),
        dates: crate::notes_export::date_presets(crate::notes_export::local_date(now, tz)),
    })
}

/// How many notes `filter` asks for, their dates read in `tz`: the
/// export dialog's live count, by the export's own rule.
pub async fn count_notes_to_export<Tz: chrono::TimeZone>(
    state: &AppState,
    filter: &NotesFilter,
    tz: &Tz,
) -> Result<usize, String> {
    filter.check(&state.books)?;
    let notes = notes_to_export(state).await?;
    Ok(notes.iter().filter(|n| filter.matches(n, tz)).count())
}

/// The reader's notes as Markdown, with each note's verses quoted in the
/// translation `translation_code`: those `filter` asks for. Dates (the
/// notes', the filter's, the export's and the file name's) are in `tz`.
/// Each chapter a note is on is read once, however many notes it has.
pub async fn build_notes_export<Tz: chrono::TimeZone>(
    state: &AppState,
    translation_code: &str,
    filter: &NotesFilter,
    now: chrono::DateTime<chrono::Utc>,
    tz: &Tz,
) -> Result<NotesFile, String> {
    let translation = state
        .translations
        .iter()
        .find(|t| t.code == translation_code)
        .ok_or_else(|| format!("There's no translation \u{201c}{translation_code}\u{201d}."))?;
    filter.check(&state.books)?;
    let all = notes_to_export(state).await?;
    let mut export = crate::notes_export::NotesExport {
        notes: &all,
        books: &state.books,
        verses: &HashMap::new(),
        translation_code: &translation.code,
        translation_name: &translation.name,
        filter,
        exported_at: now,
    };
    let notes = export.notes_in_scope(tz);
    if notes.is_empty() {
        return Err(if all.is_empty() {
            "There are no notes to export."
        } else {
            "No notes match these filters."
        }
        .to_string());
    }
    let mut verses: HashMap<(i64, i64), Vec<db::Verse>> = HashMap::new();
    for note in &notes {
        let key = (note.book_id, note.chapter);
        if verses.contains_key(&key) {
            continue;
        }
        let chapter = state
            .store
            .chapter_verses(translation.id, note.book_id, note.chapter)
            .await
            .context("Failed to read verses to export")
            .map_err(failed(NOTES_UNREADABLE))?;
        verses.insert(key, chapter);
    }
    export.verses = &verses;
    Ok(NotesFile {
        file_name: export.file_name(tz),
        contents: export.render(tz),
        notes: notes.len(),
    })
}

/// Write a notes export to `path`, completely or not at all (as
/// `save_reader_data` does).
pub async fn save_notes_export(
    file: &NotesFile,
    path: &Path,
) -> anyhow::Result<NotesExportOutcome> {
    let target = path.to_path_buf();
    let contents = file.contents.clone();
    tokio::task::spawn_blocking(move || write_atomically(&target, contents.as_bytes()))
        .await
        .context("The save was interrupted")??;
    Ok(NotesExportOutcome::Saved {
        path: path.display().to_string(),
        notes: file.notes,
    })
}

/// What the export dialog offers: the books with notes, and date presets
/// in the computer's time zone.
#[tauri::command]
pub async fn get_notes_export_options(
    state: tauri::State<'_, AppState>,
) -> Result<NotesExportOptions, String> {
    notes_export_options(&state, chrono::Utc::now(), &chrono::Local).await
}

/// How many notes `filter` asks for, in the computer's time zone.
#[tauri::command]
pub async fn count_notes_export(
    state: tauri::State<'_, AppState>,
    filter: NotesFilter,
) -> Result<usize, String> {
    count_notes_to_export(&state, &filter, &chrono::Local).await
}

/// Export the reader's notes (those `filter` asks for) as Markdown, with
/// their verses quoted in `translation_code`, to a file they choose with
/// the native save dialog. Dates are in the computer's time zone.
#[tauri::command]
pub async fn export_notes<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: tauri::State<'_, AppState>,
    translation_code: String,
    filter: NotesFilter,
) -> Result<NotesExportOutcome, String> {
    use tauri_plugin_dialog::DialogExt;
    // Built before the dialog opens, so a problem is reported without
    // asking for a file name first.
    let file = build_notes_export(
        &state,
        &translation_code,
        &filter,
        chrono::Utc::now(),
        &chrono::Local,
    )
    .await?;
    let dialog = app
        .dialog()
        .file()
        .add_filter("Markdown", &["md"])
        .set_title("Export notes")
        .set_file_name(&file.file_name);
    let Some(path) = run_dialog(move || dialog.blocking_save_file()).await? else {
        return Ok(NotesExportOutcome::Cancelled);
    };
    save_notes_export(&file, &path).await.map_err(|e| {
        tracing::error!("Notes export failed: {e:#}");
        format!("The file couldn't be saved: {e:#}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::tests::app_state;
    use crate::commands::*;
    use crate::db::Passage;
    use crate::db::test_support::bundled;

    #[tokio::test]
    async fn notes_export_quotes_each_note_in_the_chosen_translation() {
        use crate::notes_export::escape_block;
        let (db, state) = app_state("cmd_notes_export").await;
        // 05:00 UTC on Oct 10 is still Oct 9 at -06:00.
        let tz = chrono::FixedOffset::west_opt(6 * 3600).unwrap();
        let now = chrono::DateTime::parse_from_rfc3339("2026-10-10T05:00:00Z")
            .unwrap()
            .to_utc();
        let export = |code: &'static str, book: Option<i64>| {
            let state = &state;
            let filter = NotesFilter {
                books: book.into_iter().collect(),
                ..NotesFilter::default()
            };
            async move { build_notes_export(state, code, &filter, now, &tz).await }
        };
        assert_eq!(
            export("kjv", None).await.unwrap_err(),
            "There are no notes to export."
        );

        let at = |book_id, chapter, verse_start, verse_end| Passage {
            book_id,
            chapter,
            verse_start,
            verse_end,
        };
        for (passage, body) in [
            (at(62, 1, 9, 9), "Confess."),
            (
                at(43, 3, 16, 18),
                "<b>Typed</b> HTML.\n\nA second paragraph.",
            ),
            (at(43, 3, 16, 16), "# Not a heading\n- not a list"),
            (at(40, 2, 23, 23), "Nazareth."),
        ] {
            note_add(&state, passage, body).await.unwrap();
        }
        let verse = |code: &str, book: usize, chapter: usize, verse: usize| {
            escape_block(&bundled(code)[book - 1].chapters[chapter - 1][verse - 1])
        };

        // One book: only John's notes, in Bible order, quoting the KJV.
        let john = export("kjv", Some(43)).await.unwrap();
        assert_eq!(john.file_name, "gospel-getter-notes-john-2026-10-09.md");
        assert_eq!(john.notes, 2);
        assert!(john.contents.starts_with(
            "# Notes on John\n\n\
             Exported Oct 9, 2026 \u{b7} Verse text: King James Version \u{b7} 2 notes\n\n\
             ## John 3:16\n"
        ));
        assert!(john.contents.contains(&format!(
            "## John 3:16\n\n> **16** {}\n\n\\# Not a heading\\\n\\- not a list\n",
            verse("kjv", 43, 3, 16)
        )));
        assert!(john.contents.contains(&format!(
            "## John 3:16\u{2013}18\n\n> **16** {} **17** {} **18** {}\n\n\
             \\<b\\>Typed\\</b\\> HTML.\n\nA second paragraph.\n",
            verse("kjv", 43, 3, 16),
            verse("kjv", 43, 3, 17),
            verse("kjv", 43, 3, 18),
        )));
        assert!(!john.contents.contains("Confess") && !john.contents.contains("Nazareth"));

        // Every note: Matthew 2:23 is numbered in the WEB, not the KJV, and
        // is kept either way.
        let kjv = export("kjv", None).await.unwrap();
        assert_eq!(kjv.file_name, "gospel-getter-notes-2026-10-09.md");
        assert_eq!(kjv.notes, 4);
        assert!(kjv.contents.contains(
            "## Matthew\n\n### Matthew 2:23\n\n\
             *It isn\u{2019}t numbered in the KJV.*\n\nNazareth.\n"
        ));
        let web = export("web", None).await.unwrap();
        assert!(
            web.contents
                .contains("Verse text: World English Bible \u{b7} 4 notes\n")
        );
        assert!(web.contents.contains(&format!(
            "### Matthew 2:23\n\n> **23** {}\n\nNazareth.\n",
            verse("web", 40, 2, 23)
        )));
        assert!(!web.contents.contains("numbered in the"));
        let books: Vec<&str> = web
            .contents
            .lines()
            .filter(|l| l.starts_with("## "))
            .collect();
        assert_eq!(books, ["## Matthew", "## John", "## 1 John"]);

        // What's asked for is checked, even though the UI never asks for it.
        assert_eq!(
            export("niv", None).await.unwrap_err(),
            "There's no translation \u{201c}niv\u{201d}."
        );
        for book in [0, 67, -1] {
            assert_eq!(
                export("kjv", Some(book)).await.unwrap_err(),
                format!("There's no book {book}.")
            );
        }
        assert_eq!(
            export("kjv", Some(1)).await.unwrap_err(),
            "No notes match these filters."
        );

        // Saved as is: UTF-8, LF, no byte-order mark.
        let path = db.path().with_file_name(&kjv.file_name);
        let outcome = save_notes_export(&kjv, &path).await.unwrap();
        assert!(matches!(
            outcome,
            NotesExportOutcome::Saved { notes: 4, .. }
        ));
        assert_eq!(std::fs::read(&path).unwrap(), kjv.contents.as_bytes());
        assert_eq!(
            serde_json::to_value(&outcome).unwrap()["status"],
            serde_json::json!("saved")
        );
    }

    #[tokio::test]
    async fn the_export_dialogs_count_is_the_exports_own() {
        use crate::notes_export::NotesFilter;
        let (db, state) = app_state("cmd_notes_export_count").await;
        // 10 pm local on Oct 9 at -06:00 is Oct 10 in UTC.
        let tz = chrono::FixedOffset::west_opt(6 * 3600).unwrap();
        let now = chrono::DateTime::parse_from_rfc3339("2026-10-12T18:00:00Z")
            .unwrap()
            .to_utc();
        let options = notes_export_options(&state, now, &tz).await.unwrap();
        assert!(options.books.is_empty());
        assert_eq!(
            options.dates,
            crate::notes_export::date_presets(
                chrono::NaiveDate::from_ymd_opt(2026, 10, 12).unwrap()
            )
        );
        assert_eq!(
            count_notes_to_export(&state, &NotesFilter::default(), &tz).await,
            Ok(0)
        );

        let notes = [
            (19, 23, 1, "2026-10-01T15:00:00.000Z"),
            (43, 1, 1, "2026-10-05T15:00:00.000Z"),
            (43, 3, 16, "2026-10-09T15:00:00.000Z"),
            (43, 3, 17, "2026-10-10T04:00:00.000Z"),
            (43, 5, 24, "2026-10-12T15:00:00.000Z"),
            (45, 8, 28, "2026-10-11T15:00:00.000Z"),
            (62, 1, 9, "2026-10-12T15:00:00.000Z"),
        ];
        let pool = db.connect().await;
        for (book_id, chapter, verse, written) in notes {
            let passage = Passage {
                book_id,
                chapter,
                verse_start: verse,
                verse_end: verse,
            };
            let body = format!("{book_id} {chapter}:{verse}");
            note_add(&state, passage, &body).await.unwrap();
            sqlx::query("UPDATE notes SET created_at = ?, updated_at = ? WHERE body = ?")
                .bind(written)
                .bind(written)
                .bind(&body)
                .execute(pool.write().pool())
                .await
                .unwrap();
        }
        let options = notes_export_options(&state, now, &tz).await.unwrap();
        assert_eq!(
            options
                .books
                .iter()
                .map(|b| (
                    b.id,
                    b.notes,
                    b.first_chapter,
                    b.last_chapter,
                    b.noted_chapters,
                    b.chapter_count
                ))
                .collect::<Vec<_>>(),
            [
                (19, 1, 23, 23, 1, 150),
                (43, 4, 1, 5, 3, 21),
                (45, 1, 8, 8, 1, 16),
                (62, 1, 1, 1, 1, 5)
            ]
        );

        let day = |d| chrono::NaiveDate::from_ymd_opt(2026, 10, d);
        let filters = [
            NotesFilter::default(),
            NotesFilter {
                books: vec![43],
                ..NotesFilter::default()
            },
            NotesFilter {
                books: vec![43],
                chapters: Some((3, 5)),
                ..NotesFilter::default()
            },
            NotesFilter {
                books: vec![62, 19],
                ..NotesFilter::default()
            },
            NotesFilter {
                written_from: day(5),
                written_to: day(9),
                ..NotesFilter::default()
            },
            NotesFilter {
                written_from: day(11),
                ..NotesFilter::default()
            },
            NotesFilter {
                books: vec![43],
                chapters: Some((3, 3)),
                written_to: day(9),
                ..NotesFilter::default()
            },
        ];
        let expected = [7, 4, 3, 2, 3, 3, 2];
        for (filter, expected) in filters.iter().zip(expected) {
            let count = count_notes_to_export(&state, filter, &tz).await.unwrap();
            let file = build_notes_export(&state, "kjv", filter, now, &tz)
                .await
                .unwrap();
            assert_eq!((count, file.notes), (expected, expected), "{filter:?}");
            let headings = file
                .contents
                .lines()
                .filter(|l| l.starts_with("## ") || l.starts_with("### "))
                .filter(|l| l.contains(':'))
                .count();
            assert_eq!(headings, expected, "{filter:?}");
        }

        // Nothing matching is a count of 0, but no file.
        let none = NotesFilter {
            books: vec![1],
            ..NotesFilter::default()
        };
        assert_eq!(count_notes_to_export(&state, &none, &tz).await, Ok(0));
        assert_eq!(
            build_notes_export(&state, "kjv", &none, now, &tz)
                .await
                .unwrap_err(),
            "No notes match these filters."
        );
        // A filter that can't be met is refused by both, the same way.
        let backwards = NotesFilter {
            written_from: day(12),
            written_to: day(1),
            ..NotesFilter::default()
        };
        let range = NotesFilter {
            books: vec![43],
            chapters: Some((20, 22)),
            ..NotesFilter::default()
        };
        for (filter, message) in [
            (backwards, "The From date is after the To date."),
            (
                range,
                "Chapters 20\u{2013}22 aren\u{2019}t all in John, which has 21.",
            ),
        ] {
            assert_eq!(
                count_notes_to_export(&state, &filter, &tz)
                    .await
                    .unwrap_err(),
                message
            );
            assert_eq!(
                build_notes_export(&state, "kjv", &filter, now, &tz)
                    .await
                    .unwrap_err(),
                message
            );
        }
    }

    #[tokio::test]
    async fn notes_export_shows_the_kjvs_markup_as_print_does_and_the_web_as_stored() {
        use crate::notes_export::escape_block;
        let (_db, state) = app_state("cmd_notes_export_kjv").await;
        let tz = chrono::FixedOffset::west_opt(6 * 3600).unwrap();
        let now = chrono::Utc::now();
        for (book_id, chapter, verse_start, verse_end) in
            [(19, 23, 1, 3), (43, 1, 5, 5), (58, 10, 34, 34)]
        {
            let passage = Passage {
                book_id,
                chapter,
                verse_start,
                verse_end,
            };
            note_add(&state, passage, "Note.").await.unwrap();
        }
        let kjv = build_notes_export(&state, "kjv", &NotesFilter::default(), now, &tz)
            .await
            .unwrap()
            .contents;
        assert!(kjv.contains(
            "> **1** \\[A Psalm of David.\\] The LORD *is* my shepherd; I shall not want. \
             **2** He maketh me to lie down in green pastures: he leadeth me beside the still \
             waters. **3** He restoreth my soul: he leadeth me in the paths of righteousness \
             for his name\'s sake.\n"
        ));
        assert!(kjv.contains(
            "> **5** And the light shineth in darkness; and the darkness comprehended it not.\n"
        ));
        // Hebrews 10:34's markup isn't well formed, so it's quoted as stored.
        let hebrews = &bundled("kjv")[57].chapters[9][33];
        assert!(hebrews.contains('{'));
        assert!(kjv.contains(&format!("> **34** {}\n", escape_block(hebrews))));

        // The WEB has no such markup: every verse is quoted as stored.
        let web = build_notes_export(&state, "web", &NotesFilter::default(), now, &tz)
            .await
            .unwrap()
            .contents;
        for (book, chapter, verse) in [
            (19, 23, 1),
            (19, 23, 2),
            (19, 23, 3),
            (43, 1, 5),
            (58, 10, 34),
        ] {
            let text = &bundled("web")[book - 1].chapters[chapter - 1][verse - 1];
            assert!(
                web.contains(&format!("**{verse}** {}", escape_block(text))),
                "{book} {chapter}:{verse}"
            );
        }
    }
}
