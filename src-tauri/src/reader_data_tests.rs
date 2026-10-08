//! Export and import of the reader's data, end to end through the same
//! functions the commands use (everything but the native file dialog),
//! against disposable databases and real files in a scratch directory.

use serde_json::Value;
use std::path::{Path, PathBuf};

use crate::commands::{
    self, AppState, ExportOutcome, ImportChoice, ImportResultDto, build_reader_data,
    prepare_import, save_reader_data,
};
use crate::db::test_support::{
    TempDb, bundled, fresh_database, other_tables_snapshot, verses_snapshot,
};
use crate::reader_data::{LineSpacing, Preferences, TextSize, Theme};

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
        reading_mode: Some(false),
        compare: Some(true),
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
    assert_eq!((bookmarks, has_position, preferences), (3, true, 5));

    // The file is readable JSON with exactly the reader's data.
    let text = std::fs::read_to_string(&path).unwrap();
    let json: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(json["format"], "gospel-getter-reader-data");
    assert_eq!(json["format_version"], 2);
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
                           "reading_mode": false, "compare": true})
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
        ("other json", "[1, 2, 3]".into(), "isn't a Gospel Getter"),
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
            "no \u{201c}niv\u{201d} translation (it has kjv, web)",
        ),
        (
            "bad position chapter",
            file_with(ok, r#"{"translation": "kjv", "book": 65, "chapter": 2}"#),
            "there's no Jude 2",
        ),
        (
            "bad preference",
            file_with(ok, "null").replace(
                "\"bookmarks\"",
                "\"preferences\": {\"text_size\": \"huge\"}, \"bookmarks\"",
            ),
            "unknown variant `huge`",
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
    .execute(&pool)
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
        .execute(&pool)
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
    let verses_before = verses_snapshot(&pool).await;
    let tables_before: Vec<String> = other_tables_snapshot(&pool)
        .await
        .into_iter()
        .filter(|r| !r.starts_with("reading_position"))
        .collect();
    let index_before: Vec<(i64, i64)> =
        sqlx::query_as("SELECT translation_id, verse_count FROM search_index_state ORDER BY 1")
            .fetch_all(&pool)
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

    assert_eq!(verses_snapshot(&pool).await, verses_before);
    let tables_after: Vec<String> = other_tables_snapshot(&pool)
        .await
        .into_iter()
        .filter(|r| !r.starts_with("reading_position"))
        .collect();
    assert_eq!(tables_after, tables_before);
    let index_after: Vec<(i64, i64)> =
        sqlx::query_as("SELECT translation_id, verse_count FROM search_index_state ORDER BY 1")
            .fetch_all(&pool)
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
    assert_eq!(json["format_version"], 2);
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
            v2_file_with("", "").replace("\"format_version\": 2", "\"format_version\": 3"),
            "newer version of Gospel Getter (data format 3)",
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
    .execute(&pool)
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
