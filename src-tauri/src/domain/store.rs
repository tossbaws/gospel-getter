use sqlx::SqlitePool;

use crate::db::{self, CrossReference, Verse};

/// The store handles all database reads for verses. The book and
/// translation lists are loaded once at startup directly via
/// `db::get_all_books`/`db::get_all_translations` (see `main.rs`) and
/// cached in `AppState`, so `Store` itself doesn't need methods for those.
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
}
