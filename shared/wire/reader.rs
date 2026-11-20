//! The reader's and the search box's shapes: `get_home`'s books,
//! translations and places to start, `get_reading`'s cross-references,
//! edges and neighboring books (`commands::reading`), `search`'s answer
//! (`commands::search`), and `get_copy_text`'s formats
//! (`commands::passages`).

use serde::{Deserialize, Serialize};

/// A book, as `get_home` lists it (`otBooks`, `ntBooks`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BookDto {
    pub id: i64,
    pub name: String,
    pub chapter_count: i64,
}

/// A bundled translation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationDto {
    pub code: String,
    pub name: String,
    /// When it was made, as Aa's chooser shows it ("1611/1769"); `None`
    /// when there's no one date (see `seed::TRANSLATIONS`).
    #[serde(default)]
    pub date: Option<String>,
    /// "public domain".
    pub licence: String,
    /// Its line in About's credits.
    pub credit: String,
}

/// A place to start, for the welcome card (`get_home`'s `startHere`, see
/// `start_here`): opening it reads `chapter` of book `book_id`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartHereDto {
    pub book_id: i64,
    pub chapter: i64,
    pub title: String,
    pub line: String,
}

/// A verse's cross-reference: the passage it opens, and its citation as
/// shown ("Romans 8:28", "Romans 8:28–30"), both from core's
/// `domain::cross_reference`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct XrefDto {
    pub book_id: i64,
    pub chapter: i64,
    pub verse: i64,
    /// Set only for a range.
    #[serde(default)]
    pub end_verse: Option<i64>,
    pub citation: String,
    /// The data gives a range that can't be shown, so this opens and names
    /// its start verse alone, and the reader says `RANGE_UNAVAILABLE`
    /// beside it.
    #[serde(default)]
    pub range_unavailable: bool,
}

/// A quiet card in a neighbor's place, where there's no neighboring
/// chapter: before Genesis 1 ("The Old Testament", "Genesis · 50
/// chapters") and after Revelation 22 ("The end of the Bible"). Words
/// about the Bible's shape, never scripture text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BibleEdgeDto {
    pub title: String,
    #[serde(default)]
    pub detail: Option<String>,
}

/// A book's first chapter, named.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BookStartDto {
    pub book_id: i64,
    pub chapter: i64,
    pub book_name: String,
}

/// What the search box should do with the input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SearchDto {
    /// Nothing to search for.
    Empty,
    /// Go to this passage: its verses, or none for the whole chapter.
    Passage {
        book_id: i64,
        chapter: i64,
        verse_start: Option<i64>,
        verse_end: Option<i64>,
        reference: String,
    },
    /// A reference that doesn't exist (or can't be read); say why.
    Invalid { message: String },
    /// Search isn't ready or failed; say why.
    Unavailable { message: String },
    /// One page of matching verses, in Bible order.
    Text {
        /// What was searched for, as the status line says it: each phrase
        /// in curly quotes, then each word (`SearchQuery::labels`).
        terms: Vec<String>,
        total: i64,
        offset: i64,
        /// How many verses a page holds (`commands::SEARCH_PAGE_SIZE`).
        page_size: i64,
        results: Vec<SearchResultDto>,
        /// When what was typed is a book's name alone, that book's first
        /// chapter, offered before the verses; left out otherwise.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        go_to: Option<GoToDto>,
    },
}

/// A matching verse.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResultDto {
    pub book_id: i64,
    pub chapter: i64,
    pub verse: i64,
    /// e.g. "John 3:16".
    pub reference: String,
    /// The verse's plain text (see `VerseText`), the text search covers.
    pub text: String,
    /// The words of `text` the search matched, to be marked: byte ranges
    /// `[start, end)` into its UTF-8, in order (`query::term_marks`).
    #[serde(default)]
    pub marks: Vec<(usize, usize)>,
}

/// A book typed by name alone: its first chapter, to go to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoToDto {
    pub book_id: i64,
    /// Always 1.
    pub chapter: i64,
    /// e.g. "Psalms 1".
    pub reference: String,
}

/// A passage as each of Copy as…'s formats has it
/// (`passage_copy::CopyFormat`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CopyFormatsDto {
    pub reference: String,
    pub numbered: String,
    pub markdown: String,
    pub plain: String,
}

#[cfg(test)]
mod tests {
    use super::super::pin;
    use super::*;

    fn result() -> SearchResultDto {
        SearchResultDto {
            book_id: 43,
            chapter: 3,
            verse: 16,
            reference: "John 3:16".into(),
            text: "the verse's words".into(),
            marks: vec![(4, 9)],
        }
    }

    fn go_to() -> GoToDto {
        GoToDto {
            book_id: 19,
            chapter: 1,
            reference: "Psalms 1".into(),
        }
    }

    #[test]
    fn search_result() {
        pin(
            &result(),
            r#"{"bookId":43,"chapter":3,"verse":16,"reference":"John 3:16","text":"the verse's words","marks":[[4,9]]}"#,
        );
    }

    #[test]
    fn go_to_chapter() {
        pin(
            &go_to(),
            r#"{"bookId":19,"chapter":1,"reference":"Psalms 1"}"#,
        );
    }

