//! Export and import of the reader's data, end to end through the same
//! functions the commands use (everything but the native file dialog),
//! against disposable databases and real files in a scratch directory.

use serde_json::Value;
use std::path::{Path, PathBuf};

use crate::commands::{
    self, AppState, ExportOutcome, ImportChoice, ImportResultDto, build_reader_data,
    prepare_import, save_reader_data,
};
use crate::db::Passage;
use crate::db::test_support::{
    TempDb, bundled, fill_notes, fresh_database, other_tables_snapshot, verses_snapshot,
};
use crate::reader_data::{
    Layout, LineSpacing, MAX_SHOWN_FILE_CHARS, Preferences, TextSize, Theme, TranslationCode, Width,
};

async fn app(label: &str) -> (TempDb, AppState) {
    let (db, pool) = fresh_database(label).await;
    let state = AppState::load(pool).await.expect("load state");
    state.search_index.set_ready();
    (db, state)
}

/// A path for a test file, beside the test's own database.
fn file(db: &TempDb, name: &str) -> PathBuf {
    db.path().with_file_name(name)
}

fn matrix_prefs() -> Preferences {
    Preferences {
        theme: Some(Theme::Matrix),
        text_size: Some(TextSize::Large),
        line_spacing: Some(LineSpacing::Relaxed),
        width: Some(Width::Wide),
        layout: Some(Layout::Flowing),
        reading_mode: Some(false),
        compare: Some(true),
        compare_with: TranslationCode::new("bsb"),
    }
}

/// Bookmarks (coordinates and dates) and the reading position: everything
/// an import may change.
async fn personal(
    state: &AppState,
) -> (Vec<(i64, i64, i64, i64, String)>, Option<(i64, i64, i64)>) {
    let bookmarks = state
        .store
        .all_bookmarks()
        .await
        .unwrap()
        .into_iter()
        .map(|b| {
            (
                b.book_id,
                b.chapter,
                b.verse_start,
                b.verse_end,
                b.created_at,
            )
        })
        .collect();
    (bookmarks, state.store.reading_position().await.unwrap())
}

async fn export_to(state: &AppState, path: &Path, prefs: Preferences) -> ExportOutcome {
    let data = build_reader_data(state, prefs).await.unwrap();
    save_reader_data(&data, path).await.unwrap()
}

/// Choose `path` for import; panics unless it's valid. Returns the token.
async fn preview(state: &AppState, path: &Path) -> commands::ImportPreviewDto {
    match prepare_import(state, path).await.unwrap() {
        ImportChoice::Ready { preview } => *preview,
        ImportChoice::Invalid {
            message, problems, ..
        } => {
            panic!("expected a valid file: {message} {problems:?}")
        }
        ImportChoice::Cancelled => unreachable!(),
    }
}

async fn import(state: &AppState, path: &Path, replace: bool) -> ImportResultDto {
    let p = preview(state, path).await;
    commands::apply_pending_import(state, p.token, replace, true)
        .await
        .unwrap()
}

async fn invalid(state: &AppState, path: &Path) -> (String, Vec<String>) {
    match prepare_import(state, path).await.unwrap() {
        ImportChoice::Invalid {
            message, problems, ..
        } => (message, problems),
        ImportChoice::Ready { .. } => panic!("expected {} to be rejected", path.display()),
        ImportChoice::Cancelled => unreachable!(),
    }
}

/// A minimal valid file with the given bookmarks JSON.
fn file_with(bookmarks: &str, position: &str) -> String {
    format!(
        r#"{{"format": "gospel-getter-reader-data", "format_version": 1,
            "exported_at": "2026-10-04T00:00:00.000Z", "app_version": "2.2.0",
            "reading_position": {position}, "bookmarks": [{bookmarks}]}}"#
    )
}

