use sqlx::SqlitePool;

use crate::db::{
    self, Bookmark, BookmarkError, BookmarkInTranslation, CrossReference, SearchHit, Verse,
};

/// The store handles all database access after startup: verses, the
/// reading position, bookmarks and search. The book and translation lists
/// are loaded once at startup directly via
/// `db::get_all_books`/`db::get_all_translations` (see `AppState::load`)
/// and cached in `AppState`, so `Store` itself doesn't need methods for
/// those. Cloning is cheap: it shares the same connection pool.
#[derive(Clone)]
pub struct Store {
    pool: SqlitePool,
}

impl Store {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Get every verse in one chapter of one translation, in verse order.
    pub async fn chapter_verses(
        &self,
        translation_id: i64,
        book_id: i64,
        chapter: i64,
    ) -> anyhow::Result<Vec<Verse>> {
        db::get_chapter_verses(&self.pool, translation_id, book_id, chapter).await
    }

    /// The last chapter read, if any.
    pub async fn reading_position(&self) -> anyhow::Result<Option<(i64, i64, i64)>> {
        db::get_reading_position(&self.pool).await
    }

    /// Remember the last chapter read.
    pub async fn save_reading_position(
        &self,
        translation_id: i64,
        book_id: i64,
        chapter: i64,
    ) -> anyhow::Result<()> {
        db::save_reading_position(&self.pool, translation_id, book_id, chapter).await
    }

    /// Cross-references for every verse in one chapter, ordered by verse
    /// then descending relevance score.
    pub async fn chapter_cross_references(
        &self,
        book_id: i64,
        chapter: i64,
    ) -> anyhow::Result<Vec<CrossReference>> {
        db::get_chapter_cross_references(&self.pool, book_id, chapter).await
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
            &self.pool,
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
        db::bookmarks::add_bookmark(&self.pool, book_id, chapter, verse_start, verse_end).await
    }

    /// Every bookmark, newest first, as it reads in one translation.
    pub async fn bookmarks(
        &self,
        translation_id: i64,
    ) -> anyhow::Result<Vec<BookmarkInTranslation>> {
        db::bookmarks::list_bookmarks(&self.pool, translation_id).await
    }

    /// Remove a bookmark; returns whether it existed.
    pub async fn remove_bookmark(&self, id: i64) -> anyhow::Result<bool> {
        db::bookmarks::remove_bookmark(&self.pool, id).await
    }

    /// Build whatever part of the search index is missing or stale.
    pub async fn ensure_search_index(&self) -> anyhow::Result<Vec<i64>> {
        db::ensure_search_index(&self.pool).await
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
        db::search::search_verses(&self.pool, translation_id, terms, limit, offset).await
    }
}
