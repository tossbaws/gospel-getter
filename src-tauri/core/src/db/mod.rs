//! The SQLite database: its schema (`migrate`), the bundled Bible data it's
//! seeded with (`seed`), the reader's bookmarks, highlights and notes, and
//! the findings AI agents saved at the reader's request (`findings`).
//! `prepare` brings any database, new or from any earlier version, up to
//! date at startup.

pub mod bookmarks;
pub mod connect;
pub mod findings;
pub mod highlights;
pub mod legacy;
pub mod migrate;
pub mod models;
pub mod notes;
pub mod queries;
pub mod reader_data;
pub mod search;
pub mod seed;

pub use bookmarks::{Bookmark, BookmarkError, BookmarkInTranslation};
pub use connect::{
    APP_IDENTIFIER, BUSY_TIMEOUT, DATABASE_FILE, Db, WRITE_QUEUE_TIMEOUT, WritePool, begin_write,
    create_pool, create_private_dir, open_read_only,
};
pub use findings::{
    Finding, FindingRequest, FindingsError, FindingsWriter, KeepError, NewFinding, SavedFindings,
};
pub use highlights::{Highlight, HighlightColor, HighlightPassage, HighlightPassageInTranslation};
pub use legacy::migrate_legacy_db_if_needed;
pub use migrate::migrate;
pub use models::*;
pub use notes::{Note, NoteBodyError, NoteError, NoteLimit, Passage};
pub use queries::*;
pub use search::{SearchHit, ensure_search_index};
pub use seed::{seed_cross_references_if_empty, seed_missing};

#[cfg(test)]
mod contention_tests;
#[cfg(test)]
mod findings_tests;
#[cfg(test)]
mod path_tests;
#[cfg(test)]
mod read_only_tests;
#[cfg(test)]
mod search_tests;
#[cfg(any(test, feature = "test-support"))]
pub mod test_support;
#[cfg(test)]
mod tests;

/// Bring a database up to date: create any missing tables (including ones
/// added since the database was created), then seed whatever bundled data
/// is missing. Safe to run on every launch, against a fresh database or
/// one from any earlier version. The search index is built separately
/// (`ensure_search_index`), in the background.
pub async fn prepare(writer: &WritePool) -> anyhow::Result<()> {
    use anyhow::Context;
    migrate(writer)
        .await
        .context("Failed to run database migrations")?;
    seed_missing(writer)
        .await
        .context("Failed to seed Bible data")?;
    seed_cross_references_if_empty(writer)
        .await
        .context("Failed to seed cross-reference data")?;
    Ok(())
}
