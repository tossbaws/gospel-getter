//! Highlights: verses the reader has marked in a color, like a highlighter
//! pen in a paper Bible. One color per verse, stored by translation-
//! independent coordinates (book, chapter, verse); see the `highlights`
//! table in `migrate.rs`.

use anyhow::Context;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use std::fmt;
use std::str::FromStr;

use super::BookmarkError;
use super::bookmarks::check_passage;

/// The highlighter colors. The database's CHECK and the frontend's
/// swatches list the same four.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HighlightColor {
    Yellow,
    Green,
    Blue,
    Pink,
}

impl HighlightColor {
    pub const ALL: [Self; 4] = [Self::Yellow, Self::Green, Self::Blue, Self::Pink];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Yellow => "yellow",
            Self::Green => "green",
            Self::Blue => "blue",
            Self::Pink => "pink",
        }
    }
}

impl fmt::Display for HighlightColor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A name that isn't one of the four colors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownColor(pub String);

impl fmt::Display for UnknownColor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "\u{201c}{}\u{201d} isn't a highlight color (yellow, green, blue or pink)",
            self.0
        )
    }
}

impl std::error::Error for UnknownColor {}

impl FromStr for HighlightColor {
    type Err = UnknownColor;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|c| c.as_str() == s)
            .ok_or_else(|| UnknownColor(s.to_string()))
    }
}

/// One highlighted verse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Highlight {
    pub book_id: i64,
    pub chapter: i64,
    pub verse: i64,
    pub color: HighlightColor,
    /// UTC, ISO 8601 with milliseconds: when the verse was first highlighted.
    pub created_at: String,
    /// When its color was last set.
    pub updated_at: String,
}

#[derive(FromRow)]
struct HighlightRow {
    book_id: i64,
    chapter: i64,
    verse: i64,
    color: String,
    created_at: String,
    updated_at: String,
}

impl TryFrom<HighlightRow> for Highlight {
    type Error = anyhow::Error;

    fn try_from(row: HighlightRow) -> anyhow::Result<Self> {
        Ok(Self {
            book_id: row.book_id,
            chapter: row.chapter,
            verse: row.verse,
            color: row.color.parse()?,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

/// Highlight every verse in a passage in `color`, replacing any color
/// those verses had, after checking the passage exists (the same rule as
/// bookmarks: the verses must exist in at least one bundled translation).
/// A verse that already has `color` is left exactly as it is.
pub async fn set_highlights(
    pool: &SqlitePool,
    book_id: i64,
    chapter: i64,
    verse_start: i64,
    verse_end: i64,
    color: HighlightColor,
) -> Result<(), BookmarkError> {
    let mut tx = pool
        .begin()
        .await
        .context("Failed to start highlight transaction")?;
    check_passage(&mut tx, book_id, chapter, verse_start, verse_end).await?;
    for verse in verse_start..=verse_end {
        sqlx::query(
            "INSERT INTO highlights (book_id, chapter, verse, color) VALUES ($1, $2, $3, $4) \
             ON CONFLICT (book_id, chapter, verse) DO UPDATE SET \
                 color = excluded.color, \
                 updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') \
             WHERE highlights.color <> excluded.color",
        )
        .bind(book_id)
        .bind(chapter)
        .bind(verse)
        .bind(color.as_str())
        .execute(&mut *tx)
        .await
        .context("Failed to save highlight")?;
    }
    tx.commit()
        .await
        .context("Failed to commit highlight transaction")?;
    Ok(())
}

/// Remove the highlight from every verse in a passage. Returns how many
/// verses had one.
pub async fn remove_highlights(
    pool: &SqlitePool,
    book_id: i64,
    chapter: i64,
    verse_start: i64,
    verse_end: i64,
) -> anyhow::Result<usize> {
    let result = sqlx::query(
        "DELETE FROM highlights WHERE book_id = $1 AND chapter = $2 AND verse BETWEEN $3 AND $4",
    )
    .bind(book_id)
    .bind(chapter)
    .bind(verse_start)
    .bind(verse_end)
    .execute(pool)
    .await
    .context("Failed to remove highlights")?;
    Ok(result.rows_affected() as usize)
}

/// Every highlight, in Bible order.
pub async fn list_highlights(pool: &SqlitePool) -> anyhow::Result<Vec<Highlight>> {
    let rows = sqlx::query_as::<_, HighlightRow>(
        "SELECT book_id, chapter, verse, color, created_at, updated_at FROM highlights \
         ORDER BY book_id, chapter, verse",
    )
    .fetch_all(pool)
    .await
    .context("Failed to read highlights")?;
    rows.into_iter().map(Highlight::try_from).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The four colors are the same everywhere: here, the table's CHECK and
    /// the frontend's swatches.
    #[test]
    fn colors_match_the_table_and_the_frontend() {
        let names: Vec<&str> = HighlightColor::ALL.iter().map(|c| c.as_str()).collect();
        assert_eq!(names, ["yellow", "green", "blue", "pink"]);

        let quoted: Vec<String> = names.iter().map(|n| format!("'{n}'")).collect();
        let migrate = include_str!("migrate.rs");
        assert!(
            migrate.contains(&format!("color IN ({})", quoted.join(", "))),
            "the highlights table's CHECK should list exactly these colors"
        );
        let html = include_str!("../../../ui/index.html");
        assert!(
            html.contains(&format!(
                "const HIGHLIGHT_COLORS = [{}];",
                quoted.join(", ")
            )),
            "the frontend's swatches should offer exactly these colors"
        );

        for color in HighlightColor::ALL {
            assert_eq!(color.as_str().parse::<HighlightColor>(), Ok(color));
            assert_eq!(
                serde_json::to_value(color).unwrap(),
                serde_json::Value::from(color.as_str())
            );
        }
        assert_eq!(
            "Yellow".parse::<HighlightColor>(),
            Err(UnknownColor("Yellow".to_string()))
        );
    }
}
