//! Reads (and the one write) that belong to no single feature: the books
//! and translations, a chapter's verses and cross-references, verse
//! ranges, and the reading position, with where in its chapter the reader
//! was.

use anyhow::Context;
use sqlx::SqlitePool;

use crate::db::models::{Book, CrossReference, Translation, Verse};
use crate::db::{WritePool, begin_write};
use crate::domain::usfm_note::StoredFootnote;

/// Get every book, in canonical order.
pub async fn get_all_books(pool: &SqlitePool) -> anyhow::Result<Vec<Book>> {
    sqlx::query_as::<_, Book>("SELECT id, name, testament, chapter_count FROM books ORDER BY id")
        .fetch_all(pool)
        .await
        .context("Failed to fetch books")
}

/// Get every bundled translation, in the order they were seeded.
pub async fn get_all_translations(pool: &SqlitePool) -> anyhow::Result<Vec<Translation>> {
    sqlx::query_as::<_, Translation>("SELECT id, code, name FROM translations ORDER BY id")
        .fetch_all(pool)
        .await
        .context("Failed to fetch translations")
}

/// The highest verse number any translation has in one chapter (0 if none
/// has the chapter).
pub async fn get_chapter_last_verse(
    pool: &SqlitePool,
    book_id: i64,
    chapter: i64,
) -> anyhow::Result<i64> {
    let last: Option<i64> =
        sqlx::query_scalar("SELECT MAX(verse) FROM verses WHERE book_id = $1 AND chapter = $2")
            .bind(book_id)
            .bind(chapter)
            .fetch_one(pool)
            .await
            .context("Failed to look up the chapter's last verse")?;
    Ok(last.unwrap_or(0))
}

/// Get every verse in one chapter of one translation, in verse order.
pub async fn get_chapter_verses(
    pool: &SqlitePool,
    translation_id: i64,
    book_id: i64,
    chapter: i64,
) -> anyhow::Result<Vec<Verse>> {
    let mut verses = sqlx::query_as::<_, Verse>(
        "SELECT verse, text FROM verses \
         WHERE translation_id = $1 AND book_id = $2 AND chapter = $3 \
         ORDER BY verse",
    )
    .bind(translation_id)
    .bind(book_id)
    .bind(chapter)
    .fetch_all(pool)
    .await
    .context("Failed to fetch verses")?;
    let notes: Vec<(i64, i64, String)> = sqlx::query_as(
        "SELECT verse, position, usfm FROM footnotes \
         WHERE translation_id = $1 AND book_id = $2 AND chapter = $3 \
         ORDER BY verse, ordinal",
    )
    .bind(translation_id)
    .bind(book_id)
    .bind(chapter)
    .fetch_all(pool)
    .await
    .context("Failed to fetch footnotes")?;
    for (verse, position, usfm) in notes {
        if let Some(v) = verses.iter_mut().find(|v| v.verse == verse) {
            v.footnotes.push(StoredFootnote {
                position: usize::try_from(position).unwrap_or(0),
                usfm,
            });
        }
    }
    Ok(verses)
}

/// The last chapter read (translation, book, chapter), if the reader has
/// ever actually read one.
pub async fn get_reading_position(pool: &SqlitePool) -> anyhow::Result<Option<(i64, i64, i64)>> {
    sqlx::query_as("SELECT translation_id, book_id, chapter FROM reading_position WHERE id = 1")
        .fetch_optional(pool)
        .await
        .context("Failed to fetch reading position")
}

/// The verse at the top of the reading pane, and how far its top was below
/// the bar, in pixels (`None`: at the bar's bottom).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TopVerse {
    pub verse: i64,
    pub offset_px: Option<f64>,
}

/// The last chapter read (translation, book, chapter) and where in it the
/// reader was (`at`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReadingPlace {
    pub translation_id: i64,
    pub book_id: i64,
    pub chapter: i64,
    pub at: Option<TopVerse>,
}

