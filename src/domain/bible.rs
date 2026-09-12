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
    fn next_within_same_book() {
        let books = sample_books();
        let at = ChapterRef {
            book_id: 1,
            chapter: 1,
        };
        assert_eq!(
            next_chapter(&books, at),
            Some(ChapterRef {
                book_id: 1,
                chapter: 2
            })
        );
    }

    #[test]
    fn next_crosses_book_boundary() {
        let books = sample_books();
        let at = ChapterRef {
            book_id: 1,
            chapter: 50,
        };
        assert_eq!(
            next_chapter(&books, at),
            Some(ChapterRef {
                book_id: 2,
                chapter: 1
            })
        );
    }

    #[test]
    fn next_at_end_of_bible_is_none() {
        let books = sample_books();
        let at = ChapterRef {
            book_id: 3,
            chapter: 27,
        };
        assert_eq!(next_chapter(&books, at), None);
    }

    #[test]
    fn previous_within_same_book() {
        let books = sample_books();
        let at = ChapterRef {
            book_id: 2,
            chapter: 5,
        };
        assert_eq!(
            previous_chapter(&books, at),
            Some(ChapterRef {
                book_id: 2,
                chapter: 4
            })
        );
    }

    #[test]
    fn previous_crosses_book_boundary() {
        let books = sample_books();
        let at = ChapterRef {
            book_id: 2,
            chapter: 1,
        };
        assert_eq!(
            previous_chapter(&books, at),
            Some(ChapterRef {
                book_id: 1,
                chapter: 50
            })
        );
    }

    #[test]
    fn previous_at_start_of_bible_is_none() {
        let books = sample_books();
        let at = ChapterRef {
            book_id: 1,
            chapter: 1,
        };
        assert_eq!(previous_chapter(&books, at), None);
    }
}
