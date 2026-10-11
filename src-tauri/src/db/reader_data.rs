//! Reading and writing the reader's own data (bookmarks, highlights, notes
//! and reading position) for export and import. See `crate::reader_data` for the file
//! format. Bundled data — verses, books, translations, cross-references,
//! the search index — is never written here.

use anyhow::Context;
use sqlx::SqlitePool;
use std::collections::{HashMap, HashSet};

use crate::db::Bookmark;
use crate::db::notes::{NoteLimit, NoteTotals, note_totals};
use crate::reader_data::{NewBookmark, NewHighlight, NewNote, NewPosition};

pub use crate::reader_data::NoteKey;

/// Every bookmark, oldest first (the order they were made).
pub async fn all_bookmarks(pool: &SqlitePool) -> anyhow::Result<Vec<Bookmark>> {
    sqlx::query_as::<_, Bookmark>(
        "SELECT id, book_id, chapter, verse_start, verse_end, created_at FROM bookmarks \
         ORDER BY created_at, id",
    )
    .fetch_all(pool)
    .await
    .context("Failed to read bookmarks for export")
}

/// The coordinates of every bookmark, for telling new from already-present.
pub async fn bookmark_coordinates(
    pool: &SqlitePool,
) -> anyhow::Result<HashSet<(i64, i64, i64, i64)>> {
    let rows: Vec<(i64, i64, i64, i64)> =
        sqlx::query_as("SELECT book_id, chapter, verse_start, verse_end FROM bookmarks")
            .fetch_all(pool)
            .await
            .context("Failed to read bookmarks")?;
    Ok(rows.into_iter().collect())
}

/// The coordinates of every highlighted verse, for telling new from
/// already-highlighted.
pub async fn highlight_coordinates(pool: &SqlitePool) -> anyhow::Result<HashSet<(i64, i64, i64)>> {
    let rows: Vec<(i64, i64, i64)> =
        sqlx::query_as("SELECT book_id, chapter, verse FROM highlights")
            .fetch_all(pool)
            .await
            .context("Failed to read highlights")?;
    Ok(rows.into_iter().collect())
}

/// The passage and text of every note, for telling new from
/// already-present.
pub async fn note_keys(pool: &SqlitePool) -> anyhow::Result<HashSet<NoteKey>> {
    let rows: Vec<NoteKey> =
        sqlx::query_as("SELECT book_id, chapter, verse_start, verse_end, body FROM notes")
            .fetch_all(pool)
            .await
            .context("Failed to read notes")?;
    Ok(rows.into_iter().collect())
}

/// The highest verse number any installed translation has, per
/// `(book, chapter)` — the limit a bookmark's range is checked against.
pub async fn last_verses(pool: &SqlitePool) -> anyhow::Result<HashMap<(i64, i64), i64>> {
    let rows: Vec<(i64, i64, i64)> =
        sqlx::query_as("SELECT book_id, chapter, MAX(verse) FROM verses GROUP BY book_id, chapter")
            .fetch_all(pool)
            .await
            .context("Failed to read chapter lengths")?;
    Ok(rows.into_iter().map(|(b, c, v)| ((b, c), v)).collect())
}

/// The current time in the database's timestamp format.
pub async fn now(pool: &SqlitePool) -> anyhow::Result<String> {
    sqlx::query_scalar("SELECT strftime('%Y-%m-%dT%H:%M:%fZ', 'now')")
        .fetch_one(pool)
        .await
        .context("Failed to read the time")
}

/// Why an import couldn't be applied. Either way, nothing was changed.
#[derive(Debug)]
pub enum ApplyImportError {
    /// The notes it would leave would go past a limit.
    NoteLimit(NoteLimit),
    Database(anyhow::Error),
}

impl std::fmt::Display for ApplyImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoteLimit(limit) => write!(f, "the import would go past a limit: {limit}"),
            Self::Database(e) => write!(f, "{e:#}"),
        }
    }
}

impl std::error::Error for ApplyImportError {}

impl From<anyhow::Error> for ApplyImportError {
    fn from(e: anyhow::Error) -> Self {
        Self::Database(e)
    }
}

/// The notes there would be after importing `notes`: with `replace`, just
/// the file's; otherwise the reader's, plus the file's that aren't already
/// here.
async fn notes_after_import(
    conn: &mut sqlx::SqliteConnection,
    notes: &[NewNote],
    replace: bool,
) -> anyhow::Result<NoteTotals> {
    let (mut totals, existing) = if replace {
        (NoteTotals::default(), HashSet::new())
    } else {
        let keys: Vec<NoteKey> =
            sqlx::query_as("SELECT book_id, chapter, verse_start, verse_end, body FROM notes")
                .fetch_all(&mut *conn)
                .await
                .context("Failed to read notes")?;
        (note_totals(conn, None).await?, keys.into_iter().collect())
    };
    for n in notes.iter().filter(|n| !existing.contains(&n.key())) {
        totals.count += 1;
        totals.chars += n.body.chars().count();
    }
    Ok(totals)
}

/// What an import changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImportCounts {
    pub added: usize,
    pub already_present: usize,
    pub removed: usize,
    pub highlights_added: usize,
    pub highlights_already_present: usize,
    pub highlights_removed: usize,
    pub notes_added: usize,
    pub notes_already_present: usize,
    pub notes_removed: usize,
}

