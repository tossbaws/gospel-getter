//! Notes: the reader's own words on a passage, like writing in the margin
//! of a paper Bible. A note belongs to a passage within one chapter (book,
//! chapter, inclusive verse range) and to no translation, so it shows in
//! every translation that numbers those verses. A passage can have any
//! number of notes, kept in the order they were written; see the `notes`
//! table in `migrate.rs`.

use anyhow::Context;
use sqlx::{FromRow, SqlitePool};
use std::cmp::Ordering;
use std::fmt;

use super::bookmarks::check_passage;
use super::{BookmarkError, WritePool};

/// The most characters (Unicode scalar values, as SQLite's `length()`
/// counts them) one note may hold, after trimming. The `notes` table's
/// CHECK says the same.
pub const MAX_NOTE_CHARS: usize = 10_000;

/// The most notes the reader can keep. An export holds them all, and an
/// import accepts no more, so every export imports again.
pub const MAX_NOTES: usize = 10_000;

/// The most characters all the notes together may hold (as SQLite's
/// `length()` counts them), for the same reason: it bounds the size of an
/// export (see `reader_data::MAX_FILE_BYTES`).
pub const MAX_TOTAL_NOTE_CHARS: usize = 25_000_000;

/// A passage within one chapter: book, chapter and an inclusive verse
/// range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Passage {
    pub book_id: i64,
    pub chapter: i64,
    pub verse_start: i64,
    pub verse_end: i64,
}

impl Passage {
    /// Whether the two passages share at least one verse: the same book
    /// and chapter, and neither range ends before the other starts.
    pub fn overlaps(&self, other: &Passage) -> bool {
        self.book_id == other.book_id
            && self.chapter == other.chapter
            && self.verse_start <= other.verse_end
            && self.verse_end >= other.verse_start
    }
}

/// One note.
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct Note {
    pub id: i64,
    pub book_id: i64,
    pub chapter: i64,
    pub verse_start: i64,
    pub verse_end: i64,
    /// Plain text, trimmed, never empty.
    pub body: String,
    /// UTC, ISO 8601 with milliseconds — sorts chronologically as text.
    pub created_at: String,
    /// When the body was last changed.
    pub updated_at: String,
}

impl Note {
    pub fn passage(&self) -> Passage {
        Passage {
            book_id: self.book_id,
            chapter: self.chapter,
            verse_start: self.verse_start,
            verse_end: self.verse_end,
        }
    }
}

/// The order notes on a chapter are shown in: by first verse, then last
/// verse, then oldest first, then by id (the order they were saved).
fn chapter_order(a: &Note, b: &Note) -> Ordering {
    (a.verse_start, a.verse_end, &a.created_at, a.id).cmp(&(
        b.verse_start,
        b.verse_end,
        &b.created_at,
        b.id,
    ))
}

/// The notes that overlap `passage`, in the order they're shown (see
/// `chapter_order`).
pub fn overlapping(notes: impl IntoIterator<Item = Note>, passage: &Passage) -> Vec<Note> {
    let mut found: Vec<Note> = notes
        .into_iter()
        .filter(|n| n.passage().overlaps(passage))
        .collect();
    found.sort_by(chapter_order);
    found
}

/// Every note in Bible order: by book and chapter, then as on a chapter.
pub fn in_bible_order(mut notes: Vec<Note>) -> Vec<Note> {
    notes.sort_by(|a, b| {
        (a.book_id, a.chapter)
            .cmp(&(b.book_id, b.chapter))
            .then_with(|| chapter_order(a, b))
    });
    notes
}

/// Why a note's text can't be saved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteBodyError {
    /// Nothing but whitespace.
    Empty,
    /// More than `MAX_NOTE_CHARS` characters, after trimming.
    TooLong { chars: usize },
    /// A control character other than a tab or a line break.
    ControlCharacter,
}

/// `n` with thousands separators, e.g. "10,000".
pub fn grouped(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

impl fmt::Display for NoteBodyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("A note can't be empty."),
            Self::TooLong { chars } => write!(
                f,
                "A note can hold up to {} characters; this one has {}.",
                grouped(MAX_NOTE_CHARS),
                grouped(*chars)
            ),
            Self::ControlCharacter => f.write_str(
                "A note can only hold plain text, and this one has a control character.",
            ),
        }
    }
}

