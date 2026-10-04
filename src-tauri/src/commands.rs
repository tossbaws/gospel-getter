use anyhow::Context;
use serde::Serialize;
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

use crate::db::{self, Book, BookmarkError, BookmarkInTranslation, Translation};
use crate::domain::Store;
use crate::domain::bible::{self, ChapterRef};
use crate::domain::query::{self, Interpretation};

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
    let (translation_id, book_id, chapter) = match state.store.reading_position().await {
        Ok(Some(position)) => position,
        Ok(None) => (default_translation_id, 1, 1),
        Err(e) => {
            tracing::error!("Failed to load reading position: {e}");
            (default_translation_id, 1, 1)
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

/// Bookmark a passage, returning its id (the existing one if it was
/// already bookmarked), or a message for the reader.
pub async fn bookmark_passage(
    state: &AppState,
    book_id: i64,
    chapter: i64,
    verse_start: i64,
    verse_end: i64,
) -> Result<i64, String> {
    let name = book_name(state, book_id).unwrap_or("that book");
    match state
        .store
        .add_bookmark(book_id, chapter, verse_start, verse_end)
        .await
    {
        Ok(bookmark) => Ok(bookmark.id),
        Err(BookmarkError::InvalidRange { .. }) => Err(format!(
            "{verse_start}\u{2013}{verse_end} isn't a verse range that can be bookmarked."
        )),
        Err(BookmarkError::NoSuchChapter { .. }) => Err(format!("There's no {name} {chapter}.")),
        Err(BookmarkError::NoSuchVerse {
            verse, last_verse, ..
        }) => Err(format!(
            "There's no {name} {chapter}:{verse}; the chapter ends at verse {last_verse}."
        )),
        Err(BookmarkError::Database(e)) => {
            tracing::error!("Failed to add bookmark: {e:#}");
            Err("The bookmark couldn't be saved.".to_string())
        }
    }
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
