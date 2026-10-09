//! Bookmarks: the reader's saved passages. Stored as translation-independent
//! coordinates (book, chapter, inclusive verse range); see the `bookmarks`
//! table in `migrate.rs`.

use anyhow::Context;
use sqlx::{FromRow, SqlitePool};
use std::fmt;

/// A saved passage.
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct Bookmark {
    pub id: i64,
    pub book_id: i64,
    pub chapter: i64,
    pub verse_start: i64,
    pub verse_end: i64,
    /// UTC, ISO 8601 with milliseconds — sorts chronologically as text.
    pub created_at: String,
}

/// A bookmark as it reads in one particular translation.
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct BookmarkInTranslation {
    #[sqlx(flatten)]
    pub bookmark: Bookmark,
    /// The first verse's stored text, or `None` if that translation doesn't
    /// number this verse.
    pub first_verse_text: Option<String>,
    /// Whether the translation numbers every verse in the range. Verse
    /// numbers within a chapter are contiguous, so checking the first and
    /// last is enough.
    pub complete: bool,
}

/// Why a passage couldn't be bookmarked.
#[derive(Debug)]
pub enum BookmarkError {
    /// The verse range is empty, backwards, or starts before verse 1.
    InvalidRange {
        verse_start: i64,
        verse_end: i64,
    },
    /// No such book, or no such chapter in it.
    NoSuchChapter {
        book_id: i64,
        chapter: i64,
    },
    /// The range runs past the last verse any bundled translation numbers
    /// in this chapter.
    NoSuchVerse {
        book_id: i64,
        chapter: i64,
        verse: i64,
        last_verse: i64,
    },
    Database(anyhow::Error),
}

impl fmt::Display for BookmarkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRange {
                verse_start,
                verse_end,
            } => write!(f, "invalid verse range {verse_start}-{verse_end}"),
            Self::NoSuchChapter { book_id, chapter } => {
                write!(f, "book {book_id} has no chapter {chapter}")
            }
            Self::NoSuchVerse {
                book_id,
                chapter,
                verse,
                last_verse,
            } => write!(
                f,
                "book {book_id} chapter {chapter} has no verse {verse} (last is {last_verse})"
            ),
            Self::Database(e) => write!(f, "{e:#}"),
        }
    }
}

impl std::error::Error for BookmarkError {}

impl From<anyhow::Error> for BookmarkError {
    fn from(e: anyhow::Error) -> Self {
        Self::Database(e)
    }
}

impl From<sqlx::Error> for BookmarkError {
    fn from(e: sqlx::Error) -> Self {
        Self::Database(e.into())
    }
}

/// Check that a passage exists: the chapter in the book, and the verses
/// in at least one bundled translation (so a verse only one translation
/// numbers still counts). Shared by bookmarks and highlights.
pub(crate) async fn check_passage(
    conn: &mut sqlx::SqliteConnection,
    book_id: i64,
    chapter: i64,
    verse_start: i64,
    verse_end: i64,
) -> Result<(), BookmarkError> {
    if verse_start < 1 || verse_end < verse_start {
        return Err(BookmarkError::InvalidRange {
            verse_start,
            verse_end,
        });
    }

    let chapter_count: Option<i64> =
        sqlx::query_scalar("SELECT chapter_count FROM books WHERE id = $1")
            .bind(book_id)
            .fetch_optional(&mut *conn)
            .await
            .context("Failed to look up book")?;
    if !chapter_count.is_some_and(|count| (1..=count).contains(&chapter)) {
        return Err(BookmarkError::NoSuchChapter { book_id, chapter });
    }

    let last_verse: Option<i64> =
        sqlx::query_scalar("SELECT MAX(verse) FROM verses WHERE book_id = $1 AND chapter = $2")
            .bind(book_id)
            .bind(chapter)
            .fetch_one(&mut *conn)
            .await
            .context("Failed to look up chapter length")?;
    let last_verse = last_verse.unwrap_or(0);
    if verse_end > last_verse {
        return Err(BookmarkError::NoSuchVerse {
            book_id,
            chapter,
            verse: verse_end,
            last_verse,
        });
    }
    Ok(())
}

