use sqlx::FromRow;

/// A book of the Bible, in canonical order (Genesis = 1 ... Revelation = 66).
#[derive(Debug, Clone, FromRow)]
pub struct Book {
    pub id: i64,
    pub name: String,
    pub testament: String,
    pub chapter_count: i64,
}

/// A single verse. Callers already know which chapter (and translation)
/// they asked for, so there's no `chapter`/`translation_id` field here —
/// just its number and text.
#[derive(Debug, Clone, FromRow)]
pub struct Verse {
    pub verse: i64,
    pub text: String,
}

/// A Bible translation (e.g. King James Version, World English Bible).
#[derive(Debug, Clone, FromRow)]
pub struct Translation {
    pub id: i64,
    pub code: String,
    pub name: String,
}
