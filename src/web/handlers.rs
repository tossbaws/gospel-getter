use askama::Template;
use axum::{
    extract::{Path, Query, State},
    response::Html,
};
use serde::Deserialize;
use std::sync::Arc;

use crate::db::{Book, Translation};
use crate::domain::bible::{self, ChapterRef};
use crate::web::router::AppState;

struct VerseView {
    number: i64,
    text: String,
}

/// A neighboring chapter shown in full, at reduced opacity, beside the one
/// being read. Only its heading is a clickable jump-to-here control — the
/// verse text itself stays plain, selectable text.
struct ChapterSide {
    book_id: i64,
    chapter: i64,
    book_name: String,
    verses: Vec<VerseView>,
}

#[derive(Template)]
#[template(path = "reading_pane.html")]
struct ReadingPaneTemplate {
    current_book_id: i64,
    current_chapter: i64,
    current_book_name: String,
    current_verses: Vec<VerseView>,
    prev: Option<ChapterSide>,
    next: Option<ChapterSide>,
}

struct ChapterEntry {
    number: i64,
    is_active: bool,
}

#[derive(Template)]
#[template(path = "chapters.html")]
struct ChaptersTemplate {
    book_id: i64,
    book_name: String,
    chapters: Vec<ChapterEntry>,
}

#[derive(Template)]
#[template(path = "index.html")]
struct HomeTemplate {
    ot_books: Vec<Book>,
    nt_books: Vec<Book>,
    current_book_id: i64,
    translations: Vec<Translation>,
    current_translation_code: String,
    reading_pane_html: String,
    chapters_html: String,
}

fn book_name(state: &AppState, book_id: i64) -> Option<&str> {
    state
        .books
        .iter()
        .find(|b| b.id == book_id)
        .map(|b| b.name.as_str())
}

/// Resolve a `?translation=` query value to a translation id, falling back
/// to the first bundled translation (the default) when the query is absent
/// or doesn't match anything we have.
fn resolve_translation(state: &AppState, code: Option<&str>) -> i64 {
    code.and_then(|c| state.translations.iter().find(|t| t.code == c))
        .or(state.translations.first())
        .map(|t| t.id)
        .unwrap_or(1)
}

/// Build the chapter grid for one book, optionally marking one chapter
/// active. Shared by the home page (pre-populated for the initial chapter)
/// and the `/books/{id}/chapters` fragment endpoint, so there's one source
/// of truth instead of the two drifting apart.
fn build_chapters_template(
    state: &AppState,
    book_id: i64,
    active: Option<i64>,
) -> Option<ChaptersTemplate> {
    let book = state.books.iter().find(|b| b.id == book_id)?;
    let chapters = (1..=book.chapter_count)
        .map(|n| ChapterEntry {
            number: n,
            is_active: active == Some(n),
        })
        .collect();

    Some(ChaptersTemplate {
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
) -> anyhow::Result<ChapterSide> {
    let book_name = book_name(state, at.book_id)
        .ok_or_else(|| anyhow::anyhow!("unknown book id in navigation"))?
        .to_string();
    let verses = state
        .store
        .chapter_verses(translation_id, at.book_id, at.chapter)
        .await?;

    Ok(ChapterSide {
        book_id: at.book_id,
        chapter: at.chapter,
        book_name,
        verses: verses
            .into_iter()
            .map(|v| VerseView {
                number: v.verse,
                text: v.text,
            })
            .collect(),
    })
}

/// Build the reading-pane template for one chapter of one translation: the
/// chapter itself, plus the full text of the chapter immediately before and
/// after it (crossing book boundaries as needed), rendered at reduced
/// opacity so the reader can sense the surrounding text while scrolling.
async fn build_reading_pane(
    state: &AppState,
    translation_id: i64,
    book_id: i64,
    chapter: i64,
) -> anyhow::Result<ReadingPaneTemplate> {
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
    let current_verses = state
        .store
        .chapter_verses(translation_id, book_id, chapter)
        .await?
        .into_iter()
        .map(|v| VerseView {
            number: v.verse,
            text: v.text,
        })
        .collect();

    Ok(ReadingPaneTemplate {
        current_book_id: book_id,
        current_chapter: chapter,
        current_book_name,
        current_verses,
        prev,
        next,
    })
}

/// Home page — the full book list, the translation picker, the chapter grid
/// for whichever book is about to be shown, and the reading pane itself —
/// all pre-populated for the last chapter the reader was on, so nothing
/// "pops in" after the fact. Falls back to Genesis 1 in the default
/// translation if nothing has ever been read yet.
///
/// Renders the exact same `reading_pane.html`/`chapters.html` templates
/// that the AJAX fragment endpoints use, so there's one source of truth
/// for that markup instead of the page and the fragments drifting apart.
pub async fn home(State(state): State<Arc<AppState>>) -> Html<String> {
    let ot_books = state
        .books
        .iter()
        .filter(|b| b.testament == "OT")
        .cloned()
        .collect();
    let nt_books = state
        .books
        .iter()
        .filter(|b| b.testament == "NT")
        .cloned()
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

    let reading_pane_html = match build_reading_pane(&state, translation_id, book_id, chapter).await
    {
        Ok(pane) => pane.render().unwrap_or_default(),
        Err(e) => {
            tracing::error!("Failed to build initial reading pane: {e}");
            String::new()
        }
    };
    let chapters_html = build_chapters_template(&state, book_id, Some(chapter))
        .map(|t| t.render().unwrap_or_default())
        .unwrap_or_default();

    let template = HomeTemplate {
        ot_books,
        nt_books,
        current_book_id: book_id,
        translations: state.translations.clone(),
        current_translation_code,
        reading_pane_html,
        chapters_html,
    };
    Html(template.render().unwrap_or_else(|_| {
        "<!DOCTYPE html><html><body><h1>Error loading Gospel Getter</h1></body></html>".to_string()
    }))
}

#[derive(Deserialize)]
pub struct ChaptersQuery {
    pub active: Option<i64>,
}

/// Chapter grid for one book — shown when a book is clicked, and used again
/// (with `?active=`) to keep the grid in sync when arrow-key navigation
/// crosses into a different book. Chapter counts don't vary by
/// translation, so this doesn't need to know which one is selected.
pub async fn chapters_fragment(
    State(state): State<Arc<AppState>>,
    Path(book_id): Path<i64>,
    Query(query): Query<ChaptersQuery>,
) -> Html<String> {
    match build_chapters_template(&state, book_id, query.active) {
        Some(template) => Html(template.render().unwrap_or_default()),
        None => Html(String::new()),
    }
}

#[derive(Deserialize)]
pub struct ReadQuery {
    pub translation: Option<String>,
}

/// Reading-pane fragment for one chapter — used by chapter clicks, arrow-key
/// navigation, and switching translations. Also remembers this as the
/// reader's current position, so reopening the app returns here.
pub async fn read_fragment(
    State(state): State<Arc<AppState>>,
    Path((book_id, chapter)): Path<(i64, i64)>,
    Query(query): Query<ReadQuery>,
) -> Html<String> {
    let translation_id = resolve_translation(&state, query.translation.as_deref());
    match build_reading_pane(&state, translation_id, book_id, chapter).await {
        Ok(pane) => {
            let html = pane.render().unwrap_or_default();
            if let Err(e) = state
                .store
                .save_reading_position(translation_id, book_id, chapter)
                .await
            {
                tracing::warn!("Failed to save reading position: {e}");
            }
            Html(html)
        }
        Err(e) => {
            tracing::error!("Failed to build reading pane for {book_id}:{chapter}: {e}");
            Html(String::new())
        }
    }
}