/// Bookmark a passage, after checking it exists: the chapter in the book,
/// and the verses in at least one bundled translation (so a verse only one
/// translation numbers can still be bookmarked). Bookmarking a passage
/// that's already bookmarked returns the existing bookmark unchanged.
pub async fn add_bookmark(
    pool: &SqlitePool,
    book_id: i64,
    chapter: i64,
    verse_start: i64,
    verse_end: i64,
) -> Result<Bookmark, BookmarkError> {
    let mut tx = pool
        .begin()
        .await
        .context("Failed to start bookmark transaction")?;
    check_passage(&mut tx, book_id, chapter, verse_start, verse_end).await?;

    sqlx::query(
        "INSERT INTO bookmarks (book_id, chapter, verse_start, verse_end) \
         VALUES ($1, $2, $3, $4) \
         ON CONFLICT (book_id, chapter, verse_start, verse_end) DO NOTHING",
    )
    .bind(book_id)
    .bind(chapter)
    .bind(verse_start)
    .bind(verse_end)
    .execute(&mut *tx)
    .await
    .context("Failed to insert bookmark")?;

    let bookmark = sqlx::query_as::<_, Bookmark>(
        "SELECT id, book_id, chapter, verse_start, verse_end, created_at FROM bookmarks \
         WHERE book_id = $1 AND chapter = $2 AND verse_start = $3 AND verse_end = $4",
    )
    .bind(book_id)
    .bind(chapter)
    .bind(verse_start)
    .bind(verse_end)
    .fetch_one(&mut *tx)
    .await
    .context("Failed to read back bookmark")?;

    tx.commit()
        .await
        .context("Failed to commit bookmark transaction")?;
    Ok(bookmark)
}

/// How a passage reads in translation `$1`, as SQL shared by bookmarks
/// and highlighted passages so both preview a passage the same way. For
/// a passage row `p` (with `book_id`, `chapter`, `verse_start` and
/// `verse_end`), `passage_preview_sql!(columns)` selects
/// `first_verse_text` (the first verse's stored text, or NULL if the
/// translation doesn't number it) and `complete` (whether it numbers the
/// whole range; verse numbers within a chapter are contiguous, so the
/// first and last are enough), from the verses `passage_preview_sql!(joins)`
/// joins in.
macro_rules! passage_preview_sql {
    (columns) => {
        "first.text AS first_verse_text, \
         (first.verse IS NOT NULL AND last.verse IS NOT NULL) AS complete"
    };
    (joins) => {
        "LEFT JOIN verses first ON first.translation_id = $1 AND first.book_id = p.book_id \
             AND first.chapter = p.chapter AND first.verse = p.verse_start \
         LEFT JOIN verses last ON last.translation_id = $1 AND last.book_id = p.book_id \
             AND last.chapter = p.chapter AND last.verse = p.verse_end"
    };
}
pub(crate) use passage_preview_sql;

/// Every bookmark, newest first, with its first verse's text in one
/// translation.
pub async fn list_bookmarks(
    pool: &SqlitePool,
    translation_id: i64,
) -> anyhow::Result<Vec<BookmarkInTranslation>> {
    sqlx::query_as::<_, BookmarkInTranslation>(concat!(
        "SELECT p.id, p.book_id, p.chapter, p.verse_start, p.verse_end, p.created_at, ",
        passage_preview_sql!(columns),
        " FROM bookmarks p ",
        passage_preview_sql!(joins),
        " ORDER BY p.created_at DESC, p.id DESC",
    ))
    .bind(translation_id)
    .fetch_all(pool)
    .await
    .context("Failed to fetch bookmarks")
}

/// Remove a bookmark. Returns whether there was one to remove.
pub async fn remove_bookmark(pool: &SqlitePool, id: i64) -> anyhow::Result<bool> {
    let result = sqlx::query("DELETE FROM bookmarks WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
        .context("Failed to delete bookmark")?;
    Ok(result.rows_affected() > 0)
}
