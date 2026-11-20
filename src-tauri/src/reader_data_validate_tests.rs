//! `reader_data::validate`'s whole answer, pinned on mixed files: every
//! problem's exact words in their exact order (each record kind, then the
//! next, the notes' total after their records, the reading position last,
//! the limit and its summary line, the count limits that refuse a file
//! before anything else is checked), and for valid files the whole
//! `ValidatedImport`, with each kind's own rule for repeats. The files are
//! checked against a fresh install's books, translations and verse
//! numbering, as `commands::prepare_import` checks them.

use serde_json::{Value, json};
use std::collections::HashMap;

use crate::commands::AppState;
use crate::db::test_support::{TempDb, fresh_database};
use crate::db::{Book, HighlightColor, NewFinding, Passage, Translation};
use crate::reader_data::{
    self, FindingRecord, ImportError, MAX_FINDINGS, MAX_NOTES, NewBookmark, NewHighlight, NewNote,
    NewPosition, NoteRecord, Preferences, ReaderData, Theme, ValidatedImport,
};

/// This install's Bible, as an import is checked against it.
struct Install {
    _db: TempDb,
    books: Vec<Book>,
    translations: Vec<Translation>,
    last_verses: HashMap<(i64, i64), i64>,
}

impl Install {
    async fn new(label: &str) -> Self {
        let (db, pool) = fresh_database(label).await;
        let state = AppState::load(pool).await.expect("load state");
        let last_verses = state.store.last_verses().await.expect("last verses");
        Self {
            _db: db,
            books: state.books,
            translations: state.translations,
            last_verses,
        }
    }

    fn validate(&self, data: ReaderData) -> Result<ValidatedImport, ImportError> {
        reader_data::validate(data, &self.books, &self.translations, &self.last_verses)
    }

    /// Parse and validate a file, as an import does.
    fn check(&self, file: &Value) -> Result<ValidatedImport, ImportError> {
        reader_data::parse(file.to_string().as_bytes()).and_then(|data| self.validate(data))
    }

    /// The problems `check` lists; panics unless the file is refused for
    /// them.
    fn problems(&self, file: &Value) -> Vec<String> {
        invalid(self.check(file))
    }

    fn translation(&self, code: &str) -> &Translation {
        self.translations
            .iter()
            .find(|t| t.code == code)
            .expect("bundled translation")
    }
}

fn invalid(result: Result<ValidatedImport, ImportError>) -> Vec<String> {
    match result {
        Err(ImportError::Invalid(problems)) => problems,
        other => panic!("expected the file to be refused as invalid, got {other:?}"),
    }
}

/// A file of `format_version` with `fields` added.
fn file(format_version: u64, fields: Value) -> Value {
    let mut file = json!({
        "format": "gospel-getter-reader-data",
        "format_version": format_version,
        "exported_at": "2026-11-14T08:00:00.000Z",
        "app_version": "2.29.3",
        "reading_position": null,
    });
    let (Value::Object(file_fields), Value::Object(extra)) = (&mut file, fields) else {
        panic!("a file and its fields are objects");
    };
    file_fields.extend(extra);
    file
}

/// A passage record's fields: John (book 43) `chapter`:`start`–`end`.
fn john(chapter: i64, start: i64, end: i64) -> Value {
    json!({"book": 43, "chapter": chapter, "verse_start": start, "verse_end": end})
}

/// `record` with `fields` added or replaced.
fn with(mut record: Value, fields: Value) -> Value {
    let (Value::Object(record_fields), Value::Object(extra)) = (&mut record, fields) else {
        panic!("a record and its fields are objects");
    };
    record_fields.extend(extra);
    record
}

fn highlight(book: i64, chapter: i64, verse: i64) -> Value {
    json!({"book": book, "chapter": chapter, "verse": verse, "color": "yellow"})
}

fn note(passage: Value, body: &str) -> Value {
    with(passage, json!({"body": body}))
}

fn finding(passage: Value, request: &str) -> Value {
    with(passage, json!({"agent": "Claude", "request": request}))
}