/// Apply a validated import in one transaction: either every change
/// happens or none does.
///
/// With `replace`, every existing bookmark is deleted first; otherwise the
/// file's bookmarks are merged in and any already bookmarked (same book,
/// chapter and verse range) are left exactly as they are, so importing the
/// same file again changes nothing. Imported bookmarks keep their dates.
/// `position`, if given, becomes the saved reading position.
///
/// `highlights` work the same way, a verse at a time: merging adds the
/// file's highlights for verses that have none and keeps the reader's own
/// color on any that do; `replace` deletes every highlight first. `None`
/// (a format 1 file, which can't hold highlights) leaves them untouched,
/// even with `replace`.
///
/// `notes` likewise: merging adds each of the file's notes unless one with
/// the same passage and text is already here, so importing the same file
/// again changes nothing; `replace` deletes every note first. `None` (a
/// format 1 or 2 file, which can't hold notes) leaves them untouched.
/// Before anything is written, the notes there would be afterwards are
/// checked against `MAX_NOTES` and `MAX_TOTAL_NOTE_CHARS`; an import that
/// would go past either changes nothing.
pub async fn apply_import(
    pool: &SqlitePool,
    bookmarks: &[NewBookmark],
    highlights: Option<&[NewHighlight]>,
    notes: Option<&[NewNote]>,
    replace: bool,
    position: Option<&NewPosition>,
) -> Result<ImportCounts, ApplyImportError> {
    let mut tx = pool.begin().await.context("Failed to start the import")?;

    if let Some(notes) = notes
        && let Some(limit) = notes_after_import(&mut tx, notes, replace)
            .await?
            .exceeded()
    {
        return Err(ApplyImportError::NoteLimit(limit));
    }

    let removed = if replace {
        sqlx::query("DELETE FROM bookmarks")
            .execute(&mut *tx)
            .await
            .context("Failed to clear bookmarks")?
            .rows_affected() as usize
    } else {
        0
    };

    let mut added = 0;
    for b in bookmarks {
        let inserted = sqlx::query(
            "INSERT INTO bookmarks (book_id, chapter, verse_start, verse_end, created_at) \
             VALUES ($1, $2, $3, $4, COALESCE($5, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))) \
             ON CONFLICT (book_id, chapter, verse_start, verse_end) DO NOTHING",
        )
        .bind(b.book_id)
        .bind(b.chapter)
        .bind(b.verse_start)
        .bind(b.verse_end)
        .bind(b.created_at.as_deref())
        .execute(&mut *tx)
        .await
        .context("Failed to add a bookmark")?
        .rows_affected();
        added += inserted as usize;
    }

    let mut highlights_added = 0;
    let mut highlights_removed = 0;
    if let Some(highlights) = highlights {
        if replace {
            highlights_removed = sqlx::query("DELETE FROM highlights")
                .execute(&mut *tx)
                .await
                .context("Failed to clear highlights")?
                .rows_affected() as usize;
        }
        for h in highlights {
            let inserted = sqlx::query(
                "INSERT INTO highlights (book_id, chapter, verse, color, created_at, updated_at) \
                 VALUES ($1, $2, $3, $4, \
                     COALESCE($5, strftime('%Y-%m-%dT%H:%M:%fZ', 'now')), \
                     COALESCE($6, $5, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))) \
                 ON CONFLICT (book_id, chapter, verse) DO NOTHING",
            )
            .bind(h.book_id)
            .bind(h.chapter)
            .bind(h.verse)
            .bind(h.color.as_str())
            .bind(h.created_at.as_deref())
            .bind(h.updated_at.as_deref())
            .execute(&mut *tx)
            .await
            .context("Failed to add a highlight")?
            .rows_affected();
            highlights_added += inserted as usize;
        }
    }

    let mut notes_added = 0;
    let mut notes_removed = 0;
    if let Some(notes) = notes {
        if replace {
            notes_removed = sqlx::query("DELETE FROM notes")
                .execute(&mut *tx)
                .await
                .context("Failed to clear notes")?
                .rows_affected() as usize;
        }
        for n in notes {
            let inserted = sqlx::query(
                "INSERT INTO notes \
                     (book_id, chapter, verse_start, verse_end, body, created_at, updated_at) \
                 SELECT $1, $2, $3, $4, $5, \
                     COALESCE($6, strftime('%Y-%m-%dT%H:%M:%fZ', 'now')), \
                     COALESCE($7, $6, strftime('%Y-%m-%dT%H:%M:%fZ', 'now')) \
                 WHERE NOT EXISTS (SELECT 1 FROM notes WHERE book_id = $1 AND chapter = $2 \
                     AND verse_start = $3 AND verse_end = $4 AND body = $5)",
            )
            .bind(n.book_id)
            .bind(n.chapter)
            .bind(n.verse_start)
            .bind(n.verse_end)
            .bind(&n.body)
            .bind(n.created_at.as_deref())
            .bind(n.updated_at.as_deref())
            .execute(&mut *tx)
            .await
            .context("Failed to add a note")?
            .rows_affected();
            notes_added += inserted as usize;
        }
    }

    if let Some(p) = position {
        sqlx::query(
            "INSERT INTO reading_position (id, translation_id, book_id, chapter) \
             VALUES (1, $1, $2, $3) \
             ON CONFLICT(id) DO UPDATE SET \
                 translation_id = excluded.translation_id, \
                 book_id = excluded.book_id, \
                 chapter = excluded.chapter",
        )
        .bind(p.translation_id)
        .bind(p.book_id)
        .bind(p.chapter)
        .execute(&mut *tx)
        .await
        .context("Failed to save the reading position")?;
    }

    tx.commit().await.context("Failed to finish the import")?;
    Ok(ImportCounts {
        added,
        already_present: bookmarks.len() - added,
        removed,
        highlights_added,
        highlights_already_present: highlights.map_or(0, <[_]>::len) - highlights_added,
        highlights_removed,
        notes_added,
        notes_already_present: notes.map_or(0, <[_]>::len) - notes_added,
        notes_removed,
    })
}
