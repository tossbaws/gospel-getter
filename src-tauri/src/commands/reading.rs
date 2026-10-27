//! The home screen and the chapter being read: `get_home`, `get_chapters`,
//! `get_reading` and `save_reading_position`, with the DTOs they answer
//! with (a verse as shown, the neighboring chapters, the Bible's edges).

use anyhow::Context;
use serde::Serialize;
use std::collections::HashMap;

use super::{AppState, book_name, code_of, failed, resolve_translation};
use crate::db::{self, Book, TopVerse, Translation};
use crate::domain::bible::{self, ChapterRef};
use crate::domain::kjv_markup::{PartKind, VerseText};
use crate::start_here::{StartHereDto, start_here};

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
    /// Where in that chapter the reader was: the verse at the top of the
    /// reading pane, its top `current_verse_offset` px below the bar's
    /// bottom (`None`: at it). `None` opens the chapter at its top, as when
    /// the place was saved for another chapter, or not at all.
    pub current_verse: Option<i64>,
    pub current_verse_offset: Option<f64>,
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
    pub(super) fn new(translation_code: &str, verse: db::Verse) -> Self {
        Self {
            number: verse.verse,
            shown: shown(translation_code, &verse),
        }
    }

    /// The verse as shown, without its margin notes: no note parts and no
    /// footnotes, the supplied words still marked. `text` is unchanged
    /// (it never had the notes).
    pub(super) fn without_notes(translation_code: &str, verse: db::Verse) -> Self {
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
    let ((translation_id, book_id, chapter, at), has_saved_position) =
        match state.store.reading_place().await {
            Ok(Some(place)) => (
                (place.translation_id, place.book_id, place.chapter, place.at),
                true,
            ),
            Ok(None) => ((default_translation_id, 1, 1, None), false),
            Err(e) => {
                tracing::error!("Failed to load reading position: {e}");
                ((default_translation_id, 1, 1, None), true)
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
        current_verse: at.map(|at| at.verse),
        current_verse_offset: at.and_then(|at| at.offset_px),
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
/// returns there, and where in it the reader was: `verse`, the verse at the
/// top of the reading pane, its top `offset` px below the bar (none, the
/// chapter's top). The page calls this once it has put the chapter on
/// screen (`get_reading` saves nothing, so an answer the page asked for
/// and then dropped never moves the position), with no verse, and again
/// with the verse when the reader stops scrolling or the window is hidden;
/// it sends one at a time, in order, so the place shown last is the one
/// kept. With two copies of the app open, the last save from either is.
#[tauri::command]
pub async fn save_reading_position(
    state: tauri::State<'_, AppState>,
    book_id: i64,
    chapter: i64,
    translation_code: String,
    verse: Option<i64>,
    offset: Option<f64>,
) -> Result<(), String> {
    remember_place(&state, book_id, chapter, &translation_code, verse, offset).await
}

/// [`remember_place`] at the chapter's top.
pub async fn remember_position(
    state: &AppState,
    book_id: i64,
    chapter: i64,
    translation_code: &str,
) -> Result<(), String> {
    remember_place(state, book_id, chapter, translation_code, None, None).await
}

/// The logic behind `save_reading_position`: an error, as a sentence for
/// the reader, for a translation, book, chapter or verse that isn't there,
/// or a position that couldn't be saved. An offset that isn't a number is
/// left out (the verse goes at the bar).
pub async fn remember_place(
    state: &AppState,
    book_id: i64,
    chapter: i64,
    translation_code: &str,
    verse: Option<i64>,
    offset: Option<f64>,
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
    let at = match verse {
        Some(verse) => {
            let last = state
                .store
                .chapter_last_verse(book_id, chapter)
                .await
                .with_context(|| format!("Failed to read {} {chapter}", book.name))
                .map_err(failed("The reading position couldn't be saved."))?;
            if !(1..=last).contains(&verse) {
                return Err(format!("{} {chapter} has no verse {verse}.", book.name));
            }
            Some(TopVerse {
                verse,
                offset_px: offset.filter(|o| o.is_finite()),
            })
        }
        None => None,
    };
    state
        .store
        .save_reading_position(translation.id, book_id, chapter, at)
        .await
        .map_err(failed("The reading position couldn't be saved."))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::tests::app_state;

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
}
