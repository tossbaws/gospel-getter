//! Searching the text, and going to a reference typed in the search box
//! (GH-12): `search`.

use anyhow::Context;
use serde::Serialize;

use super::{AppState, book_name, code_of, failed, resolve_translation};
use crate::domain::kjv_markup::VerseText;
use crate::domain::query::{self, Interpretation, SearchQuery};
use crate::passage_reference::passage_reference;

/// How many text-search results one `search` call returns.
pub const SEARCH_PAGE_SIZE: i64 = 50;

fn translation_name(state: &AppState, translation_id: i64) -> &str {
    state
        .translations
        .iter()
        .find(|t| t.id == translation_id)
        .map_or("this translation", |t| t.name.as_str())
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
    /// The words of `text` the search matched, to be marked: byte ranges
    /// `[start, end)` into its UTF-8, in order (`query::term_marks`).
    pub marks: Vec<(usize, usize)>,
}

/// A book typed by name alone: its first chapter, to go to.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GoToDto {
    pub book_id: i64,
    /// Always 1.
    pub chapter: i64,
    /// e.g. "Psalms 1".
    pub reference: String,
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
        /// What was searched for, as the status line says it: each phrase
        /// in curly quotes, then each word (`SearchQuery::labels`).
        terms: Vec<String>,
        total: i64,
        offset: i64,
        page_size: i64,
        results: Vec<SearchResultDto>,
        /// When what was typed is a book's name alone, that book's first
        /// chapter, offered before the verses; left out otherwise.
        #[serde(skip_serializing_if = "Option::is_none")]
        go_to: Option<GoToDto>,
    },
}

/// Interpret the search box input against one translation: a passage to
/// go to (checking its verses exist in that translation), or a page of
/// text-search results; for a book's name alone, the results and its first
/// chapter to go to (or, while the index isn't ready, just the chapter).
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
            text_search(state, translation_id, terms, offset, None).await
        }
        Interpretation::TextOrBook(terms, book) => {
            let reference =
                passage_reference(book_name(state, book.book_id).unwrap_or("?"), 1, None);
            // The chapter doesn't need the index; offer it alone until the
            // index is ready, rather than nothing.
            if state.search_index.unavailable_reason().is_some() {
                return Ok(SearchDto::Passage {
                    book_id: book.book_id,
                    chapter: book.chapter,
                    verse_start: None,
                    verse_end: None,
                    reference,
                });
            }
            let go_to = GoToDto {
                book_id: book.book_id,
                chapter: book.chapter,
                reference,
            };
            text_search(state, translation_id, terms, offset, Some(go_to)).await
        }
    }
}

/// One page of the verses with every word and phrase of `terms`, in Bible
/// order, with `go_to` passed through.
async fn text_search(
    state: &AppState,
    translation_id: i64,
    terms: SearchQuery,
    offset: i64,
    go_to: Option<GoToDto>,
) -> anyhow::Result<SearchDto> {
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
        .map(|h| {
            let text = VerseText::new(code, &h.text).text;
            SearchResultDto {
                reference: passage_reference(
                    book_name(state, h.book_id).unwrap_or("?"),
                    h.chapter,
                    Some((h.verse, h.verse)),
                ),
                book_id: h.book_id,
                chapter: h.chapter,
                verse: h.verse,
                marks: query::term_marks(&text, &terms),
                text,
            }
        })
        .collect();
    Ok(SearchDto::Text {
        terms: terms.labels(),
        total,
        offset,
        page_size: SEARCH_PAGE_SIZE,
        results,
        go_to,
    })
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::tests::{app_state, search_in};
    use crate::commands::*;
    use crate::db::test_support::{bundled, fresh_database};
    use crate::domain::kjv_markup::VerseText;

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
        // Each word the search matched, where it is in that text.
        assert_eq!(
            cor["marks"],
            serde_json::json!([[16, 21], [23, 27], [29, 36], [80, 87]])
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
    async fn a_book_alone_offers_its_first_chapter_before_the_verses() {
        let (_db, state) = app_state("cmd_search_book").await;

        let v = search_in(&state, "Psalms", "kjv").await;
        assert_eq!(v["kind"], "text");
        assert_eq!(v["terms"], serde_json::json!(["psalms"]));
        assert_eq!(
            v["goTo"],
            serde_json::json!({ "bookId": 19, "chapter": 1, "reference": "Psalms 1" })
        );
        assert!(v["total"].as_i64().unwrap() > 0, "verses still say psalms");
        assert!(!v["results"].as_array().unwrap().is_empty());

        // Every page carries it, Show more's too.
        let v = run_search(&state, "mark", Some("web"), SEARCH_PAGE_SIZE)
            .await
            .unwrap();
        let SearchDto::Text {
            offset,
            go_to: Some(go_to),
            ..
        } = v
        else {
            panic!("expected text results with a go-to");
        };
        assert_eq!(offset, SEARCH_PAGE_SIZE);
        assert_eq!((go_to.book_id, go_to.chapter), (41, 1));
        assert_eq!(go_to.reference, "Mark 1");

        // No verse has the words; the chapter is still offered.
        let v = search_in(&state, "1 cor", "kjv").await;
        assert_eq!(v["kind"], "text");
        assert_eq!(v["total"], 0);
        assert_eq!(v["goTo"]["reference"], "1 Corinthians 1");
        assert_eq!(v["goTo"]["bookId"], 46);

        // Other words have no go-to at all; the shape is as before.
        let v = search_in(&state, "shepherd", "kjv").await;
        assert_eq!(v["kind"], "text");
        assert!(v.get("goTo").is_none());
        let keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(
            keys,
            ["kind", "offset", "pageSize", "results", "terms", "total"]
        );
        // And a reference is a passage, as before.
        assert_eq!(
            search_in(&state, "Psalms 1", "kjv").await,
            serde_json::json!({
                "kind": "passage",
                "bookId": 19,
                "chapter": 1,
                "verseStart": null,
                "verseEnd": null,
                "reference": "Psalms 1"
            })
        );
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
        // Nor does a book's first chapter: offered alone until it's ready.
        assert_eq!(
            search_in(&state, "Psalms", "kjv").await,
            serde_json::json!({
                "kind": "passage",
                "bookId": 19,
                "chapter": 1,
                "verseStart": null,
                "verseEnd": null,
                "reference": "Psalms 1"
            })
        );
        state.search_index.set_failed();
        let v = serde_json::to_value(run_search(&state, "love", Some("kjv"), 0).await.unwrap())
            .unwrap();
        assert_eq!(v["kind"], "unavailable");
        drop(db);
    }
}
