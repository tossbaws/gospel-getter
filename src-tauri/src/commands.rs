//! The commands the page calls with `invoke`, and the state they share
//! (`AppState`). Each `#[tauri::command]` is a thin wrapper over a function
//! taking `&AppState` (`get_reading` over `reading`), which the frontend
//! tests' bridge (`examples/frontend_bridge.rs`) calls directly. A failure
//! reaches the page as a sentence for the reader; its cause is logged.

use anyhow::Context;
use serde::Serialize;
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
use crate::domain::kjv_markup::{PartKind, VerseText};
use crate::domain::query::{self, Interpretation};
use crate::notes_export::NotesFilter;
use crate::passage_copy::{self, CopyFormat};
use crate::reader_data::{
    self, BookmarkRecord, HighlightRecord, NewBookmark, NewPosition, NoteRecord, PositionRecord,
    Preferences, ReaderData, ValidatedImport,
};
use crate::start_here::{StartHereDto, start_here};

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
    /// All bundled translations, loaded once at startup, in the order Aa
    /// lists them (`seed::TRANSLATIONS`). `translations[0]` (the KJV) is
    /// the default used when a command doesn't specify one.
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
    pub async fn load(database: db::Db) -> anyhow::Result<Self> {
        let books = db::get_all_books(database.read())
            .await
            .context("Failed to load books")?;
        let mut translations = db::get_all_translations(database.read())
            .await
            .context("Failed to load translations")?;
        translations.sort_by_key(|t| db::seed::rank(&t.code));
        Ok(Self {
            store: Store::new(database),
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
    /// When it was made, as Aa's chooser shows it ("1611/1769"); `None`
    /// when there's no one date (see `seed::TRANSLATIONS`).
    pub date: Option<String>,
    /// "public domain".
    pub licence: String,
    /// Its line in About's credits.
    pub credit: String,
}

impl From<&Translation> for TranslationDto {
    fn from(t: &Translation) -> Self {
        let source = db::seed::source(&t.code);
        Self {
            code: t.code.clone(),
            name: t.name.clone(),
            date: source.and_then(|s| s.date).map(str::to_string),
            licence: source.map_or("", |s| s.licence).to_string(),
            credit: source.map_or_else(|| t.name.clone(), |s| s.credit.to_string()),
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
    /// The welcome card's places to start reading (see `start_here`).
    pub start_here: Vec<StartHereDto>,
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

/// A verse of the chapter being read: as shown (see `VerseText`), with its
/// cross-references.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerseDto {
    pub number: i64,
    /// `text` (plain, for copying), `parts` and `footnotes`.
    #[serde(flatten)]
    pub shown: VerseText,
    pub xrefs: Vec<XrefDto>,
}

/// A verse as shown (see `VerseText`).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SimpleVerseDto {
    pub number: i64,
    /// `text` (plain, for copying), `parts` and `footnotes`.
    #[serde(flatten)]
    pub shown: VerseText,
}

/// A verse as shown: with its footnotes, where its translation keeps them
/// apart from the text (the BSB's), or else from its stored text (the
/// KJV's markup is read there; see `VerseText`).
fn shown(translation_code: &str, verse: &db::Verse) -> VerseText {
    if verse.footnotes.is_empty() {
        VerseText::new(translation_code, &verse.text)
    } else {
        VerseText::with_footnotes(&verse.text, &verse.footnotes)
    }
}

impl SimpleVerseDto {
    fn new(translation_code: &str, verse: db::Verse) -> Self {
        Self {
            number: verse.verse,
            shown: shown(translation_code, &verse),
        }
    }

    /// The verse as shown, without its margin notes: no note parts and no
    /// footnotes, the supplied words still marked. `text` is unchanged
    /// (it never had the notes).
    fn without_notes(translation_code: &str, verse: db::Verse) -> Self {
        let mut dto = Self::new(translation_code, verse);
        dto.shown.parts.retain(|p| p.kind != PartKind::Note);
        dto.shown.footnotes.clear();
        dto
    }
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

/// A quiet card in a neighbor's place, where there's no neighboring
/// chapter: before Genesis 1 ("The Old Testament", "Genesis · 50
/// chapters") and after Revelation 22 ("The end of the Bible"). Words
/// about the Bible's shape, never scripture text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BibleEdgeDto {
    pub title: String,
    pub detail: Option<String>,
}

impl BibleEdgeDto {
    /// Before the first chapter of `book`: its testament, and the book with
    /// its chapter count.
    fn start(book: &Book) -> Self {
        let testament = if book.testament == "NT" {
            "The New Testament"
        } else {
            "The Old Testament"
        };
        let chapters = if book.chapter_count == 1 {
            "chapter"
        } else {
            "chapters"
        };
        Self {
            title: testament.to_owned(),
            detail: Some(format!("{} · {} {chapters}", book.name, book.chapter_count)),
        }
    }

    /// After the last chapter.
    fn end() -> Self {
        Self {
            title: "The end of the Bible".to_owned(),
            detail: None,
        }
    }
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
    /// What shows in the previous chapter's place when there's none (at
    /// Genesis 1), and in the next's (at Revelation 22).
    pub prev_edge: Option<BibleEdgeDto>,
    pub next_edge: Option<BibleEdgeDto>,
    /// Chapter 1 of the books before and after, where `[` and `]` go;
    /// `None` in Genesis and in Revelation.
    pub prev_book: Option<BookStartDto>,
    pub next_book: Option<BookStartDto>,
    pub has_chapter_xrefs: bool,
}

/// A book's first chapter, named.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BookStartDto {
    pub book_id: i64,
    pub chapter: i64,
    pub book_name: String,
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

/// The code of the translation `translation_id` ("kjv", "web"), which
/// decides how its verses are shown (see `VerseText`).
fn code_of(state: &AppState, translation_id: i64) -> &str {
    state
        .translations
        .iter()
        .find(|t| t.id == translation_id)
        .map_or("", |t| t.code.as_str())
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
    let code = code_of(state, translation_id);

    Ok(ChapterSideDto {
        book_id: at.book_id,
        chapter: at.chapter,
        book_name,
        verses: verses
            .into_iter()
            .map(|v| SimpleVerseDto::new(code, v))
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

    let current_book = state
        .books
        .iter()
        .find(|b| b.id == book_id)
        .ok_or_else(|| anyhow::anyhow!("unknown book id in navigation"))?;
    let current_book_name = current_book.name.clone();
    let book_start = |r: ChapterRef| BookStartDto {
        book_id: r.book_id,
        chapter: r.chapter,
        book_name: book_name(state, r.book_id).unwrap_or("?").to_owned(),
    };
    let prev_book = bible::previous_book(&state.books, current).map(book_start);
    let next_book = bible::next_book(&state.books, current).map(book_start);
    let prev_edge = prev.is_none().then(|| BibleEdgeDto::start(current_book));
    let next_edge = next.is_none().then(BibleEdgeDto::end);

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

    let code = code_of(state, translation_id);
    let current_verses = state
        .store
        .chapter_verses(translation_id, book_id, chapter)
        .await?
        .into_iter()
        .map(|v| VerseDto {
            number: v.verse,
            shown: shown(code, &v),
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
        prev_edge,
        next_edge,
        prev_book,
        next_book,
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
        start_here: start_here(&state.books),
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
/// navigation, and switching translations. It saves nothing: the page
/// saves the chapter it puts on screen with `save_reading_position`.
#[tauri::command]
pub async fn get_reading(
    state: tauri::State<'_, AppState>,
    book_id: i64,
    chapter: i64,
    translation_code: Option<String>,
) -> Result<Option<ReadingPaneDto>, String> {
    reading(&state, book_id, chapter, translation_code.as_deref()).await
}

/// The logic behind `get_reading`. `None` when there's no such chapter (a
/// book the Bible doesn't have, or a chapter past its book's end); an
/// error, as a sentence for the reader, when the chapter is there but
/// couldn't be read.
pub async fn reading(
    state: &AppState,
    book_id: i64,
    chapter: i64,
    translation_code: Option<&str>,
) -> Result<Option<ReadingPaneDto>, String> {
    let Some(book) = state.books.iter().find(|b| b.id == book_id) else {
        return Ok(None);
    };
    if !(1..=book.chapter_count).contains(&chapter) {
        return Ok(None);
    }
    let translation_id = resolve_translation(state, translation_code);
    let dto = build_reading_pane_dto(state, translation_id, book_id, chapter)
        .await
        .map_err(|e| {
            tracing::error!("Failed to build reading pane for {book_id}:{chapter}: {e:#}");
            format!("{} {chapter} couldn't be loaded.", book.name)
        })?;
    Ok(Some(dto))
}

/// Remember a chapter as the reader's position, so reopening the app
/// returns there. The page calls this once it has put the chapter on
/// screen (`get_reading` saves nothing, so an answer the page asked for
/// and then dropped never moves the position), and sends one at a time,
/// in the order it showed them, so the chapter shown last is the one
/// kept. With two copies of the app open, the last save from either is.
#[tauri::command]
pub async fn save_reading_position(
    state: tauri::State<'_, AppState>,
    book_id: i64,
    chapter: i64,
    translation_code: String,
) -> Result<(), String> {
    remember_position(&state, book_id, chapter, &translation_code).await
}

/// The logic behind `save_reading_position`: an error, as a sentence for
/// the reader, for a translation, book or chapter that isn't there, or a
/// position that couldn't be saved.
pub async fn remember_position(
    state: &AppState,
    book_id: i64,
    chapter: i64,
    translation_code: &str,
) -> Result<(), String> {
    let translation = state
        .translations
        .iter()
        .find(|t| t.code == translation_code)
        .ok_or_else(|| format!("There's no translation \"{translation_code}\"."))?;
    let book = state
        .books
        .iter()
        .find(|b| b.id == book_id)
        .ok_or_else(|| format!("There's no book {book_id}."))?;
    if !(1..=book.chapter_count).contains(&chapter) {
        return Err(format!("{} has no chapter {chapter}.", book.name));
    }
    state
        .store
        .save_reading_position(translation.id, book_id, chapter)
        .await
        .map_err(failed("The reading position couldn't be saved."))
}

/// A passage to read in place: a cross-reference previewed in the
/// selection card. Its verses are the ones `translation_code` numbers in
/// `start..=end`, shown as the reader shows them but without margin-note
/// markers (a preview has no footnotes); `notice` says which of them it
/// doesn't number, when any.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PassageDto {
    pub book_id: i64,
    pub chapter: i64,
    pub start: i64,
    pub end: i64,
    /// "1 John 4:9–10".
    pub reference: String,
    pub translation_code: String,
    pub verses: Vec<SimpleVerseDto>,
    /// e.g. "Romans 16:25–27 isn't numbered in the World English Bible —
    /// Romans 16 ends at verse 24 there."; `None` when every verse is.
    pub notice: Option<String>,
}

/// One passage of one translation, fetched when a cross-reference is
/// previewed rather than embedded up front, since few are.
#[tauri::command]
pub async fn get_passage(
    state: tauri::State<'_, AppState>,
    translation_code: String,
    book_id: i64,
    chapter: i64,
    start: i64,
    end: i64,
) -> Result<PassageDto, String> {
    passage_text(&state, &translation_code, book_id, chapter, start, end).await
}

/// The logic behind `get_passage`. A translation, book or chapter that
/// doesn't exist, or verses that aren't a range from 1 up, are an error;
/// verses past the chapter's end (in this translation) aren't, as they're
/// numbered in another translation. Nor are verses past the end in every
/// translation (a few cross references reach past it): the notice names
/// them like any others, but they're never counted one by one, so `end`
/// sizes no loop and no list.
pub async fn passage_text(
    state: &AppState,
    translation_code: &str,
    book_id: i64,
    chapter: i64,
    start: i64,
    end: i64,
) -> Result<PassageDto, String> {
    let translation = state
        .translations
        .iter()
        .find(|t| t.code == translation_code)
        .ok_or_else(|| format!("There's no translation \"{translation_code}\"."))?;
    let book = state
        .books
        .iter()
        .find(|b| b.id == book_id)
        .ok_or_else(|| format!("There's no book {book_id}."))?;
    if !(1..=book.chapter_count).contains(&chapter) {
        return Err(format!("{} has no chapter {chapter}.", book.name));
    }
    if start < 1 || end < start {
        return Err(format!("Verses {start} to {end} aren't a passage."));
    }
    let unreadable = |e: anyhow::Error| {
        tracing::error!("Failed to read {} {chapter}: {e}", book.name);
        format!("{} {chapter} couldn't be read.", book.name)
    };
    let chapter_verses = state
        .store
        .chapter_verses(translation.id, book_id, chapter)
        .await
        .map_err(unreadable)?;
    let last_anywhere = state
        .store
        .chapter_last_verse(book_id, chapter)
        .await
        .map_err(unreadable)?;
    let last = chapter_verses.last().map_or(0, |v| v.verse);
    let verses: Vec<SimpleVerseDto> = chapter_verses
        .into_iter()
        .filter(|v| (start..=end).contains(&v.verse))
        .map(|v| SimpleVerseDto::without_notes(&translation.code, v))
        .collect();
    let numbered: Vec<i64> = verses.iter().map(|v| v.number).collect();
    let missing = missing_runs(&numbered, start, end, last_anywhere);
    let notice = (!missing.is_empty()).then(|| {
        let what = passage_reference(&book.name, chapter, Some((start, end)));
        let (first_missing, _) = missing[0];
        let (_, last_missing) = missing[missing.len() - 1];
        if first_missing > last {
            let missing =
                passage_reference(&book.name, chapter, Some((first_missing, last_missing)));
            format!(
                "{missing} isn't numbered in the {} — {} {chapter} ends at verse {last} there.",
                translation.name, book.name
            )
        } else {
            let (noun, verb) = if first_missing == last_missing {
                ("verse", "isn't")
            } else {
                ("verses", "aren't")
            };
            format!(
                "In {what}, {noun} {} {verb} numbered in the {}.",
                verse_list(&missing),
                translation.name
            )
        }
    });
    Ok(PassageDto {
        book_id,
        chapter,
        start,
        end,
        reference: passage_reference(&book.name, chapter, Some((start, end))),
        translation_code: translation.code.clone(),
        verses,
        notice,
    })
}

/// A passage in every Copy as… format (see `passage_copy`), fetched
/// whenever verses are selected or a chapter shown, so a copy can be
/// written at once, within the click or key that asked for it.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CopyTextDto {
    /// "John 3:16–18", or "John 3" for a whole chapter: what the status
    /// line says was copied.
    pub reference: String,
    pub translation_code: String,
    pub formats: CopyFormatsDto,
}

/// The passage as each format of `passage_copy::CopyFormat` has it.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CopyFormatsDto {
    pub reference: String,
    pub numbered: String,
    pub markdown: String,
    pub plain: String,
}

/// A passage to copy: verses `start..=end` of a chapter, or, without them,
/// the whole chapter, in `translation_code`.
#[tauri::command]
pub async fn get_copy_text(
    state: tauri::State<'_, AppState>,
    translation_code: String,
    book_id: i64,
    chapter: i64,
    start: Option<i64>,
    end: Option<i64>,
) -> Result<CopyTextDto, String> {
    copy_text(&state, &translation_code, book_id, chapter, start, end).await
}

/// The logic behind `get_copy_text`. A translation, book or chapter that
/// doesn't exist, verses that aren't a range from 1 up, or a passage with
/// no verse the translation numbers, is an error. Verses it doesn't number
/// inside the range are skipped, and the reference still names the range.
pub async fn copy_text(
    state: &AppState,
    translation_code: &str,
    book_id: i64,
    chapter: i64,
    start: Option<i64>,
    end: Option<i64>,
) -> Result<CopyTextDto, String> {
    let translation = state
        .translations
        .iter()
        .find(|t| t.code == translation_code)
        .ok_or_else(|| format!("There's no translation \"{translation_code}\"."))?;
    let book = state
        .books
        .iter()
        .find(|b| b.id == book_id)
        .ok_or_else(|| format!("There's no book {book_id}."))?;
    if !(1..=book.chapter_count).contains(&chapter) {
        return Err(format!("{} has no chapter {chapter}.", book.name));
    }
    let range = match (start, end) {
        (None, None) => None,
        (Some(start), Some(end)) if start >= 1 && end >= start => Some((start, end)),
        _ => return Err("Those verses aren't a passage.".to_string()),
    };
    let verses: Vec<db::Verse> = state
        .store
        .chapter_verses(translation.id, book_id, chapter)
        .await
        .map_err(|e| {
            tracing::error!("Failed to read {} {chapter}: {e}", book.name);
            format!("{} {chapter} couldn't be read.", book.name)
        })?
        .into_iter()
        .filter(|v| range.is_none_or(|(start, end)| (start..=end).contains(&v.verse)))
        .collect();
    let reference = passage_reference(&book.name, chapter, range);
    if verses.is_empty() {
        return Err(format!(
            "{reference} isn't numbered in the {}.",
            translation.name
        ));
    }
    let citation = format!("{reference} ({})", translation.code.to_uppercase());
    let passage = passage_copy::Passage {
        translation_code: &translation.code,
        citation: &citation,
        verses: &verses,
    };
    Ok(CopyTextDto {
        reference,
        translation_code: translation.code.clone(),
        formats: CopyFormatsDto {
            reference: passage.render(CopyFormat::Reference),
            numbered: passage.render(CopyFormat::Numbered),
            markdown: passage.render(CopyFormat::Markdown),
            plain: passage.render(CopyFormat::Plain),
        },
    })
}

/// The verses from `start` to `end` that aren't in `numbered`, in order, as
/// runs `(first, last)`. Only verses up to `last_anywhere` (the chapter's
/// last verse in any translation) are looked at one by one; those past it,
/// however far `end` goes, are one run.
fn missing_runs(numbered: &[i64], start: i64, end: i64, last_anywhere: i64) -> Vec<(i64, i64)> {
    let mut runs: Vec<(i64, i64)> = Vec::new();
    let mut add = |first: i64, last: i64| match runs.last_mut() {
        Some((_, run_end)) if *run_end + 1 == first => *run_end = last,
        _ => runs.push((first, last)),
    };
    for n in start..=end.min(last_anywhere) {
        if !numbered.contains(&n) {
            add(n, n);
        }
    }
    if end > last_anywhere {
        add(start.max(last_anywhere + 1), end);
    }
    runs
}

/// "4", "4, 6" or "4–6, 9": runs of verse numbers, a run of more than one
/// as a range.
fn verse_list(runs: &[(i64, i64)]) -> String {
    runs.iter()
        .map(|&(a, b)| {
            if a == b {
                a.to_string()
            } else {
                format!("{a}\u{2013}{b}")
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
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
    /// The first verse's plain text in the requested translation (see
    /// `VerseText`), or `None` if that translation doesn't number it.
    pub preview: Option<String>,
    /// Whether the requested translation numbers every verse bookmarked.
    pub complete: bool,
    pub created_at: String,
}

/// A preview's plain text (see `VerseText`).
fn preview_text(translation_code: &str, stored: Option<String>) -> Option<String> {
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

/// For `map_err` on a command's failure: logs the error with its causes
/// (give it its context first, with `anyhow::Context`) and answers with
/// `sentence`, which is what the reader is shown.
fn failed(sentence: &'static str) -> impl FnOnce(anyhow::Error) -> String {
    move |e| {
        tracing::error!("{e:#}");
        sentence.to_owned()
    }
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

// ---- Search and go-to-reference (GH-12)

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResultDto {
    pub book_id: i64,
    pub chapter: i64,
    pub verse: i64,
    /// e.g. "John 3:16".
    pub reference: String,
    /// The verse's plain text (see `VerseText`), the text search covers.
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
            let code = code_of(state, translation_id);
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
                    text: VerseText::new(code, &h.text).text,
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
    .with_context(|| format!("Search for {query:?} failed"))
    .map_err(failed("Search failed."))
}

// ---- Compare translations (GH-13)

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompareColumnDto {
    pub code: String,
    pub name: String,
    /// The chapter's verses as this translation numbers them, each as
    /// shown (see `VerseText`).
    pub verses: Vec<SimpleVerseDto>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompareDto {
    pub book_id: i64,
    pub chapter: i64,
    pub columns: Vec<CompareColumnDto>,
}

/// One chapter in the translations `codes`, in that order, for showing
/// side by side: the one being read, then the one it's compared with.
/// Unlike `get_reading`, it doesn't move the reading position. `None` for
/// a book or chapter that doesn't exist; an error for a translation that
/// doesn't, or one named twice.
pub async fn compare_chapter(
    state: &AppState,
    book_id: i64,
    chapter: i64,
    codes: &[String],
) -> anyhow::Result<Option<CompareDto>> {
    let mut translations = Vec::with_capacity(codes.len());
    for code in codes {
        let t = state
            .translations
            .iter()
            .find(|t| &t.code == code)
            .ok_or_else(|| anyhow::anyhow!("There's no translation \"{code}\"."))?;
        if translations
            .iter()
            .any(|seen: &&Translation| seen.id == t.id)
        {
            anyhow::bail!("{} is named twice.", t.name);
        }
        translations.push(t);
    }
    if translations.is_empty() {
        anyhow::bail!("No translations to compare.");
    }
    let Some(book) = state.books.iter().find(|b| b.id == book_id) else {
        return Ok(None);
    };
    if !(1..=book.chapter_count).contains(&chapter) {
        return Ok(None);
    }
    let mut columns = Vec::with_capacity(translations.len());
    for t in translations {
        let verses = state.store.chapter_verses(t.id, book_id, chapter).await?;
        columns.push(CompareColumnDto {
            code: t.code.clone(),
            name: t.name.clone(),
            verses: verses
                .into_iter()
                .map(|v| SimpleVerseDto::new(&t.code, v))
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
    translation_codes: Vec<String>,
) -> Result<Option<CompareDto>, String> {
    compare_chapter(&state, book_id, chapter, &translation_codes)
        .await
        .with_context(|| format!("Failed to load {book_id}:{chapter} for comparison"))
        .map_err(failed("The comparison couldn't be loaded."))
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
    use crate::db::test_support::{bundled, bundled_bsb, fresh_database};

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

    /// The welcome's places to start come with the home screen, named from
    /// the seeded books, and each opens a chapter that's really there.
    #[tokio::test]
    async fn home_lists_where_to_start_from_the_seeded_books() {
        let (_db, state) = app_state("cmd_start_here").await;
        let rows = home(&state).await.start_here;
        let titles: Vec<&str> = rows.iter().map(|r| r.title.as_str()).collect();
        assert_eq!(titles, ["Mark", "John", "Psalms", "Genesis 1"]);
        for row in &rows {
            let dto = reading(&state, row.book_id, row.chapter, Some("kjv"))
                .await
                .unwrap()
                .unwrap();
            assert_eq!(
                (dto.current_book_id, dto.current_chapter),
                (row.book_id, row.chapter)
            );
            assert!(
                row.title.starts_with(&dto.current_book_name),
                "{}",
                row.title
            );
        }
    }

    /// A failure answers with its sentence alone; what's logged is its
    /// context and causes, as each command logged them by hand before.
    #[test]
    fn a_failure_is_its_sentence_and_logs_its_context() {
        let e = anyhow::anyhow!("database is locked").context("Failed to list bookmarks");
        assert_eq!(
            format!("{e:#}"),
            "Failed to list bookmarks: database is locked"
        );
        assert_eq!(
            failed("Bookmarks couldn't be loaded.")(e),
            "Bookmarks couldn't be loaded."
        );
    }

    /// A chapter the Bible doesn't have is no chapter (the page stays where
    /// it is and says nothing), while one the database can't read is a
    /// sentence the reader is shown, naming it.
    #[tokio::test]
    async fn a_missing_chapter_is_none_and_a_failed_one_is_an_error() {
        let (_db, state) = app_state("cmd_reading_outcomes").await;
        for (book, chapter) in [(0, 1), (67, 1), (43, 0), (43, 22)] {
            let shown = reading(&state, book, chapter, Some("kjv")).await;
            assert!(matches!(shown, Ok(None)), "{book}:{chapter}");
        }
        state.store.close().await;
        let failed = reading(&state, 43, 4, Some("kjv")).await;
        assert_eq!(failed.err().as_deref(), Some("John 4 couldn't be loaded."));
    }

    /// Reading a chapter never moves the saved position (an answer the
    /// page drops mustn't); saving one does, for a translation, book and
    /// chapter that are there, and nothing else.
    #[tokio::test]
    async fn only_saving_a_position_moves_it() {
        let (_db, state) = app_state("cmd_reading_position").await;
        reading(&state, 43, 3, Some("web")).await.unwrap().unwrap();
        assert_eq!(state.store.reading_position().await.unwrap(), None);

        remember_position(&state, 43, 3, "web").await.unwrap();
        let web = state
            .translations
            .iter()
            .find(|t| t.code == "web")
            .unwrap()
            .id;
        assert_eq!(
            state.store.reading_position().await.unwrap(),
            Some((web, 43, 3))
        );
        reading(&state, 1, 1, Some("kjv")).await.unwrap().unwrap();
        for ((book, chapter, code), refused) in [
            ((43, 3, "niv"), "There's no translation \"niv\"."),
            ((67, 1, "kjv"), "There's no book 67."),
            ((43, 0, "kjv"), "John has no chapter 0."),
            ((43, 22, "kjv"), "John has no chapter 22."),
        ] {
            assert_eq!(
                remember_position(&state, book, chapter, code).await,
                Err(refused.to_string())
            );
        }
        assert_eq!(
            state.store.reading_position().await.unwrap(),
            Some((web, 43, 3))
        );
        state.store.close().await;
        assert_eq!(
            remember_position(&state, 45, 8, "kjv").await,
            Err("The reading position couldn't be saved.".to_string())
        );
    }

    /// A previewed range is every verse in it, in order, with its
    /// reference and translation, and nothing missing.
    #[tokio::test]
    async fn a_passage_is_every_verse_of_its_range() {
        let (_db, state) = app_state("cmd_passage_range").await;
        let dto = passage_text(&state, "web", 62, 4, 9, 10).await.unwrap();
        assert_eq!(dto.reference, "1 John 4:9\u{2013}10");
        assert_eq!(dto.translation_code, "web");
        assert_eq!(
            (dto.book_id, dto.chapter, dto.start, dto.end),
            (62, 4, 9, 10)
        );
        let web = bundled("web");
        let texts: Vec<(i64, &str)> = dto
            .verses
            .iter()
            .map(|v| (v.number, v.shown.text.as_str()))
            .collect();
        assert_eq!(
            texts,
            [
                (9, web[61].chapters[3][8].as_str()),
                (10, web[61].chapters[3][9].as_str())
            ]
        );
        assert_eq!(dto.notice, None);
        let json = serde_json::to_value(&dto).unwrap();
        for key in [
            "bookId",
            "chapter",
            "start",
            "end",
            "reference",
            "translationCode",
            "verses",
            "notice",
        ] {
            assert!(json.get(key).is_some(), "{key}");
        }
        assert_eq!(json["verses"][0]["parts"][0]["kind"], "text");
    }

    /// A KJV preview keeps the supplied words in italics' parts but drops
    /// the margin notes, markers and footnotes alike; its plain text is the
    /// reader's, and the stored verse is untouched.
    #[tokio::test]
    async fn a_kjv_passage_keeps_supplied_words_and_drops_margin_notes() {
        let (_db, state) = app_state("cmd_passage_kjv").await;
        // John 3:5 has supplied words, and 3:3 a margin note.
        let dto = passage_text(&state, "kjv", 43, 3, 3, 5).await.unwrap();
        let kjv = bundled("kjv");
        for v in &dto.verses {
            let stored = &kjv[42].chapters[2][(v.number - 1) as usize];
            let shown = VerseText::new("kjv", stored);
            assert_eq!(v.shown.text, shown.text, "{}", v.number);
            assert!(v.shown.footnotes.is_empty());
            assert!(v.shown.parts.iter().all(|p| p.kind != PartKind::Note));
            let kept: Vec<_> = shown
                .parts
                .iter()
                .filter(|p| p.kind != PartKind::Note)
                .collect();
            assert_eq!(v.shown.parts.iter().collect::<Vec<_>>(), kept);
        }
        assert!(
            dto.verses[0].shown.footnotes.is_empty()
                && VerseText::new("kjv", &kjv[42].chapters[2][2])
                    .footnotes
                    .len()
                    == 1
        );
        assert!(
            dto.verses[2]
                .shown
                .parts
                .iter()
                .any(|p| p.kind == PartKind::Supplied && p.text == "of")
        );
    }

    /// Verses a translation doesn't number are left out, and the notice
    /// says so, as opening the passage does; the rest still come.
    #[tokio::test]
    async fn a_passage_says_which_verses_a_translation_does_not_number() {
        let (_db, state) = app_state("cmd_passage_missing").await;
        // The WEB numbers Romans 14:24-26; the KJV ends the chapter at 23
        // (it has those verses at 16:25-27).
        let kjv = passage_text(&state, "kjv", 45, 14, 24, 26).await.unwrap();
        assert!(kjv.verses.is_empty());
        assert_eq!(
            kjv.notice.as_deref(),
            Some(
                "Romans 14:24\u{2013}26 isn't numbered in the King James Version \u{2014} \
                 Romans 14 ends at verse 23 there."
            )
        );
        let partly = passage_text(&state, "kjv", 45, 14, 23, 25).await.unwrap();
        assert_eq!(partly.verses.len(), 1);
        assert_eq!(partly.verses[0].number, 23);
        assert_eq!(
            partly.notice.as_deref(),
            Some(
                "Romans 14:24\u{2013}25 isn't numbered in the King James Version \u{2014} \
                 Romans 14 ends at verse 23 there."
            )
        );
        let web = passage_text(&state, "web", 45, 14, 24, 26).await.unwrap();
        assert_eq!(web.verses.len(), 3);
        assert_eq!(web.notice, None);
        assert_eq!(
            verse_list(&missing_runs(&[5, 9, 10], 4, 11, 20)),
            "4, 6\u{2013}8, 11"
        );
    }

    /// However far past the chapter's end a passage reaches, it's answered
    /// at once: the verses past the end in every translation are named as
    /// one range, never counted out.
    #[tokio::test]
    async fn a_passage_past_the_end_everywhere_is_named_not_counted() {
        let (_db, state) = app_state("cmd_passage_far").await;
        let far = passage_text(&state, "kjv", 43, 3, 35, i64::MAX)
            .await
            .unwrap();
        assert_eq!(
            far.verses.iter().map(|v| v.number).collect::<Vec<_>>(),
            [35, 36]
        );
        assert_eq!(
            far.notice.as_deref(),
            Some(
                "John 3:37\u{2013}9223372036854775807 isn't numbered in the King James \
                 Version \u{2014} John 3 ends at verse 36 there."
            )
        );
        let none = passage_text(&state, "kjv", 43, 3, i64::MAX, i64::MAX)
            .await
            .unwrap();
        assert!(none.verses.is_empty());
        assert_eq!(
            none.notice.as_deref(),
            Some(
                "John 3:9223372036854775807 isn't numbered in the King James Version \
                 \u{2014} John 3 ends at verse 36 there."
            )
        );
        // A gap inside the chapter, and the end past it everywhere, are
        // both named. The KJV ends Romans 14 at 23; the WEB at 26.
        let web = passage_text(&state, "web", 45, 14, 25, 1_000_000)
            .await
            .unwrap();
        assert_eq!(web.verses.len(), 2);
        assert_eq!(
            web.notice.as_deref(),
            Some(
                "Romans 14:27\u{2013}1000000 isn't numbered in the World English Bible \
                 \u{2014} Romans 14 ends at verse 26 there."
            )
        );
        assert_eq!(
            missing_runs(&[2, 3], 1, i64::MAX, 3),
            [(1, 1), (4, i64::MAX)]
        );
        assert_eq!(missing_runs(&[], 5, i64::MAX, 3), [(5, i64::MAX)]);
        assert_eq!(missing_runs(&[1, 2, 3], 1, 3, 3), []);
    }

    /// A translation, book or chapter that doesn't exist, or verses that
    /// aren't a range, are refused with a reason.
    #[tokio::test]
    async fn a_passage_refuses_what_is_not_one() {
        let (_db, state) = app_state("cmd_passage_invalid").await;
        for (code, book, chapter, start, end) in [
            ("niv", 43, 3, 16, 16),
            ("kjv", 0, 1, 1, 1),
            ("kjv", 67, 1, 1, 1),
            ("kjv", 43, 0, 1, 1),
            ("kjv", 43, 22, 1, 1),
            ("kjv", 43, 3, 0, 1),
            ("kjv", 43, 3, 17, 16),
        ] {
            let result = passage_text(&state, code, book, chapter, start, end).await;
            assert!(result.is_err(), "{code} {book} {chapter}:{start}-{end}");
        }
        assert_eq!(
            passage_text(&state, "kjv", 43, 22, 1, 1).await.unwrap_err(),
            "John has no chapter 22."
        );
    }

    /// Copy as…'s four formats, from the database, for the passages the
    /// release notes show: John 3:16–17 in the WEB and Psalm 23:1 in the
    /// KJV (its title, brackets and all, and its supplied word).
    #[tokio::test]
    async fn copy_text_has_every_format_of_a_passage() {
        let (_db, state) = app_state("cmd_copy_formats").await;
        let dto = copy_text(&state, "web", 43, 3, Some(16), Some(17))
            .await
            .unwrap();
        let web = bundled("web");
        let (v16, v17) = (&web[42].chapters[2][15], &web[42].chapters[2][16]);
        assert_eq!(dto.reference, "John 3:16\u{2013}17");
        assert_eq!(dto.translation_code, "web");
        assert_eq!(
            dto.formats.reference,
            format!("{v16} {v17} \u{2014} John 3:16\u{2013}17 (WEB)")
        );
        assert_eq!(
            dto.formats.numbered,
            format!("16 {v16}\n17 {v17}\n\u{2014} John 3:16\u{2013}17 (WEB)")
        );
        assert_eq!(
            dto.formats.markdown,
            format!("> **16** {v16} **17** {v17}\n>\n> \u{2014} John 3:16\u{2013}17 (WEB)")
        );
        assert_eq!(dto.formats.plain, format!("{v16} {v17}"));

        let psalm = copy_text(&state, "kjv", 19, 23, Some(1), Some(1))
            .await
            .unwrap();
        let one = "[A Psalm of David.] The LORD is my shepherd; I shall not want.";
        assert_eq!(
            psalm.formats.reference,
            format!("{one} \u{2014} Psalms 23:1 (KJV)")
        );
        assert_eq!(
            psalm.formats.numbered,
            format!("1 {one}\n\u{2014} Psalms 23:1 (KJV)")
        );
        assert_eq!(
            psalm.formats.markdown,
            "> **1** \\[A Psalm of David.\\] The LORD *is* my shepherd; I shall not want.\n>\n\
             > \u{2014} Psalms 23:1 (KJV)"
        );
        assert_eq!(psalm.formats.plain, one);

        let json = serde_json::to_value(&dto).unwrap();
        for key in ["reference", "numbered", "markdown", "plain"] {
            assert!(json["formats"][key].is_string(), "{key}");
        }
        assert_eq!(json["translationCode"], "web");
    }

    /// Copy (and Ctrl+C) copies what it always has: the verses' plain text
    /// as the reader shows it, joined by spaces, then the citation.
    #[tokio::test]
    async fn copy_text_with_a_reference_is_what_copy_always_copied() {
        let (_db, state) = app_state("cmd_copy_as_before").await;
        for (code, book, chapter, start, end, citation) in [
            ("web", 43, 3, 16, 18, "John 3:16\u{2013}18 (WEB)"),
            ("kjv", 19, 119, 105, 106, "Psalms 119:105\u{2013}106 (KJV)"),
            ("bsb", 40, 17, 20, 22, "Matthew 17:20\u{2013}22 (BSB)"),
        ] {
            let shown = reading(&state, book, chapter, Some(code))
                .await
                .unwrap()
                .unwrap();
            let text = shown
                .current_verses
                .iter()
                .filter(|v| (start..=end).contains(&v.number))
                .map(|v| v.shown.text.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            let dto = copy_text(&state, code, book, chapter, Some(start), Some(end))
                .await
                .unwrap();
            assert_eq!(dto.formats.reference, format!("{text} \u{2014} {citation}"));
        }
    }

    /// The KJV's margin notes and the BSB's footnotes are in no format:
    /// Psalm 119:105 ("lamp: or, candle") and Matthew 17:20 (whose note
    /// gives verse 21's wording).
    #[tokio::test]
    async fn copy_text_never_has_a_footnote() {
        let (_db, state) = app_state("cmd_copy_footnotes").await;
        let kjv = copy_text(&state, "kjv", 19, 119, Some(105), Some(105))
            .await
            .unwrap();
        let bsb = copy_text(&state, "bsb", 40, 17, Some(20), Some(20))
            .await
            .unwrap();
        let stored = bundled_bsb()[39].chapters[16][19].clone().unwrap();
        assert_eq!(bsb.formats.plain, stored);
        for (dto, note) in [(&kjv, "candle"), (&bsb, "fasting")] {
            let f = &dto.formats;
            for text in [&f.reference, &f.numbered, &f.markdown, &f.plain] {
                assert!(!text.contains(note), "{text}");
                assert!(!text.contains('{') && !text.contains("\\f"), "{text}");
            }
        }
        // The section's letter is the verse's own, as stored.
        assert_eq!(
            kjv.formats.plain,
            "NUN. Thy word is a lamp unto my feet, and a light unto my path."
        );
    }

    /// A verse the translation doesn't number is skipped without comment,
    /// in a range and in a whole chapter, which is every verse it numbers,
    /// a verse to a line with numbers, named "Matthew 17".
    #[tokio::test]
    async fn copy_text_skips_unnumbered_verses_and_copies_whole_chapters() {
        let (_db, state) = app_state("cmd_copy_chapter").await;
        let bsb = bundled_bsb();
        let text = |n: usize| bsb[39].chapters[16][n - 1].clone().unwrap();
        let range = copy_text(&state, "bsb", 40, 17, Some(20), Some(22))
            .await
            .unwrap();
        assert_eq!(range.reference, "Matthew 17:20\u{2013}22");
        assert_eq!(
            range.formats.numbered,
            format!(
                "20 {}\n22 {}\n\u{2014} Matthew 17:20\u{2013}22 (BSB)",
                text(20),
                text(22)
            )
        );

        let chapter = copy_text(&state, "bsb", 40, 17, None, None).await.unwrap();
        assert_eq!(chapter.reference, "Matthew 17");
        let numbers: Vec<usize> = (1..=27).filter(|&n| n != 21).collect();
        let lines: Vec<String> = numbers
            .iter()
            .map(|&n| format!("{n} {}", text(n)))
            .collect();
        assert_eq!(
            chapter.formats.numbered,
            format!("{}\n\u{2014} Matthew 17 (BSB)", lines.join("\n"))
        );
        let all: Vec<String> = numbers.iter().map(|&n| text(n)).collect();
        assert_eq!(chapter.formats.plain, all.join(" "));
        assert_eq!(
            chapter.formats.reference,
            format!("{} \u{2014} Matthew 17 (BSB)", all.join(" "))
        );
        assert!(chapter.formats.markdown.starts_with("> **1** "));
        assert!(chapter.formats.markdown.contains(" **22** "));
        assert!(!chapter.formats.markdown.contains("**21**"));
        assert!(
            chapter
                .formats
                .markdown
                .ends_with("\n>\n> \u{2014} Matthew 17 (BSB)")
        );

        // A whole KJV chapter: every verse, as the reader shows it.
        let kjv = copy_text(&state, "kjv", 19, 23, None, None).await.unwrap();
        let shown = reading(&state, 19, 23, Some("kjv")).await.unwrap().unwrap();
        let lines: Vec<String> = shown
            .current_verses
            .iter()
            .map(|v| format!("{} {}", v.number, v.shown.text))
            .collect();
        assert_eq!(lines.len(), 6);
        assert_eq!(
            kjv.formats.numbered,
            format!("{}\n\u{2014} Psalms 23 (KJV)", lines.join("\n"))
        );
    }

    /// What isn't a passage, or has no verse the translation numbers, is
    /// refused with a reason.
    #[tokio::test]
    async fn copy_text_refuses_what_is_not_a_passage() {
        let (_db, state) = app_state("cmd_copy_invalid").await;
        for (code, book, chapter, start, end) in [
            ("niv", 43, 3, Some(16), Some(16)),
            ("kjv", 67, 1, None, None),
            ("kjv", 43, 22, None, None),
            ("kjv", 43, 3, Some(0), Some(1)),
            ("kjv", 43, 3, Some(17), Some(16)),
            ("kjv", 43, 3, Some(16), None),
        ] {
            let result = copy_text(&state, code, book, chapter, start, end).await;
            assert!(result.is_err(), "{code} {book} {chapter}:{start:?}-{end:?}");
        }
        assert_eq!(
            copy_text(&state, "bsb", 40, 17, Some(21), Some(21))
                .await
                .unwrap_err(),
            "Matthew 17:21 isn't numbered in the Berean Standard Bible."
        );
    }

    /// `[` and `]` go to chapter 1 of the book before and after, named;
    /// Genesis has none before and Revelation none after.
    #[tokio::test]
    async fn the_books_before_and_after_are_named_at_chapter_one() {
        let (_db, state) = app_state("cmd_book_steps").await;
        let start = |id, name: &str| {
            Some(BookStartDto {
                book_id: id,
                chapter: 1,
                book_name: name.to_owned(),
            })
        };
        let psalm = reading(&state, 19, 23, Some("kjv")).await.unwrap().unwrap();
        assert_eq!(psalm.prev_book, start(18, "Job"));
        assert_eq!(psalm.next_book, start(20, "Proverbs"));
        let genesis = reading(&state, 1, 50, Some("kjv")).await.unwrap().unwrap();
        assert_eq!(genesis.prev_book, None);
        assert_eq!(genesis.next_book, start(2, "Exodus"));
        let revelation = reading(&state, 66, 1, Some("web")).await.unwrap().unwrap();
        assert_eq!(revelation.prev_book, start(65, "Jude"));
        assert_eq!(revelation.next_book, None);
        let json = serde_json::to_value(&psalm).unwrap();
        assert_eq!(json["nextBook"]["bookName"], "Proverbs");
        assert_eq!(json["prevBook"]["chapter"], 1);
    }

    /// Only the first and last chapters of the Bible have an edge card, in
    /// the place of the neighbor they lack; every other chapter, the book
    /// boundaries included, has both neighbors and no card.
    #[tokio::test]
    async fn the_bible_s_first_and_last_chapters_have_an_edge_card() {
        let (_db, state) = app_state("cmd_bible_edges").await;
        let genesis_1 = reading(&state, 1, 1, Some("kjv")).await.unwrap().unwrap();
        assert!(genesis_1.prev.is_none());
        assert_eq!(
            genesis_1.prev_edge,
            Some(BibleEdgeDto {
                title: "The Old Testament".to_owned(),
                detail: Some("Genesis · 50 chapters".to_owned()),
            })
        );
        assert_eq!(genesis_1.next_edge, None);
        assert_eq!(genesis_1.next.as_ref().map(|n| n.chapter), Some(2));

        let revelation_22 = reading(&state, 66, 22, Some("web")).await.unwrap().unwrap();
        assert!(revelation_22.next.is_none());
        assert_eq!(
            revelation_22.next_edge,
            Some(BibleEdgeDto {
                title: "The end of the Bible".to_owned(),
                detail: None,
            })
        );
        assert_eq!(revelation_22.prev_edge, None);

        for (book, chapter) in [(1, 50), (2, 1), (39, 4), (40, 1), (66, 21), (19, 119)] {
            let dto = reading(&state, book, chapter, Some("kjv"))
                .await
                .unwrap()
                .unwrap();
            assert!(dto.prev.is_some() && dto.next.is_some(), "{book}:{chapter}");
            assert_eq!(
                (dto.prev_edge, dto.next_edge),
                (None, None),
                "{book}:{chapter}"
            );
        }

        // The words are the book's own facts: the testament and chapter
        // count, as stored; one chapter is singular.
        let obadiah = state.books.iter().find(|b| b.name == "Obadiah").unwrap();
        assert_eq!(
            BibleEdgeDto::start(obadiah).detail.as_deref(),
            Some("Obadiah · 1 chapter")
        );
        let matthew = state.books.iter().find(|b| b.name == "Matthew").unwrap();
        assert_eq!(BibleEdgeDto::start(matthew).title, "The New Testament");
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
    async fn text_search_pages_through_verse_text() {
        let (_db, state) = app_state("cmd_search_text").await;

        let v = search_in(&state, "Faith, hope & charity", "kjv").await;
        assert_eq!(v["kind"], "text");
        assert_eq!(v["terms"], serde_json::json!(["faith", "hope", "charity"]));
        let results = v["results"].as_array().unwrap();
        let cor = results
            .iter()
            .find(|r| r["reference"] == "1 Corinthians 13:13")
            .expect("1 Corinthians 13:13 found");
        // The verse's plain text: "And now abideth faith, hope, charity,
        // these three; but the greatest of these {is} charity." without
        // its braces.
        let stored = &bundled("kjv")[45].chapters[12][12];
        assert!(stored.contains("{is}"));
        assert_eq!(cor["text"], VerseText::new("kjv", stored).text);
        assert_eq!(
            cor["text"],
            "And now abideth faith, hope, charity, these three; but the greatest of these is \
             charity."
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
    async fn kjv_verses_are_shown_clean_everywhere_and_the_web_as_stored() {
        let (_db, state) = app_state("cmd_kjv_shown").await;
        let psalm_119_105 =
            "NUN. Thy word is a lamp unto my feet, and a light unto my path.".to_string();

        // The chapter being read (which the note panel quotes from too):
        // plain text, parts and footnotes.
        let dto = reading(&state, 19, 119, Some("kjv"))
            .await
            .unwrap()
            .unwrap();
        let v105 = serde_json::to_value(&dto.current_verses[104]).unwrap();
        assert_eq!(
            v105,
            serde_json::json!({
                "number": 105,
                "text": psalm_119_105,
                "parts": [
                    {"kind": "text", "text": "NUN. Thy word "},
                    {"kind": "supplied", "text": "is"},
                    {"kind": "text", "text": " a lamp unto my feet, and a light unto my path."},
                    {"kind": "note", "text": "lamp: or, candle", "label": "a"},
                ],
                "footnotes": [{
                    "label": "a",
                    "lemma": "lamp",
                    "rest": ": or, candle",
                    "text": "lamp: or, candle",
                }],
                "xrefs": v105["xrefs"],
            })
        );
        // A note without a colon has no lemma: Psalms 73:22's second.
        let psalm_73 = reading(&state, 19, 73, Some("kjv")).await.unwrap().unwrap();
        assert_eq!(
            serde_json::to_value(&psalm_73.current_verses[21].shown.footnotes).unwrap(),
            serde_json::json!([
                {
                    "label": "a",
                    "lemma": "ignorant",
                    "rest": ": Heb. I knew not",
                    "text": "ignorant: Heb. I knew not",
                },
                {"label": "b", "lemma": null, "rest": "before Heb. with", "text": "before Heb. with"},
            ])
        );
        assert!(!v105["xrefs"].as_array().unwrap().is_empty());
        let kjv = bundled("kjv");
        let every_verse = dto
            .current_verses
            .iter()
            .map(|v| (&v.shown, v.number))
            .chain(
                dto.prev
                    .iter()
                    .chain(&dto.next)
                    .flat_map(|side| side.verses.iter().map(|v| (&v.shown, v.number))),
            );
        for (shown, _) in every_verse {
            assert!(!shown.text.contains(['{', '}']), "{}", shown.text);
        }
        // The neighbors are shown the same way (Psalms 118 and 120).
        let prev = dto.prev.as_ref().unwrap();
        assert_eq!(
            prev.verses[0].shown,
            VerseText::new("kjv", &kjv[18].chapters[117][0])
        );

        // A cross-reference previewed (without its margin note), and the
        // comparison's KJV column.
        let previewed = passage_text(&state, "kjv", 19, 119, 105, 105)
            .await
            .unwrap();
        assert_eq!(previewed.verses[0].shown.text, psalm_119_105);
        assert!(previewed.verses[0].shown.footnotes.is_empty());
        let codes = ["kjv".to_string(), "web".to_string()];
        let compare = compare_chapter(&state, 19, 119, &codes)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(compare.columns[0].verses[104].shown.text, psalm_119_105);

        // Library previews.
        bookmark_passage(&state, 19, 119, 105, 106).await.unwrap();
        highlight_passage(&state, 19, 119, 105, 105, "yellow")
            .await
            .unwrap();
        let bookmarks = bookmarks_in(&state, Some("kjv")).await.unwrap();
        assert_eq!(
            bookmarks[0].preview.as_deref(),
            Some(psalm_119_105.as_str())
        );
        let highlights = highlight_passages_in(&state, Some("kjv")).await.unwrap();
        assert_eq!(
            highlights[0].preview.as_deref(),
            Some(psalm_119_105.as_str())
        );

        // A malformed verse is shown exactly as stored (Hebrews 10:34).
        let hebrews = reading(&state, 58, 10, Some("kjv")).await.unwrap().unwrap();
        let stored = &kjv[57].chapters[9][33];
        assert_eq!(
            hebrews.current_verses[33].shown,
            VerseText::as_stored(stored)
        );

        // The WEB, everywhere, exactly as stored: one text part, no
        // footnotes.
        let web = bundled("web");
        let dto = reading(&state, 19, 119, Some("web"))
            .await
            .unwrap()
            .unwrap();
        for (v, stored) in dto.current_verses.iter().zip(&web[18].chapters[118]) {
            assert_eq!(v.shown, VerseText::as_stored(stored));
        }
        let previewed = passage_text(&state, "web", 19, 119, 105, 105)
            .await
            .unwrap();
        assert_eq!(
            previewed.verses[0].shown,
            VerseText::as_stored(&web[18].chapters[118][104])
        );
        let bookmarks = bookmarks_in(&state, Some("web")).await.unwrap();
        assert_eq!(
            bookmarks[0].preview.as_ref(),
            Some(&web[18].chapters[118][104])
        );
    }

    /// The BSB: listed between the KJV and the WEB (the KJV still the
    /// default), read with its footnotes kept apart from its text, and
    /// working in every place a translation's verses are used, including
    /// across the verses it doesn't number in mid-chapter.
    #[tokio::test]
    async fn the_bsb_is_a_translation_like_the_others_with_its_own_footnotes() {
        use crate::notes_export::escape_block;
        let (_db, state) = app_state("cmd_bsb").await;
        let bsb = bundled_bsb();
        let text = |b: usize, c: usize, v: usize| {
            bsb[b - 1].chapters[c - 1][v - 1].clone().expect("numbered")
        };

        // The list, in order, with what Aa's chooser shows.
        let home = home(&state).await;
        let listed: Vec<_> = home
            .translations
            .iter()
            .map(|t| {
                (
                    t.code.as_str(),
                    t.name.as_str(),
                    t.date.as_deref(),
                    t.licence.as_str(),
                )
            })
            .collect();
        assert_eq!(
            listed,
            [
                (
                    "kjv",
                    "King James Version",
                    Some("1611/1769"),
                    "public domain"
                ),
                (
                    "bsb",
                    "Berean Standard Bible",
                    Some("2023"),
                    "public domain"
                ),
                ("web", "World English Bible", None, "public domain"),
            ]
        );
        assert_eq!(
            home.translations[1].credit,
            "Berean Standard Bible (BSB), public domain, berean.bible"
        );
        assert_eq!(home.current_translation_code, "kjv");

        // Genesis 1: each verse is its stored text, and verse 3's note
        // stands after “light,”, lettered, its words as Berean's.
        let dto = reading(&state, 1, 1, Some("bsb")).await.unwrap().unwrap();
        for v in &dto.current_verses {
            assert_eq!(v.shown.text, text(1, 1, v.number as usize));
        }
        let v3 = &dto.current_verses[2].shown;
        let kinds: Vec<_> = v3.parts.iter().map(|p| (p.kind, p.text.as_str())).collect();
        assert_eq!(
            kinds,
            [
                (PartKind::Text, "And God said, “Let there be light,”"),
                (PartKind::Note, "Cited in 2 Corinthians 4:6"),
                (PartKind::Text, " and there was light."),
            ]
        );
        assert_eq!(v3.footnotes[0].label, "a");
        // Verse 6's note: its alternative readings in italics.
        let v6 = &dto.current_verses[5].shown.footnotes[0];
        assert_eq!(
            v6.text,
            "Or a canopy or a firmament or a vault; also in verses 7, 8, 14, 15, 17, and 20"
        );
        let segments = v6.segments.as_ref().unwrap();
        assert_eq!(segments[1].kind, crate::domain::usfm_note::SegmentKind::Alt);
        assert_eq!(segments[1].text, "a canopy ");
        // The faded neighbours carry them too; the frontend leaves their
        // markers out, as for the KJV.
        assert!(
            dto.next
                .as_ref()
                .unwrap()
                .verses
                .iter()
                .any(|v| !v.shown.footnotes.is_empty())
        );

        // Matthew 17 in the BSB: no verse 21, and verse 20's note says so.
        let dto = reading(&state, 40, 17, Some("bsb")).await.unwrap().unwrap();
        let numbers: Vec<i64> = dto.current_verses.iter().map(|v| v.number).collect();
        assert_eq!(numbers, (1..=27).filter(|&n| n != 21).collect::<Vec<_>>());
        assert!(
            dto.current_verses[19].shown.footnotes[0]
                .text
                .starts_with("BYZ and TR include 21But")
        );

        // A preview: the verses it numbers, no notes, and which it doesn't.
        let passage = passage_text(&state, "bsb", 40, 17, 20, 22).await.unwrap();
        assert_eq!(
            passage.verses.iter().map(|v| v.number).collect::<Vec<_>>(),
            [20, 22]
        );
        assert!(passage.verses.iter().all(|v| v.shown.footnotes.is_empty()
            && v.shown.parts.iter().all(|p| p.kind != PartKind::Note)));
        assert_eq!(passage.verses[0].shown.text, text(40, 17, 20));
        assert_eq!(
            passage.notice.as_deref(),
            Some(
                "In Matthew 17:20\u{2013}22, verse 21 isn't numbered in the Berean Standard Bible."
            )
        );
        let missing = passage_text(&state, "bsb", 40, 17, 21, 21).await.unwrap();
        assert!(missing.verses.is_empty());
        assert_eq!(
            missing.notice.as_deref(),
            Some("In Matthew 17:21, verse 21 isn't numbered in the Berean Standard Bible.")
        );

        // Search: the verse text, never the footnotes.
        let v = search_in(&state, "shepherd want", "bsb").await;
        let results = v["results"].as_array().unwrap();
        let psalm = results
            .iter()
            .find(|r| r["reference"] == "Psalms 23:1")
            .unwrap();
        assert_eq!(psalm["text"], text(19, 23, 1));
        let v = search_in(&state, "BYZ", "bsb").await;
        assert_eq!(v["results"].as_array().map_or(0, Vec::len), 0, "{v}");

        // Bookmarks belong to no translation: one across the gap is partly
        // numbered in the BSB, one on the gap not at all.
        bookmark_passage(&state, 40, 17, 20, 22).await.unwrap();
        bookmark_passage(&state, 40, 17, 21, 21).await.unwrap();
        let marks = bookmarks_in(&state, Some("bsb")).await.unwrap();
        let across = marks.iter().find(|b| b.verse_start == 20).unwrap();
        assert!(!across.complete);
        assert_eq!(across.preview.as_deref(), Some(text(40, 17, 20).as_str()));
        let on = marks.iter().find(|b| b.verse_start == 21).unwrap();
        assert_eq!((on.complete, on.preview.as_deref()), (false, None));

        // The notes export quotes the BSB, and says what it doesn't number.
        note_add(&state, passage_at(40, 17, 20, 22), "The gap.")
            .await
            .unwrap();
        let tz = chrono::FixedOffset::east_opt(0).unwrap();
        let file = build_notes_export(
            &state,
            "bsb",
            &NotesFilter::default(),
            chrono::Utc::now(),
            &tz,
        )
        .await
        .unwrap();
        assert!(file.contents.contains("Verse text: Berean Standard Bible"));
        for verse in [20, 22] {
            let quoted = format!("**{verse}** {}", escape_block(&text(40, 17, verse)));
            assert!(file.contents.contains(&quoted), "{quoted}");
        }
        assert!(
            file.contents
                .contains("*Not all of it is numbered in the BSB.*")
        );
        assert!(!file.contents.contains("BYZ"));
    }

    fn passage_at(book_id: i64, chapter: i64, verse_start: i64, verse_end: i64) -> Passage {
        Passage {
            book_id,
            chapter,
            verse_start,
            verse_end,
        }
    }

    #[tokio::test]
    async fn compare_returns_each_translations_own_numbering_and_text() {
        let (_db, state) = app_state("cmd_compare").await;
        let pair = |a: &str, b: &str| vec![a.to_string(), b.to_string()];

        // Romans 14: 23 verses in the KJV, 26 in the WEB (which places the
        // doxology the KJV has at 16:25-27 here, as 14:24-26).
        let dto = compare_chapter(&state, 45, 14, &pair("kjv", "web"))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(dto.columns.len(), 2);
        for (column, code) in dto.columns.iter().zip(["kjv", "web"]) {
            assert_eq!(column.code, code);
            let stored = &bundled(code)[44].chapters[13];
            assert_eq!(column.verses.len(), stored.len());
            for (i, (verse, text)) in column.verses.iter().zip(stored).enumerate() {
                assert_eq!(verse.number, i as i64 + 1);
                // Each as shown: the WEB exactly as stored.
                assert_eq!(verse.shown, VerseText::new(code, text));
            }
        }
        assert_eq!(dto.columns[0].verses.len(), 23);
        assert_eq!(dto.columns[1].verses.len(), 26);

        // Any two, in the order asked: the one being read first. The BSB
        // doesn't number Matthew 17:21, mid-chapter; its column skips it.
        let dto = compare_chapter(&state, 40, 17, &pair("bsb", "kjv"))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            dto.columns
                .iter()
                .map(|c| c.code.as_str())
                .collect::<Vec<_>>(),
            ["bsb", "kjv"]
        );
        let numbers = |c: &CompareColumnDto| c.verses.iter().map(|v| v.number).collect::<Vec<_>>();
        assert_eq!(
            numbers(&dto.columns[0]),
            (1..=27).filter(|&n| n != 21).collect::<Vec<_>>()
        );
        assert_eq!(numbers(&dto.columns[1]), (1..=27).collect::<Vec<_>>());
        let bsb = bundled_bsb();
        for verse in &dto.columns[0].verses {
            assert_eq!(
                Some(&verse.shown.text),
                bsb[39].chapters[16][verse.number as usize - 1].as_ref()
            );
        }
        // The BSB's footnotes show in its column.
        assert!(
            dto.columns[0].verses[19].shown.footnotes[0]
                .text
                .contains("BYZ and TR include")
        );

        // Translations that aren't there, or one twice, are refused.
        for codes in [pair("kjv", "niv"), pair("web", "web"), Vec::new()] {
            assert!(
                compare_chapter(&state, 45, 14, &codes).await.is_err(),
                "{codes:?}"
            );
        }
        assert!(
            compare_chapter(&state, 43, 22, &pair("kjv", "web"))
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            compare_chapter(&state, 99, 1, &pair("kjv", "web"))
                .await
                .unwrap()
                .is_none()
        );

        // Comparing doesn't move the saved reading position.
        assert_eq!(state.store.reading_position().await.unwrap(), None);
    }
}