impl std::error::Error for NoteBodyError {}

/// A note's text as it's stored: line breaks as `\n`, surrounding
/// whitespace trimmed. It must then be non-empty, at most
/// `MAX_NOTE_CHARS` characters, and plain text — no control characters
/// besides tabs and line breaks. (That also bounds how large a note can
/// grow in an export file; see `reader_data::MAX_FILE_BYTES`.)
pub fn normalize_body(raw: &str) -> Result<String, NoteBodyError> {
    let body = raw.replace("\r\n", "\n").replace('\r', "\n");
    let body = body.trim();
    if body.is_empty() {
        return Err(NoteBodyError::Empty);
    }
    if body
        .chars()
        .any(|c| c.is_control() && c != '\n' && c != '\t')
    {
        return Err(NoteBodyError::ControlCharacter);
    }
    let chars = body.chars().count();
    if chars > MAX_NOTE_CHARS {
        return Err(NoteBodyError::TooLong { chars });
    }
    Ok(body.to_string())
}

/// Which limit on the reader's notes a change would go past.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteLimit {
    /// `MAX_NOTES` notes.
    Count,
    /// `MAX_TOTAL_NOTE_CHARS` characters in all.
    TotalText,
}

impl fmt::Display for NoteLimit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Count => write!(
                f,
                "You've reached the limit of {} notes.",
                grouped(MAX_NOTES)
            ),
            Self::TotalText => write!(
                f,
                "Your notes have reached the total size limit ({} characters).",
                grouped(MAX_TOTAL_NOTE_CHARS)
            ),
        }
    }
}

impl std::error::Error for NoteLimit {}

/// How many notes there are and the characters they hold, the ones the
/// limits count.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NoteTotals {
    pub count: usize,
    pub chars: usize,
}

impl NoteTotals {
    /// Which limit, if any, these totals are past.
    pub fn exceeded(self) -> Option<NoteLimit> {
        if self.count > MAX_NOTES {
            Some(NoteLimit::Count)
        } else if self.chars > MAX_TOTAL_NOTE_CHARS {
            Some(NoteLimit::TotalText)
        } else {
            None
        }
    }
}

/// The notes' totals, leaving out note `except` (one being rewritten).
pub(crate) async fn note_totals(
    conn: &mut sqlx::SqliteConnection,
    except: Option<i64>,
) -> anyhow::Result<NoteTotals> {
    let (count, chars): (i64, i64) = sqlx::query_as(
        "SELECT COUNT(*), COALESCE(SUM(length(body)), 0) FROM notes \
         WHERE $1 IS NULL OR id <> $1",
    )
    .bind(except)
    .fetch_one(&mut *conn)
    .await
    .context("Failed to count notes")?;
    Ok(NoteTotals {
        count: usize::try_from(count).context("negative note count")?,
        chars: usize::try_from(chars).context("negative note length")?,
    })
}

/// Why a note couldn't be saved.
#[derive(Debug)]
pub enum NoteError {
    /// The passage doesn't exist (the same checks as bookmarks).
    Passage(BookmarkError),
    Body(NoteBodyError),
    /// No note has this id (it was deleted).
    NotFound {
        id: i64,
    },
    /// Saving it would go past a limit on all the reader's notes.
    Limit(NoteLimit),
    Database(anyhow::Error),
}

impl fmt::Display for NoteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Passage(e) => write!(f, "{e}"),
            Self::Body(e) => write!(f, "{e}"),
            Self::NotFound { id } => write!(f, "there's no note {id}"),
            Self::Limit(limit) => write!(f, "{limit}"),
            Self::Database(e) => write!(f, "{e:#}"),
        }
    }
}

impl std::error::Error for NoteError {}

impl From<BookmarkError> for NoteError {
    fn from(e: BookmarkError) -> Self {
        match e {
            BookmarkError::Database(e) => Self::Database(e),
            other => Self::Passage(other),
        }
    }
}

impl From<NoteLimit> for NoteError {
    fn from(e: NoteLimit) -> Self {
        Self::Limit(e)
    }
}

impl From<NoteBodyError> for NoteError {
    fn from(e: NoteBodyError) -> Self {
        Self::Body(e)
    }
}

