//! `Store`: the database access the commands use once the app has
//! started.

use std::collections::{HashMap, HashSet};

use crate::db::reader_data::{ApplyImportError, ImportCounts};
use crate::db::reader_data::{NewBookmark, NewHighlight, NewNote, NewPosition};
use crate::db::{
    self, Bookmark, BookmarkError, BookmarkInTranslation, CrossReference, Db, Highlight,
    HighlightColor, HighlightPassageInTranslation, Note, NoteError, Passage, SearchHit, Verse,
};

/// The store handles all database access after startup: verses, the
/// reading position, bookmarks, highlights, notes and search. The book and translation lists
/// are loaded once at startup directly via
/// `db::get_all_books`/`db::get_all_translations` (see `AppState::load`)
/// and cached in `AppState`, so `Store` itself doesn't need methods for
/// those. Every write goes through the database's one writing connection
/// (see `db::Db`). Cloning is cheap: it shares the same connections.
#[derive(Clone)]
pub struct Store {
    db: Db,
}

impl Store {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    /// Close every connection, waiting for any in use to be returned. Run
    /// before the app exits to install an update or restarts.
    pub async fn close(&self) {
        self.db.close().await;
    }

    /// Get every verse in one chapter of one translation, in verse order.
    pub async fn chapter_verses(
        &self,
        translation_id: i64,
        book_id: i64,
        chapter: i64,
    ) -> anyhow::Result<Vec<Verse>> {
        db::get_chapter_verses(self.db.read(), translation_id, book_id, chapter).await
    }

    /// The highest verse number any translation has in one chapter.
    pub async fn chapter_last_verse(&self, book_id: i64, chapter: i64) -> anyhow::Result<i64> {
        db::get_chapter_last_verse(self.db.read(), book_id, chapter).await
    }

    /// The last chapter read, if any.
    pub async fn reading_position(&self) -> anyhow::Result<Option<(i64, i64, i64)>> {
        db::get_reading_position(self.db.read()).await
    }

    /// Remember the last chapter read.
    pub async fn save_reading_position(
        &self,
        translation_id: i64,
        book_id: i64,
        chapter: i64,
    ) -> anyhow::Result<()> {
        db::save_reading_position(self.db.write(), translation_id, book_id, chapter).await
    }

    /// Cross-references for every verse in one chapter, ordered by verse
    /// then descending relevance score.
    pub async fn chapter_cross_references(
        &self,
        book_id: i64,
        chapter: i64,
    ) -> anyhow::Result<Vec<CrossReference>> {
        db::get_chapter_cross_references(self.db.read(), book_id, chapter).await
    }

    /// The text of a verse or verse range, for expanding a cross-reference.
    pub async fn verse_range(
        &self,
        translation_id: i64,
        book_id: i64,
        chapter: i64,
        start_verse: i64,
        end_verse: i64,
    ) -> anyhow::Result<Vec<Verse>> {
        db::get_verse_range(
            self.db.read(),
            translation_id,
            book_id,
            chapter,
            start_verse,
            end_verse,
        )
        .await
    }

    /// Bookmark a passage (see `db::bookmarks::add_bookmark`).
    pub async fn add_bookmark(
        &self,
        book_id: i64,
        chapter: i64,
        verse_start: i64,
        verse_end: i64,
    ) -> Result<Bookmark, BookmarkError> {
        db::bookmarks::add_bookmark(self.db.write(), book_id, chapter, verse_start, verse_end).await
    }

    /// Every bookmark, newest first, as it reads in one translation.
    pub async fn bookmarks(
        &self,
        translation_id: i64,
    ) -> anyhow::Result<Vec<BookmarkInTranslation>> {
        db::bookmarks::list_bookmarks(self.db.read(), translation_id).await
    }

    /// Remove a bookmark; returns whether it existed.
    pub async fn remove_bookmark(&self, id: i64) -> anyhow::Result<bool> {
        db::bookmarks::remove_bookmark(self.db.write(), id).await
    }

    /// Highlight every verse of a passage in `color` (see
    /// `db::highlights::set_highlights`).
    pub async fn set_highlights(
        &self,
        book_id: i64,
        chapter: i64,
        verse_start: i64,
        verse_end: i64,
        color: HighlightColor,
    ) -> Result<(), BookmarkError> {
        db::highlights::set_highlights(
            self.db.write(),
            book_id,
            chapter,
            verse_start,
            verse_end,
            color,
        )
        .await
    }