fn position(translation: &str, book: i64, chapter: i64) -> Value {
    json!({"translation": translation, "book": book, "chapter": chapter})
}

const BAD_DATE: &str = "its date isn't a valid UTC time like 2026-10-04T12:30:00Z.";

#[tokio::test]
async fn every_bookmark_problem_is_listed_in_the_files_order() {
    let install = Install::new("rdv_bookmarks").await;
    let problems = install.problems(&file(
        1,
        json!({
            "bookmarks": [
                john(3, 16, 18),
                {"book": 99, "chapter": 1, "verse_start": 1, "verse_end": 1},
                with(john(3, 16, 16), json!({"book_name": "Mark"})),
                with(john(0, 1, 1), json!({"book_name": " john "})),
                john(22, 1, 1),
                john(3, 0, 2),
                john(3, 5, 4),
                john(3, 36, 37),
                with(john(3, 16, 16), json!({"created_at": "2026-02-30T00:00:00Z"})),
                // The first check that fails is the one reported.
                {"book": 99, "chapter": 0, "verse_start": 0, "verse_end": 0,
                 "created_at": "never"},
                with(john(3, 37, 37), json!({"created_at": "never"})),
                with(john(3, 16, 18), json!({"created_at": "2026-10-04T12:30:00"})),
            ],
            "reading_position": position("kjv", 43, 22),
        }),
    ));
    assert_eq!(
        problems,
        [
            "Bookmark 2: there's no book number 99.".to_string(),
            "Bookmark 3: book 43 is John, not Mark.".to_string(),
            "Bookmark 4: there's no John 0 (John has 21 chapters).".to_string(),
            "Bookmark 5: there's no John 22 (John has 21 chapters).".to_string(),
            "Bookmark 6: John 3:0\u{2013}2 isn't a valid verse range.".to_string(),
            "Bookmark 7: John 3:5\u{2013}4 isn't a valid verse range.".to_string(),
            "Bookmark 8: there's no John 3:36\u{2013}37; John 3 ends at verse 36.".to_string(),
            format!("Bookmark 9 (John 3:16): {BAD_DATE}"),
            "Bookmark 10: there's no book number 99.".to_string(),
            "Bookmark 11: there's no John 3:37; John 3 ends at verse 36.".to_string(),
            format!("Bookmark 12 (John 3:16\u{2013}18): {BAD_DATE}"),
            "Reading position: there's no John 22 (John has 21 chapters).".to_string(),
        ]
    );
}

#[tokio::test]
async fn every_highlight_problem_is_listed_after_the_bookmarks() {
    let install = Install::new("rdv_highlights").await;
    let problems = install.problems(&file(
        2,
        json!({
            "highlights": [
                highlight(43, 3, 16),
                highlight(0, 3, 16),
                with(highlight(43, 3, 16), json!({"book_name": "Mark"})),
                highlight(43, 22, 1),
                highlight(43, 0, 1),
                highlight(43, 3, 0),
                highlight(43, 3, 37),
                with(highlight(43, 3, 16), json!({"created_at": "2026-13-01T00:00:00Z"})),
                with(highlight(43, 3, 16), json!({
                    "created_at": "2026-01-01T00:00:00Z",
                    "updated_at": "2026-01-01 00:00:00Z",
                })),
                with(highlight(43, 3, 16), json!({"created_at": "x", "updated_at": "y"})),
                with(highlight(43, 3, 99), json!({"created_at": "x"})),
            ],
            "bookmarks": [john(3, 16, 37)],
        }),
    ));
    assert_eq!(
        problems,
        [
            "Bookmark 1: there's no John 3:16\u{2013}37; John 3 ends at verse 36.".to_string(),
            "Highlight 2: there's no book number 0.".to_string(),
            "Highlight 3: book 43 is John, not Mark.".to_string(),
            "Highlight 4: there's no John 22 (John has 21 chapters).".to_string(),
            "Highlight 5: there's no John 0 (John has 21 chapters).".to_string(),
            "Highlight 6: John 3:0 isn't a valid verse.".to_string(),
            "Highlight 7: there's no John 3:37; John 3 ends at verse 36.".to_string(),
            format!("Highlight 8 (John 3:16): {BAD_DATE}"),
            format!("Highlight 9 (John 3:16): {BAD_DATE}"),
            format!("Highlight 10 (John 3:16): {BAD_DATE}"),
            "Highlight 11: there's no John 3:99; John 3 ends at verse 36.".to_string(),
        ]
    );
}

