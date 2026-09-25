use serde::Serialize;
use std::collections::HashMap;

use crate::db::{Book, Translation};
use crate::domain::Store;
use crate::domain::bible::{self, ChapterRef};

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
fn build_chapters_dto(state: &AppState, book_id: i64, active: Option<i64>) -> Option<ChaptersDto> {
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

    let default_translation_id = resolve_translation(&state, None);
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

    Ok(HomeDto {
        ot_books,
        nt_books,
        translations,
        current_translation_code,
        current_book_id: book_id,
        current_chapter: chapter,
    })
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
    let translation_id = resolve_translation(&state, translation_code.as_deref());
    match build_reading_pane_dto(&state, translation_id, book_id, chapter).await {
        Ok(dto) => {
            if let Err(e) = state
                .store
                .save_reading_position(translation_id, book_id, chapter)
                .await
            {
                tracing::warn!("Failed to save reading position: {e}");
            }
            Ok(Some(dto))
        }
        Err(e) => {
            tracing::error!("Failed to build reading pane for {book_id}:{chapter}: {e}");
            Ok(None)
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
    let translation_id = resolve_translation(&state, translation_code.as_deref());
    let end_verse = end_verse.unwrap_or(verse);
    match state
        .store
        .verse_range(translation_id, book_id, chapter, verse, end_verse)
        .await
    {
        Ok(verses) => Ok(verses
            .into_iter()
            .map(|v| SimpleVerseDto {
                number: v.verse,
                text: v.text,
            })
            .collect()),
        Err(e) => {
            tracing::error!(
                "Failed to fetch xref verse text for {book_id} {chapter}:{verse}-{end_verse}: {e}"
            );
            Ok(Vec::new())
        }
    }
}