/// The last chapter read, if the reader has ever read one, with the verse
/// at the top of the reading pane (`reading_place`) only while it's for
/// that translation, book and chapter: any other (an older app saved
/// another chapter, not knowing the verse) is none, and the chapter opens
/// at its top. One statement, so both are from the same save.
pub async fn get_reading_place(pool: &SqlitePool) -> anyhow::Result<Option<ReadingPlace>> {
    /// The position, and its place's verse and offset if it has one.
    #[derive(sqlx::FromRow)]
    struct Row {
        translation_id: i64,
        book_id: i64,
        chapter: i64,
        verse: Option<i64>,
        offset_px: Option<f64>,
    }
    let found: Option<Row> = sqlx::query_as(
        "SELECT position.translation_id, position.book_id, position.chapter, \
             place.verse, place.offset_px \
         FROM reading_position AS position LEFT JOIN reading_place AS place \
             ON place.id = 1 \
             AND place.translation_id = position.translation_id \
             AND place.book_id = position.book_id \
             AND place.chapter = position.chapter \
         WHERE position.id = 1",
    )
    .fetch_optional(pool)
    .await
    .context("Failed to fetch the reading place")?;
    Ok(found.map(|row| ReadingPlace {
        translation_id: row.translation_id,
        book_id: row.book_id,
        chapter: row.chapter,
        at: row.verse.map(|verse| TopVerse {
            verse,
            offset_px: row.offset_px,
        }),
    }))
}

/// Remember the last chapter read, at its top, overwriting whatever was
/// there before (and forgetting where in the last one the reader was).
pub async fn save_reading_position(
    writer: &WritePool,
    translation_id: i64,
    book_id: i64,
    chapter: i64,
) -> anyhow::Result<()> {
    save_reading_position_at(writer, translation_id, book_id, chapter, None).await
}

/// Remember the last chapter read and where in it the reader was (`at`:
/// the verse at the top of the reading pane; `None`, its top), together,
/// overwriting both.
pub async fn save_reading_position_at(
    writer: &WritePool,
    translation_id: i64,
    book_id: i64,
    chapter: i64,
    at: Option<TopVerse>,
) -> anyhow::Result<()> {
    let mut tx = begin_write(writer)
        .await
        .context("Failed to start saving the reading position")?;
    sqlx::query(
        "INSERT INTO reading_position (id, translation_id, book_id, chapter) \
         VALUES (1, $1, $2, $3) \
         ON CONFLICT(id) DO UPDATE SET \
             translation_id = excluded.translation_id, \
             book_id = excluded.book_id, \
             chapter = excluded.chapter",
    )
    .bind(translation_id)
    .bind(book_id)
    .bind(chapter)
    .execute(&mut *tx)
    .await
    .context("Failed to save reading position")?;
    match at {
        Some(TopVerse { verse, offset_px }) => sqlx::query(
            "INSERT INTO reading_place \
                 (id, translation_id, book_id, chapter, verse, offset_px) \
             VALUES (1, $1, $2, $3, $4, $5) \
             ON CONFLICT(id) DO UPDATE SET \
                 translation_id = excluded.translation_id, \
                 book_id = excluded.book_id, \
                 chapter = excluded.chapter, \
                 verse = excluded.verse, \
                 offset_px = excluded.offset_px",
        )
        .bind(translation_id)
        .bind(book_id)
        .bind(chapter)
        .bind(verse)
        .bind(offset_px),
        None => sqlx::query("DELETE FROM reading_place"),
    }
    .execute(&mut *tx)
    .await
    .context("Failed to save the reading place")?;
    tx.commit()
        .await
        .context("Failed to finish saving the reading position")?;
    Ok(())
}

/// Cross-references for every verse in one chapter, ordered by verse then
/// descending relevance score.
pub async fn get_chapter_cross_references(
    pool: &SqlitePool,
    book_id: i64,
    chapter: i64,
) -> anyhow::Result<Vec<CrossReference>> {
    sqlx::query_as::<_, CrossReference>(
        "SELECT verse, ref_book_id, ref_chapter, ref_verse, ref_end_verse, score \
         FROM cross_references WHERE book_id = $1 AND chapter = $2 \
         ORDER BY verse, score DESC",
    )
    .bind(book_id)
    .bind(chapter)
    .fetch_all(pool)
    .await
    .context("Failed to fetch cross references")
}

/// The text of a verse or verse range, for expanding a cross-reference.
pub async fn get_verse_range(
    pool: &SqlitePool,
    translation_id: i64,
    book_id: i64,
    chapter: i64,
    start_verse: i64,
    end_verse: i64,
) -> anyhow::Result<Vec<Verse>> {
    sqlx::query_as::<_, Verse>(
        "SELECT verse, text FROM verses \
         WHERE translation_id = $1 AND book_id = $2 AND chapter = $3 \
         AND verse BETWEEN $4 AND $5 ORDER BY verse",
    )
    .bind(translation_id)
    .bind(book_id)
    .bind(chapter)
    .bind(start_verse)
    .bind(end_verse)
    .fetch_all(pool)
    .await
    .context("Failed to fetch verse range")
}