#[tokio::test]
async fn every_note_problem_is_listed_after_the_highlights() {
    let install = Install::new("rdv_notes").await;
    let problems = install.problems(&file(
        3,
        json!({
            "notes": [
                note(john(3, 1, 1), "Nicodemus."),
                note(json!({"book": 99, "chapter": 1, "verse_start": 1, "verse_end": 1}), "x"),
                note(with(john(3, 1, 1), json!({"book_name": "Luke"})), "x"),
                note(john(30, 1, 1), "x"),
                note(john(3, 5, 4), "x"),
                note(john(3, 1, 40), "x"),
                note(john(3, 1, 1), " \r\n\t "),
                note(john(3, 1, 1), "bell \u{7}"),
                note(john(3, 1, 1), &"n".repeat(10_001)),
                with(note(john(3, 1, 1), "x"), json!({"created_at": "2026-10-04"})),
                with(note(john(3, 1, 2), "x"), json!({"updated_at": "1969-12-31T23:59:59Z"})),
                // The text is checked before the dates.
                with(note(john(3, 1, 1), ""), json!({"created_at": "never"})),
            ],
            "highlights": [highlight(43, 3, 0)],
        }),
    ));
    assert_eq!(
        problems,
        [
            "Highlight 1: John 3:0 isn't a valid verse.".to_string(),
            "Note 2: there's no book number 99.".to_string(),
            "Note 3: book 43 is John, not Luke.".to_string(),
            "Note 4: there's no John 30 (John has 21 chapters).".to_string(),
            "Note 5: John 3:5\u{2013}4 isn't a valid verse range.".to_string(),
            "Note 6: there's no John 3:1\u{2013}40; John 3 ends at verse 36.".to_string(),
            "Note 7 (John 3:1): A note can't be empty.".to_string(),
            "Note 8 (John 3:1): A note can only hold plain text, and this one has a control \
             character."
                .to_string(),
            "Note 9 (John 3:1): A note can hold up to 10,000 characters; this one has 10,001."
                .to_string(),
            format!("Note 10 (John 3:1): {BAD_DATE}"),
            format!("Note 11 (John 3:1\u{2013}2): {BAD_DATE}"),
            "Note 12 (John 3:1): A note can't be empty.".to_string(),
        ]
    );
}

#[tokio::test]
async fn every_finding_problem_is_listed_after_the_notes() {
    let install = Install::new("rdv_findings").await;
    let problems = install.problems(&file(
        4,
        json!({
            "findings": [
                finding(john(11, 35, 35), "grief"),
                finding(json!({"book": 67, "chapter": 1, "verse_start": 1, "verse_end": 1}),
                        "grief"),
                finding(with(john(11, 35, 35), json!({"book_name": "Acts"})), "grief"),
                finding(john(22, 1, 1), "grief"),
                finding(john(11, 0, 0), "grief"),
                finding(john(11, 58, 58), "grief"),
                // A request label today's rule refuses is no problem: it's
                // cleaned (`db::findings::recover_request`).
                finding(john(11, 35, 35), "   "),
                finding(john(11, 35, 35), "grief: here He weeps"),
                finding(john(11, 35, 35), &"r".repeat(61)),
                with(finding(john(11, 35, 36), "grief"), json!({"created_at": "today"})),
                // One cleaned to nothing still has its date checked.
                with(finding(john(11, 35, 35), ""), json!({"created_at": "today"})),
            ],
            "notes": [note(john(3, 1, 1), "")],
        }),
    ));
    assert_eq!(
        problems,
        [
            "Note 1 (John 3:1): A note can't be empty.".to_string(),
            "AI finding 2: there's no book number 67.".to_string(),
            "AI finding 3: book 43 is John, not Acts.".to_string(),
            "AI finding 4: there's no John 22 (John has 21 chapters).".to_string(),
            "AI finding 5: John 11:0 isn't a valid verse range.".to_string(),
            "AI finding 6: there's no John 11:58; John 11 ends at verse 57.".to_string(),
            format!("AI finding 10 (John 11:35\u{2013}36): {BAD_DATE}"),
            format!("AI finding 11 (John 11:35): {BAD_DATE}"),
        ]
    );
}

