//! Exporting the reader's notes as Markdown, as one file or as one file per
//! book in a folder, each note's verses quoted or as a commentary (the
//! files themselves are `crate::notes_export`'s); and the commentary for
//! the page to print.

use anyhow::Context;
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::Ordering;

use super::{AppState, failed, run_dialog, write_atomically};
use crate::db::{self, Note};
use crate::notes_export::{BookFile, Commentary, NotesExport, NotesFilter, NotesLayout};

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

/// What an export is made from, read once: the translation its verses are
/// quoted in, every note, and the verses of each chapter a note in scope
/// is on.
struct ExportData<'a> {
    translation: &'a db::Translation,
    notes: Vec<Note>,
    verses: HashMap<(i64, i64), Vec<db::Verse>>,
}

impl ExportData<'_> {
    fn export<'b>(
        &'b self,
        state: &'b AppState,
        filter: &'b NotesFilter,
        now: chrono::DateTime<chrono::Utc>,
    ) -> NotesExport<'b> {
        NotesExport {
            notes: &self.notes,
            books: &state.books,
            verses: &self.verses,
            translation_code: &self.translation.code,
            translation_name: &self.translation.name,
            filter,
            exported_at: now,
        }
    }
}

/// Reads what an export of the notes `filter` asks for (their dates read
/// in `tz`) is made from, quoting the translation `translation_code`, or
/// says why there's nothing to export. Each chapter a note is on is read
/// once, however many notes it has.
async fn export_data<'a, Tz: chrono::TimeZone>(
    state: &'a AppState,
    translation_code: &str,
    filter: &NotesFilter,
    tz: &Tz,
) -> Result<ExportData<'a>, String> {
    let translation = state
        .translations
        .iter()
        .find(|t| t.code == translation_code)
        .ok_or_else(|| format!("There's no translation \u{201c}{translation_code}\u{201d}."))?;
    filter.check(&state.books)?;
    let all = notes_to_export(state).await?;
    let mut data = ExportData {
        translation,
        notes: all,
        verses: HashMap::new(),
    };
    let notes = data
        .export(state, filter, chrono::Utc::now())
        .notes_in_scope(tz);
    if notes.is_empty() {
        return Err(if data.notes.is_empty() {
            "There are no notes to export."
        } else {
            "No notes match these filters."
        }
        .to_string());
    }
    for note in &notes {
        let key = (note.book_id, note.chapter);
        if data.verses.contains_key(&key) {
            continue;
        }
        let chapter = state
            .store
            .chapter_verses(translation.id, note.book_id, note.chapter)
            .await
            .context("Failed to read verses to export")
            .map_err(failed(NOTES_UNREADABLE))?;
        data.verses.insert(key, chapter);
    }
    Ok(data)
}

/// The reader's notes as Markdown, with each note's verses quoted in the
/// translation `translation_code`: those `filter` asks for. Dates (the
/// notes', the filter's, the export's and the file name's) are in `tz`.
pub async fn build_notes_export<Tz: chrono::TimeZone>(
    state: &AppState,
    translation_code: &str,
    filter: &NotesFilter,
    now: chrono::DateTime<chrono::Utc>,
    tz: &Tz,
) -> Result<NotesFile, String> {
    let data = export_data(state, translation_code, filter, tz).await?;
    let export = data.export(state, filter, now);
    let notes = export.notes_in_scope(tz).len();
    Ok(NotesFile {
        file_name: export.file_name(tz),
        contents: export.render(tz),
        notes,
    })
}

