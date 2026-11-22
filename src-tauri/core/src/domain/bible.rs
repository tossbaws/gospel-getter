//! Moving through the Bible: the chapter or book before and after one, by
//! the books' canonical order and chapter counts.

use crate::db::Book;

/// A reference to one chapter, identified by its book and chapter number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChapterRef {
    pub book_id: i64,
    pub chapter: i64,
}

/// The chapter immediately before `at`, crossing into the previous book once
/// chapter 1 is reached. Returns `None` at the very start of the Bible
/// (Genesis 1) so callers know there's nowhere further back to go.
pub fn previous_chapter(books: &[Book], at: ChapterRef) -> Option<ChapterRef> {
    if at.chapter > 1 {
        return Some(ChapterRef {
            book_id: at.book_id,
            chapter: at.chapter - 1,
        });
    }
    let prev_book = books
        .iter()
        .filter(|b| b.id < at.book_id)
        .max_by_key(|b| b.id)?;
    Some(ChapterRef {
        book_id: prev_book.id,
        chapter: prev_book.chapter_count,
    })
}

/// The chapter immediately after `at`, crossing into the next book once the
/// current book's last chapter is reached. Returns `None` at the very end of
/// the Bible (Revelation 22).
pub fn next_chapter(books: &[Book], at: ChapterRef) -> Option<ChapterRef> {
    let book = books.iter().find(|b| b.id == at.book_id)?;
    if at.chapter < book.chapter_count {
        return Some(ChapterRef {
            book_id: at.book_id,
            chapter: at.chapter + 1,
        });
    }
    let next_book = books
        .iter()
        .filter(|b| b.id > at.book_id)
        .min_by_key(|b| b.id)?;
    Some(ChapterRef {
        book_id: next_book.id,
        chapter: 1,
    })
}

/// Chapter 1 of the book before `at`'s, as `[` goes to it; `None` in
/// Genesis.
pub fn previous_book(books: &[Book], at: ChapterRef) -> Option<ChapterRef> {
    let book = books
        .iter()
        .filter(|b| b.id < at.book_id)
        .max_by_key(|b| b.id)?;
    Some(ChapterRef {
        book_id: book.id,
        chapter: 1,
    })
}

/// Chapter 1 of the book after `at`'s, as `]` goes to it; `None` in
/// Revelation.
pub fn next_book(books: &[Book], at: ChapterRef) -> Option<ChapterRef> {
    let book = books
        .iter()
        .filter(|b| b.id > at.book_id)
        .min_by_key(|b| b.id)?;
    Some(ChapterRef {
        book_id: book.id,
        chapter: 1,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_books() -> Vec<Book> {
        vec![
            Book {
                id: 1,
                name: "Genesis".to_string(),
                testament: "OT".to_string(),
                chapter_count: 50,
            },
            Book {
                id: 2,
                name: "Exodus".to_string(),
                testament: "OT".to_string(),
                chapter_count: 40,
            },
            Book {
                id: 3,
                name: "Leviticus".to_string(),
                testament: "OT".to_string(),
                chapter_count: 27,
            },
        ]
    }

    #[test]
    fn next_chapter_steps_forward_across_books_and_stops_at_the_end() {
        let books = sample_books();
        let at = |book_id, chapter| ChapterRef { book_id, chapter };
        let rows = [
            ("within the same book", at(1, 1), Some(at(1, 2))),
            ("across a book boundary", at(1, 50), Some(at(2, 1))),
            ("at the end of the Bible", at(3, 27), None),
        ];
        for (row, from, expected) in rows {
            assert_eq!(next_chapter(&books, from), expected, "{row}");
        }
    }

    #[test]
    fn previous_chapter_steps_back_across_books_and_stops_at_the_start() {
        let books = sample_books();
        let at = |book_id, chapter| ChapterRef { book_id, chapter };
        let rows = [
            ("within the same book", at(2, 5), Some(at(2, 4))),
            ("across a book boundary", at(2, 1), Some(at(1, 50))),
            ("at the start of the Bible", at(1, 1), None),
        ];
        for (row, from, expected) in rows {
            assert_eq!(previous_chapter(&books, from), expected, "{row}");
        }
    }

    #[test]
    fn book_steps_go_to_chapter_one_of_the_book_beside() {
        let books = sample_books();
        let at = |book_id, chapter| ChapterRef { book_id, chapter };
        assert_eq!(next_book(&books, at(1, 37)), Some(at(2, 1)));
        assert_eq!(next_book(&books, at(2, 1)), Some(at(3, 1)));
        assert_eq!(next_book(&books, at(3, 27)), None);
        assert_eq!(previous_book(&books, at(3, 5)), Some(at(2, 1)));
        assert_eq!(previous_book(&books, at(2, 40)), Some(at(1, 1)));
        assert_eq!(previous_book(&books, at(1, 1)), None);
        assert_eq!(previous_book(&books, at(1, 50)), None);
    }
}