/// Each way a reading position can be wrong, as the only problem in its
/// file: the translation is checked first, then the book, its name and
/// the chapter.
#[tokio::test]
async fn every_reading_position_problem() {
    let install = Install::new("rdv_position").await;
    let codes: Vec<&str> = install
        .translations
        .iter()
        .map(|t| t.code.as_str())
        .collect();
    let no_niv = format!(
        "Reading position: this install has no \u{201c}niv\u{201d} translation (it has {}).",
        codes.join(", ")
    );
    for (reading_position, expected) in [
        (position("niv", 43, 3), no_niv.clone()),
        (position("niv", 99, 0), no_niv),
        (
            position("kjv", 99, 1),
            "Reading position: there's no book number 99.".to_string(),
        ),
        (
            with(position("kjv", 43, 3), json!({"book_name": "Mark"})),
            "Reading position: book 43 is John, not Mark.".to_string(),
        ),
        (
            with(position("kjv", 43, 22), json!({"book_name": "Mark"})),
            "Reading position: book 43 is John, not Mark.".to_string(),
        ),
        (
            position("kjv", 43, 22),
            "Reading position: there's no John 22 (John has 21 chapters).".to_string(),
        ),
        (
            position("web", 43, 0),
            "Reading position: there's no John 0 (John has 21 chapters).".to_string(),
        ),
    ] {
        let problems = install.problems(&file(
            1,
            json!({ "reading_position": reading_position.clone() }),
        ));
        assert_eq!(problems, [expected], "{reading_position}");
    }
    assert_eq!(codes, ["kjv", "bsb", "web"]);

    // The code is matched without regard to case or surrounding spaces,
    // and the book's name likewise.
    let web = install.translation("web");
    let valid = install
        .check(&file(
            1,
            json!({ "reading_position": with(position(" WEB ", 45, 8),
                                             json!({"book_name": " romans "})) }),
        ))
        .expect("a valid position");
    assert_eq!(
        valid.position,
        Some(NewPosition {
            translation_id: web.id,
            translation_code: "web".to_string(),
            translation_name: web.name.clone(),
            book_id: 45,
            book_name: "Romans".to_string(),
            chapter: 8,
        })
    );
}

/// One problem of every kind, the notes' total among them: each kind's
/// problems come in the order `validate` checks the kinds, whatever the
/// order of the file's fields.
#[tokio::test]
async fn the_kinds_are_checked_in_order_and_the_notes_total_follows_their_records() {
    let install = Install::new("rdv_order").await;
    let fields = json!({
        "reading_position": position("nkjv", 43, 3),
        "findings": [finding(john(11, 58, 58), "grief")],
        "notes": [note(john(3, 1, 1), ""), note(john(3, 2, 2), "kept")],
        "highlights": [highlight(43, 3, 37)],
        "bookmarks": [john(3, 0, 1)],
    });
    let mut data = reader_data::parse(file(4, fields).to_string().as_bytes()).expect("parses");
    // 2,501 full notes: just over the total, and each one valid.
    let full = NoteRecord {
        book: 43,
        book_name: None,
        chapter: 3,
        verse_start: 16,
        verse_end: 16,
        body: "t".repeat(10_000),
        created_at: Some("2026-01-01T00:00:00Z".to_string()),
        updated_at: None,
    };
    data.notes.extend(
        std::iter::repeat_n(full, 2_501)
            .enumerate()
            .map(|(i, mut n)| {
                n.body.replace_range(0..6, &format!("{i:06}"));
                n
            }),
    );
    let codes: Vec<&str> = install
        .translations
        .iter()
        .map(|t| t.code.as_str())
        .collect();
    assert_eq!(
        invalid(install.validate(data)),
        [
            "Bookmark 1: John 3:0\u{2013}1 isn't a valid verse range.".to_string(),
            "Highlight 1: there's no John 3:37; John 3 ends at verse 36.".to_string(),
            "Note 1 (John 3:1): A note can't be empty.".to_string(),
            // The four characters of "kept", and none of the empty note's.
            "Its notes hold 25010004 characters in all; the most one file can hold is 25000000."
                .to_string(),
            "AI finding 1: there's no John 11:58; John 11 ends at verse 57.".to_string(),
            format!(
                "Reading position: this install has no \u{201c}nkjv\u{201d} translation (it has \
                 {}).",
                codes.join(", ")
            ),
        ]
    );
}