    /// Remove the highlight from every verse of a passage; returns how
    /// many verses had one.
    pub async fn remove_highlights(
        &self,
        book_id: i64,
        chapter: i64,
        verse_start: i64,
        verse_end: i64,
    ) -> anyhow::Result<usize> {
        db::highlights::remove_highlights(self.db.write(), book_id, chapter, verse_start, verse_end)
            .await
    }

    /// Every highlight, in Bible order.
    pub async fn highlights(&self) -> anyhow::Result<Vec<Highlight>> {
        db::highlights::list_highlights(self.db.read()).await
    }

    /// Every highlighted passage (consecutive verses in one color), in
    /// Bible order, as it reads in one translation.
    pub async fn highlight_passages(
        &self,
        translation_id: i64,
    ) -> anyhow::Result<Vec<HighlightPassageInTranslation>> {
        db::highlights::list_highlight_passages(self.db.read(), translation_id).await
    }

    /// The coordinates of every highlighted verse.
    pub async fn highlight_coordinates(&self) -> anyhow::Result<HashSet<(i64, i64, i64)>> {
        db::reader_data::highlight_coordinates(self.db.read()).await
    }

    /// Save a new note on a passage (see `db::notes::add_note`); `dates`
    /// keeps a restored note's own `created_at` and `updated_at`.
    pub async fn add_note(
        &self,
        passage: &Passage,
        body: &str,
        dates: Option<(&str, &str)>,
    ) -> Result<Note, NoteError> {
        db::notes::add_note(self.db.write(), passage, body, dates).await
    }

    /// Change a note's text.
    pub async fn update_note(&self, id: i64, body: &str) -> Result<Note, NoteError> {
        db::notes::update_note(self.db.write(), id, body).await
    }

    /// Delete a note, returning it as it was, if it existed.
    pub async fn delete_note(&self, id: i64) -> anyhow::Result<Option<Note>> {
        db::notes::delete_note(self.db.write(), id).await
    }

    /// Every note overlapping a passage, in the order they're shown.
    pub async fn notes_overlapping(&self, passage: &Passage) -> anyhow::Result<Vec<Note>> {
        db::notes::notes_overlapping(self.db.read(), passage).await
    }

    /// Every note, in Bible order.
    pub async fn notes(&self) -> anyhow::Result<Vec<Note>> {
        db::notes::list_notes(self.db.read()).await
    }

    /// Build whatever part of the search index is missing or stale.
    pub async fn ensure_search_index(&self) -> anyhow::Result<Vec<i64>> {
        db::ensure_search_index(&self.db).await
    }

    /// One page of verses matching every term, in Bible order, and the
    /// total number of matches.
    pub async fn search(
        &self,
        translation_id: i64,
        terms: &[String],
        limit: i64,
        offset: i64,
    ) -> anyhow::Result<(i64, Vec<SearchHit>)> {
        db::search::search_verses(self.db.read(), translation_id, terms, limit, offset).await
    }

    /// Every bookmark, oldest first, for export.
    pub async fn all_bookmarks(&self) -> anyhow::Result<Vec<Bookmark>> {
        db::reader_data::all_bookmarks(self.db.read()).await
    }

    /// The coordinates of every bookmark.
    pub async fn bookmark_coordinates(&self) -> anyhow::Result<HashSet<(i64, i64, i64, i64)>> {
        db::reader_data::bookmark_coordinates(self.db.read()).await
    }

    /// The passage, text and date of every note.
    pub async fn stored_notes(&self) -> anyhow::Result<Vec<db::reader_data::StoredNote>> {
        db::reader_data::stored_notes(self.db.read()).await
    }

    /// The highest verse number any translation has, per (book, chapter).
    pub async fn last_verses(&self) -> anyhow::Result<HashMap<(i64, i64), i64>> {
        db::reader_data::last_verses(self.db.read()).await
    }

    /// The current time, in the database's timestamp format.
    pub async fn now(&self) -> anyhow::Result<String> {
        db::reader_data::now(self.db.read()).await
    }

    /// Apply a validated import, all or nothing.
    pub async fn apply_import(
        &self,
        bookmarks: &[NewBookmark],
        highlights: Option<&[NewHighlight]>,
        notes: Option<&[NewNote]>,
        replace: bool,
        position: Option<&NewPosition>,
    ) -> Result<ImportCounts, ApplyImportError> {
        db::reader_data::apply_import(
            self.db.write(),
            bookmarks,
            highlights,
            notes,
            replace,
            position,
        )
        .await
    }
}