#[tokio::test]
async fn export_round_trips_into_a_fresh_install() {
    let (db, a) = app("rd_export").await;
    commands::bookmark_passage(&a, 43, 3, 16, 18).await.unwrap();
    commands::bookmark_passage(&a, 19, 23, 1, 1).await.unwrap();
    // Matthew 2:23 is numbered only in the WEB; it still travels.
    commands::bookmark_passage(&a, 40, 2, 23, 23).await.unwrap();
    commands::reading(&a, 45, 8, Some("web")).await.unwrap();
    let before = personal(&a).await;

    let path = file(&db, "backup.gospel-getter.json");
    let ExportOutcome::Saved {
        bookmarks,
        has_position,
        preferences,
        ..
    } = export_to(&a, &path, matrix_prefs()).await
    else {
        panic!("expected saved")
    };
    assert_eq!((bookmarks, has_position, preferences), (3, true, 8));

    // The file is readable JSON with exactly the reader's data.
    let text = std::fs::read_to_string(&path).unwrap();
    let json: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(json["format"], "gospel-getter-reader-data");
    assert_eq!(json["format_version"], 3);
    assert_eq!(json["app_version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(
        json["reading_position"],
        serde_json::json!({"translation": "web", "book": 45, "book_name": "Romans", "chapter": 8})
    );
    assert_eq!(json["bookmarks"][0]["book"], 43);
    assert_eq!(json["bookmarks"][0]["book_name"], "John");
    assert_eq!(
        json["preferences"],
        serde_json::json!({"theme": "matrix", "text_size": "large", "line_spacing": "relaxed",
                           "width": "wide", "layout": "flowing", "reading_mode": false,
                           "compare": true, "compare_with": "bsb"})
    );
    // Never Bible text, ids or anything internal.
    let john_3_16 = &bundled("kjv")[42].chapters[2][15];
    assert!(!text.contains(john_3_16.as_str()));
    assert!(!text.contains("\"id\""));
    for internal in [
        "verse_search",
        "translation_id",
        "cross_ref",
        "search_index",
    ] {
        assert!(!text.contains(internal), "export mentions {internal}");
    }
    assert!(
        !path
            .with_file_name(".backup.gospel-getter.json.tmp")
            .exists()
    );

    // Into a different, fresh install: same bookmarks (dates kept), same
    // position, and the preview showed the preferences to apply.
    let (_db2, b) = app("rd_export_into").await;
    let p = preview(&b, &path).await;
    assert_eq!(p.bookmarks_in_file, 3);
    assert_eq!(
        (p.new_bookmarks, p.already_present, p.current_bookmarks),
        (3, 0, 0)
    );
    assert_eq!(p.preferences, matrix_prefs());
    assert_eq!(p.position.as_ref().unwrap().reference, "Romans 8 (WEB)");
    assert_eq!(p.sample[0], "John 3:16\u{2013}18");
    let result = commands::apply_pending_import(&b, p.token, false, true)
        .await
        .unwrap();
    assert_eq!(
        (result.added, result.already_present, result.removed),
        (3, 0, 0)
    );
    assert_eq!(personal(&b).await, before);
}

#[tokio::test]
async fn merge_deduplicates_keeps_dates_and_reimporting_changes_nothing() {
    let (db, state) = app("rd_merge").await;
    commands::bookmark_passage(&state, 43, 3, 16, 16)
        .await
        .unwrap();
    let existing = personal(&state).await.0[0].clone();

    let path = file(&db, "merge.gospel-getter.json");
    std::fs::write(
        &path,
        file_with(
            r#"{"book": 43, "book_name": "John", "chapter": 3, "verse_start": 16, "verse_end": 16, "created_at": "2001-01-01T00:00:00Z"},
               {"book": 1, "chapter": 1, "verse_start": 1, "verse_end": 3, "created_at": "2020-05-06T07:08:09.5Z"},
               {"book": 1, "chapter": 1, "verse_start": 1, "verse_end": 3, "created_at": "2019-01-01T00:00:00Z"},
               {"book": 66, "chapter": 22, "verse_start": 21, "verse_end": 21}"#,
            r#"{"translation": "KJV", "book": 19, "chapter": 23}"#,
        ),
    )
    .unwrap();

    let p = preview(&state, &path).await;
    assert_eq!(
        (
            p.bookmarks_in_file,
            p.duplicates_in_file,
            p.new_bookmarks,
            p.already_present
        ),
        (3, 1, 2, 1)
    );
    let first = commands::apply_pending_import(&state, p.token, false, true)
        .await
        .unwrap();
    assert_eq!(
        (first.added, first.already_present, first.removed),
        (2, 1, 0)
    );

    let (bookmarks, position) = personal(&state).await;
    assert_eq!(bookmarks.len(), 3);
    // The bookmark that was already here is untouched (its own date).
    assert!(bookmarks.contains(&existing));
    // A duplicate in the file keeps the earliest date, normalized.
    assert!(bookmarks.contains(&(1, 1, 1, 3, "2019-01-01T00:00:00.000Z".to_string())));
    // Translation codes match regardless of case.
    assert_eq!(position, Some((1, 19, 23)));

    // Importing the same file again: nothing new.
    let after_first = personal(&state).await;
    let again = import(&state, &path, false).await;
    assert_eq!(
        (again.added, again.already_present, again.removed),
        (0, 3, 0)
    );
    assert_eq!(personal(&state).await, after_first);
}

#[tokio::test]
async fn replace_swaps_bookmarks_and_an_earlier_export_undoes_it() {
    let (db, state) = app("rd_replace").await;
    for (book, chapter, verse) in [(43, 3, 16), (45, 8, 28), (19, 23, 1)] {
        commands::bookmark_passage(&state, book, chapter, verse, verse)
            .await
            .unwrap();
    }
    commands::reading(&state, 43, 3, Some("kjv")).await.unwrap();
    let original = personal(&state).await;
    let backup = file(&db, "before-replace.gospel-getter.json");
    export_to(&state, &backup, Preferences::default()).await;

    let other = file(&db, "other.gospel-getter.json");
    std::fs::write(
        &other,
        file_with(
            r#"{"book": 1, "chapter": 1, "verse_start": 1, "verse_end": 1}"#,
            "null",
        ),
    )
    .unwrap();
    let p = preview(&state, &other).await;
    assert_eq!(
        p.current_bookmarks, 3,
        "the preview says what replacing removes"
    );
    let result = commands::apply_pending_import(&state, p.token, true, true)
        .await
        .unwrap();
    assert_eq!((result.added, result.removed), (1, 3));
    let (bookmarks, position) = personal(&state).await;
    assert_eq!(bookmarks.len(), 1);
    assert_eq!((bookmarks[0].0, bookmarks[0].1), (1, 1));
    assert_eq!(
        position, original.1,
        "a file without a position leaves it alone"
    );

    // Replacing again from the backup restores everything exactly.
    let restored = import(&state, &backup, true).await;
    assert_eq!((restored.added, restored.removed), (3, 1));
    assert_eq!(personal(&state).await, original);
}

#[tokio::test]
async fn restoring_the_position_is_optional() {
    let (db, state) = app("rd_position").await;
    commands::reading(&state, 1, 1, Some("kjv")).await.unwrap();
    let path = file(&db, "pos.gospel-getter.json");
    std::fs::write(
        &path,
        file_with("", r#"{"translation": "web", "book": 43, "chapter": 3}"#),
    )
    .unwrap();
    let p = preview(&state, &path).await;
    let result = commands::apply_pending_import(&state, p.token, false, false)
        .await
        .unwrap();
    assert!(result.position.is_none());
    assert_eq!(personal(&state).await.1, Some((1, 1, 1)));
}

/// A display preference this version can't apply (here a theme from a
/// later version, and a text size that doesn't exist) is skipped and
/// named in the preview; the bookmarks, position and the valid
/// preferences import as usual. (An invalid preference used to refuse the
/// whole file: see `invalid_files_are_rejected_whole_and_change_nothing`
/// for what still does.)
#[tokio::test]
async fn unknown_preferences_are_skipped_and_everything_else_imports() {
    let (db, state) = app("rd_lenient_prefs").await;
    let ok = r#"{"book": 43, "chapter": 3, "verse_start": 16, "verse_end": 16}"#;
    let path = file(&db, "later.gospel-getter.json");
    std::fs::write(
        &path,
        file_with(ok, r#"{"translation": "web", "book": 19, "chapter": 23}"#).replace(
            "\"bookmarks\"",
            "\"preferences\": {\"theme\": \"aurora\", \"text_size\": \"huge\", \
             \"line_spacing\": \"relaxed\"}, \"bookmarks\"",
        ),
    )
    .unwrap();
    let p = preview(&state, &path).await;
    assert_eq!(
        p.skipped_preferences,
        ["theme “aurora”", "text size “huge”"]
    );
    assert_eq!(
        p.skipped_preferences_notice.as_deref(),
        Some(
            "Skipped 2 display settings this version of Gospel Getter doesn’t know: \
             theme “aurora”, text size “huge”."
        )
    );
    assert_eq!(
        p.preferences,
        Preferences {
            line_spacing: Some(LineSpacing::Relaxed),
            ..Preferences::default()
        }
    );
    let result = commands::apply_pending_import(&state, p.token, false, true)
        .await
        .unwrap();
    assert_eq!(result.added, 1);
    let (bookmarks, position) = personal(&state).await;
    let coordinates: Vec<_> = bookmarks.iter().map(|b| (b.0, b.1, b.2, b.3)).collect();
    assert_eq!(coordinates, [(43, 3, 16, 16)]);
    assert_eq!(
        position.map(|(_, book, chapter)| (book, chapter)),
        Some((19, 23))
    );

    // A file with nothing to skip says nothing about it.
    let plain = file(&db, "plain.gospel-getter.json");
    std::fs::write(&plain, file_with(ok, "null")).unwrap();
    let p = preview(&state, &plain).await;
    assert!(p.skipped_preferences.is_empty());
    assert_eq!(p.skipped_preferences_notice, None);
}

#[tokio::test]
async fn invalid_files_are_rejected_whole_and_change_nothing() {
    let (db, state) = app("rd_invalid").await;
    commands::bookmark_passage(&state, 43, 3, 16, 16)
        .await
        .unwrap();
    commands::reading(&state, 43, 3, Some("kjv")).await.unwrap();
    let before = personal(&state).await;

    let ok = r#"{"book": 43, "chapter": 3, "verse_start": 1, "verse_end": 1}"#;
    let cases: Vec<(&str, String, &str)> = vec![
        ("not json", "{bookmarks".into(), "isn't valid JSON (line 1"),
        (
            "other json",
            "[1, 2, 3]".into(),
            "This isn't a Gospel Getter file of bookmarks, highlights, notes and settings.",
        ),
        (
            "newer",
            r#"{"format": "gospel-getter-reader-data", "format_version": 7}"#.into(),
            "newer version of Gospel Getter (data format 7)",
        ),
        (
            "unknown field",
            file_with(ok, "null").replace("\"bookmarks\"", "\"verses\": [], \"bookmarks\""),
            "unknown field `verses`",
        ),
        (
            "no book 67",
            file_with(
                r#"{"book": 67, "chapter": 1, "verse_start": 1, "verse_end": 1}"#,
                "null",
            ),
            "there's no book number 67",
        ),
        (
            "no chapter",
            file_with(
                r#"{"book": 43, "chapter": 22, "verse_start": 1, "verse_end": 1}"#,
                "null",
            ),
            "there's no John 22 (John has 21 chapters)",
        ),
        (
            "past the end",
            file_with(
                r#"{"book": 43, "chapter": 3, "verse_start": 36, "verse_end": 40}"#,
                "null",
            ),
            "John 3 ends at verse 36",
        ),
        (
            "backwards",
            file_with(
                r#"{"book": 43, "chapter": 3, "verse_start": 5, "verse_end": 4}"#,
                "null",
            ),
            "isn't a valid verse range",
        ),
        (
            "verse 0",
            file_with(
                r#"{"book": 43, "chapter": 3, "verse_start": 0, "verse_end": 1}"#,
                "null",
            ),
            "isn't a valid verse range",
        ),
        (
            "wrong name",
            file_with(
                r#"{"book": 43, "book_name": "Mark", "chapter": 3, "verse_start": 1, "verse_end": 1}"#,
                "null",
            ),
            "book 43 is John, not Mark",
        ),
        (
            "bad date",
            file_with(
                r#"{"book": 43, "chapter": 3, "verse_start": 1, "verse_end": 1, "created_at": "last Tuesday"}"#,
                "null",
            ),
            "isn't a valid UTC time",
        ),
        (
            "bad translation",
            file_with(ok, r#"{"translation": "niv", "book": 43, "chapter": 3}"#),
            "no \u{201c}niv\u{201d} translation (it has kjv, bsb, web)",
        ),
        (
            "bad position chapter",
            file_with(ok, r#"{"translation": "kjv", "book": 65, "chapter": 2}"#),
            "there's no Jude 2",
        ),
        (
            "negative book",
            file_with(
                r#"{"book": -1, "chapter": 1, "verse_start": 1, "verse_end": 1}"#,
                "null",
            ),
            "there's no book number -1",
        ),
        (
            "string number",
            file_with(
                r#"{"book": "43", "chapter": 3, "verse_start": 1, "verse_end": 1}"#,
                "null",
            ),
            "invalid type",
        ),
    ];
    for (label, contents, expected) in &cases {
        let path = file(
            &db,
            &format!("{}.gospel-getter.json", label.replace(' ', "-")),
        );
        std::fs::write(&path, contents).unwrap();
        let (message, problems) = invalid(&state, &path).await;
        let all = format!("{message} {}", problems.join(" "));
        assert!(all.contains(expected), "{label}: {all}");
        assert_eq!(personal(&state).await, before, "{label} changed something");
    }

    // One bad bookmark among good ones still rejects the whole file.
    let mixed = file(&db, "mixed.gospel-getter.json");
    std::fs::write(
        &mixed,
        file_with(
            &format!(
                "{ok}, {}",
                r#"{"book": 99, "chapter": 1, "verse_start": 1, "verse_end": 1}"#
            ),
            "null",
        ),
    )
    .unwrap();
    let (_, problems) = invalid(&state, &mixed).await;
    assert_eq!(
        problems,
        vec!["Bookmark 2: there's no book number 99.".to_string()]
    );
    assert_eq!(personal(&state).await, before);

    // Too large, and missing.
    let big = file(&db, "big.gospel-getter.json");
    std::fs::write(
        &big,
        vec![b' '; crate::reader_data::MAX_FILE_BYTES as usize + 1],
    )
    .unwrap();
    assert!(invalid(&state, &big).await.0.contains("too large"));
    let (message, _) = invalid(&state, &file(&db, "missing.json")).await;
    assert!(
        message.starts_with("The file couldn't be read"),
        "{message}"
    );
    assert_eq!(personal(&state).await, before);
}

#[tokio::test]
async fn an_empty_export_imports_cleanly() {
    let (db, state) = app("rd_empty").await;
    let path = file(&db, "empty.gospel-getter.json");
    let ExportOutcome::Saved {
        bookmarks,
        has_position,
        preferences,
        ..
    } = export_to(&state, &path, Preferences::default()).await
    else {
        panic!()
    };
    assert_eq!((bookmarks, has_position, preferences), (0, false, 0));
    let json: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(json["bookmarks"], serde_json::json!([]));
    assert_eq!(json["reading_position"], Value::Null);
    assert_eq!(json["preferences"], serde_json::json!({}));

    let p = preview(&state, &path).await;
    assert_eq!((p.bookmarks_in_file, p.position.is_none()), (0, true));
    let result = commands::apply_pending_import(&state, p.token, false, true)
        .await
        .unwrap();
    assert_eq!((result.added, result.removed), (0, 0));
    assert_eq!(personal(&state).await, (Vec::new(), None));
}

#[tokio::test]
async fn a_failed_import_rolls_back_completely() {
    let (db, pool) = fresh_database("rd_rollback").await;
    let state = AppState::load(pool.clone()).await.unwrap();
    commands::bookmark_passage(&state, 43, 3, 16, 16)
        .await
        .unwrap();
    commands::bookmark_passage(&state, 45, 8, 28, 28)
        .await
        .unwrap();
    commands::reading(&state, 43, 3, Some("kjv")).await.unwrap();
    let before = personal(&state).await;

    let path = file(&db, "fails.gospel-getter.json");
    std::fs::write(
        &path,
        file_with(
            r#"{"book": 1, "chapter": 1, "verse_start": 1, "verse_end": 1},
               {"book": 1, "chapter": 2, "verse_start": 1, "verse_end": 1}"#,
            r#"{"translation": "web", "book": 1, "chapter": 2}"#,
        ),
    )
    .unwrap();
    // Make the second insert fail part-way through the transaction, after
    // the replace has already deleted everything.
    sqlx::query(
        "CREATE TRIGGER fail_import BEFORE INSERT ON bookmarks \
         WHEN NEW.chapter = 2 BEGIN SELECT RAISE(ABORT, 'simulated failure'); END",
    )
    .execute(pool.write().pool())
    .await
    .unwrap();

    let p = preview(&state, &path).await;
    let error = commands::apply_pending_import(&state, p.token, true, true)
        .await
        .unwrap_err();
    assert_eq!(error, "The import failed, so nothing was changed.");
    assert_eq!(personal(&state).await, before);

    // The preview is still pending, so the reader can retry once the
    // problem is gone.
    sqlx::query("DROP TRIGGER fail_import")
        .execute(pool.write().pool())
        .await
        .unwrap();
    let retry = commands::apply_pending_import(&state, p.token, true, true)
        .await
        .unwrap();
    assert_eq!((retry.added, retry.removed), (2, 2));
}

#[tokio::test]
async fn import_never_touches_bundled_data() {
    let (db, pool) = fresh_database("rd_bundled").await;
    let state = AppState::load(pool.clone()).await.unwrap();
    let verses_before = verses_snapshot(pool.read()).await;
    let tables_before: Vec<String> = other_tables_snapshot(pool.read())
        .await
        .into_iter()
        .filter(|r| !r.starts_with("reading_position"))
        .collect();
    let index_before: Vec<(i64, i64)> =
        sqlx::query_as("SELECT translation_id, verse_count FROM search_index_state ORDER BY 1")
            .fetch_all(pool.read())
            .await
            .unwrap();

    let path = file(&db, "bundled.gospel-getter.json");
    std::fs::write(
        &path,
        file_with(
            r#"{"book": 43, "chapter": 3, "verse_start": 16, "verse_end": 18}"#,
            r#"{"translation": "web", "book": 45, "chapter": 8}"#,
        ),
    )
    .unwrap();
    import(&state, &path, true).await;

    assert_eq!(verses_snapshot(pool.read()).await, verses_before);
    let tables_after: Vec<String> = other_tables_snapshot(pool.read())
        .await
        .into_iter()
        .filter(|r| !r.starts_with("reading_position"))
        .collect();
    assert_eq!(tables_after, tables_before);
    let index_after: Vec<(i64, i64)> =
        sqlx::query_as("SELECT translation_id, verse_count FROM search_index_state ORDER BY 1")
            .fetch_all(pool.read())
            .await
            .unwrap();
    assert_eq!(index_after, index_before);
}

#[tokio::test]
async fn saving_is_atomic_and_failures_leave_no_file() {
    let (db, state) = app("rd_write").await;
    let data = build_reader_data(&state, Preferences::default())
        .await
        .unwrap();

    // Into a folder that doesn't exist: an error, and nothing created.
    let nowhere = file(&db, "no-such-dir").join("x.gospel-getter.json");
    let error = save_reader_data(&data, &nowhere).await.unwrap_err();
    assert!(format!("{error:#}").contains("Couldn't write"), "{error:#}");
    assert!(!nowhere.exists());
    assert!(!nowhere.parent().unwrap().exists());

    // Over an existing file: replaced completely, no temporary left.
    let path = file(&db, "existing.gospel-getter.json");
    std::fs::write(&path, "old contents").unwrap();
    save_reader_data(&data, &path).await.unwrap();
    let written = std::fs::read_to_string(&path).unwrap();
    assert!(written.starts_with('{') && written.ends_with("}\n"));
    let leftovers: Vec<_> = std::fs::read_dir(path.parent().unwrap())
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

#[tokio::test]
async fn only_the_previewed_import_can_be_applied() {
    let (db, state) = app("rd_token").await;
    let path = file(&db, "token.gospel-getter.json");
    std::fs::write(
        &path,
        file_with(
            r#"{"book": 43, "chapter": 3, "verse_start": 16, "verse_end": 16}"#,
            "null",
        ),
    )
    .unwrap();

    let first = preview(&state, &path).await;
    let second = preview(&state, &path).await;
    assert_ne!(first.token, second.token);
    // Choosing another file replaced the first preview.
    assert!(
        commands::apply_pending_import(&state, first.token, false, true)
            .await
            .unwrap_err()
            .contains("no longer waiting")
    );

    // The previewed contents are what's applied, even if the file changes.
    std::fs::write(&path, "garbage").unwrap();
    // Cancelling discards it.
    commands::discard_pending_import(&state, second.token);
    assert!(
        commands::apply_pending_import(&state, second.token, false, true)
            .await
            .is_err()
    );
    assert_eq!(personal(&state).await, (Vec::new(), None));

    std::fs::write(
        &path,
        file_with(
            r#"{"book": 43, "chapter": 3, "verse_start": 16, "verse_end": 16}"#,
            "null",
        ),
    )
    .unwrap();
    let third = preview(&state, &path).await;
    std::fs::write(&path, "garbage").unwrap();
    let result = commands::apply_pending_import(&state, third.token, false, true)
        .await
        .unwrap();
    assert_eq!(result.added, 1);
    // Applied once; the same token can't be applied twice.
    assert!(
        commands::apply_pending_import(&state, third.token, false, true)
            .await
            .is_err()
    );
}

/// Files in `dir` that look like leftover temporaries.
fn temp_leftovers(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".tmp"))
        .collect()
}

#[tokio::test]
async fn temporary_files_never_reuse_or_disturb_an_existing_file() {
    let db = TempDb::new("rd_temp_collision");
    let dir = db.path().parent().unwrap().to_path_buf();

    // A file already has the first candidate name (a leftover, or another
    // save's): it's skipped, not truncated or removed.
    let taken = dir.join(".x.tmp");
    std::fs::write(&taken, "someone else's data").unwrap();
    let temp =
        commands::create_temp_file(&dir, [".x.tmp".to_string(), ".y.tmp".to_string()]).unwrap();
    assert_eq!(temp.path(), dir.join(".y.tmp"));
    assert_eq!(
        std::fs::read_to_string(&taken).unwrap(),
        "someone else's data"
    );

    // Dropped without being persisted: only its own file is cleaned up.
    drop(temp);
    assert!(!dir.join(".y.tmp").exists());
    assert_eq!(
        std::fs::read_to_string(&taken).unwrap(),
        "someone else's data"
    );

    // Every candidate taken: an error, and nothing touched.
    let error = commands::create_temp_file(&dir, [".x.tmp".to_string()])
        .err()
        .expect("no free name");
    assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
    assert_eq!(
        std::fs::read_to_string(&taken).unwrap(),
        "someone else's data"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn simultaneous_saves_to_one_file_each_complete_cleanly() {
    let (db, state) = app("rd_concurrent").await;
    let path = file(&db, "shared.gospel-getter.json");
    let mut datas = Vec::new();
    for (i, theme) in [
        Theme::Vaporwave,
        Theme::Matrix,
        Theme::HotPink,
        Theme::ClassicDark,
    ]
    .into_iter()
    .cycle()
    .take(8)
    .enumerate()
    {
        let mut data = build_reader_data(
            &state,
            Preferences {
                theme: Some(theme),
                ..Preferences::default()
            },
        )
        .await
        .unwrap();
        data.exported_at = format!("2026-10-05T00:00:0{i}.000Z");
        datas.push(data);
    }

    let saves: Vec<_> = datas
        .iter()
        .cloned()
        .map(|data| {
            let path = path.clone();
            tokio::spawn(async move { save_reader_data(&data, &path).await.map(|_| ()) })
        })
        .collect();
    for save in saves {
        save.await.unwrap().unwrap();
    }

    // Whichever finished last, the file is one complete export, and no
    // temporary file is left behind.
    let written = crate::reader_data::parse(&std::fs::read(&path).unwrap()).unwrap();
    assert!(datas.contains(&written));
    assert_eq!(temp_leftovers(path.parent().unwrap()), Vec::<String>::new());
}

#[tokio::test]
async fn a_failed_save_removes_its_temporary_file_and_keeps_the_target() {
    let (db, state) = app("rd_rename_fails").await;
    let data = build_reader_data(&state, Preferences::default())
        .await
        .unwrap();

    // The chosen name is an existing folder: the final rename fails.
    let target = file(&db, "a-folder.gospel-getter.json");
    std::fs::create_dir(&target).unwrap();
    std::fs::write(target.join("inside.txt"), "kept").unwrap();
    let error = save_reader_data(&data, &target).await.unwrap_err();
    assert!(format!("{error:#}").contains("Couldn't write"), "{error:#}");
    assert_eq!(
        std::fs::read_to_string(target.join("inside.txt")).unwrap(),
        "kept"
    );
    assert_eq!(
        temp_leftovers(target.parent().unwrap()),
        Vec::<String>::new()
    );
}

// ---- Highlights (format 2)

/// (book, chapter, verse, color, created_at, updated_at) of every highlight.
async fn highlight_rows(state: &AppState) -> Vec<(i64, i64, i64, String, String, String)> {
    state
        .store
        .highlights()
        .await
        .unwrap()
        .into_iter()
        .map(|h| {
            (
                h.book_id,
                h.chapter,
                h.verse,
                h.color.to_string(),
                h.created_at,
                h.updated_at,
            )
        })
        .collect()
}

/// (book, chapter, verse, color) of every highlight.
async fn colors(state: &AppState) -> Vec<(i64, i64, i64, String)> {
    highlight_rows(state)
        .await
        .into_iter()
        .map(|(b, c, v, color, ..)| (b, c, v, color))
        .collect()
}

/// A minimal valid format 2 file with the given bookmarks and highlights.
fn v2_file_with(bookmarks: &str, highlights: &str) -> String {
    format!(
        r#"{{"format": "gospel-getter-reader-data", "format_version": 2,
            "exported_at": "2026-10-08T00:00:00.000Z", "app_version": "2.5.0",
            "reading_position": null, "bookmarks": [{bookmarks}], "highlights": [{highlights}]}}"#
    )
}

#[tokio::test]
async fn highlights_round_trip_in_a_format_2_export() {
    let (db, a) = app("rd_hl_export").await;
    commands::highlight_passage(&a, 43, 3, 16, 17, "yellow")
        .await
        .unwrap();
    commands::highlight_passage(&a, 19, 23, 1, 1, "green")
        .await
        .unwrap();
    // Matthew 2:23 is numbered only in the WEB; its highlight still travels.
    commands::highlight_passage(&a, 40, 2, 23, 23, "blue")
        .await
        .unwrap();
    commands::highlight_passage(&a, 45, 8, 28, 28, "pink")
        .await
        .unwrap();
    commands::bookmark_passage(&a, 43, 3, 16, 16).await.unwrap();
    let before = highlight_rows(&a).await;

    let path = file(&db, "highlights.gospel-getter.json");
    let ExportOutcome::Saved { highlights, .. } =
        export_to(&a, &path, Preferences::default()).await
    else {
        panic!("expected saved")
    };
    assert_eq!(highlights, 5);

    let json: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(json["format_version"], 3);
    let first = &json["highlights"][0];
    assert_eq!(
        (
            &first["book"],
            &first["book_name"],
            &first["chapter"],
            &first["verse"],
            &first["color"]
        ),
        (
            &Value::from(19),
            &Value::from("Psalms"),
            &Value::from(23),
            &Value::from(1),
            &Value::from("green")
        )
    );
    assert!(first["created_at"].is_string() && first["updated_at"].is_string());
    assert!(
        !std::fs::read_to_string(&path).unwrap().contains("Yahweh"),
        "no verse text in the file"
    );

    // Into a fresh install: the same verses, colors and dates.
    let (_db_b, b) = app("rd_hl_export_b").await;
    let p = preview(&b, &path).await;
    assert!(p.has_highlights);
    assert_eq!(
        (
            p.highlights_in_file,
            p.new_highlights,
            p.already_highlighted,
            p.current_highlights
        ),
        (5, 5, 0, 0)
    );
    let result = commands::apply_pending_import(&b, p.token, false, true)
        .await
        .unwrap();
    assert_eq!(
        (
            result.highlights_added,
            result.highlights_already_present,
            result.highlights_removed
        ),
        (5, 0, 0)
    );
    assert_eq!(highlight_rows(&b).await, before);
}

#[tokio::test]
async fn merging_highlights_keeps_the_readers_colors_and_reimporting_changes_nothing() {
    let (db, state) = app("rd_hl_merge").await;
    commands::highlight_passage(&state, 43, 3, 16, 16, "pink")
        .await
        .unwrap();
    let path = file(&db, "hl-merge.gospel-getter.json");
    std::fs::write(
        &path,
        v2_file_with(
            "",
            r#"{"book": 43, "book_name": "John", "chapter": 3, "verse": 16, "color": "yellow"},
               {"book": 43, "chapter": 3, "verse": 17, "color": "yellow", "created_at": "2001-02-03T04:05:06Z"},
               {"book": 43, "chapter": 3, "verse": 17, "color": "green"},
               {"book": 1, "chapter": 1, "verse": 1, "color": "blue"}"#,
        ),
    )
    .unwrap();

    let p = preview(&state, &path).await;
    assert_eq!(
        (
            p.highlights_in_file,
            p.highlight_duplicates_in_file,
            p.new_highlights,
            p.already_highlighted,
            p.current_highlights
        ),
        (3, 1, 2, 1, 1)
    );
    let first = commands::apply_pending_import(&state, p.token, false, true)
        .await
        .unwrap();
    assert_eq!(
        (
            first.highlights_added,
            first.highlights_already_present,
            first.highlights_removed
        ),
        (2, 1, 0)
    );
    assert_eq!(
        colors(&state).await,
        vec![
            (1, 1, 1, "blue".to_string()),
            // The reader's own color stays.
            (43, 3, 16, "pink".to_string()),
            // A verse repeated in the file: the first entry counts.
            (43, 3, 17, "yellow".to_string()),
        ]
    );
    // Dates are kept (normalized); a missing updated_at follows created_at.
    let rows = highlight_rows(&state).await;
    assert_eq!(
        (rows[2].4.as_str(), rows[2].5.as_str()),
        ("2001-02-03T04:05:06.000Z", "2001-02-03T04:05:06.000Z")
    );

    let after_first = highlight_rows(&state).await;
    let again = import(&state, &path, false).await;
    assert_eq!(
        (again.highlights_added, again.highlights_already_present),
        (0, 3)
    );
    assert_eq!(highlight_rows(&state).await, after_first);
}

#[tokio::test]
async fn replace_swaps_highlights_for_format_2_but_format_1_leaves_them_alone() {
    let (db, state) = app("rd_hl_replace").await;
    commands::highlight_passage(&state, 43, 3, 16, 18, "yellow")
        .await
        .unwrap();
    commands::bookmark_passage(&state, 43, 3, 16, 16)
        .await
        .unwrap();
    let original = highlight_rows(&state).await;

    // A format 1 file can't hold highlights: replacing swaps bookmarks only,
    // exactly as it always did.
    let v1 = file(&db, "v1.gospel-getter.json");
    std::fs::write(
        &v1,
        file_with(
            r#"{"book": 1, "chapter": 1, "verse_start": 1, "verse_end": 1}"#,
            "null",
        ),
    )
    .unwrap();
    let p = preview(&state, &v1).await;
    assert!(!p.has_highlights);
    assert_eq!((p.highlights_in_file, p.new_highlights), (0, 0));
    let result = commands::apply_pending_import(&state, p.token, true, true)
        .await
        .unwrap();
    assert_eq!(
        (result.added, result.removed, result.highlights_removed),
        (1, 1, 0)
    );
    assert_eq!(highlight_rows(&state).await, original);

    // A format 2 file replaces them.
    let v2 = file(&db, "v2.gospel-getter.json");
    std::fs::write(
        &v2,
        v2_file_with(
            "",
            r#"{"book": 19, "chapter": 23, "verse": 4, "color": "green"}"#,
        ),
    )
    .unwrap();
    let p = preview(&state, &v2).await;
    assert_eq!(
        p.current_highlights, 3,
        "the preview says what replacing removes"
    );
    let result = commands::apply_pending_import(&state, p.token, true, true)
        .await
        .unwrap();
    assert_eq!((result.highlights_added, result.highlights_removed), (1, 3));
    assert_eq!(colors(&state).await, vec![(19, 23, 4, "green".to_string())]);
}

#[tokio::test]
async fn invalid_highlights_reject_the_whole_file_and_change_nothing() {
    let (db, state) = app("rd_hl_invalid").await;
    commands::highlight_passage(&state, 43, 3, 16, 16, "pink")
        .await
        .unwrap();
    commands::bookmark_passage(&state, 43, 3, 16, 16)
        .await
        .unwrap();
    let before = (personal(&state).await, highlight_rows(&state).await);
    let ok = r#"{"book": 43, "chapter": 3, "verse": 17, "color": "yellow"}"#;
    let cases = [
        (
            "bad book",
            v2_file_with(
                "",
                &format!(r#"{ok}, {{"book": 99, "chapter": 1, "verse": 1, "color": "blue"}}"#),
            ),
            "Highlight 2: there's no book number 99.",
        ),
        (
            "wrong name",
            v2_file_with(
                "",
                r#"{"book": 43, "book_name": "Mark", "chapter": 3, "verse": 1, "color": "blue"}"#,
            ),
            "book 43 is John, not Mark",
        ),
        (
            "bad chapter",
            v2_file_with(
                "",
                r#"{"book": 43, "chapter": 22, "verse": 1, "color": "blue"}"#,
            ),
            "there's no John 22",
        ),
        (
            "past the end",
            v2_file_with(
                "",
                r#"{"book": 43, "chapter": 3, "verse": 37, "color": "blue"}"#,
            ),
            "there's no John 3:37; John 3 ends at verse 36",
        ),
        (
            "verse zero",
            v2_file_with(
                "",
                r#"{"book": 43, "chapter": 3, "verse": 0, "color": "blue"}"#,
            ),
            "isn't a valid verse",
        ),
        (
            "bad date",
            v2_file_with(
                "",
                r#"{"book": 43, "chapter": 3, "verse": 1, "color": "blue", "updated_at": "soon"}"#,
            ),
            "isn't a valid UTC time",
        ),
        (
            "bad color",
            v2_file_with(
                "",
                r#"{"book": 43, "chapter": 3, "verse": 1, "color": "purple"}"#,
            ),
            "unknown variant `purple`",
        ),
        (
            "unknown field",
            v2_file_with(
                "",
                r#"{"book": 43, "chapter": 3, "verse": 1, "color": "blue", "note": "x"}"#,
            ),
            "unknown field `note`",
        ),
        (
            "format 1 with highlights",
            file_with("", "null").replace("\"bookmarks\"", "\"highlights\": [], \"bookmarks\""),
            "unknown field `highlights`",
        ),
        (
            "newer format",
            v2_file_with("", "").replace("\"format_version\": 2", "\"format_version\": 4"),
            "newer version of Gospel Getter (data format 4). This version reads format 3;",
        ),
    ];
    for (label, contents, expected) in &cases {
        let path = file(
            &db,
            &format!("{}.gospel-getter.json", label.replace(' ', "-")),
        );
        std::fs::write(&path, contents).unwrap();
        let (message, problems) = invalid(&state, &path).await;
        let all = format!("{message} {}", problems.join(" "));
        assert!(all.contains(expected), "{label}: {all}");
        assert_eq!(
            (personal(&state).await, highlight_rows(&state).await),
            before,
            "{label} changed something"
        );
    }
}

#[tokio::test]
async fn a_failed_highlight_import_rolls_back_bookmarks_too() {
    let (db, pool) = fresh_database("rd_hl_rollback").await;
    let state = AppState::load(pool.clone()).await.unwrap();
    commands::bookmark_passage(&state, 43, 3, 16, 16)
        .await
        .unwrap();
    commands::highlight_passage(&state, 43, 3, 16, 16, "pink")
        .await
        .unwrap();
    let before = (personal(&state).await, highlight_rows(&state).await);

    let path = file(&db, "hl-fails.gospel-getter.json");
    std::fs::write(
        &path,
        v2_file_with(
            r#"{"book": 1, "chapter": 1, "verse_start": 1, "verse_end": 1}"#,
            r#"{"book": 1, "chapter": 1, "verse": 1, "color": "blue"},
               {"book": 1, "chapter": 2, "verse": 1, "color": "blue"}"#,
        ),
    )
    .unwrap();
    sqlx::query(
        "CREATE TRIGGER fail_highlight BEFORE INSERT ON highlights \
         WHEN NEW.chapter = 2 BEGIN SELECT RAISE(ABORT, 'simulated failure'); END",
    )
    .execute(pool.write().pool())
    .await
    .unwrap();

    let p = preview(&state, &path).await;
    let error = commands::apply_pending_import(&state, p.token, true, true)
        .await
        .unwrap_err();
    assert_eq!(error, "The import failed, so nothing was changed.");
    assert_eq!(
        (personal(&state).await, highlight_rows(&state).await),
        before
    );
}

// ---- Notes (format 3)

/// (book, chapter, verse_start, verse_end, body, created_at, updated_at) of
/// every note, in Bible order.
type NoteRow = (i64, i64, i64, i64, String, String, String);

async fn note_rows(state: &AppState) -> Vec<NoteRow> {
    state
        .store
        .notes()
        .await
        .unwrap()
        .into_iter()
        .map(|n| {
            (
                n.book_id,
                n.chapter,
                n.verse_start,
                n.verse_end,
                n.body,
                n.created_at,
                n.updated_at,
            )
        })
        .collect()
}

/// (book, chapter, verse_start, verse_end, body) of every note.
async fn note_texts(state: &AppState) -> Vec<(i64, i64, i64, i64, String)> {
    note_rows(state)
        .await
        .into_iter()
        .map(|(b, c, s, e, body, ..)| (b, c, s, e, body))
        .collect()
}

fn john_3(verse_start: i64, verse_end: i64) -> Passage {
    Passage {
        book_id: 43,
        chapter: 3,
        verse_start,
        verse_end,
    }
}

/// A minimal valid format 3 file with the given notes.
fn v3_file_with(notes: &str) -> String {
    format!(
        r#"{{"format": "gospel-getter-reader-data", "format_version": 3,
            "exported_at": "2026-10-10T00:00:00.000Z", "app_version": "2.8.0",
            "reading_position": null, "bookmarks": [], "notes": [{notes}]}}"#
    )
}

/// Everything an import may change.
async fn everything(state: &AppState) -> impl PartialEq + std::fmt::Debug {
    (
        personal(state).await,
        highlight_rows(state).await,
        note_rows(state).await,
    )
}

#[tokio::test]
async fn notes_round_trip_in_a_format_3_export() {
    let (db, a) = app("rd_notes_export").await;
    commands::note_add(
        &a,
        john_3(16, 18),
        "Line one.\n\n\tIndented \u{201c}quoted\u{201d} line — with \"marks\" and \\.",
    )
    .await
    .unwrap();
    commands::note_add(&a, john_3(16, 16), "<b>not bold</b> \u{1f54a}")
        .await
        .unwrap();
    commands::note_add(&a, john_3(16, 16), "<b>not bold</b> \u{1f54a}")
        .await
        .unwrap();
    // Matthew 2:23 is numbered only in the WEB; its note still travels.
    let web_only = Passage {
        book_id: 40,
        chapter: 2,
        verse_start: 23,
        verse_end: 23,
    };
    commands::note_add(&a, web_only, "Nazarene.").await.unwrap();
    commands::highlight_passage(&a, 43, 3, 16, 16, "yellow")
        .await
        .unwrap();
    let before = note_rows(&a).await;

    let path = file(&db, "notes.gospel-getter.json");
    let ExportOutcome::Saved {
        notes, highlights, ..
    } = export_to(&a, &path, Preferences::default()).await
    else {
        panic!("expected saved")
    };
    assert_eq!((notes, highlights), (4, 1));

    let text = std::fs::read_to_string(&path).unwrap();
    let json: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(json["format_version"], 3);
    let first = &json["notes"][0];
    assert_eq!(
        first,
        &serde_json::json!({
            "book": 40, "book_name": "Matthew", "chapter": 2, "verse_start": 23,
            "verse_end": 23, "body": "Nazarene.",
            "created_at": before[0].5, "updated_at": before[0].6,
        })
    );
    assert!(
        !text.contains(bundled("kjv")[42].chapters[2][15].as_str()),
        "no verse text in the file"
    );
    assert!(!text.contains("\"id\""));

    // Into a fresh install: the same passages, text and dates.
    let (_db_b, b) = app("rd_notes_export_b").await;
    let p = preview(&b, &path).await;
    assert!(p.has_notes);
    assert_eq!(
        (
            p.notes_in_file,
            p.note_duplicates_in_file,
            p.new_notes,
            p.notes_already_present,
            p.current_notes
        ),
        // The two identical notes on 3:16 are one note to an import.
        (3, 1, 3, 0, 0)
    );
    let result = commands::apply_pending_import(&b, p.token, false, true)
        .await
        .unwrap();
    assert_eq!(
        (
            result.notes_added,
            result.notes_already_present,
            result.notes_removed
        ),
        (3, 0, 0)
    );
    let mut expected = before.clone();
    expected.remove(2);
    assert_eq!(note_rows(&b).await, expected);
}

#[tokio::test]
async fn merging_notes_skips_ones_already_here_and_reimporting_changes_nothing() {
    let (db, state) = app("rd_notes_merge").await;
    commands::note_add(&state, john_3(16, 16), "Already mine.")
        .await
        .unwrap();
    let path = file(&db, "notes-merge.gospel-getter.json");
    std::fs::write(
        &path,
        v3_file_with(
            r#"{"book": 43, "book_name": "John", "chapter": 3, "verse_start": 16, "verse_end": 16,
                "body": "  Already mine.\r\n", "created_at": "2001-01-01T00:00:00Z"},
               {"book": 43, "chapter": 3, "verse_start": 16, "verse_end": 16, "body": "Another on 3:16."},
               {"book": 43, "chapter": 3, "verse_start": 16, "verse_end": 18, "body": "Already mine.",
                "created_at": "2001-02-03T04:05:06Z"},
               {"book": 43, "chapter": 3, "verse_start": 16, "verse_end": 18, "body": "Already mine."},
               {"book": 1, "chapter": 1, "verse_start": 1, "verse_end": 1, "body": "In the beginning."}"#,
        ),
    )
    .unwrap();

    let p = preview(&state, &path).await;
    assert_eq!(
        (
            p.notes_in_file,
            p.note_duplicates_in_file,
            p.new_notes,
            p.notes_already_present,
            p.current_notes
        ),
        (4, 1, 3, 1, 1)
    );
    let first = commands::apply_pending_import(&state, p.token, false, true)
        .await
        .unwrap();
    assert_eq!(
        (
            first.notes_added,
            first.notes_already_present,
            first.notes_removed
        ),
        (3, 1, 0)
    );
    assert_eq!(
        note_texts(&state).await,
        vec![
            (1, 1, 1, 1, "In the beginning.".to_string()),
            // The reader's own note stays as it was, and isn't doubled.
            (43, 3, 16, 16, "Already mine.".to_string()),
            (43, 3, 16, 16, "Another on 3:16.".to_string()),
            // The same text on another passage is another note.
            (43, 3, 16, 18, "Already mine.".to_string()),
        ]
    );
    // Dates are kept (normalized); a missing updated_at follows created_at.
    let rows = note_rows(&state).await;
    assert_eq!(
        (rows[3].5.as_str(), rows[3].6.as_str()),
        ("2001-02-03T04:05:06.000Z", "2001-02-03T04:05:06.000Z")
    );
    assert_ne!(rows[1].5, "2001-01-01T00:00:00.000Z");

    let after_first = note_rows(&state).await;
    let again = import(&state, &path, false).await;
    assert_eq!((again.notes_added, again.notes_already_present), (0, 4));
    assert_eq!(note_rows(&state).await, after_first);
}

#[tokio::test]
async fn replace_swaps_notes_for_format_3_but_formats_1_and_2_leave_them_alone() {
    let (db, state) = app("rd_notes_replace").await;
    commands::note_add(&state, john_3(16, 16), "Mine.")
        .await
        .unwrap();
    commands::note_add(&state, john_3(17, 18), "Also mine.")
        .await
        .unwrap();
    commands::bookmark_passage(&state, 43, 3, 16, 16)
        .await
        .unwrap();
    commands::highlight_passage(&state, 43, 3, 16, 16, "pink")
        .await
        .unwrap();
    let original = note_rows(&state).await;

    // Formats 1 and 2 can't hold notes: replacing swaps what they can hold,
    // exactly as it always did, and leaves the notes alone.
    let v1 = file(&db, "v1.gospel-getter.json");
    std::fs::write(
        &v1,
        file_with(
            r#"{"book": 1, "chapter": 1, "verse_start": 1, "verse_end": 1}"#,
            "null",
        ),
    )
    .unwrap();
    let v2 = file(&db, "v2.gospel-getter.json");
    std::fs::write(
        &v2,
        v2_file_with(
            r#"{"book": 1, "chapter": 1, "verse_start": 2, "verse_end": 2}"#,
            r#"{"book": 19, "chapter": 23, "verse": 4, "color": "green"}"#,
        ),
    )
    .unwrap();
    for (path, highlights_removed) in [(&v1, 0), (&v2, 1)] {
        let p = preview(&state, path).await;
        assert!(!p.has_notes);
        assert_eq!((p.notes_in_file, p.new_notes, p.current_notes), (0, 0, 2));
        let result = commands::apply_pending_import(&state, p.token, true, true)
            .await
            .unwrap();
        assert_eq!(
            (
                result.added,
                result.removed,
                result.highlights_removed,
                result.notes_added,
                result.notes_removed
            ),
            (1, 1, highlights_removed, 0, 0),
            "{}",
            path.display()
        );
        assert_eq!(note_rows(&state).await, original, "{}", path.display());
    }
    // Merging one leaves them alone too.
    import(&state, &v2, false).await;
    assert_eq!(note_rows(&state).await, original);

    // A format 3 file replaces them all.
    let v3 = file(&db, "v3.gospel-getter.json");
    std::fs::write(
        &v3,
        v3_file_with(
            r#"{"book": 19, "chapter": 23, "verse_start": 1, "verse_end": 6, "body": "The file's."}"#,
        ),
    )
    .unwrap();
    let p = preview(&state, &v3).await;
    assert_eq!(
        p.current_notes, 2,
        "the preview says what replacing removes"
    );
    let result = commands::apply_pending_import(&state, p.token, true, true)
        .await
        .unwrap();
    assert_eq!((result.notes_added, result.notes_removed), (1, 2));
    assert_eq!(
        note_texts(&state).await,
        vec![(19, 23, 1, 6, "The file's.".to_string())]
    );

    // And a format 3 file with no notes replaces them with none.
    let empty = file(&db, "v3-empty.gospel-getter.json");
    std::fs::write(&empty, v3_file_with("")).unwrap();
    let result = import(&state, &empty, true).await;
    assert_eq!(result.notes_removed, 1);
    assert_eq!(note_rows(&state).await, vec![]);
}

#[tokio::test]
async fn invalid_notes_reject_the_whole_file_and_change_nothing() {
    let (db, state) = app("rd_notes_invalid").await;
    commands::note_add(&state, john_3(16, 16), "Mine.")
        .await
        .unwrap();
    commands::bookmark_passage(&state, 43, 3, 16, 16)
        .await
        .unwrap();
    let before = everything(&state).await;
    let ok = r#"{"book": 43, "chapter": 3, "verse_start": 17, "verse_end": 17, "body": "fine"}"#;
    let note = |fields: &str| {
        v3_file_with(&format!(
            r#"{ok}, {{"book": 43, "chapter": 3, "verse_start": 1, "verse_end": 1, {fields}}}"#
        ))
    };
    let too_long = "x".repeat(crate::db::notes::MAX_NOTE_CHARS + 1);
    let cases = [
        (
            "bad book",
            v3_file_with(&format!(
                r#"{ok}, {{"book": 99, "chapter": 1, "verse_start": 1, "verse_end": 1, "body": "x"}}"#
            )),
            "Note 2: there's no book number 99.".to_string(),
        ),
        (
            "wrong name",
            v3_file_with(
                r#"{"book": 43, "book_name": "Mark", "chapter": 3, "verse_start": 1, "verse_end": 1, "body": "x"}"#,
            ),
            "Note 1: book 43 is John, not Mark.".to_string(),
        ),
        (
            "bad chapter",
            v3_file_with(
                r#"{"book": 43, "chapter": 22, "verse_start": 1, "verse_end": 1, "body": "x"}"#,
            ),
            "Note 1: there's no John 22 (John has 21 chapters).".to_string(),
        ),
        (
            "past the end",
            v3_file_with(
                r#"{"book": 43, "chapter": 3, "verse_start": 36, "verse_end": 37, "body": "x"}"#,
            ),
            "Note 1: there's no John 3:36\u{2013}37; John 3 ends at verse 36.".to_string(),
        ),
        (
            "backwards",
            v3_file_with(
                r#"{"book": 43, "chapter": 3, "verse_start": 5, "verse_end": 4, "body": "x"}"#,
            ),
            "Note 1: John 3:5\u{2013}4 isn't a valid verse range.".to_string(),
        ),
        (
            "empty",
            note(r#""body": " \n ""#),
            "Note 2 (John 3:1): A note can't be empty.".to_string(),
        ),
        (
            "too long",
            note(&format!(r#""body": "{too_long}""#)),
            "Note 2 (John 3:1): A note can hold up to 10,000 characters; this one has 10,001."
                .to_string(),
        ),
        (
            "control character",
            note(r#""body": "bell\u0007""#),
            "Note 2 (John 3:1): A note can only hold plain text".to_string(),
        ),
        (
            "bad date",
            note(r#""body": "x", "created_at": "tomorrow""#),
            "Note 2 (John 3:1): its date isn't a valid UTC time".to_string(),
        ),
        (
            "missing body",
            v3_file_with(r#"{"book": 43, "chapter": 3, "verse_start": 1, "verse_end": 1}"#),
            "missing field `body`".to_string(),
        ),
        (
            "unknown field",
            note(r#""body": "x", "color": "blue""#),
            "unknown field `color`".to_string(),
        ),
        (
            "format 2 with notes",
            v2_file_with("", "").replace("\"bookmarks\"", "\"notes\": [], \"bookmarks\""),
            "unknown field `notes` (data format 2 has no notes)".to_string(),
        ),
        (
            "newer format",
            v3_file_with("").replace("\"format_version\": 3", "\"format_version\": 4"),
            "This file was made by a newer version of Gospel Getter (data format 4). This \
             version reads format 3; update Gospel Getter to import it."
                .to_string(),
        ),
    ];
    for (label, contents, expected) in &cases {
        let path = file(
            &db,
            &format!("{}.gospel-getter.json", label.replace(' ', "-")),
        );
        std::fs::write(&path, contents).unwrap();
        let (message, problems) = invalid(&state, &path).await;
        let all = format!("{message} {}", problems.join(" "));
        assert!(all.contains(expected.as_str()), "{label}: {all}");
        assert_eq!(
            everything(&state).await,
            before,
            "{label} changed something"
        );
    }
}

#[tokio::test]
async fn a_failed_note_import_rolls_back_everything() {
    let (db, pool) = fresh_database("rd_notes_rollback").await;
    let state = AppState::load(pool.clone()).await.unwrap();
    commands::bookmark_passage(&state, 43, 3, 16, 16)
        .await
        .unwrap();
    commands::highlight_passage(&state, 43, 3, 16, 16, "pink")
        .await
        .unwrap();
    commands::note_add(&state, john_3(16, 16), "Mine.")
        .await
        .unwrap();
    let before = everything(&state).await;

    let path = file(&db, "notes-fail.gospel-getter.json");
    std::fs::write(
        &path,
        r#"{"format": "gospel-getter-reader-data", "format_version": 3,
            "exported_at": "2026-10-10T00:00:00.000Z", "app_version": "2.8.0",
            "reading_position": {"translation": "web", "book": 19, "chapter": 23},
            "bookmarks": [{"book": 1, "chapter": 1, "verse_start": 1, "verse_end": 1}],
            "highlights": [{"book": 1, "chapter": 1, "verse": 1, "color": "blue"}],
            "notes": [{"book": 1, "chapter": 1, "verse_start": 1, "verse_end": 1, "body": "ok"},
                      {"book": 1, "chapter": 2, "verse_start": 1, "verse_end": 1, "body": "fails"}]}"#,
    )
    .unwrap();
    sqlx::query(
        "CREATE TRIGGER fail_note BEFORE INSERT ON notes \
         WHEN NEW.chapter = 2 BEGIN SELECT RAISE(ABORT, 'simulated failure'); END",
    )
    .execute(pool.write().pool())
    .await
    .unwrap();

    let p = preview(&state, &path).await;
    let error = commands::apply_pending_import(&state, p.token, true, true)
        .await
        .unwrap_err();
    assert_eq!(error, "The import failed, so nothing was changed.");
    assert_eq!(everything(&state).await, before);
}

// ---- The limits on notes

use crate::db::notes::{MAX_NOTE_CHARS, MAX_NOTES, MAX_TOTAL_NOTE_CHARS};

/// A database (and app) with `count` notes of `chars` characters each.
async fn app_with_notes(label: &str, count: usize, chars: usize) -> (TempDb, AppState) {
    let (db, pool) = fresh_database(label).await;
    fill_notes(pool.write(), count, chars).await;
    let state = AppState::load(pool).await.expect("load state");
    (db, state)
}

#[tokio::test]
async fn an_import_that_would_pass_a_note_limit_changes_nothing() {
    // One note short of the count limit; the file has two new ones.
    let (db, state) = app_with_notes("rd_notes_count_limit", MAX_NOTES - 1, 10).await;
    commands::bookmark_passage(&state, 43, 3, 16, 16)
        .await
        .unwrap();
    let before = everything(&state).await;
    let path = file(&db, "two-more.gospel-getter.json");
    std::fs::write(
        &path,
        v3_file_with(
            r#"{"book": 43, "chapter": 3, "verse_start": 1, "verse_end": 1, "body": "One."},
               {"book": 43, "chapter": 3, "verse_start": 2, "verse_end": 2, "body": "Two."}"#,
        )
        .replace(
            "\"bookmarks\": []",
            r#""bookmarks": [{"book": 1, "chapter": 1, "verse_start": 1, "verse_end": 1}]"#,
        ),
    )
    .unwrap();
    let p = preview(&state, &path).await;
    let error = commands::apply_pending_import(&state, p.token, false, true)
        .await
        .unwrap_err();
    assert_eq!(
        error,
        "Importing this file would give you more than 10,000 notes, the most Gospel Getter \
         keeps, so nothing was changed."
    );
    assert_eq!(everything(&state).await, before, "not even the bookmark");
    // Replacing leaves just the file's two, which is fine.
    let result = import(&state, &path, true).await;
    assert_eq!(
        (result.notes_added, result.notes_removed),
        (2, MAX_NOTES - 1)
    );

    // At the total size limit, less a little; the file has more than that.
    let (db, state) = app_with_notes(
        "rd_notes_text_limit",
        MAX_TOTAL_NOTE_CHARS / MAX_NOTE_CHARS,
        MAX_NOTE_CHARS,
    )
    .await;
    let first = state.store.notes().await.unwrap()[0].clone();
    commands::note_update(&state, first.id, "Short now.")
        .await
        .unwrap();
    let before = everything(&state).await;
    let path = file(&db, "too-much.gospel-getter.json");
    let long = "w".repeat(MAX_NOTE_CHARS);
    std::fs::write(
        &path,
        v3_file_with(&format!(
            r#"{{"book": 43, "chapter": 3, "verse_start": 1, "verse_end": 1, "body": "{long}"}},
               {{"book": 43, "chapter": 3, "verse_start": 2, "verse_end": 2, "body": "w"}}"#
        )),
    )
    .unwrap();
    let p = preview(&state, &path).await;
    let error = commands::apply_pending_import(&state, p.token, false, true)
        .await
        .unwrap_err();
    assert_eq!(
        error,
        "Importing this file would take your notes past their total size limit (25,000,000 \
         characters), so nothing was changed."
    );
    assert_eq!(everything(&state).await, before);
    // Notes already here count once: a file holding only the reader's own
    // notes merges at the limit without complaint.
    let own = file(&db, "own.gospel-getter.json");
    export_to(&state, &own, Preferences::default()).await;
    let result = import(&state, &own, false).await;
    assert_eq!(
        (result.notes_added, result.notes_already_present),
        (0, MAX_TOTAL_NOTE_CHARS / MAX_NOTE_CHARS)
    );
}

#[tokio::test]
async fn a_file_whose_notes_hold_too_much_text_is_refused() {
    let (db, state) = app("rd_notes_file_total").await;
    let count = MAX_TOTAL_NOTE_CHARS / MAX_NOTE_CHARS + 1;
    let notes: Vec<String> = (0..count)
        .map(|i| {
            format!(
                r#"{{"book": 43, "chapter": 3, "verse_start": 1, "verse_end": 1, "body": "{i:06}{}"}}"#,
                "v".repeat(MAX_NOTE_CHARS - 6)
            )
        })
        .collect();
    let path = file(&db, "file-total.gospel-getter.json");
    std::fs::write(&path, v3_file_with(&notes.join(","))).unwrap();
    let (_, problems) = invalid(&state, &path).await;
    assert_eq!(
        problems,
        ["Its notes hold 25010000 characters in all; the most one file can hold is 25000000."]
    );
    assert_eq!(note_rows(&state).await, vec![]);
}

/// The point of the limits: whatever the reader can keep, their export
/// holds and imports again.
#[tokio::test]
async fn an_export_at_both_note_limits_imports_again() {
    let (db, a) = app_with_notes(
        "rd_notes_limits_export",
        MAX_NOTES,
        MAX_TOTAL_NOTE_CHARS / MAX_NOTES,
    )
    .await;
    // At both limits: no room for one more note.
    assert!(
        commands::note_add(&a, john_3(16, 16), "x").await.is_err(),
        "the database is at its limits"
    );
    let before = note_rows(&a).await;
    assert_eq!(before.len(), MAX_NOTES);
    assert_eq!(
        before.iter().map(|n| n.4.chars().count()).sum::<usize>(),
        MAX_TOTAL_NOTE_CHARS
    );
    let path = file(&db, "at-limits.gospel-getter.json");
    let ExportOutcome::Saved { notes, .. } = export_to(&a, &path, Preferences::default()).await
    else {
        panic!("expected saved")
    };
    assert_eq!(notes, MAX_NOTES);
    assert!(std::fs::metadata(&path).unwrap().len() <= crate::reader_data::MAX_FILE_BYTES);

    let (_db_b, b) = app("rd_notes_limits_import").await;
    let p = preview(&b, &path).await;
    assert_eq!((p.notes_in_file, p.new_notes), (MAX_NOTES, MAX_NOTES));
    let result = commands::apply_pending_import(&b, p.token, true, true)
        .await
        .unwrap();
    assert_eq!(result.notes_added, MAX_NOTES);
    assert_eq!(note_rows(&b).await, before);
}

/// A string from the file (a book name, a translation code, a value the
/// layout check quotes) is cut to 80 characters with "…" wherever a
/// problem message shows it, so a huge one doesn't make a huge message.
#[tokio::test]
async fn strings_from_the_file_are_cut_short_in_problem_messages() {
    let (db, state) = app("rd_long_strings").await;
    let long = "x".repeat(10_000);
    let cut = format!("{}…", "x".repeat(MAX_SHOWN_FILE_CHARS - 1));
    let passage = r#""book": 43, "chapter": 3, "verse_start": 1, "verse_end": 1"#;
    let cases: Vec<(&str, String, String)> = vec![
        (
            "bookmark book name",
            file_with(&format!(r#"{{{passage}, "book_name": "{long}"}}"#), "null"),
            format!("Bookmark 1: book 43 is John, not {cut}."),
        ),
        (
            "highlight book name",
            v2_file_with(
                "",
                &format!(
                    r#"{{"book": 43, "book_name": "{long}", "chapter": 3, "verse": 1, "color": "blue"}}"#
                ),
            ),
            format!("Highlight 1: book 43 is John, not {cut}."),
        ),
        (
            "note book name",
            v3_file_with(&format!(
                r#"{{{passage}, "book_name": "{long}", "body": "A note."}}"#
            )),
            format!("Note 1: book 43 is John, not {cut}."),
        ),
        (
            "position translation",
            file_with(
                "",
                &format!(r#"{{"translation": "{long}", "book": 43, "chapter": 3}}"#),
            ),
            format!(
                "Reading position: this install has no \u{201c}{cut}\u{201d} translation \
                 (it has kjv, bsb, web)."
            ),
        ),
        (
            "position book name",
            file_with(
                "",
                &format!(
                    r#"{{"translation": "kjv", "book": 43, "book_name": "{long}", "chapter": 3}}"#
                ),
            ),
            format!("Reading position: book 43 is John, not {cut}."),
        ),
        (
            "highlight colour",
            v2_file_with(
                "",
                &format!(r#"{{"book": 43, "chapter": 3, "verse": 1, "color": "{long}"}}"#),
            ),
            format!("unknown variant `{cut}`, expected one of"),
        ),
        (
            "unknown field",
            file_with(&format!(r#"{{{passage}, "{long}": 1}}"#), "null"),
            format!("unknown field `{cut}`, expected one of"),
        ),
        (
            "a string for a number, quotes and all",
            file_with(
                &format!(
                    r#"{{"book": 43, "chapter": "\"{long}", "verse_start": 1, "verse_end": 1}}"#
                ),
                "null",
            ),
            format!("invalid type: string \"\\\"{}…\", expected", "x".repeat(77)),
        ),
    ];
    for (label, contents, expected) in &cases {
        let path = file(
            &db,
            &format!("{}.gospel-getter.json", label.replace(' ', "-")),
        );
        std::fs::write(&path, contents).unwrap();
        let (message, problems) = invalid(&state, &path).await;
        let all = format!("{message} {}", problems.join(" "));
        assert!(all.contains(expected.as_str()), "{label}: {all}");
        assert!(
            !all.contains(&"x".repeat(MAX_SHOWN_FILE_CHARS)),
            "{label}: {all}"
        );
        assert!(
            all.chars().count() < 600,
            "{label}: {} characters",
            all.len()
        );
    }
}