/// A file with `bookmarks`, `highlights`, `notes` and `findings` bad
/// records of each kind (each one past its chapter's end), and a bad
/// reading position.
fn many_problems(bookmarks: usize, highlights: usize, notes: usize, findings: usize) -> Value {
    file(
        4,
        json!({
            "bookmarks": vec![john(3, 40, 40); bookmarks],
            "highlights": vec![highlight(43, 3, 40); highlights],
            "notes": vec![note(john(3, 40, 40), "x"); notes],
            "findings": vec![finding(john(3, 40, 40), "grief"); findings],
            "reading_position": position("kjv", 43, 40),
        }),
    )
}

#[tokio::test]
async fn twelve_problems_are_listed_and_the_rest_counted() {
    let install = Install::new("rdv_limit").await;
    let past = |label: &str| format!("{label}: there's no John 3:40; John 3 ends at verse 36.");
    let position = "Reading position: there's no John 40 (John has 21 chapters).".to_string();

    // Exactly twelve: all of them, and no summary.
    assert_eq!(
        install.problems(&many_problems(3, 3, 3, 2)),
        [
            past("Bookmark 1"),
            past("Bookmark 2"),
            past("Bookmark 3"),
            past("Highlight 1"),
            past("Highlight 2"),
            past("Highlight 3"),
            past("Note 1"),
            past("Note 2"),
            past("Note 3"),
            past("AI finding 1"),
            past("AI finding 2"),
            position.clone(),
        ]
    );
    // Thirteen: the first twelve and one more.
    assert_eq!(
        install.problems(&many_problems(4, 3, 3, 2)),
        [
            past("Bookmark 1"),
            past("Bookmark 2"),
            past("Bookmark 3"),
            past("Bookmark 4"),
            past("Highlight 1"),
            past("Highlight 2"),
            past("Highlight 3"),
            past("Note 1"),
            past("Note 2"),
            past("Note 3"),
            past("AI finding 1"),
            past("AI finding 2"),
            "…and 1 more.".to_string(),
        ]
    );
    // Many of every kind: the first twelve are the bookmarks', and the
    // rest of every kind are counted, the position among them.
    assert_eq!(
        install.problems(&many_problems(5, 6, 7, 8)),
        [
            past("Bookmark 1"),
            past("Bookmark 2"),
            past("Bookmark 3"),
            past("Bookmark 4"),
            past("Bookmark 5"),
            past("Highlight 1"),
            past("Highlight 2"),
            past("Highlight 3"),
            past("Highlight 4"),
            past("Highlight 5"),
            past("Highlight 6"),
            past("Note 1"),
            "…and 15 more.".to_string(),
        ]
    );
    // A problem with a valid record is refused just the same: nothing is
    // imported.
    let mut one_bad = many_problems(0, 0, 0, 0);
    one_bad["bookmarks"] = json!([john(3, 16, 16)]);
    assert_eq!(install.problems(&one_bad), [position]);
}