impl From<anyhow::Error> for NoteError {
    fn from(e: anyhow::Error) -> Self {
        Self::Database(e)
    }
}

impl From<sqlx::Error> for NoteError {
    fn from(e: sqlx::Error) -> Self {
        Self::Database(e.into())
    }
}

/// The columns a `Note` is read from.
macro_rules! note_columns {
    () => {
        "id, book_id, chapter, verse_start, verse_end, body, created_at, updated_at"
    };
}

/// Save a new note on `passage`, after checking the passage exists (the
/// same rule as bookmarks) and normalizing its text (see
/// `normalize_body`) and that one more note, with this text, stays within
/// `MAX_NOTES` and `MAX_TOTAL_NOTE_CHARS`. `dates` are the `created_at` and `updated_at` to
/// keep, already in the database's format — for restoring a deleted note
/// — or `None` for "now".
pub async fn add_note(
    writer: &WritePool,
    passage: &Passage,
    body: &str,
    dates: Option<(&str, &str)>,
) -> Result<Note, NoteError> {
    let body = normalize_body(body)?;
    let mut tx = crate::db::begin_write(writer)
        .await
        .context("Failed to start note transaction")?;
    check_passage(
        &mut tx,
        passage.book_id,
        passage.chapter,
        passage.verse_start,
        passage.verse_end,
    )
    .await?;
    let totals = note_totals(&mut tx, None).await?;
    let after = NoteTotals {
        count: totals.count + 1,
        chars: totals.chars + body.chars().count(),
    };
    if let Some(limit) = after.exceeded() {
        return Err(limit.into());
    }
    let (created_at, updated_at) = dates.unzip();
    let note = sqlx::query_as::<_, Note>(concat!(
        "INSERT INTO notes (book_id, chapter, verse_start, verse_end, body, created_at, updated_at) \
         VALUES ($1, $2, $3, $4, $5, \
             COALESCE($6, strftime('%Y-%m-%dT%H:%M:%fZ', 'now')), \
             COALESCE($7, $6, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))) \
         RETURNING ",
        note_columns!()
    ))
    .bind(passage.book_id)
    .bind(passage.chapter)
    .bind(passage.verse_start)
    .bind(passage.verse_end)
    .bind(&body)
    .bind(created_at)
    .bind(updated_at)
    .fetch_one(&mut *tx)
    .await
    .context("Failed to insert note")?;
    tx.commit()
        .await
        .context("Failed to commit note transaction")?;
    Ok(note)
}

/// Change a note's text. Only its text and `updated_at` change, and
/// `updated_at` only if the text did. Text that grows must keep all the
/// notes within `MAX_TOTAL_NOTE_CHARS`; text that doesn't grow is always
/// allowed, so a note can be shortened even past the limit.
pub async fn update_note(writer: &WritePool, id: i64, body: &str) -> Result<Note, NoteError> {
    let body = normalize_body(body)?;
    let mut tx = crate::db::begin_write(writer)
        .await
        .context("Failed to start note transaction")?;
    let old_chars: Option<i64> = sqlx::query_scalar("SELECT length(body) FROM notes WHERE id = $1")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .context("Failed to read note")?;
    let old_chars = old_chars.ok_or(NoteError::NotFound { id })?;
    let chars = body.chars().count();
    if i64::try_from(chars).is_ok_and(|c| c > old_chars) {
        let others = note_totals(&mut tx, Some(id)).await?;
        if others.chars + chars > MAX_TOTAL_NOTE_CHARS {
            return Err(NoteLimit::TotalText.into());
        }
    }
    let note = sqlx::query_as::<_, Note>(concat!(
        "UPDATE notes SET \
             updated_at = CASE WHEN body = $2 THEN updated_at \
                 ELSE strftime('%Y-%m-%dT%H:%M:%fZ', 'now') END, \
             body = $2 \
         WHERE id = $1 RETURNING ",
        note_columns!()
    ))
    .bind(id)
    .bind(&body)
    .fetch_optional(&mut *tx)
    .await
    .context("Failed to update note")?
    .ok_or(NoteError::NotFound { id })?;
    tx.commit()
        .await
        .context("Failed to commit note transaction")?;
    Ok(note)
}

