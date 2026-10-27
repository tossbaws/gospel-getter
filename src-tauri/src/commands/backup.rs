//! Export and import of the reader's own data (bookmarks, highlights,
//! notes, the reading position and display preferences): the export file,
//! the preview of a file to import, and applying it.

use anyhow::Context;
use serde::Serialize;
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::Ordering;

use super::{AppState, app_version, book_name, failed, run_dialog, write_atomically};
use crate::db::notes::grouped;
use crate::db::reader_data::ApplyImportError;
use crate::db::{self, NoteLimit};
use crate::passage_reference::passage_reference;
use crate::reader_data::{
    self, BookmarkRecord, HighlightRecord, NewBookmark, NewPosition, NoteRecord, PositionRecord,
    Preferences, ReaderData, ValidatedImport,
};

// ---- Export and import the reader's own data

/// An import the reader is previewing: the file, already validated against
/// this install, waiting for them to confirm. Applying it uses exactly what
/// was previewed, even if the file has changed on disk since.
pub struct PendingImport {
    token: u64,
    import: ValidatedImport,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PositionPreviewDto {
    pub translation_code: String,
    pub translation_name: String,
    pub book_id: i64,
    pub book_name: String,
    pub chapter: i64,
    /// e.g. "Romans 8 (WEB)".
    pub reference: String,
}

impl From<&NewPosition> for PositionPreviewDto {
    fn from(p: &NewPosition) -> Self {
        Self {
            reference: format!(
                "{} {} ({})",
                p.book_name,
                p.chapter,
                p.translation_code.to_uppercase()
            ),
            translation_code: p.translation_code.clone(),
            translation_name: p.translation_name.clone(),
            book_id: p.book_id,
            book_name: p.book_name.clone(),
            chapter: p.chapter,
        }
    }
}

/// How an export ended.
#[derive(Debug, Serialize)]
#[serde(
    tag = "status",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ExportOutcome {
    /// The reader closed the save dialog; no file was written.
    Cancelled,
    /// The file was completely written.
    Saved {
        path: String,
        bookmarks: usize,
        highlights: usize,
        notes: usize,
        has_position: bool,
        preferences: usize,
    },
}

/// What's in an import file, and what importing it would do.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreviewDto {
    pub token: u64,
    pub file_name: String,
    pub exported_at: String,
    pub app_version: String,
    /// Distinct bookmarks in the file.
    pub bookmarks_in_file: usize,
    /// Entries in the file that repeated another one (ignored).
    pub duplicates_in_file: usize,
    /// How many of the file's bookmarks aren't bookmarked here yet.
    pub new_bookmarks: usize,
    /// How many are already bookmarked here (merge leaves those as they are).
    pub already_present: usize,
    /// How many bookmarks there are here now (what replacing would remove).
    pub current_bookmarks: usize,
    /// The first few of the file's bookmarks, e.g. "John 3:16–18".
    pub sample: Vec<String>,
    /// Whether the file's format can hold highlights (format 2 on). A
    /// format 1 file never touches the reader's highlights, even when
    /// replacing.
    pub has_highlights: bool,
    /// Distinct highlighted verses in the file.
    pub highlights_in_file: usize,
    /// Highlight entries that repeated a verse already in the file (ignored).
    pub highlight_duplicates_in_file: usize,
    /// How many of them are for verses not highlighted here yet.
    pub new_highlights: usize,
    /// How many are for verses already highlighted here (merge keeps the
    /// reader's own color on those).
    pub already_highlighted: usize,
    /// How many verses are highlighted here now (what replacing removes).
    pub current_highlights: usize,
    /// Whether the file's format can hold notes (format 3 on). An older
    /// file never touches the reader's notes, even when replacing.
    pub has_notes: bool,
    /// The notes in the file, less repeats (`reader_data::validate`).
    pub notes_in_file: usize,
    /// Note entries without a date that repeated another one in the file
    /// (ignored).
    pub note_duplicates_in_file: usize,
    /// How many of them aren't here yet.
    pub new_notes: usize,
    /// How many are here already (the same passage, text and date; merge
    /// skips those: `reader_data::notes_to_add`).
    pub notes_already_present: usize,
    /// How many notes there are here now (what replacing removes).
    pub current_notes: usize,
    pub position: Option<PositionPreviewDto>,
    pub preferences: Preferences,
    /// The display preferences in the file this version can't apply (a
    /// theme added later, say), each named for the reader. They're
    /// skipped; everything else imports.
    pub skipped_preferences: Vec<String>,
    /// What to tell the reader about them, if any were skipped.
    pub skipped_preferences_notice: Option<String>,
}

