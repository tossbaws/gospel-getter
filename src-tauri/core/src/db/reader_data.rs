//! Reading and writing the reader's own data (bookmarks, highlights, notes
//! and reading position) for export and import. See `crate::reader_data` for the file
//! format. Bundled data — verses, books, translations, cross-references,
//! the search index — is never written here.

use anyhow::Context;
use sqlx::SqlitePool;
use std::collections::{HashMap, HashSet};

use crate::db::notes::{NoteLimit, NoteTotals, note_totals};
use crate::db::{Bookmark, HighlightColor, WritePool};

/// A bookmark ready to insert, after validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewBookmark {
    pub book_id: i64,
    pub chapter: i64,
    pub verse_start: i64,
    pub verse_end: i64,
    /// Normalized to the database's own format; `None` means "now".
    pub created_at: Option<String>,
}

impl NewBookmark {
    pub fn coordinates(&self) -> (i64, i64, i64, i64) {
        (self.book_id, self.chapter, self.verse_start, self.verse_end)
    }
}

/// A highlight ready to save, after validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewHighlight {
    pub book_id: i64,
    pub chapter: i64,
    pub verse: i64,
    pub color: HighlightColor,
    /// Normalized to the database's own format; `None` means "now".
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

impl NewHighlight {
    pub fn coordinates(&self) -> (i64, i64, i64) {
        (self.book_id, self.chapter, self.verse)
    }
}

/// The passage and text of a note. For importing, a note is this and when
/// it was written (see `notes_to_add`).
pub type NoteKey = (i64, i64, i64, i64, String);

/// A note already here, for matching a file's notes against: its passage
/// and text, and when it was written.
pub type StoredNote = (NoteKey, String);

/// Which of a file's `notes` an import adds, in the file's order, given
/// the notes already here (`stored`; none when replacing them).
///
/// A note is its passage, its text and when it was written. Of each such
/// note, the import adds the copies the file has beyond those already
/// here: replacing restores every note in the file, two notes that differ
/// only in their dates are both kept, and merging the same file again adds
/// nothing. A note with no date (a file written by hand, say) is already
/// here if any note here has its passage and text.
pub fn notes_to_add<'a>(notes: &'a [NewNote], stored: &[StoredNote]) -> Vec<&'a NewNote> {
    let mut copies_here: HashMap<(NoteKey, &str), usize> = HashMap::new();
    let mut passages_and_texts: HashSet<&NoteKey> = HashSet::new();
    for (key, created_at) in stored {
        *copies_here
            .entry((key.clone(), created_at.as_str()))
            .or_default() += 1;
        passages_and_texts.insert(key);
    }
    notes
        .iter()
        .filter(|n| match n.created_at.as_deref() {
            Some(created_at) => match copies_here.get_mut(&(n.key(), created_at)) {
                Some(copies) if *copies > 0 => {
                    *copies -= 1;
                    false
                }
                _ => true,
            },
            None => !passages_and_texts.contains(&n.key()),
        })
        .collect()
}

/// A note ready to save, after validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewNote {
    pub book_id: i64,
    pub chapter: i64,
    pub verse_start: i64,
    pub verse_end: i64,
    /// Normalized, as it's stored.
    pub body: String,
    /// Normalized to the database's own format; `None` means "now".
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

impl NewNote {
    pub fn key(&self) -> NoteKey {
        (
            self.book_id,
            self.chapter,
            self.verse_start,
            self.verse_end,
            self.body.clone(),
        )
    }
}

/// A reading position ready to save, after validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewPosition {
    pub translation_id: i64,
    pub translation_code: String,
    pub translation_name: String,
    pub book_id: i64,
    pub book_name: String,
    pub chapter: i64,
}

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

/// The passage, text and date of every note, for telling new from
/// already-present (`reader_data::notes_to_add`).
pub async fn stored_notes<'c>(
    conn: impl sqlx::SqliteExecutor<'c>,
) -> anyhow::Result<Vec<StoredNote>> {
    let rows: Vec<(i64, i64, i64, i64, String, String)> = sqlx::query_as(
        "SELECT book_id, chapter, verse_start, verse_end, body, created_at FROM notes",
    )
    .fetch_all(conn)
    .await
    .context("Failed to read notes")?;
    Ok(rows
        .into_iter()
        .map(|(b, c, s, e, body, created_at)| ((b, c, s, e, body), created_at))
        .collect())
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

/// The file's notes importing `notes` adds (`reader_data::notes_to_add`),
/// and the notes there would be afterwards: with `replace`, just the
/// file's; otherwise the reader's, plus those.
async fn notes_after_import<'a>(
    conn: &mut sqlx::SqliteConnection,
    notes: &'a [NewNote],
    replace: bool,
) -> anyhow::Result<(Vec<&'a NewNote>, NoteTotals)> {
    let (mut totals, stored) = if replace {
        (NoteTotals::default(), Vec::new())
    } else {
        let stored = stored_notes(&mut *conn).await?;
        (note_totals(conn, None).await?, stored)
    };
    let adding = notes_to_add(notes, &stored);
    for n in &adding {
        totals.count += 1;
        totals.chars += n.body.chars().count();
    }
    Ok((adding, totals))
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
/// `notes` likewise, a note being its passage, text and date: merging adds
/// each of the file's notes but those already here
/// (`reader_data::notes_to_add`), so importing the same file again changes
/// nothing; `replace` deletes every note first and restores every one in
/// the file. `None` (a format 1 or 2 file, which can't hold notes) leaves
/// them untouched.
/// Before anything is written, the notes there would be afterwards are
/// checked against `MAX_NOTES` and `MAX_TOTAL_NOTE_CHARS`; an import that
/// would go past either changes nothing.
pub async fn apply_import(
    writer: &WritePool,
    bookmarks: &[NewBookmark],
    highlights: Option<&[NewHighlight]>,
    notes: Option<&[NewNote]>,
    replace: bool,
    position: Option<&NewPosition>,
) -> Result<ImportCounts, ApplyImportError> {
    let mut tx = crate::db::begin_write(writer)
        .await
        .context("Failed to start the import")?;

    let adding_notes = match notes {
        Some(notes) => {
            let (adding, totals) = notes_after_import(&mut tx, notes, replace).await?;
            if let Some(limit) = totals.exceeded() {
                return Err(ApplyImportError::NoteLimit(limit));
            }
            Some(adding)
        }
        None => None,
    };

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
    if let Some(adding) = adding_notes {
        if replace {
            notes_removed = sqlx::query("DELETE FROM notes")
                .execute(&mut *tx)
                .await
                .context("Failed to clear notes")?
                .rows_affected() as usize;
        }
        for n in adding {
            let inserted = sqlx::query(
                "INSERT INTO notes \
                     (book_id, chapter, verse_start, verse_end, body, created_at, updated_at) \
                 VALUES ($1, $2, $3, $4, $5, \
                     COALESCE($6, strftime('%Y-%m-%dT%H:%M:%fZ', 'now')), \
                     COALESCE($7, $6, strftime('%Y-%m-%dT%H:%M:%fZ', 'now')))",
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
        // A backup has no place in its chapter (only this computer keeps
        // one): the restored chapter opens at its top.
        sqlx::query("DELETE FROM reading_place")
            .execute(&mut *tx)
            .await
            .context("Failed to forget the reading place")?;
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