/// Delete a note, returning it as it was (so it can be restored with
/// `add_note`), or `None` if there was no such note.
pub async fn delete_note(writer: &WritePool, id: i64) -> anyhow::Result<Option<Note>> {
    sqlx::query_as::<_, Note>(concat!(
        "DELETE FROM notes WHERE id = $1 RETURNING ",
        note_columns!()
    ))
    .bind(id)
    .fetch_optional(writer.pool())
    .await
    .context("Failed to delete note")
}

/// Every note that overlaps `passage`, in the order they're shown (see
/// `overlapping`).
pub async fn notes_overlapping(pool: &SqlitePool, passage: &Passage) -> anyhow::Result<Vec<Note>> {
    let in_chapter = sqlx::query_as::<_, Note>(concat!(
        "SELECT ",
        note_columns!(),
        " FROM notes WHERE book_id = $1 AND chapter = $2"
    ))
    .bind(passage.book_id)
    .bind(passage.chapter)
    .fetch_all(pool)
    .await
    .context("Failed to read notes")?;
    Ok(overlapping(in_chapter, passage))
}

/// Every note, in Bible order (see `in_bible_order`).
pub async fn list_notes(pool: &SqlitePool) -> anyhow::Result<Vec<Note>> {
    let all = sqlx::query_as::<_, Note>(concat!("SELECT ", note_columns!(), " FROM notes"))
        .fetch_all(pool)
        .await
        .context("Failed to read notes")?;
    Ok(in_bible_order(all))
}

#[cfg(test)]
mod tests {
    use super::*;

    const JOHN: i64 = 43;

    fn passage(book_id: i64, chapter: i64, (verse_start, verse_end): (i64, i64)) -> Passage {
        Passage {
            book_id,
            chapter,
            verse_start,
            verse_end,
        }
    }

    fn note(id: i64, at: Passage, created_at: &str) -> Note {
        Note {
            id,
            book_id: at.book_id,
            chapter: at.chapter,
            verse_start: at.verse_start,
            verse_end: at.verse_end,
            body: format!("note {id}"),
            created_at: created_at.to_string(),
            updated_at: created_at.to_string(),
        }
    }

    fn ids(notes: &[Note]) -> Vec<i64> {
        notes.iter().map(|n| n.id).collect()
    }

    /// Which of `notes` (as `(start, end)` ranges in John 3) overlap
    /// John 3:`selection`.
    fn overlapping_ranges(ranges: &[(i64, i64)], selection: (i64, i64)) -> Vec<i64> {
        let notes = ranges.iter().enumerate().map(|(i, &r)| {
            note(
                i as i64 + 1,
                passage(JOHN, 3, r),
                "2026-01-01T00:00:00.000Z",
            )
        });
        ids(&overlapping(notes, &passage(JOHN, 3, selection)))
    }

    #[test]
    fn nothing_overlaps_when_there_are_no_notes() {
        assert_eq!(overlapping_ranges(&[], (16, 21)), Vec::<i64>::new());
    }

    #[test]
    fn an_exact_match_overlaps() {
        assert_eq!(overlapping_ranges(&[(16, 16)], (16, 16)), [1]);
        assert_eq!(overlapping_ranges(&[(16, 21)], (16, 21)), [1]);
    }

    #[test]
    fn a_partial_overlap_at_either_end_counts() {
        // The note starts before the selection and ends inside it…
        assert_eq!(overlapping_ranges(&[(14, 16)], (16, 21)), [1]);
        // …or starts inside it and ends after it.
        assert_eq!(overlapping_ranges(&[(21, 24)], (16, 21)), [1]);
    }

    #[test]
    fn containment_either_way_counts() {
        // A note on 3:16–18 shows for 3:17…
        assert_eq!(overlapping_ranges(&[(16, 18)], (17, 17)), [1]);
        // …and notes on 3:16 and 3:21 both show for 3:16–21.
        assert_eq!(overlapping_ranges(&[(16, 16), (21, 21)], (16, 21)), [1, 2]);
    }

    #[test]
    fn adjacent_passages_do_not_overlap() {
        assert_eq!(
            overlapping_ranges(&[(10, 15), (22, 30)], (16, 21)),
            Vec::<i64>::new()
        );
    }

