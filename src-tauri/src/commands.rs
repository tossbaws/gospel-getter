use anyhow::Context;
use serde::Serialize;
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::db::notes::grouped;
use crate::db::reader_data::ApplyImportError;
use crate::db::{
    self, Book, BookmarkError, BookmarkInTranslation, HighlightColor,
    HighlightPassageInTranslation, Note, NoteError, NoteLimit, Passage, Translation,
};
use crate::domain::Store;
use crate::domain::bible::{self, ChapterRef};
use crate::domain::query::{self, Interpretation};
use crate::reader_data::{
    self, BookmarkRecord, HighlightRecord, NewBookmark, NewPosition, NoteRecord, PositionRecord,
    Preferences, ReaderData, ValidatedImport,
};

/// How many text-search results one `search` call returns.
pub const SEARCH_PAGE_SIZE: i64 = 50;

/// Application state, `.manage()`d by Tauri and injected into every command
/// via `tauri::State`.
pub struct AppState {
    pub store: Store,
    /// All 66 books, loaded once at startup. Small and immutable, so it's
    /// kept in memory rather than re-queried on every command — both for
    /// rendering the book list and for computing chapter navigation.
    pub books: Vec<Book>,
    /// All bundled translations, loaded once at startup. `translations[0]`
    /// is the default used when a command doesn't specify one.
    pub translations: Vec<Translation>,
    /// Progress of the search index build, which runs in the background
    /// after startup (see `main.rs`) rather than holding up the window.
    pub search_index: Arc<SearchIndexStatus>,
    /// The import file the reader is previewing, validated and waiting for
    /// them to confirm (or cancel). Only one at a time; choosing another
    /// file replaces it.
    pub pending_import: Mutex<Option<PendingImport>>,
    next_import_token: AtomicU64,
}

impl AppState {
    /// Load the books and translations that stay in memory for the life of
    /// the app. The database must already be migrated and seeded.
    pub async fn load(pool: SqlitePool) -> anyhow::Result<Self> {
        let books = db::get_all_books(&pool)
            .await
            .context("Failed to load books")?;
        let translations = db::get_all_translations(&pool)
            .await
            .context("Failed to load translations")?;
        Ok(Self {
            store: Store::new(pool),
            books,
            translations,
            search_index: Arc::default(),
            pending_import: Mutex::new(None),
            next_import_token: AtomicU64::new(1),
        })
    }
}

/// Whether the search index is ready to use.
#[derive(Debug, Default)]
pub struct SearchIndexStatus(AtomicU8);

impl SearchIndexStatus {
    const BUILDING: u8 = 0;
    const READY: u8 = 1;
    const FAILED: u8 = 2;

    pub fn set_ready(&self) {
        self.0.store(Self::READY, Ordering::Release);
    }

    pub fn set_failed(&self) {
        self.0.store(Self::FAILED, Ordering::Release);
    }