/// The result of choosing a file to import.
#[derive(Debug, Serialize)]
#[serde(
    tag = "status",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ImportChoice {
    /// The reader closed the open dialog.
    Cancelled,
    /// The file can't be imported; nothing was changed.
    Invalid {
        file_name: String,
        message: String,
        problems: Vec<String>,
    },
    /// Valid; here's what it would do. Nothing has been changed yet.
    Ready { preview: Box<ImportPreviewDto> },
}

/// What a confirmed import did.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResultDto {
    pub added: usize,
    pub already_present: usize,
    pub removed: usize,
    pub highlights_added: usize,
    pub highlights_already_present: usize,
    pub highlights_removed: usize,
    pub notes_added: usize,
    pub notes_already_present: usize,
    pub notes_removed: usize,
    /// The reading position saved, if the reader chose to restore it.
    pub position: Option<PositionPreviewDto>,
}

fn passage_reference_for(state: &AppState, b: &NewBookmark) -> String {
    passage_reference(
        book_name(state, b.book_id).unwrap_or("?"),
        b.chapter,
        Some((b.verse_start, b.verse_end)),
    )
}

/// The reader's data as an export file: bookmarks (oldest first),
/// highlights and notes (in Bible order), the saved reading position, and
/// the display `preferences` the frontend passed in.
pub async fn build_reader_data(
    state: &AppState,
    preferences: Preferences,
) -> anyhow::Result<ReaderData> {
    let bookmarks = state.store.all_bookmarks().await?;
    let highlights = state.store.highlights().await?;
    let notes = state.store.notes().await?;
    let position = state.store.reading_position().await?;
    let reading_position = position.and_then(|(translation_id, book_id, chapter)| {
        let translation = state.translations.iter().find(|t| t.id == translation_id)?;
        Some(PositionRecord {
            translation: translation.code.clone(),
            book: book_id,
            book_name: book_name(state, book_id).map(str::to_string),
            chapter,
        })
    });
    Ok(ReaderData {
        format: reader_data::FORMAT.to_string(),
        format_version: reader_data::FORMAT_VERSION,
        exported_at: state.store.now().await?,
        app_version: app_version().to_string(),
        reading_position,
        bookmarks: bookmarks
            .into_iter()
            .map(|b| BookmarkRecord {
                book_name: book_name(state, b.book_id).map(str::to_string),
                book: b.book_id,
                chapter: b.chapter,
                verse_start: b.verse_start,
                verse_end: b.verse_end,
                created_at: Some(b.created_at),
            })
            .collect(),
        highlights: highlights
            .into_iter()
            .map(|h| HighlightRecord {
                book_name: book_name(state, h.book_id).map(str::to_string),
                book: h.book_id,
                chapter: h.chapter,
                verse: h.verse,
                color: h.color,
                created_at: Some(h.created_at),
                updated_at: Some(h.updated_at),
            })
            .collect(),
        notes: notes
            .into_iter()
            .map(|n| NoteRecord {
                book_name: book_name(state, n.book_id).map(str::to_string),
                book: n.book_id,
                chapter: n.chapter,
                verse_start: n.verse_start,
                verse_end: n.verse_end,
                body: n.body,
                created_at: Some(n.created_at),
                updated_at: Some(n.updated_at),
            })
            .collect(),
        preferences,
        skipped_preferences: Vec::new(),
    })
}

/// Write `data` to `path`, completely or not at all: it's written to a
/// temporary file beside `path`, flushed to disk, then renamed over it, so
/// a failure never leaves a half-written export (or clobbers an existing
/// file with one).
pub async fn save_reader_data(data: &ReaderData, path: &Path) -> anyhow::Result<ExportOutcome> {
    let contents =
        reader_data::to_file_contents(data).context("The export couldn't be converted to JSON")?;
    let target = path.to_path_buf();
    tokio::task::spawn_blocking(move || write_atomically(&target, contents.as_bytes()))
        .await
        .context("The save was interrupted")??;
    Ok(ExportOutcome::Saved {
        path: path.display().to_string(),
        bookmarks: data.bookmarks.len(),
        highlights: data.highlights.len(),
        notes: data.notes.len(),
        has_position: data.reading_position.is_some(),
        preferences: data.preferences.count(),
    })
}