    #[test]
    fn another_chapter_or_book_never_overlaps() {
        let at = "2026-01-01T00:00:00.000Z";
        let notes = [
            note(1, passage(JOHN, 4, (16, 21)), at),
            note(2, passage(JOHN - 1, 3, (16, 21)), at),
            note(3, passage(JOHN + 1, 3, (1, 40)), at),
        ];
        assert_eq!(
            ids(&overlapping(notes, &passage(JOHN, 3, (16, 21)))),
            Vec::<i64>::new()
        );
    }

    #[test]
    fn overlapping_notes_are_ordered_by_verses_then_age_then_id() {
        let early = "2025-01-01T00:00:00.000Z";
        let late = "2026-01-01T00:00:00.000Z";
        let notes = vec![
            note(1, passage(JOHN, 3, (21, 21)), early),
            note(2, passage(JOHN, 3, (16, 18)), early),
            note(3, passage(JOHN, 3, (16, 16)), late),
            note(4, passage(JOHN, 3, (16, 16)), early),
            note(6, passage(JOHN, 3, (17, 17)), early),
            note(5, passage(JOHN, 3, (17, 17)), early),
        ];
        assert_eq!(
            ids(&overlapping(notes, &passage(JOHN, 3, (16, 21)))),
            [4, 3, 2, 5, 6, 1]
        );
    }

    #[test]
    fn every_note_lists_in_bible_order() {
        let at = "2026-01-01T00:00:00.000Z";
        let notes = vec![
            note(1, passage(JOHN, 3, (16, 16)), at),
            note(2, passage(1, 1, (1, 1)), at),
            note(3, passage(JOHN, 1, (1, 5)), at),
            note(4, passage(JOHN, 3, (1, 2)), at),
        ];
        assert_eq!(ids(&in_bible_order(notes)), [2, 3, 4, 1]);
    }

    #[test]
    fn totals_say_which_limit_they_pass() {
        let at = |count, chars| NoteTotals { count, chars }.exceeded();
        assert_eq!(at(MAX_NOTES, MAX_TOTAL_NOTE_CHARS), None);
        assert_eq!(at(MAX_NOTES + 1, 0), Some(NoteLimit::Count));
        assert_eq!(at(1, MAX_TOTAL_NOTE_CHARS + 1), Some(NoteLimit::TotalText));
        assert_eq!(
            NoteLimit::Count.to_string(),
            "You've reached the limit of 10,000 notes."
        );
        assert_eq!(
            NoteLimit::TotalText.to_string(),
            "Your notes have reached the total size limit (25,000,000 characters)."
        );
    }

    #[test]
    fn note_text_is_trimmed_and_checked() {
        assert_eq!(
            normalize_body("  For God so loved\r\nthe world.\n\n "),
            Ok("For God so loved\nthe world.".to_string())
        );
        assert_eq!(normalize_body("a\rb\tc"), Ok("a\nb\tc".to_string()));
        assert_eq!(normalize_body(""), Err(NoteBodyError::Empty));
        assert_eq!(normalize_body(" \n\t "), Err(NoteBodyError::Empty));
        assert_eq!(
            normalize_body("bell\u{7}"),
            Err(NoteBodyError::ControlCharacter)
        );
        assert_eq!(
            normalize_body("nul\0"),
            Err(NoteBodyError::ControlCharacter)
        );

        // The limit counts characters, not bytes, after trimming.
        let longest = "\u{1d11e}".repeat(MAX_NOTE_CHARS);
        assert_eq!(
            normalize_body(&format!("  {longest}\n")),
            Ok(longest.clone())
        );
        assert_eq!(
            normalize_body(&format!("{longest}x")),
            Err(NoteBodyError::TooLong {
                chars: MAX_NOTE_CHARS + 1
            })
        );
        assert_eq!(
            NoteBodyError::TooLong { chars: 10_214 }.to_string(),
            "A note can hold up to 10,000 characters; this one has 10,214."
        );
    }

    /// The limit here is the one the table's CHECK enforces.
    #[test]
    fn the_length_limit_matches_the_table() {
        let migrate = include_str!("migrate.rs");
        assert!(
            migrate.contains(&format!(
                "CHECK (length(body) BETWEEN 1 AND {MAX_NOTE_CHARS})"
            )),
            "the notes table's CHECK should allow 1 to {MAX_NOTE_CHARS} characters"
        );
    }
}
