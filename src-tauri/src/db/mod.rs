pub mod bookmarks;
pub mod connect;
pub mod highlights;
pub mod legacy;
pub mod migrate;
pub mod models;
pub mod queries;
pub mod reader_data;
pub mod search;
pub mod seed;

pub use bookmarks::{Bookmark, BookmarkError, BookmarkInTranslation};
pub use connect::create_pool;
pub use highlights::{Highlight, HighlightColor, HighlightPassage, HighlightPassageInTranslation};
pub use legacy::migrate_legacy_db_if_needed;
pub use migrate::migrate;
pub use models::*;
pub use queries::*;
pub use search::{SearchHit, ensure_search_index};
pub use seed::{seed_cross_references_if_empty, seed_missing};

#[cfg(test)]
pub mod test_support;
#[cfg(test)]
mod tests;

/// Bring a database up to date: create any missing tables (including ones
/// added since the database was created), then seed whatever bundled data
/// is missing. Safe to run on every launch, against a fresh database or
/// one from any earlier version. The search index is built separately
/// (`ensure_search_index`), in the background.
pub async fn prepare(pool: &sqlx::SqlitePool) -> anyhow::Result<()> {
    use anyhow::Context;
    migrate(pool)
        .await
        .context("Failed to run database migrations")?;
    seed_missing(pool)
        .await
        .context("Failed to seed Bible data")?;
    seed_cross_references_if_empty(pool)
        .await
        .context("Failed to seed cross-reference data")?;
    Ok(())
}