/// Read at most `MAX_FILE_BYTES + 1` bytes, so an oversized file is
/// reported as too large without reading all of it.
async fn read_bounded(path: &Path) -> std::io::Result<Vec<u8>> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        use std::io::Read;
        let mut bytes = Vec::new();
        std::fs::File::open(&path)?
            .take(reader_data::MAX_FILE_BYTES + 1)
            .read_to_end(&mut bytes)?;
        Ok(bytes)
    })
    .await
    .map_err(std::io::Error::other)?
}

/// Read and check an import file and work out what importing it would do.
/// Nothing is changed: a valid file becomes the pending import, waiting
/// for `apply_pending_import` (or `discard_pending_import`).
pub async fn prepare_import(state: &AppState, path: &Path) -> anyhow::Result<ImportChoice> {
    let file_name = path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    let bytes = match read_bounded(path).await {
        Ok(bytes) => bytes,
        Err(e) => {
            return Ok(ImportChoice::Invalid {
                file_name,
                message: format!("The file couldn't be read: {e}."),
                problems: Vec::new(),
            });
        }
    };
    let last_verses = state.store.last_verses().await?;
    let validated = reader_data::parse(&bytes).and_then(|data| {
        reader_data::validate(data, &state.books, &state.translations, &last_verses)
    });
    let import = match validated {
        Ok(import) => import,
        Err(e) => {
            return Ok(ImportChoice::Invalid {
                file_name,
                message: e.to_string(),
                problems: e.problems().to_vec(),
            });
        }
    };

    let existing = state.store.bookmark_coordinates().await?;
    let new_bookmarks = import
        .bookmarks
        .iter()
        .filter(|b| !existing.contains(&b.coordinates()))
        .count();
    let highlighted = state.store.highlight_coordinates().await?;
    let file_highlights = import.highlights.as_deref().unwrap_or_default();
    let new_highlights = file_highlights
        .iter()
        .filter(|h| !highlighted.contains(&h.coordinates()))
        .count();
    let stored_notes = state.store.stored_notes().await?;
    let file_notes = import.notes.as_deref().unwrap_or_default();
    let new_notes = reader_data::notes_to_add(file_notes, &stored_notes).len();
    let preview = ImportPreviewDto {
        token: state.next_import_token.fetch_add(1, Ordering::Relaxed),
        file_name,
        exported_at: import.exported_at.clone(),
        app_version: import.app_version.clone(),
        bookmarks_in_file: import.bookmarks.len(),
        duplicates_in_file: import.duplicates_in_file,
        new_bookmarks,
        already_present: import.bookmarks.len() - new_bookmarks,
        current_bookmarks: existing.len(),
        sample: import
            .bookmarks
            .iter()
            .take(5)
            .map(|b| passage_reference_for(state, b))
            .collect(),
        has_highlights: import.highlights.is_some(),
        highlights_in_file: file_highlights.len(),
        highlight_duplicates_in_file: import.highlight_duplicates_in_file,
        new_highlights,
        already_highlighted: file_highlights.len() - new_highlights,
        current_highlights: highlighted.len(),
        has_notes: import.notes.is_some(),
        notes_in_file: file_notes.len(),
        note_duplicates_in_file: import.note_duplicates_in_file,
        new_notes,
        notes_already_present: file_notes.len() - new_notes,
        current_notes: stored_notes.len(),
        position: import.position.as_ref().map(PositionPreviewDto::from),
        preferences: import.preferences.clone(),
        skipped_preferences_notice: reader_data::skipped_preferences_notice(
            &import.skipped_preferences,
        ),
        skipped_preferences: import.skipped_preferences.clone(),
    };
    *lock(&state.pending_import) = Some(PendingImport {
        token: preview.token,
        import,
    });
    Ok(ImportChoice::Ready {
        preview: Box::new(preview),
    })
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Apply the pending import the reader previewed (`token`), in one
/// database transaction. `replace` swaps every current bookmark (and, for
/// a file that can hold them, every highlight and note) for the file's; otherwise
/// they're merged. `restore_position` also saves the
/// file's reading position. Display preferences live in the webview and
/// are applied there, after this succeeds.
pub async fn apply_pending_import(
    state: &AppState,
    token: u64,
    replace: bool,
    restore_position: bool,
) -> Result<ImportResultDto, String> {
    // Cloned out so the lock isn't held across the database work.
    let import = match lock(&state.pending_import).as_ref() {
        Some(pending) if pending.token == token => pending.import.clone(),
        _ => {
            return Err(
                "That import is no longer waiting to be confirmed. Choose the file again."
                    .to_string(),
            );
        }
    };
    let position = import.position.as_ref().filter(|_| restore_position);
    let counts = state
        .store
        .apply_import(
            &import.bookmarks,
            import.highlights.as_deref(),
            import.notes.as_deref(),
            replace,
            position,
        )
        .await
        .map_err(|e| match e {
            ApplyImportError::NoteLimit(limit) => import_limit_message(limit),
            ApplyImportError::Database(e) => {
                tracing::error!("Import failed: {e:#}");
                "The import failed, so nothing was changed.".to_string()
            }
        })?;
    discard_pending_import(state, token);
    Ok(ImportResultDto {
        added: counts.added,
        already_present: counts.already_present,
        removed: counts.removed,
        highlights_added: counts.highlights_added,
        highlights_already_present: counts.highlights_already_present,
        highlights_removed: counts.highlights_removed,
        notes_added: counts.notes_added,
        notes_already_present: counts.notes_already_present,
        notes_removed: counts.notes_removed,
        position: position.map(PositionPreviewDto::from),
    })
}

/// Why an import that would go past a limit on notes was refused.
fn import_limit_message(limit: NoteLimit) -> String {
    match limit {
        NoteLimit::Count => format!(
            "Importing this file would give you more than {} notes, the most Gospel Getter \
             keeps, so nothing was changed.",
            grouped(db::notes::MAX_NOTES)
        ),
        NoteLimit::TotalText => format!(
            "Importing this file would take your notes past their total size limit ({} \
             characters), so nothing was changed.",
            grouped(db::notes::MAX_TOTAL_NOTE_CHARS)
        ),
    }
}

/// Forget the pending import, if it's still `token`.
pub fn discard_pending_import(state: &AppState, token: u64) {
    let mut pending = lock(&state.pending_import);
    if pending.as_ref().is_some_and(|p| p.token == token) {
        *pending = None;
    }
}

fn reader_data_dialog<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> tauri_plugin_dialog::FileDialogBuilder<R> {
    use tauri_plugin_dialog::DialogExt;
    app.dialog().file().add_filter(
        "Gospel Getter bookmarks, highlights, notes and settings",
        &["json"],
    )
}

/// Export the reader's bookmarks, highlights, notes, reading position and
/// display preferences to a file they choose with the native save dialog.
#[tauri::command]
pub async fn export_reader_data<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: tauri::State<'_, AppState>,
    preferences: Preferences,
) -> Result<ExportOutcome, String> {
    // Gathered before the dialog opens, so a failure is reported without
    // asking for a file name first.
    let data = build_reader_data(&state, preferences)
        .await
        .context("Failed to gather data to export")
        .map_err(failed("Your data couldn't be read for export."))?;
    let suggested = format!(
        "gospel-getter-{}{}",
        data.exported_at.get(..10).unwrap_or("export"),
        reader_data::FILE_SUFFIX
    );
    let dialog = reader_data_dialog(&app)
        .set_title("Export bookmarks, highlights, notes and settings")
        .set_file_name(suggested);
    let Some(path) = run_dialog(move || dialog.blocking_save_file()).await? else {
        return Ok(ExportOutcome::Cancelled);
    };
    save_reader_data(&data, &path).await.map_err(|e| {
        tracing::error!("Export failed: {e:#}");
        format!("The file couldn't be saved: {e:#}")
    })
}

/// Choose a file to import with the native open dialog, and preview it.
/// Changes nothing.
#[tauri::command]
pub async fn choose_import_file<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: tauri::State<'_, AppState>,
) -> Result<ImportChoice, String> {
    let dialog =
        reader_data_dialog(&app).set_title("Import bookmarks, highlights, notes and settings");
    let Some(path) = run_dialog(move || dialog.blocking_pick_file()).await? else {
        return Ok(ImportChoice::Cancelled);
    };
    prepare_import(&state, &path)
        .await
        .context("Failed to check import file")
        .map_err(failed(
            "The file couldn't be checked against your Bible data.",
        ))
}

/// Apply the previewed import (see `apply_pending_import`).
#[tauri::command]
pub async fn apply_import(
    state: tauri::State<'_, AppState>,
    token: u64,
    replace: bool,
    restore_position: bool,
) -> Result<ImportResultDto, String> {
    apply_pending_import(&state, token, replace, restore_position).await
}

/// Forget a previewed import the reader cancelled.
#[tauri::command]
pub fn cancel_import(state: tauri::State<'_, AppState>, token: u64) {
    discard_pending_import(&state, token);
}