/// Too many notes refuses the file with that alone: nothing else in it is
/// checked. However many AI findings it holds, each is checked as any
/// record is: their count never refuses a backup (`db::findings::MAX_FINDINGS`).
#[tokio::test]
async fn the_note_count_limit_refuses_a_file_before_anything_else() {
    let install = Install::new("rdv_counts").await;
    let base = || {
        let fields = json!({
            "bookmarks": [{"book": 99, "chapter": 1, "verse_start": 1, "verse_end": 1}],
            "reading_position": position("niv", 99, 0),
        });
        reader_data::parse(file(4, fields).to_string().as_bytes()).expect("parses")
    };
    // Every record bad, so a problem would show if any were checked.
    let bad_note = NoteRecord {
        book: 99,
        book_name: None,
        chapter: 0,
        verse_start: 0,
        verse_end: 0,
        body: String::new(),
        created_at: Some("never".to_string()),
        updated_at: None,
    };
    let bad_finding = FindingRecord {
        book: 99,
        book_name: None,
        chapter: 0,
        verse_start: 0,
        verse_end: 0,
        agent: String::new(),
        request: String::new(),
        created_at: Some("never".to_string()),
    };
    let too_many_notes = format!(
        "It has {} notes; the most one file can hold is {MAX_NOTES}.",
        MAX_NOTES + 1
    );
    assert_eq!(
        too_many_notes,
        "It has 10001 notes; the most one file can hold is 10000."
    );

    let mut data = base();
    data.notes = vec![bad_note.clone(); MAX_NOTES + 1];
    data.findings = vec![bad_finding.clone(); MAX_FINDINGS + 1];
    assert_eq!(
        invalid(install.validate(data)),
        std::slice::from_ref(&too_many_notes)
    );

    let mut data = base();
    data.notes = vec![bad_note.clone(); MAX_NOTES + 1];
    assert_eq!(invalid(install.validate(data)), [too_many_notes]);

    // At the notes' limit, and past the agents' limit on findings, every
    // record is checked: the first twelve problems are the bookmark's and
    // eleven of the notes'.
    let mut data = base();
    data.notes = vec![bad_note; MAX_NOTES];
    data.findings = vec![bad_finding; MAX_FINDINGS + 1];
    let mut expected = vec!["Bookmark 1: there's no book number 99.".to_string()];
    expected.extend((1..=11).map(|i| format!("Note {i}: there's no book number 99.")));
    expected.push(format!("…and {} more.", MAX_NOTES - 11 + MAX_FINDINGS + 2));
    assert_eq!(invalid(install.validate(data)), expected);
}

fn bookmark(book_id: i64, chapter: i64, verses: (i64, i64), date: Option<&str>) -> NewBookmark {
    NewBookmark {
        book_id,
        chapter,
        verse_start: verses.0,
        verse_end: verses.1,
        created_at: date.map(str::to_string),
    }
}

fn new_highlight(
    verse: i64,
    color: HighlightColor,
    created_at: Option<&str>,
    updated_at: Option<&str>,
) -> NewHighlight {
    NewHighlight {
        book_id: 43,
        chapter: 3,
        verse,
        color,
        created_at: created_at.map(str::to_string),
        updated_at: updated_at.map(str::to_string),
    }
}

fn new_note(verse: i64, body: &str, created_at: Option<&str>, updated_at: Option<&str>) -> NewNote {
    NewNote {
        book_id: 43,
        chapter: 3,
        verse_start: verse,
        verse_end: verse,
        body: body.to_string(),
        created_at: created_at.map(str::to_string),
        updated_at: updated_at.map(str::to_string),
    }
}