    /// `None` when ready, otherwise what to tell the reader.
    fn unavailable_reason(&self) -> Option<&'static str> {
        match self.0.load(Ordering::Acquire) {
            Self::READY => None,
            Self::BUILDING => Some(
                "Search is still being prepared \u{2014} this only happens once. Try again in a moment.",
            ),
            _ => Some(
                "Search isn't available: its index couldn't be built. Restart the app to try again.",
            ),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BookDto {
    pub id: i64,
    pub name: String,
    pub chapter_count: i64,
}

impl From<&Book> for BookDto {
    fn from(b: &Book) -> Self {
        Self {
            id: b.id,
            name: b.name.clone(),
            chapter_count: b.chapter_count,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationDto {
    pub code: String,
    pub name: String,
}

impl From<&Translation> for TranslationDto {
    fn from(t: &Translation) -> Self {
        Self {
            code: t.code.clone(),
            name: t.name.clone(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HomeDto {
    pub ot_books: Vec<BookDto>,
    pub nt_books: Vec<BookDto>,
    pub translations: Vec<TranslationDto>,
    pub current_translation_code: String,
    pub current_book_id: i64,
    pub current_chapter: i64,
    /// Whether a reading position was saved before this call, i.e. the
    /// app has been used to read before. The first-run welcome shows only
    /// when it hasn't. If the position couldn't be read, this is `true`,
    /// so an existing reader never gets the welcome by mistake.
    pub has_saved_position: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChapterEntryDto {
    pub number: i64,
    pub is_active: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChaptersDto {
    pub book_id: i64,
    pub book_name: String,
    pub chapters: Vec<ChapterEntryDto>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct XrefDto {
    pub book_id: i64,
    pub chapter: i64,
    pub verse: i64,
    pub end_verse: Option<i64>,
    pub citation: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerseDto {
    pub number: i64,
    pub text: String,
    pub xrefs: Vec<XrefDto>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SimpleVerseDto {
    pub number: i64,
    pub text: String,
}

/// A neighboring chapter shown in full, at reduced opacity, beside the one
/// being read. Only its heading is a clickable jump-to-here control — the
/// verse text itself stays plain, selectable text.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChapterSideDto {
    pub book_id: i64,
    pub chapter: i64,
    pub book_name: String,
    pub verses: Vec<SimpleVerseDto>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadingPaneDto {
    pub current_book_id: i64,
    pub current_chapter: i64,
    pub current_book_name: String,
    pub current_verses: Vec<VerseDto>,
    pub prev: Option<ChapterSideDto>,
    pub next: Option<ChapterSideDto>,
    pub has_chapter_xrefs: bool,
}

fn book_name(state: &AppState, book_id: i64) -> Option<&str> {
    state
        .books
        .iter()
        .find(|b| b.id == book_id)
        .map(|b| b.name.as_str())
}

/// Render a cross-reference's target passage as a human-readable citation,
/// e.g. "Romans 8:28" or "Romans 8:28-30" for a verse range.
fn build_citation(
    state: &AppState,
    ref_book_id: i64,
    ref_chapter: i64,
    ref_verse: i64,
    ref_end_verse: Option<i64>,
) -> String {
    let name = book_name(state, ref_book_id).unwrap_or("?");
    match ref_end_verse {
        Some(end) if end != ref_verse => format!("{name} {ref_chapter}:{ref_verse}-{end}"),
        _ => format!("{name} {ref_chapter}:{ref_verse}"),
    }
}

/// Resolve a translation code to a translation id, falling back to the
/// first bundled translation (the default) when the code is absent or
/// doesn't match anything we have.
fn resolve_translation(state: &AppState, code: Option<&str>) -> i64 {
    code.and_then(|c| state.translations.iter().find(|t| t.code == c))
        .or(state.translations.first())
        .map(|t| t.id)
        .unwrap_or(1)
}

/// Build the chapter grid for one book, optionally marking one chapter
/// active. Shared by `get_home`'s initial position and the `get_chapters`
/// command, so there's one source of truth instead of the two drifting
/// apart.
pub fn build_chapters_dto(
    state: &AppState,
    book_id: i64,
    active: Option<i64>,
) -> Option<ChaptersDto> {
    let book = state.books.iter().find(|b| b.id == book_id)?;
    let chapters = (1..=book.chapter_count)
        .map(|n| ChapterEntryDto {
            number: n,
            is_active: active == Some(n),
        })
        .collect();

    Some(ChaptersDto {
        book_id,
        book_name: book.name.clone(),
        chapters,
    })
}

/// Build the full text of a neighboring chapter, to render at reduced
/// opacity beside the one being read.
async fn build_side(
    state: &AppState,
    translation_id: i64,
    at: ChapterRef,
) -> anyhow::Result<ChapterSideDto> {
    let book_name = book_name(state, at.book_id)
        .ok_or_else(|| anyhow::anyhow!("unknown book id in navigation"))?
        .to_string();
    let verses = state
        .store
        .chapter_verses(translation_id, at.book_id, at.chapter)
        .await?;

    Ok(ChapterSideDto {
        book_id: at.book_id,
        chapter: at.chapter,
        book_name,
        verses: verses
            .into_iter()
            .map(|v| SimpleVerseDto {
                number: v.verse,
                text: v.text,
            })
            .collect(),
    })
}

/// Build the reading pane for one chapter of one translation: the chapter
/// itself, plus the full text of the chapter immediately before and after
/// it (crossing book boundaries as needed), so the caller can render it at
/// reduced opacity for a sense of the surrounding text while scrolling.
async fn build_reading_pane_dto(
    state: &AppState,
    translation_id: i64,
    book_id: i64,
    chapter: i64,
) -> anyhow::Result<ReadingPaneDto> {
    let current = ChapterRef { book_id, chapter };

    let prev_ref = bible::previous_chapter(&state.books, current);
    let next_ref = bible::next_chapter(&state.books, current);

    let prev = match prev_ref {
        Some(r) => Some(build_side(state, translation_id, r).await?),
        None => None,
    };
    let next = match next_ref {
        Some(r) => Some(build_side(state, translation_id, r).await?),
        None => None,
    };

    let current_book_name = book_name(state, book_id)
        .ok_or_else(|| anyhow::anyhow!("unknown book id in navigation"))?
        .to_string();

    let cross_refs = state
        .store
        .chapter_cross_references(book_id, chapter)
        .await?;
    let mut xrefs_by_verse: HashMap<i64, Vec<XrefDto>> = HashMap::new();
    for r in cross_refs {
        xrefs_by_verse.entry(r.verse).or_default().push(XrefDto {
            book_id: r.ref_book_id,
            chapter: r.ref_chapter,
            verse: r.ref_verse,
            end_verse: r.ref_end_verse,
            citation: build_citation(
                state,
                r.ref_book_id,
                r.ref_chapter,
                r.ref_verse,
                r.ref_end_verse,
            ),
        });
    }
    let has_chapter_xrefs = !xrefs_by_verse.is_empty();

    let current_verses = state
        .store
        .chapter_verses(translation_id, book_id, chapter)
        .await?
        .into_iter()
        .map(|v| VerseDto {
            number: v.verse,
            text: v.text,
            xrefs: xrefs_by_verse.remove(&v.verse).unwrap_or_default(),
        })
        .collect();

    Ok(ReadingPaneDto {
        current_book_id: book_id,
        current_chapter: chapter,
        current_book_name,
        current_verses,
        prev,
        next,
        has_chapter_xrefs,
    })
}

/// This build's version (the crate's, which `tauri.conf.json` tracks), shown
/// at the foot of the Settings menu.
pub fn app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[tauri::command]
pub fn get_app_version() -> &'static str {
    app_version()
}

/// Initial state for the home screen: the full book list, the translation
/// picker, and whichever chapter the reader was last on — falling back to
/// Genesis 1 in the default translation if nothing has ever been read yet.
/// The frontend turns around and calls `get_chapters`/`get_reading` with
/// this position to actually populate the page, the same as any other
/// navigation, so there's only one code path for rendering a chapter.
#[tauri::command]
pub async fn get_home(state: tauri::State<'_, AppState>) -> Result<HomeDto, String> {
    Ok(home(&state).await)
}

/// The logic behind `get_home`.
pub async fn home(state: &AppState) -> HomeDto {
    let ot_books = state
        .books
        .iter()
        .filter(|b| b.testament == "OT")
        .map(BookDto::from)
        .collect();
    let nt_books = state
        .books
        .iter()
        .filter(|b| b.testament == "NT")
        .map(BookDto::from)
        .collect();
    let translations = state
        .translations
        .iter()
        .map(TranslationDto::from)
        .collect();

    let default_translation_id = resolve_translation(state, None);
    let ((translation_id, book_id, chapter), has_saved_position) =
        match state.store.reading_position().await {
            Ok(Some(position)) => (position, true),
            Ok(None) => ((default_translation_id, 1, 1), false),
            Err(e) => {
                tracing::error!("Failed to load reading position: {e}");
                ((default_translation_id, 1, 1), true)
            }
        };
    let current_translation_code = state
        .translations
        .iter()
        .find(|t| t.id == translation_id)
        .map(|t| t.code.clone())
        .unwrap_or_default();

    HomeDto {
        ot_books,
        nt_books,
        translations,
        current_translation_code,
        current_book_id: book_id,
        current_chapter: chapter,
        has_saved_position,
    }
}

/// Chapter grid for one book — shown when a book is clicked, and used
/// again (with `active`) to keep the grid in sync when arrow-key
/// navigation crosses into a different book. Chapter counts don't vary by
/// translation, so this doesn't need to know which one is selected.
#[tauri::command]
pub fn get_chapters(
    state: tauri::State<'_, AppState>,
    book_id: i64,
    active: Option<i64>,
) -> Option<ChaptersDto> {
    build_chapters_dto(&state, book_id, active)
}

/// The reading pane for one chapter — used by chapter clicks, arrow-key
/// navigation, and switching translations. Also remembers this as the
/// reader's current position, so reopening the app returns here.
#[tauri::command]
pub async fn get_reading(
    state: tauri::State<'_, AppState>,
    book_id: i64,
    chapter: i64,
    translation_code: Option<String>,
) -> Result<Option<ReadingPaneDto>, String> {
    Ok(reading(&state, book_id, chapter, translation_code.as_deref()).await)
}

/// The logic behind `get_reading`.
pub async fn reading(
    state: &AppState,
    book_id: i64,
    chapter: i64,
    translation_code: Option<&str>,
) -> Option<ReadingPaneDto> {
    let translation_id = resolve_translation(state, translation_code);
    match build_reading_pane_dto(state, translation_id, book_id, chapter).await {
        Ok(dto) => {
            if let Err(e) = state
                .store
                .save_reading_position(translation_id, book_id, chapter)
                .await
            {
                tracing::warn!("Failed to save reading position: {e}");
            }
            Some(dto)
        }
        Err(e) => {
            tracing::error!("Failed to build reading pane for {book_id}:{chapter}: {e}");
            None
        }
    }
}

/// The verse text a cross-reference citation expands to, fetched on click
/// rather than embedded up front, since only a small fraction of citations
/// actually get expanded in any given reading session.
#[tauri::command]
pub async fn get_xref_text(
    state: tauri::State<'_, AppState>,
    book_id: i64,
    chapter: i64,
    verse: i64,
    end_verse: Option<i64>,
    translation_code: Option<String>,
) -> Result<Vec<SimpleVerseDto>, String> {
    Ok(xref_text(
        &state,
        book_id,
        chapter,
        verse,
        end_verse,
        translation_code.as_deref(),
    )
    .await)
}

/// The logic behind `get_xref_text`.
pub async fn xref_text(
    state: &AppState,
    book_id: i64,
    chapter: i64,
    verse: i64,
    end_verse: Option<i64>,
    translation_code: Option<&str>,
) -> Vec<SimpleVerseDto> {
    let translation_id = resolve_translation(state, translation_code);
    let end_verse = end_verse.unwrap_or(verse);
    match state
        .store
        .verse_range(translation_id, book_id, chapter, verse, end_verse)
        .await
    {
        Ok(verses) => verses
            .into_iter()
            .map(|v| SimpleVerseDto {
                number: v.verse,
                text: v.text,
            })
            .collect(),
        Err(e) => {
            tracing::error!(
                "Failed to fetch xref verse text for {book_id} {chapter}:{verse}-{end_verse}: {e}"
            );
            Vec::new()
        }
    }
}

/// "John 3", "John 3:16" or "John 3:16–18".
fn passage_reference(book_name: &str, chapter: i64, verses: Option<(i64, i64)>) -> String {
    match verses {
        None => format!("{book_name} {chapter}"),
        Some((first, last)) if first == last => format!("{book_name} {chapter}:{first}"),
        Some((first, last)) => format!("{book_name} {chapter}:{first}\u{2013}{last}"),
    }
}

fn translation_name(state: &AppState, translation_id: i64) -> &str {
    state
        .translations
        .iter()
        .find(|t| t.id == translation_id)
        .map_or("this translation", |t| t.name.as_str())
}

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
    /// The first verse's stored text in the requested translation, or
    /// `None` if that translation doesn't number it.
    pub preview: Option<String>,
    /// Whether the requested translation numbers every verse bookmarked.
    pub complete: bool,
    pub created_at: String,
}

fn bookmark_dto(state: &AppState, b: BookmarkInTranslation) -> BookmarkDto {
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
        preview: first_verse_text,
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
    Ok(state
        .store
        .bookmarks(translation_id)
        .await?
        .into_iter()
        .map(|b| bookmark_dto(state, b))
        .collect())
}

/// A passage that couldn't be bookmarked or highlighted, in words for the
/// reader. `action` is "bookmarked" or "highlighted"; `failed` is the
/// message for a database failure.
fn passage_error_message(
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
        .map_err(|e| {
            tracing::error!("Failed to list bookmarks: {e:#}");
            "Bookmarks couldn't be loaded.".to_string()
        })
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
    state.store.remove_bookmark(id).await.map_err(|e| {
        tracing::error!("Failed to remove bookmark {id}: {e:#}");
        "The bookmark couldn't be removed.".to_string()
    })
}

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
    /// The first verse's stored text in the requested translation, or
    /// `None` if that translation doesn't number it.
    pub preview: Option<String>,
    /// Whether the requested translation numbers every verse in it.
    pub complete: bool,
}

fn highlight_passage_dto(
    state: &AppState,
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
        preview: first_verse_text,
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
    Ok(state
        .store
        .highlight_passages(translation_id)
        .await?
        .into_iter()
        .map(|h| highlight_passage_dto(state, h))
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
        .map_err(|e| {
            tracing::error!("Failed to remove highlights: {e:#}");
            "The highlight couldn't be removed.".to_string()
        })
}

#[tauri::command]
pub async fn list_highlights(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<HighlightDto>, String> {
    highlights_list(&state).await.map_err(|e| {
        tracing::error!("Failed to list highlights: {e:#}");
        "Highlights couldn't be loaded.".to_string()
    })
}

#[tauri::command]
pub async fn list_highlight_passages(
    state: tauri::State<'_, AppState>,
    translation_code: Option<String>,
) -> Result<Vec<HighlightPassageDto>, String> {
    highlight_passages_in(&state, translation_code.as_deref())
        .await
        .map_err(|e| {
            tracing::error!("Failed to list highlighted passages: {e:#}");
            "Highlights couldn't be loaded.".to_string()
        })
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
    let notes = state.store.notes_overlapping(&passage).await.map_err(|e| {
        tracing::error!("Failed to list notes: {e:#}");
        "Notes couldn't be loaded.".to_string()
    })?;
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
        .map_err(|e| {
            tracing::error!("Failed to delete note {id}: {e:#}");
            "The note couldn't be deleted.".to_string()
        })
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
    notes_list(&state).await.map_err(|e| {
        tracing::error!("Failed to list notes: {e:#}");
        "Notes couldn't be loaded.".to_string()
    })
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

// ---- Search and go-to-reference (GH-12)

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResultDto {
    pub book_id: i64,
    pub chapter: i64,
    pub verse: i64,
    /// e.g. "John 3:16".
    pub reference: String,
    /// The verse's stored text, from `verses`.
    pub text: String,
}

/// What the search box should do with the input.
#[derive(Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SearchDto {
    /// Nothing to search for.
    Empty,
    /// Go to this passage.
    Passage {
        book_id: i64,
        chapter: i64,
        verse_start: Option<i64>,
        verse_end: Option<i64>,
        reference: String,
    },
    /// A reference that doesn't exist (or can't be read); say why.
    Invalid { message: String },
    /// Search isn't ready or failed; say why.
    Unavailable { message: String },
    /// One page of matching verses, in Bible order.
    Text {
        terms: Vec<String>,
        total: i64,
        offset: i64,
        page_size: i64,
        results: Vec<SearchResultDto>,
    },
}

/// Interpret the search box input against one translation: a passage to
/// go to (checking its verses exist in that translation), or a page of
/// text-search results.
pub async fn run_search(
    state: &AppState,
    query_text: &str,
    translation_code: Option<&str>,
    offset: i64,
) -> anyhow::Result<SearchDto> {
    let translation_id = resolve_translation(state, translation_code);
    match query::interpret(query_text, &state.books) {
        Interpretation::Empty => Ok(SearchDto::Empty),
        Interpretation::InvalidReference(message) => Ok(SearchDto::Invalid { message }),
        Interpretation::Passage(p) => {
            let name = book_name(state, p.book_id).unwrap_or("?");
            if let Some((_, last)) = p.verses {
                let verses = state
                    .store
                    .chapter_verses(translation_id, p.book_id, p.chapter)
                    .await?;
                let verse_count = verses.last().map_or(0, |v| v.verse);
                if last > verse_count {
                    return Ok(SearchDto::Invalid {
                        message: format!(
                            "No such chapter or verse: {}. {name} {} has {verse_count} verses in the {}.",
                            passage_reference(name, p.chapter, p.verses),
                            p.chapter,
                            translation_name(state, translation_id),
                        ),
                    });
                }
            }
            Ok(SearchDto::Passage {
                book_id: p.book_id,
                chapter: p.chapter,
                verse_start: p.verses.map(|(first, _)| first),
                verse_end: p.verses.map(|(_, last)| last),
                reference: passage_reference(name, p.chapter, p.verses),
            })
        }
        Interpretation::Text(terms) => {
            if let Some(message) = state.search_index.unavailable_reason() {
                return Ok(SearchDto::Unavailable {
                    message: message.to_string(),
                });
            }
            let offset = offset.max(0);
            let (total, hits) = state
                .store
                .search(translation_id, &terms, SEARCH_PAGE_SIZE, offset)
                .await?;
            let results = hits
                .into_iter()
                .map(|h| SearchResultDto {
                    reference: passage_reference(
                        book_name(state, h.book_id).unwrap_or("?"),
                        h.chapter,
                        Some((h.verse, h.verse)),
                    ),
                    book_id: h.book_id,
                    chapter: h.chapter,
                    verse: h.verse,
                    text: h.text,
                })
                .collect();
            Ok(SearchDto::Text {
                terms,
                total,
                offset,
                page_size: SEARCH_PAGE_SIZE,
                results,
            })
        }
    }
}

#[tauri::command]
pub async fn search(
    state: tauri::State<'_, AppState>,
    query: String,
    translation_code: Option<String>,
    offset: Option<i64>,
) -> Result<SearchDto, String> {
    run_search(
        &state,
        &query,
        translation_code.as_deref(),
        offset.unwrap_or(0),
    )
    .await
    .map_err(|e| {
        tracing::error!("Search for {query:?} failed: {e:#}");
        "Search failed.".to_string()
    })
}

// ---- Compare translations (GH-13)

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompareColumnDto {
    pub code: String,
    pub name: String,
    /// The chapter's verses exactly as this translation stores and numbers
    /// them.
    pub verses: Vec<SimpleVerseDto>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompareDto {
    pub book_id: i64,
    pub chapter: i64,
    pub columns: Vec<CompareColumnDto>,
}

/// One chapter in every bundled translation, for showing side by side.
/// Unlike `get_reading`, it doesn't move the reading position.
pub async fn compare_chapter(
    state: &AppState,
    book_id: i64,
    chapter: i64,
) -> anyhow::Result<Option<CompareDto>> {
    let Some(book) = state.books.iter().find(|b| b.id == book_id) else {
        return Ok(None);
    };
    if !(1..=book.chapter_count).contains(&chapter) {
        return Ok(None);
    }
    let mut columns = Vec::with_capacity(state.translations.len());
    for t in &state.translations {
        let verses = state.store.chapter_verses(t.id, book_id, chapter).await?;
        columns.push(CompareColumnDto {
            code: t.code.clone(),
            name: t.name.clone(),
            verses: verses
                .into_iter()
                .map(|v| SimpleVerseDto {
                    number: v.verse,
                    text: v.text,
                })
                .collect(),
        });
    }
    Ok(Some(CompareDto {
        book_id,
        chapter,
        columns,
    }))
}

#[tauri::command]
pub async fn get_compare(
    state: tauri::State<'_, AppState>,
    book_id: i64,
    chapter: i64,
) -> Result<Option<CompareDto>, String> {
    compare_chapter(&state, book_id, chapter)
        .await
        .map_err(|e| {
            tracing::error!("Failed to load {book_id}:{chapter} for comparison: {e:#}");
            "The comparison couldn't be loaded.".to_string()
        })
}

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
    /// Distinct notes in the file.
    pub notes_in_file: usize,
    /// Note entries that repeated another one in the file (ignored).
    pub note_duplicates_in_file: usize,
    /// How many of them aren't here yet.
    pub new_notes: usize,
    /// How many are here already (the same passage and text; merge skips
    /// those).
    pub notes_already_present: usize,
    /// How many notes there are here now (what replacing removes).
    pub current_notes: usize,
    pub position: Option<PositionPreviewDto>,
    pub preferences: Preferences,
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

fn write_atomically(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    use std::io::Write;
    let name = path
        .file_name()
        .with_context(|| format!("{} isn't a file name", path.display()))?
        .to_string_lossy()
        .into_owned();
    let dir = match path.parent() {
        Some(d) if !d.as_os_str().is_empty() => d.to_path_buf(),
        _ => PathBuf::from("."),
    };
    let write = || -> std::io::Result<()> {
        let mut temp = create_temp_file(&dir, temp_names(&name))?;
        let file = temp.file.as_mut().expect("open until persisted");
        file.write_all(bytes)?;
        file.sync_all()?;
        temp.persist_as(path)
    };
    write().with_context(|| format!("Couldn't write {}", path.display()))
}

/// A newly created temporary file, deleted when dropped unless it was
/// renamed into place with `persist_as`.
pub(crate) struct TempFile {
    file: Option<std::fs::File>,
    path: Option<PathBuf>,
}

impl TempFile {
    /// Close it and rename it over `target` (replacing any file there).
    fn persist_as(mut self, target: &Path) -> std::io::Result<()> {
        drop(self.file.take());
        let path = self.path.as_ref().expect("not yet persisted");
        std::fs::rename(path, target)?;
        self.path = None;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn path(&self) -> &Path {
        self.path.as_deref().expect("not yet persisted")
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        // Closed first: Windows can't delete a file that's still open.
        drop(self.file.take());
        if let Some(path) = self.path.take() {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// Hidden candidate names beside `name`, unique to this process, this
/// moment and each attempt, so simultaneous saves never share one.
fn temp_names(name: &str) -> impl Iterator<Item = String> + '_ {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    (0..64).map(move |_| {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        format!(".{name}.{pid}-{nanos:x}-{n:x}.tmp")
    })
}

/// Create a brand-new file in `dir` with the first of `names` not already
/// taken. An existing file is never opened, truncated or removed: a name
/// that exists is skipped.
pub(crate) fn create_temp_file(
    dir: &Path,
    names: impl IntoIterator<Item = String>,
) -> std::io::Result<TempFile> {
    for name in names {
        let path = dir.join(name);
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(file) => {
                return Ok(TempFile {
                    file: Some(file),
                    path: Some(path),
                });
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "no unused temporary file name was found",
    ))
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
    let noted = state.store.note_keys().await?;
    let file_notes = import.notes.as_deref().unwrap_or_default();
    let new_notes = file_notes
        .iter()
        .filter(|n| !noted.contains(&n.key()))
        .count();
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
        current_notes: noted.len(),
        position: import.position.as_ref().map(PositionPreviewDto::from),
        preferences: import.preferences,
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

/// Runs a blocking native file dialog off the async runtime's worker
/// threads, and turns its answer into a path (`None` if cancelled).
async fn run_dialog(
    show: impl FnOnce() -> Option<tauri_plugin_dialog::FilePath> + Send + 'static,
) -> Result<Option<PathBuf>, String> {
    let picked = tokio::task::spawn_blocking(show)
        .await
        .map_err(|e| format!("The file dialog failed: {e}"))?;
    picked
        .map(|p| {
            p.into_path()
                .map_err(|e| format!("That location can't be used: {e}"))
        })
        .transpose()
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
    let data = build_reader_data(&state, preferences).await.map_err(|e| {
        tracing::error!("Failed to gather data to export: {e:#}");
        "Your data couldn't be read for export.".to_string()
    })?;
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
    prepare_import(&state, &path).await.map_err(|e| {
        tracing::error!("Failed to check import file: {e:#}");
        "The file couldn't be checked against your Bible data.".to_string()
    })
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

/// The reader's notes as Markdown, with each note's verses quoted in the
/// translation `translation_code`: every note, or only `book_id`'s. Dates
/// (the notes', the export's and the file name's) are in `tz`. Each chapter
/// a note is on is read once, however many notes it has.
pub async fn build_notes_export<Tz: chrono::TimeZone>(
    state: &AppState,
    translation_code: &str,
    book_id: Option<i64>,
    now: chrono::DateTime<chrono::Utc>,
    tz: &Tz,
) -> Result<NotesFile, String> {
    let translation = state
        .translations
        .iter()
        .find(|t| t.code == translation_code)
        .ok_or_else(|| format!("There's no translation \u{201c}{translation_code}\u{201d}."))?;
    let book = book_id
        .map(|id| {
            state
                .books
                .iter()
                .find(|b| b.id == id && (1..=66).contains(&id))
                .ok_or_else(|| format!("There's no book {id}."))
        })
        .transpose()?;
    const FAILED: &str = "Your notes couldn't be read for export.";
    let all = state.store.notes().await.map_err(|e| {
        tracing::error!("Failed to read notes to export: {e:#}");
        FAILED.to_string()
    })?;
    let notes: Vec<Note> = all
        .into_iter()
        .filter(|n| book_id.is_none_or(|b| n.book_id == b))
        .collect();
    if notes.is_empty() {
        return Err("There are no notes to export.".to_string());
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
            .map_err(|e| {
                tracing::error!("Failed to read verses to export: {e:#}");
                FAILED.to_string()
            })?;
        verses.insert(key, chapter);
    }
    let export = crate::notes_export::NotesExport {
        notes: &notes,
        books: &state.books,
        verses: &verses,
        translation_code: &translation.code,
        translation_name: &translation.name,
        book: book_id,
        exported_at: now,
    };
    Ok(NotesFile {
        file_name: crate::notes_export::file_name(
            book.map(|b| b.name.as_str()),
            crate::notes_export::local_date(now, tz),
        ),
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

/// Export the reader's notes (all of them, or one book's) as Markdown, with
/// their verses quoted in `translation_code`, to a file they choose with
/// the native save dialog. Dates are in the computer's time zone.
#[tauri::command]
pub async fn export_notes<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: tauri::State<'_, AppState>,
    translation_code: String,
    book_id: Option<i64>,
) -> Result<NotesExportOutcome, String> {
    use tauri_plugin_dialog::DialogExt;
    // Built before the dialog opens, so a problem is reported without
    // asking for a file name first.
    let file = build_notes_export(
        &state,
        &translation_code,
        book_id,
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
    use crate::db::test_support::{bundled, fresh_database};

    async fn app_state(label: &str) -> (crate::db::test_support::TempDb, AppState) {
        let (db, pool) = fresh_database(label).await;
        let state = AppState::load(pool).await.expect("load state");
        state.search_index.set_ready();
        (db, state)
    }

    async fn search_in(state: &AppState, query: &str, code: &str) -> serde_json::Value {
        let dto = run_search(state, query, Some(code), 0)
            .await
            .expect("search");
        serde_json::to_value(dto).expect("serialize")
    }

    #[tokio::test]
    async fn search_goes_to_references_checked_against_the_translation() {
        let (_db, state) = app_state("cmd_search_ref").await;

        let v = search_in(&state, "jn 3:16-18", "kjv").await;
        assert_eq!(
            v,
            serde_json::json!({
                "kind": "passage", "bookId": 43, "chapter": 3,
                "verseStart": 16, "verseEnd": 18, "reference": "John 3:16\u{2013}18"
            })
        );
        let v = search_in(&state, "Psalm 23", "web").await;
        assert_eq!(v["kind"], "passage");
        assert_eq!(v["verseStart"], serde_json::Value::Null);
        assert_eq!(v["reference"], "Psalms 23");

        // Matthew 2:23 is numbered in the WEB only.
        assert_eq!(
            search_in(&state, "Matthew 2:23", "web").await["kind"],
            "passage"
        );
        assert_eq!(
            search_in(&state, "Matthew 2:23", "kjv").await,
            serde_json::json!({
                "kind": "invalid",
                "message": "No such chapter or verse: Matthew 2:23. Matthew 2 has 22 verses in the King James Version."
            })
        );
        assert_eq!(
            search_in(&state, "John 22", "kjv").await["message"],
            "No such chapter or verse: John 22. John has 21 chapters."
        );
        assert_eq!(search_in(&state, "   ", "kjv").await["kind"], "empty");
    }

    #[tokio::test]
    async fn text_search_pages_through_stored_text() {
        let (_db, state) = app_state("cmd_search_text").await;

        let v = search_in(&state, "Faith, hope & charity", "kjv").await;
        assert_eq!(v["kind"], "text");
        assert_eq!(v["terms"], serde_json::json!(["faith", "hope", "charity"]));
        let results = v["results"].as_array().unwrap();
        let cor = results
            .iter()
            .find(|r| r["reference"] == "1 Corinthians 13:13")
            .expect("1 Corinthians 13:13 found");
        assert_eq!(
            cor["text"].as_str().unwrap().as_bytes(),
            bundled("kjv")[45].chapters[12][12].as_bytes()
        );

        let first = run_search(&state, "love", Some("web"), 0).await.unwrap();
        let second = run_search(&state, "love", Some("web"), SEARCH_PAGE_SIZE)
            .await
            .unwrap();
        let (
            SearchDto::Text {
                total, results: a, ..
            },
            SearchDto::Text {
                offset, results: b, ..
            },
        ) = (first, second)
        else {
            panic!("expected text results");
        };
        assert!(total > SEARCH_PAGE_SIZE);
        assert_eq!(a.len() as i64, SEARCH_PAGE_SIZE);
        assert_eq!(offset, SEARCH_PAGE_SIZE);
        assert_ne!(a[0].reference, b[0].reference);
    }

    #[tokio::test]
    async fn text_search_waits_for_the_index() {
        let (db, pool) = fresh_database("cmd_search_wait").await;
        let state = AppState::load(pool).await.unwrap();
        let v = serde_json::to_value(run_search(&state, "love", Some("kjv"), 0).await.unwrap())
            .unwrap();
        assert_eq!(v["kind"], "unavailable");
        // Going to a reference doesn't need the index.
        assert_eq!(
            search_in(&state, "John 3:16", "kjv").await["kind"],
            "passage"
        );
        state.search_index.set_failed();
        let v = serde_json::to_value(run_search(&state, "love", Some("kjv"), 0).await.unwrap())
            .unwrap();
        assert_eq!(v["kind"], "unavailable");
        drop(db);
    }

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
            async move { build_notes_export(state, code, book, now, &tz).await }
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
            "There are no notes to export."
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
        let kjv = build_notes_export(&state, "kjv", None, now, &tz)
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
        let web = build_notes_export(&state, "web", None, now, &tz)
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
    async fn compare_returns_each_translations_own_numbering_and_text() {
        let (_db, state) = app_state("cmd_compare").await;

        // Romans 14: 23 verses in the KJV, 26 in the WEB (which places the
        // doxology the KJV has at 16:25-27 here, as 14:24-26).
        let dto = compare_chapter(&state, 45, 14).await.unwrap().unwrap();
        assert_eq!(dto.columns.len(), 2);
        for (column, code) in dto.columns.iter().zip(["kjv", "web"]) {
            assert_eq!(column.code, code);
            let stored = &bundled(code)[44].chapters[13];
            assert_eq!(column.verses.len(), stored.len());
            for (i, (verse, text)) in column.verses.iter().zip(stored).enumerate() {
                assert_eq!(verse.number, i as i64 + 1);
                assert_eq!(verse.text.as_bytes(), text.as_bytes());
            }
        }
        assert_eq!(dto.columns[0].verses.len(), 23);
        assert_eq!(dto.columns[1].verses.len(), 26);

        assert!(compare_chapter(&state, 43, 22).await.unwrap().is_none());
        assert!(compare_chapter(&state, 99, 1).await.unwrap().is_none());

        // Comparing doesn't move the saved reading position.
        assert_eq!(state.store.reading_position().await.unwrap(), None);
    }
}