    #[test]
    fn search() {
        pin(&SearchDto::Empty, r#"{"kind":"empty"}"#);
        let passage = SearchDto::Passage {
            book_id: 43,
            chapter: 3,
            verse_start: Some(16),
            verse_end: None,
            reference: "John 3:16".into(),
        };
        pin(
            &passage,
            r#"{"kind":"passage","bookId":43,"chapter":3,"verseStart":16,"verseEnd":null,"reference":"John 3:16"}"#,
        );
        let invalid = SearchDto::Invalid {
            message: "John has 21 chapters.".into(),
        };
        pin(
            &invalid,
            r#"{"kind":"invalid","message":"John has 21 chapters."}"#,
        );
        let unavailable = SearchDto::Unavailable {
            message: "Search isn't ready yet.".into(),
        };
        pin(
            &unavailable,
            r#"{"kind":"unavailable","message":"Search isn't ready yet."}"#,
        );
        let text = SearchDto::Text {
            terms: vec!["love".into()],
            total: 1,
            offset: 0,
            page_size: 50,
            results: vec![result()],
            go_to: None,
        };
        pin(
            &text,
            r#"{"kind":"text","terms":["love"],"total":1,"offset":0,"pageSize":50,"results":[{"bookId":43,"chapter":3,"verse":16,"reference":"John 3:16","text":"the verse's words","marks":[[4,9]]}]}"#,
        );
        let book = SearchDto::Text {
            terms: vec!["psalms".into()],
            total: 0,
            offset: 0,
            page_size: 50,
            results: vec![],
            go_to: Some(go_to()),
        };
        pin(
            &book,
            r#"{"kind":"text","terms":["psalms"],"total":0,"offset":0,"pageSize":50,"results":[],"goTo":{"bookId":19,"chapter":1,"reference":"Psalms 1"}}"#,
        );
    }

    #[test]
    fn translation() {
        let dated = TranslationDto {
            code: "KJV".into(),
            name: "King James Version".into(),
            date: Some("1611/1769".into()),
            licence: "Public domain".into(),
            credit: "King James Version (1611/1769), public domain.".into(),
        };
        pin(
            &dated,
            r#"{"code":"KJV","name":"King James Version","date":"1611/1769","licence":"Public domain","credit":"King James Version (1611/1769), public domain."}"#,
        );
        let undated = TranslationDto {
            code: "WEB".into(),
            name: "World English Bible".into(),
            date: None,
            licence: "Public domain".into(),
            credit: "World English Bible, public domain.".into(),
        };
        pin(
            &undated,
            r#"{"code":"WEB","name":"World English Bible","date":null,"licence":"Public domain","credit":"World English Bible, public domain."}"#,
        );
    }

    #[test]
    fn xref() {
        let verse = XrefDto {
            book_id: 45,
            chapter: 5,
            verse: 8,
            end_verse: None,
            citation: "Rom 5:8".into(),
            range_unavailable: false,
        };
        pin(
            &verse,
            r#"{"bookId":45,"chapter":5,"verse":8,"endVerse":null,"citation":"Rom 5:8","rangeUnavailable":false}"#,
        );
        let range = XrefDto {
            book_id: 62,
            chapter: 4,
            verse: 9,
            end_verse: Some(10),
            citation: "1 John 4:9–10".into(),
            range_unavailable: false,
        };
        pin(
            &range,
            r#"{"bookId":62,"chapter":4,"verse":9,"endVerse":10,"citation":"1 John 4:9–10","rangeUnavailable":false}"#,
        );
        let unavailable = XrefDto {
            book_id: 58,
            chapter: 6,
            verse: 20,
            end_verse: None,
            citation: "Hebrews 6:20".into(),
            range_unavailable: true,
        };
        pin(
            &unavailable,
            r#"{"bookId":58,"chapter":6,"verse":20,"endVerse":null,"citation":"Hebrews 6:20","rangeUnavailable":true}"#,
        );
    }

    #[test]
    fn bible_edge() {
        let end = BibleEdgeDto {
            title: "The end of the Bible".into(),
            detail: None,
        };
        pin(&end, r#"{"title":"The end of the Bible","detail":null}"#);
        let start = BibleEdgeDto {
            title: "The Old Testament".into(),
            detail: Some("Genesis · 50 chapters".into()),
        };
        pin(
            &start,
            r#"{"title":"The Old Testament","detail":"Genesis · 50 chapters"}"#,
        );
    }

    #[test]
    fn book_start() {
        let start = BookStartDto {
            book_id: 2,
            chapter: 1,
            book_name: "Exodus".into(),
        };
        pin(&start, r#"{"bookId":2,"chapter":1,"bookName":"Exodus"}"#);
    }

    #[test]
    fn copy_formats() {
        let formats = CopyFormatsDto {
            reference: "John 3:16 (KJV)".into(),
            numbered: "16 the verse's words".into(),
            markdown: "> the verse's words".into(),
            plain: "the verse's words".into(),
        };
        pin(
            &formats,
            r#"{"reference":"John 3:16 (KJV)","numbered":"16 the verse's words","markdown":"> the verse's words","plain":"the verse's words"}"#,
        );
    }

    #[test]
    fn book_dto() {
        let genesis = BookDto {
            id: 1,
            name: "Genesis".into(),
            chapter_count: 50,
        };
        pin(&genesis, r#"{"id":1,"name":"Genesis","chapterCount":50}"#);
    }

    #[test]
    fn start_here() {
        let row = StartHereDto {
            book_id: 43,
            chapter: 1,
            title: "John".into(),
            line: "Begin with the life of Jesus.".into(),
        };
        pin(
            &row,
            r#"{"bookId":43,"chapter":1,"title":"John","line":"Begin with the life of Jesus."}"#,
        );
    }
}
