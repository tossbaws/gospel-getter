use askama::Template;
use axum::body::Body;
use axum::{
    extract::{Path, Query, State},
    http::header,
    response::{Html, Response},
};

/// Bundled application favicon (the same icon used for the Windows build).
const FAVICON_BYTES: &[u8] = include_bytes!("../../packaging/windows/gospel-getter.ico");

use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;

use crate::db::{Book, Translation};
use crate::domain::bible::{self, ChapterRef};
use crate::web::router::AppState;

struct CrossRefView {
    book_id: i64,
    chapter: i64,
    verse: i64,
    end_verse: Option<i64>,
    citation: String,
}

struct VerseView {
    number: i64,
    text: String,
    xrefs: Vec<CrossRefView>,
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
    has_chapter_xrefs: bool,
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
                xrefs: Vec::new(),
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

    let cross_refs = state
        .store
        .chapter_cross_references(book_id, chapter)
        .await?;
    let mut xrefs_by_verse: HashMap<i64, Vec<CrossRefView>> = HashMap::new();
    for r in cross_refs {
        xrefs_by_verse
            .entry(r.verse)
            .or_default()
            .push(CrossRefView {
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
        .map(|v| VerseView {
            number: v.verse,
            text: v.text,
            xrefs: xrefs_by_verse.remove(&v.verse).unwrap_or_default(),
        })
        .collect();

    Ok(ReadingPaneTemplate {
        current_book_id: book_id,
        current_chapter: chapter,
        current_book_name,
        current_verses,
        prev,
        next,
        has_chapter_xrefs,
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

struct SimpleVerseView {
    number: i64,
    text: String,
}

#[derive(Template)]
#[template(path = "xref_verses.html")]
struct XrefVersesTemplate {
    verses: Vec<SimpleVerseView>,
}

#[derive(Deserialize)]
pub struct XrefTextQuery {
    pub translation: Option<String>,
    pub end_verse: Option<i64>,
}

/// The verse text a cross-reference citation expands to, fetched on click
/// rather than embedded up front, since only a small fraction of citations
/// actually get expanded in any given reading session.
pub async fn xref_text_fragment(
    State(state): State<Arc<AppState>>,
    Path((book_id, chapter, verse)): Path<(i64, i64, i64)>,
    Query(query): Query<XrefTextQuery>,
) -> Html<String> {
    let translation_id = resolve_translation(&state, query.translation.as_deref());
    let end_verse = query.end_verse.unwrap_or(verse);
    match state
        .store
        .verse_range(translation_id, book_id, chapter, verse, end_verse)
        .await
    {
        Ok(verses) => {
            let template = XrefVersesTemplate {
                verses: verses
                    .into_iter()
                    .map(|v| SimpleVerseView {
                        number: v.verse,
                        text: v.text,
                    })
                    .collect(),
            };
            Html(template.render().unwrap_or_default())
        }
        Err(e) => {
            tracing::error!(
                "Failed to fetch xref verse text for {book_id} {chapter}:{verse}-{end_verse}: {e}"
            );
            Html(String::new())
        }
    }
}

/// Serve the application's favicon from the bundled Windows icon.
pub async fn serve_favicon() -> Response {
    Response::builder()
        .header(header::CONTENT_TYPE, "image/x-icon")
        .body(Body::from(FAVICON_BYTES.to_vec()))
        .unwrap()
}

#[cfg(test)]
mod tests {
    /// Regression guard for GH-1: on narrow screens the reader must only
    /// see the current chapter, not scroll past the full previous one
    /// first. This pins down the `@media (max-width: 800px)` rule in
    /// index.html that hides `.chapter-side` (the prev/next chapter
    /// columns) rather than just re-stacking them under `.chapter-main`.
    #[test]
    fn mobile_breakpoint_hides_chapter_side_neighbors() {
        let css = include_str!("../../templates/index.html");
        let start = css
            .find("@media (max-width: 800px)")
            .expect("mobile breakpoint rule should exist in index.html");
        let block_start = css[start..].find('{').map(|i| start + i).unwrap();

        // Walk forward from the block's opening brace, tracking nesting
        // depth, to find the brace that closes the whole `@media` rule
        // (not just the first nested selector inside it).
        let mut depth = 0usize;
        let mut block_end = None;
        for (i, ch) in css[block_start..].char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        block_end = Some(block_start + i);
                        break;
                    }
                }
                _ => {}
            }
        }
        let block_end = block_end.expect("mobile breakpoint rule should be closed");
        let block = &css[block_start..block_end];

        assert!(
            block.contains(".chapter-side"),
            "mobile breakpoint should target .chapter-side"
        );
        assert!(
            block.contains("display: none"),
            "mobile breakpoint should hide .chapter-side instead of just \
             restacking it, so only the current chapter is visible"
        );
    }

    /// Regression guard for GH-6: printing a chapter should hide all page
    /// chrome (settings, header, book list, chapter chooser, neighboring
    /// chapter panes, nav hint, footer, cross-reference popups) but must
    /// never target the reading content itself — the chapter heading and
    /// verses are the whole point of printing the page.
    #[test]
    fn print_stylesheet_hides_chrome_but_not_reading_content() {
        let css = include_str!("../../templates/index.html");
        let start = css
            .find("@media print")
            .expect("a @media print rule should exist in index.html");
        let block_start = css[start..].find('{').map(|i| start + i).unwrap();

        let mut depth = 0usize;
        let mut block_end = None;
        for (i, ch) in css[block_start..].char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        block_end = Some(block_start + i);
                        break;
                    }
                }
                _ => {}
            }
        }
        let block_end = block_end.expect("@media print rule should be closed");
        let block = &css[block_start..block_end];

        assert!(
            block.contains("display: none"),
            "print stylesheet should actually hide the page chrome it targets"
        );
        for chrome_selector in [
            ".settings-toggle",
            ".settings-menu",
            "header",
            ".book-list",
            "#chapter-list",
            ".chapter-side",
            ".nav-hint",
            "footer",
            ".xref-popup",
        ] {
            assert!(
                block.contains(chrome_selector),
                "print stylesheet should hide `{chrome_selector}` — it's page \
                 chrome, not reading content"
            );
        }

        for reading_selector in ["#reading-pane {", ".chapter-main {", ".verse {"] {
            assert!(
                !block.contains(reading_selector),
                "print stylesheet must not hide `{reading_selector}` — the \
                 chapter heading and verses are exactly what should remain \
                 visible when printed"
            );
        }
    }

    /// Regression guard for GH-5: the "Random chapter" feature picks a
    /// chapter entirely client-side, so each `.book-item` button must carry
    /// its `chapter_count` as a data attribute (in both the OT and NT
    /// loops) or the client has no valid upper bound to pick from.
    #[test]
    fn book_item_buttons_expose_chapter_count() {
        let html = include_str!("../../templates/index.html");
        let occurrences = html
            .matches(
                "data-book-id=\"{{ book.id }}\" data-chapter-count=\"{{ book.chapter_count }}\"",
            )
            .count();
        assert_eq!(
            occurrences, 2,
            "both the OT and NT book-item loops should render data-chapter-count \
             alongside data-book-id"
        );
    }

    /// Regression guard for GH-5: the random chapter control must be a real,
    /// labeled `<button>` (not a link or a bare clickable `<div>`) so it's
    /// reachable and announced correctly by assistive tech.
    #[test]
    fn random_chapter_control_is_a_real_labeled_button() {
        let html = include_str!("../../templates/index.html");
        let id_pos = html
            .find("id=\"random-chapter-btn\"")
            .expect("a #random-chapter-btn control should exist in index.html");

        let tag_start = html[..id_pos]
            .rfind("<button")
            .expect("the random chapter control should be a <button> element");
        let tag_end = html[tag_start..]
            .find('>')
            .map(|i| tag_start + i)
            .expect("the random chapter button's opening tag should be closed");
        let opening_tag = &html[tag_start..=tag_end];
        assert!(
            opening_tag.contains("type=\"button\""),
            "random chapter control should be an explicit type=\"button\", not a \
             form-submitting default"
        );

        let close_pos = html[tag_end..]
            .find("</button>")
            .map(|i| tag_end + i)
            .expect("the random chapter button should have a matching </button>");
        let label = html[tag_end + 1..close_pos].trim();
        assert!(
            !label.is_empty(),
            "random chapter button should have a non-empty visible label so its \
             accessible name isn't blank"
        );
    }

    /// Regression guard for GH-5: the random-pick logic must be seeded from
    /// `.book-item` buttons (uniform choice of book, then 1..=chapter_count
    /// for that book) and must bail out rather than call `loadReading` when
    /// that data is missing or non-numeric — it must never fabricate a
    /// request to an invalid chapter.
    #[test]
    fn random_chapter_script_guards_against_malformed_data() {
        let html = include_str!("../../templates/index.html");
        assert!(
            html.contains("document.querySelectorAll('.book-item')"),
            "random pick should choose uniformly from the rendered book buttons"
        );
        assert!(
            html.contains("Number.isInteger(chapterCount) && chapterCount < 1")
                || html.contains("!Number.isInteger(chapterCount) || chapterCount < 1"),
            "random pick should guard against a missing/non-numeric/non-positive \
             chapter count instead of issuing an invalid request"
        );
        assert!(
            html.contains("Math.floor(Math.random() * chapterCount) + 1"),
            "chapter pick should be uniform over 1..=chapter_count"
        );
    }
}