/// A valid file of every kind, with repeats of each: the whole import,
/// each kind keeping its own repeats its own way.
#[tokio::test]
async fn a_valid_file_imports_whole_with_each_kinds_rule_for_repeats() {
    let install = Install::new("rdv_valid").await;
    let file = file(
        4,
        json!({
            "bookmarks": [
                john(3, 16, 18),
                {"book": 1, "book_name": "Genesis", "chapter": 1, "verse_start": 1,
                 "verse_end": 1, "created_at": "2026-03-01T00:00:00Z"},
                // A repeat takes the earliest date, written in the
                // database's form, and keeps the first one's place.
                with(john(3, 16, 18), json!({"created_at": " 2026-02-01T10:00:00.5Z "})),
                with(john(3, 16, 18), json!({"created_at": "2026-05-01T00:00:00Z"})),
                {"book": 1, "chapter": 1, "verse_start": 1, "verse_end": 1},
                {"book": 1, "chapter": 1, "verse_start": 1, "verse_end": 1,
                 "created_at": "2026-01-01T00:00:00.123456789Z"},
                {"book": 19, "chapter": 23, "verse_start": 1, "verse_end": 1},
            ],
            "highlights": [
                with(highlight(43, 3, 16), json!({"created_at": "2026-04-01T00:00:00Z"})),
                // The first one of a verse counts, whatever the others say.
                with(highlight(43, 3, 16), json!({
                    "color": "pink",
                    "created_at": "2025-01-01T00:00:00Z",
                })),
                with(highlight(43, 3, 17), json!({"color": "green"})),
                with(highlight(43, 3, 17), json!({"color": "blue"})),
                with(highlight(43, 3, 18), json!({
                    "color": "blue",
                    "book_name": "JOHN",
                    "updated_at": "2026-04-02T00:00:00Z",
                })),
            ],
            "notes": [
                // Undated, and the same as a dated one later: dropped.
                note(john(3, 16, 16), "Love."),
                with(note(john(3, 16, 16), "Love."), json!({
                    "created_at": "2026-06-01T00:00:00Z",
                })),
                // Every dated note is kept, even two alike.
                with(note(john(3, 16, 16), " Love.\r\n"), json!({
                    "created_at": "2026-06-01T00:00:00Z",
                    "updated_at": "2026-06-02T00:00:00Z",
                })),
                note(john(3, 17, 17), "Saved."),
                // Undated repeats of an undated note, its text as stored.
                note(john(3, 17, 17), "Saved."),
                note(john(3, 17, 17), "\tSaved.\r"),
                // Only its creation date makes a note dated.
                with(note(john(3, 18, 18), "Believes."), json!({
                    "updated_at": "2026-07-01T00:00:00Z",
                })),
                with(note(john(3, 18, 18), "Believes."), json!({
                    "updated_at": "2026-07-02T00:00:00Z",
                })),
                note(john(3, 16, 16), "Love, again."),
            ],
            "findings": [
                with(finding(john(11, 35, 35), " grief "), json!({
                    "created_at": "2026-08-01T00:00:00Z",
                })),
                // Findings are all kept: merging keeps one per passage and
                // request (`reader_data::new_findings`).
                finding(john(11, 35, 35), "grief"),
                with(finding(with(john(11, 33, 35), json!({"book_name": "John"})), "tears"),
                     json!({"agent": " \u{200b} "})),
            ],
            "ai_findings_enabled": false,
            "reading_position": position("bsb", 19, 23),
            "preferences": {"theme": "neon", "text_size": "large", "font": "serif"},
        }),
    );
    let bsb = install.translation("bsb");
    assert_eq!(
        install.check(&file).expect("a valid file"),
        ValidatedImport {
            exported_at: "2026-11-14T08:00:00.000Z".to_string(),
            app_version: "2.29.3".to_string(),
            bookmarks: vec![
                bookmark(43, 3, (16, 18), Some("2026-02-01T10:00:00.500Z")),
                bookmark(1, 1, (1, 1), Some("2026-01-01T00:00:00.123Z")),
                bookmark(19, 23, (1, 1), None),
            ],
            duplicates_in_file: 4,
            highlights: Some(vec![
                new_highlight(
                    16,
                    HighlightColor::Yellow,
                    Some("2026-04-01T00:00:00.000Z"),
                    None
                ),
                new_highlight(17, HighlightColor::Green, None, None),
                new_highlight(
                    18,
                    HighlightColor::Blue,
                    None,
                    Some("2026-04-02T00:00:00.000Z")
                ),
            ]),
            highlight_duplicates_in_file: 2,
            notes: Some(vec![
                new_note(16, "Love.", Some("2026-06-01T00:00:00.000Z"), None),
                new_note(
                    16,
                    "Love.",
                    Some("2026-06-01T00:00:00.000Z"),
                    Some("2026-06-02T00:00:00.000Z")
                ),
                new_note(17, "Saved.", None, None),
                new_note(18, "Believes.", None, Some("2026-07-01T00:00:00.000Z")),
                new_note(16, "Love, again.", None, None),
            ]),
            note_duplicates_in_file: 4,
            findings: Some(vec![
                NewFinding {
                    passage: Passage {
                        book_id: 43,
                        chapter: 11,
                        verse_start: 35,
                        verse_end: 35
                    },
                    agent: "Claude".to_string(),
                    request: "grief".to_string(),
                    created_at: Some("2026-08-01T00:00:00.000Z".to_string()),
                },
                NewFinding {
                    passage: Passage {
                        book_id: 43,
                        chapter: 11,
                        verse_start: 35,
                        verse_end: 35
                    },
                    agent: "Claude".to_string(),
                    request: "grief".to_string(),
                    created_at: None,
                },
                NewFinding {
                    passage: Passage {
                        book_id: 43,
                        chapter: 11,
                        verse_start: 33,
                        verse_end: 35
                    },
                    agent: "AI agent".to_string(),
                    request: "tears".to_string(),
                    created_at: None,
                },
            ]),
            findings_tidied: 0,
            ai_findings_enabled: Some(false),
            position: Some(NewPosition {
                translation_id: bsb.id,
                translation_code: "bsb".to_string(),
                translation_name: bsb.name.clone(),
                book_id: 19,
                book_name: "Psalms".to_string(),
                chapter: 23,
            }),
            preferences: Preferences {
                text_size: Some(reader_data::TextSize::Large),
                ..Preferences::default()
            },
            skipped_preferences: vec![
                "theme \u{201c}neon\u{201d}".to_string(),
                "\u{201c}font\u{201d} (not a setting in this version)".to_string(),
            ],
        }
    );
}

