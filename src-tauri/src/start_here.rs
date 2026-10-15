//! Where to start reading: the welcome card's short list of places to
//! begin, from `data/start_here.json`, bundled with the app. Each entry
//! names a book by its canonical number (Genesis = 1 … Revelation = 66), a
//! chapter, whether its title is the book's name ("Mark") or the chapter's
//! ("Genesis 1"), and a line about it. Book names come from the book data,
//! so the file holds no names of its own. The list can grow into reading
//! paths later; for now it's these few chapters.

use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

use crate::db::models::Book;

/// The bundled list, as written.
const DATA: &str = include_str!("../data/start_here.json");

/// One entry of `data/start_here.json`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    book: i64,
    chapter: i64,
    title: Title,
    line: String,
}

/// What an entry is called: its book ("Mark"), or its book and chapter
/// ("Genesis 1").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Title {
    Book,
    Chapter,
}

/// The entries, read once. The file is bundled and tested, so it always
/// parses; if it somehow didn't, there'd be nowhere to start listed, and
/// the welcome would still work.
static ENTRIES: LazyLock<Vec<Entry>> = LazyLock::new(|| {
    serde_json::from_str(DATA).unwrap_or_else(|e| {
        tracing::error!("start_here.json doesn't parse: {e}");
        Vec::new()
    })
});

/// A place to start, for the welcome card: opening it reads `chapter` of
/// book `book_id`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartHereDto {
    pub book_id: i64,
    pub chapter: i64,
    pub title: String,
    pub line: String,
}

/// The places to start, in order, named from `books`. An entry whose book
/// or chapter isn't in `books` is left out.
pub fn start_here(books: &[Book]) -> Vec<StartHereDto> {
    resolve(&ENTRIES, books)
}

fn resolve(entries: &[Entry], books: &[Book]) -> Vec<StartHereDto> {
    entries
        .iter()
        .filter_map(|entry| {
            let book = books.iter().find(|b| b.id == entry.book)?;
            if !(1..=book.chapter_count).contains(&entry.chapter) {
                return None;
            }
            let title = match entry.title {
                Title::Book => book.name.clone(),
                Title::Chapter => format!("{} {}", book.name, entry.chapter),
            };
            Some(StartHereDto {
                book_id: book.id,
                chapter: entry.chapter,
                title,
                line: entry.line.clone(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The 66 books as the bundled KJV has them, numbered in order, as the
    /// database seeds them.
    fn books() -> Vec<Book> {
        let kjv: serde_json::Value =
            serde_json::from_str(include_str!("../data/kjv.json").trim_start_matches('\u{feff}'))
                .unwrap();
        kjv.as_array()
            .unwrap()
            .iter()
            .zip(1..)
            .map(|(book, id)| Book {
                id,
                name: book["name"].as_str().unwrap().to_owned(),
                testament: if id <= 39 { "OT" } else { "NT" }.to_owned(),
                chapter_count: i64::try_from(book["chapters"].as_array().unwrap().len()).unwrap(),
            })
            .collect()
    }

    /// Every entry names a book that exists and a chapter it has, with a
    /// line to show: none is dropped.
    #[test]
    fn every_entry_is_a_real_chapter() {
        let entries: Vec<Entry> = serde_json::from_str(DATA).unwrap();
        assert!(!entries.is_empty());
        let books = books();
        for entry in &entries {
            let book = books
                .iter()
                .find(|b| b.id == entry.book)
                .unwrap_or_else(|| panic!("no book {}", entry.book));
            assert!(
                (1..=book.chapter_count).contains(&entry.chapter),
                "{} has no chapter {}",
                book.name,
                entry.chapter
            );
            assert!(!entry.line.trim().is_empty(), "{}", book.name);
        }
        assert_eq!(start_here(&books).len(), entries.len());
    }

    /// The owner's list, word for word, in order.
    #[test]
    fn the_list_is_mark_john_psalms_and_genesis_1() {
        let rows: Vec<_> = start_here(&books())
            .into_iter()
            .map(|r| (r.title, r.book_id, r.chapter, r.line))
            .collect();
        let row = |title: &str, book, chapter, line: &str| {
            (title.to_owned(), book, chapter, line.to_owned())
        };
        assert_eq!(
            rows,
            [
                row(
                    "Mark",
                    41,
                    1,
                    "The shortest Gospel, 16 chapters. A good first read."
                ),
                row("John", 43, 1, "Who Jesus is, in his own words."),
                row("Psalms", 19, 1, "Prayers for every mood. Read one a day."),
                row("Genesis 1", 1, 1, "Start at the beginning."),
            ]
        );
        // "16 chapters" is Mark's own count.
        assert_eq!(books()[40].chapter_count, 16);
    }

    /// An entry for a book or chapter that isn't there is left out, not
    /// shown with a made-up name.
    #[test]
    fn an_entry_outside_the_bible_is_left_out() {
        let entries: Vec<Entry> = serde_json::from_str(
            r#"[{"book": 67, "chapter": 1, "title": "book", "line": "x"},
                {"book": 41, "chapter": 17, "title": "chapter", "line": "x"},
                {"book": 41, "chapter": 0, "title": "chapter", "line": "x"},
                {"book": 41, "chapter": 16, "title": "chapter", "line": "The end."}]"#,
        )
        .unwrap();
        let rows = resolve(&entries, &books());
        assert_eq!(
            rows,
            [StartHereDto {
                book_id: 41,
                chapter: 16,
                title: "Mark 16".to_owned(),
                line: "The end.".to_owned(),
            }]
        );
        assert!(
            serde_json::from_str::<Vec<Entry>>(
                r#"[{"book": 1, "chapter": 1, "title": "book", "line": "x", "extra": 1}]"#
            )
            .is_err()
        );
    }

    /// The shape the page reads.
    #[test]
    fn it_serializes_for_the_page() {
        let dto = &start_here(&books())[0];
        assert_eq!(
            serde_json::to_value(dto).unwrap(),
            serde_json::json!({
                "bookId": 41,
                "chapter": 1,
                "title": "Mark",
                "line": "The shortest Gospel, 16 chapters. A good first read."
            })
        );
    }
}
