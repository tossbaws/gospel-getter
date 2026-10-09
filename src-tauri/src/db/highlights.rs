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
use super::bookmarks::{check_passage, passage_preview_sql};

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

/// A run of highlighted verses: consecutive verses of one chapter, all in
/// one color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HighlightPassage {
    pub book_id: i64,
    pub chapter: i64,
    pub verse_start: i64,
    pub verse_end: i64,
    pub color: HighlightColor,
}

/// A highlighted passage as it reads in one particular translation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HighlightPassageInTranslation {
    pub passage: HighlightPassage,
    /// The first verse's stored text, or `None` if that translation
    /// doesn't number this verse.
    pub first_verse_text: Option<String>,
    /// Whether the translation numbers every verse in the passage.
    pub complete: bool,
}

/// Group highlights, in Bible order, into passages: a verse joins the
/// passage before it when it's the next verse of the same chapter in the
/// same color. A different color, a gap, or a new chapter or book starts
/// a new passage. The passages stay in Bible order.
pub fn group_into_passages(highlights: &[Highlight]) -> Vec<HighlightPassage> {
    let mut passages: Vec<HighlightPassage> = Vec::new();
    for h in highlights {
        match passages.last_mut() {
            Some(p)
                if p.book_id == h.book_id
                    && p.chapter == h.chapter
                    && p.color == h.color
                    && p.verse_end + 1 == h.verse =>
            {
                p.verse_end = h.verse;
            }
            _ => passages.push(HighlightPassage {
                book_id: h.book_id,
                chapter: h.chapter,
                verse_start: h.verse,
                verse_end: h.verse,
                color: h.color,
            }),
        }
    }
    passages
}

/// Every highlighted passage, in Bible order, with its first verse's text
/// in one translation (previewed exactly as bookmarks are).
pub async fn list_highlight_passages(
    pool: &SqlitePool,
    translation_id: i64,
) -> anyhow::Result<Vec<HighlightPassageInTranslation>> {
    let passages = group_into_passages(&list_highlights(pool).await?);
    if passages.is_empty() {
        return Ok(Vec::new());
    }
    // The passages go to SQLite as one JSON array of
    // [book, chapter, first verse, last verse], so one query previews
    // them all, in order.
    let coordinates: Vec<[i64; 4]> = passages
        .iter()
        .map(|p| [p.book_id, p.chapter, p.verse_start, p.verse_end])
        .collect();
    let previews: Vec<(Option<String>, bool)> = sqlx::query_as(concat!(
        "WITH p AS (SELECT key AS position, \
             json_extract(value, '$[0]') AS book_id, json_extract(value, '$[1]') AS chapter, \
             json_extract(value, '$[2]') AS verse_start, json_extract(value, '$[3]') AS verse_end \
             FROM json_each($2)) \
         SELECT ",
        passage_preview_sql!(columns),
        " FROM p ",
        passage_preview_sql!(joins),
        " ORDER BY p.position",
    ))
    .bind(translation_id)
    .bind(serde_json::to_string(&coordinates).context("Failed to encode passages")?)
    .fetch_all(pool)
    .await
    .context("Failed to preview highlighted passages")?;
    anyhow::ensure!(
        previews.len() == passages.len(),
        "Previewed {} highlighted passages, expected {}",
        previews.len(),
        passages.len()
    );
    Ok(passages
        .into_iter()
        .zip(previews)
        .map(
            |(passage, (first_verse_text, complete))| HighlightPassageInTranslation {
                passage,
                first_verse_text,
                complete,
            },
        )
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use HighlightColor::{Blue, Green, Pink, Yellow};

    fn verse(book_id: i64, chapter: i64, verse: i64, color: HighlightColor) -> Highlight {
        Highlight {
            book_id,
            chapter,
            verse,
            color,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    fn passage(
        book_id: i64,
        chapter: i64,
        (verse_start, verse_end): (i64, i64),
        color: HighlightColor,
    ) -> HighlightPassage {
        HighlightPassage {
            book_id,
            chapter,
            verse_start,
            verse_end,
            color,
        }
    }

    #[test]
    fn no_highlights_make_no_passages() {
        assert_eq!(group_into_passages(&[]), []);
    }

    #[test]
    fn a_single_verse_is_a_passage_of_one() {
        assert_eq!(
            group_into_passages(&[verse(43, 3, 16, Yellow)]),
            [passage(43, 3, (16, 16), Yellow)]
        );
    }

    #[test]
    fn consecutive_verses_in_one_color_merge() {
        let run = [
            verse(43, 3, 16, Green),
            verse(43, 3, 17, Green),
            verse(43, 3, 18, Green),
        ];
        assert_eq!(group_into_passages(&run), [passage(43, 3, (16, 18), Green)]);
    }

    #[test]
    fn a_color_change_starts_a_new_passage() {
        let mixed = [
            verse(43, 3, 16, Yellow),
            verse(43, 3, 17, Pink),
            verse(43, 3, 18, Pink),
            verse(43, 3, 19, Yellow),
        ];
        assert_eq!(
            group_into_passages(&mixed),
            [
                passage(43, 3, (16, 16), Yellow),
                passage(43, 3, (17, 18), Pink),
                passage(43, 3, (19, 19), Yellow),
            ]
        );
    }

    #[test]
    fn a_gap_starts_a_new_passage() {
        let gapped = [
            verse(19, 23, 1, Blue),
            verse(19, 23, 2, Blue),
            verse(19, 23, 4, Blue),
        ];
        assert_eq!(
            group_into_passages(&gapped),
            [passage(19, 23, (1, 2), Blue), passage(19, 23, (4, 4), Blue)]
        );
    }

    #[test]
    fn a_new_chapter_starts_a_new_passage() {
        // John 3:36 then John 4:1: adjacent in reading, but two chapters.
        let across = [verse(43, 3, 36, Green), verse(43, 4, 1, Green)];
        assert_eq!(
            group_into_passages(&across),
            [
                passage(43, 3, (36, 36), Green),
                passage(43, 4, (1, 1), Green)
            ]
        );
        // Verse numbers that would line up still don't merge chapters.
        let lined_up = [verse(43, 3, 1, Green), verse(43, 4, 2, Green)];
        assert_eq!(group_into_passages(&lined_up).len(), 2);
    }

    #[test]
    fn a_new_book_starts_a_new_passage() {
        let across = [verse(42, 1, 1, Yellow), verse(43, 1, 2, Yellow)];
        assert_eq!(
            group_into_passages(&across),
            [
                passage(42, 1, (1, 1), Yellow),
                passage(43, 1, (2, 2), Yellow)
            ]
        );
    }

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