/// The notes `filter` asks for as a commentary (no Bible text, so no
/// translation, and no verses read), or why there's nothing to show: what
/// the file and the page's Print are made from. Dates are in `tz`.
async fn with_commentary<Tz: chrono::TimeZone, T>(
    state: &AppState,
    filter: &NotesFilter,
    now: chrono::DateTime<chrono::Utc>,
    tz: &Tz,
    make: impl FnOnce(&NotesExport<'_>) -> T,
) -> Result<T, String> {
    filter.check(&state.books)?;
    let notes = notes_to_export(state).await?;
    let verses = HashMap::new();
    let export = NotesExport {
        notes: &notes,
        books: &state.books,
        verses: &verses,
        translation_code: "",
        translation_name: "",
        filter,
        exported_at: now,
    };
    if export.notes_in_scope(tz).is_empty() {
        return Err(if notes.is_empty() {
            "There are no notes to export."
        } else {
            "No notes match these filters."
        }
        .to_string());
    }
    Ok(make(&export))
}

/// The notes `filter` asks for as a commentary file.
pub async fn build_notes_commentary<Tz: chrono::TimeZone>(
    state: &AppState,
    filter: &NotesFilter,
    now: chrono::DateTime<chrono::Utc>,
    tz: &Tz,
) -> Result<NotesFile, String> {
    with_commentary(state, filter, now, tz, |export| NotesFile {
        file_name: export.commentary_file_name(tz),
        contents: export.render_commentary(tz),
        notes: export.notes_in_scope(tz).len(),
    })
    .await
}

/// The commentary itself, for the page to print: the same as the file's.
pub async fn notes_commentary<Tz: chrono::TimeZone>(
    state: &AppState,
    filter: &NotesFilter,
    now: chrono::DateTime<chrono::Utc>,
    tz: &Tz,
) -> Result<Commentary, String> {
    with_commentary(state, filter, now, tz, |export| export.commentary(tz)).await
}

/// The export in `layout` (the quoted one in `translation_code`), as one
/// file.
pub async fn build_notes_export_as<Tz: chrono::TimeZone>(
    state: &AppState,
    layout: NotesLayout,
    translation_code: &str,
    filter: &NotesFilter,
    now: chrono::DateTime<chrono::Utc>,
    tz: &Tz,
) -> Result<NotesFile, String> {
    match layout {
        NotesLayout::Quoted => build_notes_export(state, translation_code, filter, now, tz).await,
        NotesLayout::Commentary => build_notes_commentary(state, filter, now, tz).await,
    }
}

/// The export in `layout`, as one file per book.
pub async fn build_notes_export_by_book_as<Tz: chrono::TimeZone>(
    state: &AppState,
    layout: NotesLayout,
    translation_code: &str,
    filter: &NotesFilter,
    now: chrono::DateTime<chrono::Utc>,
    tz: &Tz,
) -> Result<Vec<BookFile>, String> {
    match layout {
        NotesLayout::Quoted => {
            build_notes_export_by_book(state, translation_code, filter, now, tz).await
        }
        NotesLayout::Commentary => {
            with_commentary(state, filter, now, tz, |export| {
                export.by_book_as(NotesLayout::Commentary, tz)
            })
            .await
        }
    }
}

/// The same notes as [`build_notes_export`], as one file per book.
pub async fn build_notes_export_by_book<Tz: chrono::TimeZone>(
    state: &AppState,
    translation_code: &str,
    filter: &NotesFilter,
    now: chrono::DateTime<chrono::Utc>,
    tz: &Tz,
) -> Result<Vec<BookFile>, String> {
    let data = export_data(state, translation_code, filter, tz).await?;
    Ok(data.export(state, filter, now).by_book(tz))
}

// ---- One file per book, in a folder the reader chooses.

/// How exporting notes as one file per book ended (or paused).
#[derive(Debug, Serialize)]
#[serde(
    tag = "status",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum NotesFolderOutcome {
    /// The reader closed the folder dialog, or chose not to replace the
    /// files already there; nothing was written.
    Cancelled,
    /// Some of the files (`existing`, by name) are already in `folder`:
    /// nothing is written until the reader says whether to replace them
    /// (`confirm_notes_export_by_book` with `token`).
    Exists {
        token: u64,
        folder: String,
        existing: Vec<String>,
    },
    /// Every file was completely written.
    Saved {
        folder: String,
        files: usize,
        notes: usize,
    },
}

/// An export as one file per book waiting for the reader to say whether
/// to replace the files of it already in the folder they chose. Only one
/// at a time; another export replaces it.
pub struct PendingNotesFolder {
    token: u64,
    folder: PathBuf,
    files: Vec<BookFile>,
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The reader chose `folder` for `files`: they're written, unless any is
/// already there, which is never replaced without asking (the export
/// waits, see [`NotesFolderOutcome::Exists`]).
pub async fn notes_folder_chosen(
    state: &AppState,
    files: Vec<BookFile>,
    folder: PathBuf,
) -> Result<NotesFolderOutcome, String> {
    let existing: Vec<String> = files
        .iter()
        .filter(|f| folder.join(&f.file_name).exists())
        .map(|f| f.file_name.clone())
        .collect();
    if existing.is_empty() {
        return save_notes_folder(&files, &folder).await;
    }
    let token = state.next_import_token.fetch_add(1, Ordering::Relaxed);
    let shown = folder.display().to_string();
    *lock(&state.pending_notes_folder) = Some(PendingNotesFolder {
        token,
        folder,
        files,
    });
    Ok(NotesFolderOutcome::Exists {
        token,
        folder: shown,
        existing,
    })
}

/// The reader's answer to [`NotesFolderOutcome::Exists`]: Replace writes
/// every file (those there replaced), Cancel writes none.
pub async fn confirm_notes_folder(
    state: &AppState,
    token: u64,
    replace: bool,
) -> Result<NotesFolderOutcome, String> {
    let pending = lock(&state.pending_notes_folder).take_if(|p| p.token == token);
    let Some(pending) = pending else {
        return Err(
            "That export isn\u{2019}t waiting any more. Export the notes again.".to_string(),
        );
    };
    if !replace {
        return Ok(NotesFolderOutcome::Cancelled);
    }
    save_notes_folder(&pending.files, &pending.folder).await
}

/// Writes each of `files` into `folder`, each completely or not at all
/// (as `save_reader_data` does), in order; a failure says how many were
/// written first.
async fn save_notes_folder(
    files: &[BookFile],
    folder: &Path,
) -> Result<NotesFolderOutcome, String> {
    let total = files.len();
    let notes = files.iter().map(|f| f.notes).sum();
    let shown = folder.display().to_string();
    let (to_write, target) = (files.to_vec(), folder.to_path_buf());
    let written = tokio::task::spawn_blocking(move || {
        for (done, file) in to_write.iter().enumerate() {
            write_atomically(&target.join(&file.file_name), file.contents.as_bytes())
                .map_err(|e| (done, e))?;
        }
        Ok::<(), (usize, anyhow::Error)>(())
    })
    .await
    .map_err(|e| format!("The files couldn't be saved: {e}"))?;
    match written {
        Ok(()) => Ok(NotesFolderOutcome::Saved {
            folder: shown,
            files: total,
            notes,
        }),
        Err((0, e)) => {
            tracing::error!("Notes export by book failed: {e:#}");
            Err(format!("The files couldn't be saved: {e:#}"))
        }
        Err((done, e)) => {
            tracing::error!("Notes export by book failed after {done} files: {e:#}");
            Err(format!(
                "Saved {done} of {total} files to {shown}, then this went wrong: {e:#}"
            ))
        }
    }
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

/// Export the reader's notes (those `filter` asks for) as one Markdown file
/// per book, with their verses quoted in `translation_code` (or, with
/// `layout` Commentary, as a commentary), into a folder they choose with
/// the native folder dialog. Files already there are replaced only once the
/// reader says so (`confirm_notes_export_by_book`).
#[tauri::command]
pub async fn export_notes_by_book<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: tauri::State<'_, AppState>,
    translation_code: String,
    filter: NotesFilter,
    layout: Option<NotesLayout>,
) -> Result<NotesFolderOutcome, String> {
    use tauri_plugin_dialog::DialogExt;
    let files = build_notes_export_by_book_as(
        &state,
        layout.unwrap_or_default(),
        &translation_code,
        &filter,
        chrono::Utc::now(),
        &chrono::Local,
    )
    .await?;
    let dialog = app
        .dialog()
        .file()
        .set_title("Export notes: choose a folder for one file per book");
    let Some(folder) = run_dialog(move || dialog.blocking_pick_folder()).await? else {
        return Ok(NotesFolderOutcome::Cancelled);
    };
    notes_folder_chosen(&state, files, folder).await
}

/// The reader's Replace (`replace`) or Cancel for the folder export
/// `token`, which found some of its files already there.
#[tauri::command]
pub async fn confirm_notes_export_by_book(
    state: tauri::State<'_, AppState>,
    token: u64,
    replace: bool,
) -> Result<NotesFolderOutcome, String> {
    confirm_notes_folder(&state, token, replace).await
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
/// their verses quoted in `translation_code` (or, with `layout`
/// Commentary, as a commentary), to a file they choose with the native save
/// dialog. Dates are in the computer's time zone.
#[tauri::command]
pub async fn export_notes<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: tauri::State<'_, AppState>,
    translation_code: String,
    filter: NotesFilter,
    layout: Option<NotesLayout>,
) -> Result<NotesExportOutcome, String> {
    use tauri_plugin_dialog::DialogExt;
    // Built before the dialog opens, so a problem is reported without
    // asking for a file name first.
    let file = build_notes_export_as(
        &state,
        layout.unwrap_or_default(),
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

/// The notes `filter` asks for as a commentary, for the page to print (the
/// same as the commentary file says), in the computer's time zone.
#[tauri::command]
pub async fn get_notes_commentary(
    state: tauri::State<'_, AppState>,
    filter: NotesFilter,
) -> Result<Commentary, String> {
    notes_commentary(&state, &filter, chrono::Utc::now(), &chrono::Local).await
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

    #[tokio::test]
    async fn one_file_per_book_writes_each_books_section_and_asks_before_replacing() {
        use crate::notes_export::NotesFilter;
        let (db, state) = app_state("cmd_notes_export_by_book").await;
        let tz = chrono::FixedOffset::west_opt(6 * 3600).unwrap();
        let now = chrono::DateTime::parse_from_rfc3339("2026-10-12T18:00:00Z")
            .unwrap()
            .to_utc();
        for (book_id, chapter, verse, body) in [
            (43, 3, 16, "Loved."),
            (19, 23, 1, "Shepherd."),
            (43, 1, 1, "Word."),
            (62, 1, 9, "Confess."),
        ] {
            let passage = Passage {
                book_id,
                chapter,
                verse_start: verse,
                verse_end: verse,
            };
            note_add(&state, passage, body).await.unwrap();
        }
        let every = NotesFilter::default();
        let files = build_notes_export_by_book(&state, "kjv", &every, now, &tz)
            .await
            .unwrap();
        assert_eq!(
            files
                .iter()
                .map(|f| (f.file_name.as_str(), f.notes))
                .collect::<Vec<_>>(),
            [("19 Psalms.md", 1), ("43 John.md", 2), ("62 1 John.md", 1)]
        );
        // Each is that book's export alone, byte for byte.
        for file in &files {
            let id: i64 = file.file_name[..2].parse().unwrap();
            let alone = NotesFilter {
                books: vec![id],
                ..NotesFilter::default()
            };
            let single = build_notes_export(&state, "kjv", &alone, now, &tz)
                .await
                .unwrap();
            assert_eq!(file.contents, single.contents, "{}", file.file_name);
        }
        // The filters hold: one book's chapters, dates.
        let john_3 = NotesFilter {
            books: vec![43],
            chapters: Some((3, 3)),
            ..NotesFilter::default()
        };
        let one = build_notes_export_by_book(&state, "kjv", &john_3, now, &tz)
            .await
            .unwrap();
        assert_eq!(one.len(), 1);
        assert!(one[0].contents.starts_with("# Notes on John 3\n"));
        assert!(one[0].contents.contains("Loved.") && !one[0].contents.contains("Word."));
        assert_eq!(
            build_notes_export_by_book(
                &state,
                "kjv",
                &NotesFilter {
                    books: vec![1],
                    ..NotesFilter::default()
                },
                now,
                &tz
            )
            .await
            .unwrap_err(),
            "No notes match these filters."
        );

        // An empty folder: every file written, said how many.
        let folder = db.path().with_file_name("vault");
        std::fs::create_dir_all(&folder).unwrap();
        let outcome = notes_folder_chosen(&state, files.clone(), folder.clone())
            .await
            .unwrap();
        assert!(matches!(
            &outcome,
            NotesFolderOutcome::Saved { files: 3, notes: 4, folder: shown } if *shown == folder.display().to_string()
        ));
        assert_eq!(serde_json::to_value(&outcome).unwrap()["status"], "saved");
        for file in &files {
            assert_eq!(
                std::fs::read(folder.join(&file.file_name)).unwrap(),
                file.contents.as_bytes()
            );
        }

        // Some there already: nothing written until the reader answers.
        std::fs::write(folder.join("43 John.md"), "my own edits").unwrap();
        std::fs::remove_file(folder.join("19 Psalms.md")).unwrap();
        let outcome = notes_folder_chosen(&state, files.clone(), folder.clone())
            .await
            .unwrap();
        let NotesFolderOutcome::Exists {
            token, existing, ..
        } = &outcome
        else {
            panic!("expected a question, got {outcome:?}");
        };
        assert_eq!(existing, &["43 John.md", "62 1 John.md"]);
        assert_eq!(serde_json::to_value(&outcome).unwrap()["status"], "exists");
        assert!(!folder.join("19 Psalms.md").exists(), "nothing written yet");
        assert_eq!(
            std::fs::read_to_string(folder.join("43 John.md")).unwrap(),
            "my own edits"
        );
        // Cancel: still nothing written, and the question is gone.
        assert!(matches!(
            confirm_notes_folder(&state, *token, false).await,
            Ok(NotesFolderOutcome::Cancelled)
        ));
        assert!(!folder.join("19 Psalms.md").exists());
        assert_eq!(
            std::fs::read_to_string(folder.join("43 John.md")).unwrap(),
            "my own edits"
        );
        assert_eq!(
            confirm_notes_folder(&state, *token, true)
                .await
                .unwrap_err(),
            "That export isn\u{2019}t waiting any more. Export the notes again."
        );
        // Asked again, Replace: every file written.
        let NotesFolderOutcome::Exists { token, .. } =
            notes_folder_chosen(&state, files.clone(), folder.clone())
                .await
                .unwrap()
        else {
            panic!("expected a question");
        };
        assert!(
            confirm_notes_folder(&state, token + 1, true).await.is_err(),
            "another token"
        );
        assert!(matches!(
            confirm_notes_folder(&state, token, true).await,
            Ok(NotesFolderOutcome::Saved { files: 3, .. })
        ));
        assert_eq!(
            std::fs::read(folder.join("43 John.md")).unwrap(),
            files[1].contents.as_bytes()
        );
        assert!(folder.join("19 Psalms.md").exists());

        // A folder that can't be written says so.
        let missing = folder.join("not-there").join("deeper");
        let failed = notes_folder_chosen(&state, files, missing)
            .await
            .unwrap_err();
        assert!(
            failed.starts_with("The files couldn't be saved: "),
            "{failed}"
        );
    }

    #[tokio::test]
    async fn the_commentary_has_no_bible_text_and_print_shows_the_same() {
        let (_db, state) = app_state("cmd_notes_commentary").await;
        let tz = chrono::FixedOffset::west_opt(6 * 3600).unwrap();
        let now = chrono::DateTime::parse_from_rfc3339("2026-10-10T05:00:00Z")
            .unwrap()
            .to_utc();
        let all = NotesFilter::default();
        assert_eq!(
            build_notes_commentary(&state, &all, now, &tz)
                .await
                .unwrap_err(),
            "There are no notes to export."
        );
        let at = |book_id, chapter, verse_start, verse_end| Passage {
            book_id,
            chapter,
            verse_start,
            verse_end,
        };
        for (passage, body) in [
            (
                at(43, 3, 16, 18),
                "<b>Typed</b> HTML.\n\nA second paragraph.",
            ),
            (at(43, 3, 16, 16), "# Not a heading\n- not a list"),
            (at(43, 1, 1, 1), "In the beginning."),
        ] {
            note_add(&state, passage, body).await.unwrap();
        }
        let file = build_notes_commentary(&state, &all, now, &tz)
            .await
            .unwrap();
        assert_eq!(file.file_name, "gospel-getter-commentary-2026-10-09.md");
        assert_eq!(file.notes, 3);
        assert_eq!(
            file.contents,
            "# Gospel Getter commentary\n\
             \n\
             Exported Oct 9, 2026 \u{b7} 3 notes\n\
             \n\
             ## John\n\
             \n\
             ### John 1\n\
             \n\
             1:1  In the beginning.\n\
             \n\
             ### John 3\n\
             \n\
             3:16  \\# Not a heading\\\n\\- not a list\n\
             \n\
             3:16\u{2013}18  \\<b\\>Typed\\</b\\> HTML.\n\nA second paragraph.\n"
        );
        // No verse is quoted, in any translation.
        let john_3_16 = &bundled("kjv")[42].chapters[2][15];
        assert!(!file.contents.contains(&john_3_16[..20]));
        // What the page prints is the same commentary.
        let printed = notes_commentary(&state, &all, now, &tz).await.unwrap();
        assert_eq!(printed.title, "Gospel Getter commentary");
        assert_eq!(printed.books.len(), 1);
        assert_eq!(
            printed.books[0].chapters[1].notes[1].body,
            "<b>Typed</b> HTML.\n\nA second paragraph."
        );
        // The quoted layout is as it was, whichever way it's asked for.
        let quoted = build_notes_export(&state, "kjv", &all, now, &tz)
            .await
            .unwrap();
        let as_quoted = build_notes_export_as(&state, NotesLayout::Quoted, "kjv", &all, now, &tz)
            .await
            .unwrap();
        assert_eq!(quoted, as_quoted);
        let by_book =
            build_notes_export_by_book_as(&state, NotesLayout::Commentary, "kjv", &all, now, &tz)
                .await
                .unwrap();
        assert_eq!(by_book.len(), 1);
        assert_eq!(by_book[0].file_name, "43 John commentary.md");
        // A filter nothing matches says so.
        let romans = NotesFilter {
            books: vec![45],
            ..NotesFilter::default()
        };
        assert_eq!(
            notes_commentary(&state, &romans, now, &tz)
                .await
                .unwrap_err(),
            "No notes match these filters."
        );
    }
}