/// What each older format's import leaves alone, and what an empty file
/// of each comes to.
#[tokio::test]
async fn each_format_imports_only_the_kinds_it_can_hold() {
    let install = Install::new("rdv_formats").await;
    let empty = |highlights, notes, findings| ValidatedImport {
        exported_at: "2026-11-14T08:00:00.000Z".to_string(),
        app_version: "2.29.3".to_string(),
        bookmarks: Vec::new(),
        duplicates_in_file: 0,
        highlights,
        highlight_duplicates_in_file: 0,
        notes,
        note_duplicates_in_file: 0,
        findings,
        findings_tidied: 0,
        ai_findings_enabled: None,
        position: None,
        preferences: Preferences::default(),
        skipped_preferences: Vec::new(),
    };
    for (format_version, expected) in [
        (1, empty(None, None, None)),
        (2, empty(Some(Vec::new()), None, None)),
        (3, empty(Some(Vec::new()), Some(Vec::new()), None)),
        (
            4,
            empty(Some(Vec::new()), Some(Vec::new()), Some(Vec::new())),
        ),
    ] {
        assert_eq!(
            install.check(&file(format_version, json!({}))),
            Ok(expected),
            "format {format_version}"
        );
    }

    // A format 1 file's bookmarks, position and preferences, and nothing
    // else.
    let kjv = install.translation("kjv");
    let mut expected = empty(None, None, None);
    expected.bookmarks = vec![bookmark(43, 3, (16, 16), Some("2026-10-04T12:30:00.000Z"))];
    expected.position = Some(NewPosition {
        translation_id: kjv.id,
        translation_code: "kjv".to_string(),
        translation_name: kjv.name.clone(),
        book_id: 43,
        book_name: "John".to_string(),
        chapter: 3,
    });
    expected.preferences = Preferences {
        theme: Some(Theme::Vellum),
        ..Preferences::default()
    };
    assert_eq!(
        install.check(&file(
            1,
            json!({
                "bookmarks": [with(john(3, 16, 16), json!({"created_at": "2026-10-04T12:30:00Z"}))],
                "reading_position": position("kjv", 43, 3),
                "preferences": {"theme": "vellum"},
            })
        )),
        Ok(expected)
    );
}
