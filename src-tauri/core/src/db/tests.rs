//! Database tests against real files: fresh installs, upgrades from
//! v2.1.0, bookmarks, highlights, notes and the search index. Each test gets its own
//! disposable database (see `test_support`).

use std::time::Instant;

use super::bookmarks::{add_bookmark, list_bookmarks, remove_bookmark};
use super::highlights::{
    list_highlight_passages, list_highlights, remove_highlights, set_highlights,
};
use super::migrate::SCHEMA_VERSION;
use super::notes::{
    MAX_NOTE_CHARS, MAX_NOTES, MAX_TOTAL_NOTE_CHARS, NoteLimit, Passage, add_note, delete_note,
    list_notes, notes_overlapping, update_note,
};
use super::search::{SEARCH_CONTENT_VERSION, search_verses};
use super::test_support::{
    KJV_TAKEN_VERSE, TempDb, WEB_UNNUMBERED, as_2_29_0_left_it, as_2_29_1_left_it, bundled,
    bundled_bsb, empty_web_verses, fill_notes, fresh_database, indexed_web_unnumbered,
    other_tables_snapshot, prepared_database, table_names, verses_snapshot, web_index_count,
    write_v2_1_0_database,
};
use super::{
    BookmarkError, Db, HighlightColor, NoteBodyError, NoteError, ReadingPlace, TopVerse, Verse,
    ensure_search_index, get_chapter_verses, get_reading_place, get_verse_range, migrate, prepare,
    save_reading_position, save_reading_position_at, verses_of_passages,
};
use crate::domain::kjv_markup::VerseText;
use crate::domain::query::{SearchQuery, search_query, term_marks};

const KJV: i64 = 1;
const WEB: i64 = 2;
/// Seeded third (see `seed::SEED_ORDER`), on a new install or an upgrade.
const BSB: i64 = 3;

fn terms(words: &[&str]) -> SearchQuery {
    SearchQuery::of_words(words)
}

/// The (book, chapter, verse) of every verse in a translation the search
/// `typed` finds (as the search box reads it).
async fn found_by(
    pool: &sqlx::SqlitePool,
    translation_id: i64,
    typed: &str,
) -> Vec<(i64, i64, i64)> {
    let (total, hits) = search_verses(pool, translation_id, &search_query(typed), 1000, 0)
        .await
        .unwrap_or_else(|e| panic!("{typed:?}: {e:#}"));
    assert_eq!(total, hits.len() as i64, "{typed:?}: one page");
    hits.iter()
        .map(|h| (h.book_id, h.chapter, h.verse))
        .collect()
}

/// `verses` must hold exactly the bundled text, byte for byte, numbered as
/// the bundled files number it: every translation's.
fn assert_verses_match_bundled(snapshot: &[(i64, i64, i64, i64, Vec<u8>)]) {
    assert_verses_match(snapshot, &[(KJV, "kjv"), (WEB, "web"), (BSB, "bsb")]);
}

/// `verses` must hold exactly these translations' bundled text, byte for
/// byte, and nothing else. A verse a translation doesn't number (`null` in
/// its file, or no text at all: the WEB's five) has no row.
fn assert_verses_match(snapshot: &[(i64, i64, i64, i64, Vec<u8>)], translations: &[(i64, &str)]) {
    let mut expected = Vec::new();
    for &(translation_id, code) in translations {
        let books: Vec<Vec<Vec<Option<String>>>> = if code == "bsb" {
            bundled_bsb().into_iter().map(|b| b.chapters).collect()
        } else {
            bundled(code)
                .into_iter()
                .map(|b| {
                    b.chapters
                        .into_iter()
                        .map(|c| c.into_iter().map(Some).collect())
                        .collect()
                })
                .collect()
        };
        for (b, chapters) in books.into_iter().enumerate() {
            for (c, verses) in chapters.into_iter().enumerate() {
                for (v, text) in verses.into_iter().enumerate() {
                    let Some(text) = text.filter(|t| !t.is_empty()) else {
                        continue;
                    };
                    expected.push((
                        translation_id,
                        b as i64 + 1,
                        c as i64 + 1,
                        v as i64 + 1,
                        text.into_bytes(),
                    ));
                }
            }
        }
    }
    expected.sort();
    assert_eq!(snapshot.len(), expected.len(), "verse row count");
    assert!(snapshot == expected, "verses differ from the bundled text");
}

#[tokio::test]
async fn fresh_database_has_every_table_and_a_complete_search_index() {
    let (_db, pool) = fresh_database("fresh").await;

    let tables = table_names(pool.read()).await;
    for table in [
        "books",
        "translations",
        "verses",
        "footnotes",
        "reading_position",
        "reading_place",
        "cross_references",
        "bookmarks",
        "highlights",
        "notes",
        "ai_findings",
        "app_settings",
        "verse_search",
        "search_index_state",
        "search_index_version",
        "translation_data_version",
    ] {
        assert!(tables.iter().any(|t| t == table), "missing table {table}");
    }
    assert_eq!(
        index_version(pool.read()).await,
        Some(SEARCH_CONTENT_VERSION)
    );

    let state: Vec<(i64, i64)> =
        sqlx::query_as("SELECT translation_id, verse_count FROM search_index_state ORDER BY 1")
            .fetch_all(pool.read())
            .await
            .unwrap();
    // The WEB's file has 31,103 verses, five of them with no text, which
    // the WEB doesn't number and which aren't seeded. The KJV has its
    // 31,102 since 2.29.2 (31,100 before: ten chapters misnumbered).
    assert_eq!(state, vec![(KJV, 31_102), (WEB, 31_098), (BSB, 31_086)]);
    // Each seeded as bundled now: the WEB and the KJV past their one step
    // each (the KJV's is 2.29.2's).
    assert_eq!(
        data_versions(pool.read()).await,
        [(KJV, 2), (WEB, 2), (BSB, 1)]
    );

    let bookmarks: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM bookmarks")
        .fetch_one(pool.read())
        .await
        .unwrap();
    assert_eq!(bookmarks, 0);

    // A normal later launch finds nothing to (re)index.
    prepare(pool.write()).await.unwrap();
    assert_eq!(ensure_search_index(&pool).await.unwrap(), Vec::<i64>::new());

    assert_verses_match_bundled(&verses_snapshot(pool.read()).await);
    assert_eq!(user_version(pool.read()).await, SCHEMA_VERSION);
}

/// The databases most tests start from are copies of templates prepared
/// once per test binary (see `test_support`). A copy of each must be
/// exactly what startup makes of a new database: the same schema, version
/// and rows, row for row, rowids included.
#[tokio::test]
async fn the_template_databases_are_what_startup_prepares() {
    let db = TempDb::new("template_check");
    let pool = db.connect().await;
    prepare(pool.write()).await.unwrap();
    let (prepared, _prepared_pool) = prepared_database("template_prepared").await;
    assert_same_database(&pool, &prepared.path()).await;

    ensure_search_index(&pool).await.unwrap();
    let (indexed, _indexed_pool) = fresh_database("template_indexed").await;
    assert_same_database(&pool, &indexed.path()).await;
}

/// The database `pool` opens and the one at `other` hold the same schema,
/// `user_version` and rows: every table's, the search index's own tables
/// included, each row with its rowid (a table without rowids by its key).
async fn assert_same_database(pool: &Db, other: &std::path::Path) {
    let mut conn = pool.write().pool().acquire().await.unwrap();
    sqlx::query("ATTACH DATABASE $1 AS other")
        .bind(other.to_str().unwrap())
        .execute(&mut *conn)
        .await
        .unwrap();

    let mut schemas = Vec::new();
    for schema in ["main", "other"] {
        // `rootpage` is left out: it's where a table starts in the file.
        let rows: Vec<(String, String, String, Option<String>)> =
            sqlx::query_as(sqlx::AssertSqlSafe(format!(
                "SELECT type, name, tbl_name, sql FROM {schema}.sqlite_schema ORDER BY type, name"
            )))
            .fetch_all(&mut *conn)
            .await
            .unwrap();
        let version: i64 =
            sqlx::query_scalar(sqlx::AssertSqlSafe(format!("PRAGMA {schema}.user_version")))
                .fetch_one(&mut *conn)
                .await
                .unwrap();
        schemas.push((rows, version));
    }
    assert_eq!(schemas[0], schemas[1], "schema or user_version differs");
    assert_eq!(schemas[0].1, SCHEMA_VERSION);

    let tables = schemas[0].0.iter().filter(|(kind, _, _, sql)| {
        kind == "table"
            && !sql
                .as_deref()
                .unwrap_or("")
                .starts_with("CREATE VIRTUAL TABLE")
    });
    for (_, table, _, sql) in tables {
        let key = if sql.as_deref().unwrap_or("").contains("WITHOUT ROWID") {
            ""
        } else {
            "rowid, "
        };
        // Names from `sqlite_schema` just above.
        let counts: (i64, i64) = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT (SELECT COUNT(*) FROM main.\"{table}\"), \
                    (SELECT COUNT(*) FROM other.\"{table}\")"
        )))
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert_eq!(counts.0, counts.1, "{table}: row count differs");
        for (from, to) in [("main", "other"), ("other", "main")] {
            let unmatched: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
                "SELECT COUNT(*) FROM (SELECT {key}* FROM {from}.\"{table}\" \
                 EXCEPT SELECT {key}* FROM {to}.\"{table}\")"
            )))
            .fetch_one(&mut *conn)
            .await
            .unwrap();
            assert_eq!(unmatched, 0, "{table}: rows in {from} missing from {to}");
        }
    }

    sqlx::query("DETACH DATABASE other")
        .execute(&mut *conn)
        .await
        .unwrap();
}

async fn user_version(pool: &sqlx::SqlitePool) -> i64 {
    sqlx::query_scalar("PRAGMA user_version")
        .fetch_one(pool)
        .await
        .unwrap()
}

/// [`WEB_UNNUMBERED`] as (translation, book, chapter, verse).
fn web_unnumbered() -> Vec<(i64, i64, i64, i64)> {
    WEB_UNNUMBERED
        .iter()
        .map(|&(b, c, v)| (WEB, b, c, v))
        .collect()
}

/// Each translation's recorded data version (`seed::DATA_STEPS`).
async fn data_versions(pool: &sqlx::SqlitePool) -> Vec<(i64, i64)> {
    sqlx::query_as(
        "SELECT translation_id, version FROM translation_data_version ORDER BY translation_id",
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

/// No verses, as [`empty_web_verses`] lists them.
const NONE: Vec<(i64, i64, i64)> = Vec::new();

/// Every row of the reader's own tables, each as its columns' SQL literals.
async fn readers_tables(pool: &sqlx::SqlitePool) -> Vec<String> {
    let mut rows = Vec::new();
    for table in [
        "notes",
        "bookmarks",
        "highlights",
        "ai_findings",
        "reading_position",
        "reading_place",
        "app_settings",
    ] {
        let columns: Vec<String> =
            sqlx::query_scalar("SELECT name FROM pragma_table_info($1) ORDER BY cid")
                .bind(table)
                .fetch_all(pool)
                .await
                .unwrap();
        assert!(!columns.is_empty(), "{table}");
        let row = columns
            .iter()
            .map(|c| format!("quote(\"{c}\")"))
            .collect::<Vec<_>>()
            .join(" || '|' || ");
        // Table and column names from the list above and the schema.
        let found: Vec<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "SELECT '{table}: ' || {row} FROM \"{table}\" ORDER BY rowid"
        )))
        .fetch_all(pool)
        .await
        .unwrap();
        rows.extend(found);
    }
    rows
}

/// The reader's records on Luke 17:36 and Romans 16:25, two of the verses
/// the WEB doesn't number: a note, a bookmark, a highlight, an AI finding,
/// and the reading place, in the WEB at Luke 17:36.
async fn records_on_the_webs_unnumbered_verses(pool: &Db) {
    let luke_17_36 = Passage {
        book_id: 42,
        chapter: 17,
        verse_start: 36,
        verse_end: 36,
    };
    add_note(pool.write(), &luke_17_36, "Two in the field.", None)
        .await
        .unwrap();
    add_bookmark(pool.write(), 42, 17, 35, 37).await.unwrap();
    set_highlights(pool.write(), 45, 16, 25, 25, HighlightColor::Blue)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO ai_findings (book_id, chapter, verse_start, verse_end, agent, request) \
         VALUES (42, 17, 36, 36, 'Claude', 'taken')",
    )
    .execute(pool.write().pool())
    .await
    .unwrap();
    save_reading_position_at(
        pool.write(),
        WEB,
        42,
        17,
        Some(TopVerse {
            verse: 36,
            offset_px: Some(12.0),
        }),
    )
    .await
    .unwrap();
}

/// A new install seeds the WEB without the five verses its file has with
/// no text: they're ones it doesn't number, as the BSB's `null`s are, so
/// they have no row, and the WEB has five fewer verses than its file's
/// entries. Every verse it does have is its text, byte for byte.
#[tokio::test]
async fn the_web_is_seeded_without_the_verses_it_does_not_number() {
    let (_db, pool) = fresh_database("web_unnumbered_seed").await;
    let entries: usize = bundled("web")
        .iter()
        .map(|b| b.chapters.iter().map(Vec::len).sum::<usize>())
        .sum();
    assert_eq!(entries, 31_103);
    let web_rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM verses WHERE translation_id = $1")
        .bind(WEB)
        .fetch_one(pool.read())
        .await
        .unwrap();
    assert_eq!(web_rows, 31_098);
    assert_eq!(entries as i64 - web_rows, 5);
    let empty: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM verses WHERE text = ''")
        .fetch_one(pool.read())
        .await
        .unwrap();
    assert_eq!(empty, 0, "no translation has a verse with no text");
    for (book, chapter, verse) in WEB_UNNUMBERED {
        assert_eq!(
            bundled("web")[book as usize - 1].chapters[chapter as usize - 1][verse as usize - 1],
            "",
            "the file's entry for {book} {chapter}:{verse}"
        );
        let row: Option<String> = sqlx::query_scalar(
            "SELECT text FROM verses \
             WHERE translation_id = $1 AND book_id = $2 AND chapter = $3 AND verse = $4",
        )
        .bind(WEB)
        .bind(book)
        .bind(chapter)
        .bind(verse)
        .fetch_optional(pool.read())
        .await
        .unwrap();
        assert_eq!(row, None, "{book} {chapter}:{verse}");
        // The KJV numbers each of them.
        let kjv: Option<String> = sqlx::query_scalar(
            "SELECT text FROM verses \
             WHERE translation_id = $1 AND book_id = $2 AND chapter = $3 AND verse = $4",
        )
        .bind(KJV)
        .bind(book)
        .bind(chapter)
        .bind(verse)
        .fetch_optional(pool.read())
        .await
        .unwrap();
        assert!(
            kjv.is_some_and(|t| !t.is_empty()),
            "{book} {chapter}:{verse}"
        );
    }
    assert_eq!(indexed_web_unnumbered(pool.read()).await, 0);
    assert_verses_match_bundled(&verses_snapshot(pool.read()).await);
}

/// The first launch after upgrading from 2.29.0 takes the WEB's five empty
/// verses out, with their search index rows, and keeps the index complete
/// (no rebuild); the reader's notes, bookmarks, highlights, findings and
/// reading place on those verses keep their coordinates, and every other
/// verse is as it was. The next launch changes nothing at all.
#[tokio::test]
async fn upgrading_from_2_29_0_takes_out_the_webs_empty_verses_once() {
    let (db, pool) = fresh_database("web_unnumbered_upgrade").await;
    as_2_29_0_left_it(&pool).await;
    records_on_the_webs_unnumbered_verses(&pool).await;
    let verses_before = verses_snapshot(pool.read()).await;
    let readers_before = readers_tables(pool.read()).await;
    assert_eq!(readers_before.len(), 6, "{readers_before:?}");

    // Exactly what startup does.
    pool.close().await;
    let pool = db.connect().await;
    prepare(pool.write()).await.unwrap();

    // The five are gone, and nothing else.
    assert_eq!(empty_web_verses(pool.read()).await, NONE);
    let verses_after = verses_snapshot(pool.read()).await;
    let expected: Vec<_> = verses_before
        .iter()
        .filter(|(t, b, c, v, _)| !web_unnumbered().contains(&(*t, *b, *c, *v)))
        .cloned()
        .collect();
    assert_eq!(verses_before.len() - expected.len(), 5);
    assert!(verses_after == expected, "only the five are taken out");
    assert_verses_match_bundled(&verses_after);
    // The reader's tables, row for row.
    assert_eq!(readers_tables(pool.read()).await, readers_before);
    // The search index lost their rows, kept its count right, and is
    // complete as it is: nothing of the WEB's to rebuild. (The KJV's own
    // 2.29.2 step seeds it again, the same text here, as this stand-in has
    // today's KJV, so only the KJV is indexed again.)
    assert_eq!(indexed_web_unnumbered(pool.read()).await, 0);
    assert_eq!(web_index_count(pool.read()).await, Some(31_098));
    assert_eq!(ensure_search_index(&pool).await.unwrap(), vec![KJV]);
    assert_eq!(data_versions(pool.read()).await, [(KJV, 2), (WEB, 2)]);
    let (total, _) = search_verses(pool.read(), WEB, &terms(&["field"]), 1, 0)
        .await
        .unwrap();
    assert!(total > 0);

    // The next launch: every row of every table is as the first left it,
    // rowids included (a copy of the file, made once it's closed and its
    // write-ahead log is folded in).
    pool.close().await;
    let snapshot = TempDb::new("web_unnumbered_upgrade_snapshot");
    for suffix in ["-wal", "-shm"] {
        assert!(
            !super::connect::beside(&db.path(), suffix).exists(),
            "{suffix}"
        );
    }
    std::fs::copy(db.path(), snapshot.path()).unwrap();
    let pool = db.connect().await;
    prepare(pool.write()).await.unwrap();
    assert_eq!(ensure_search_index(&pool).await.unwrap(), Vec::<i64>::new());
    assert_same_database(&pool, &snapshot.path()).await;
}

/// Until the upgrade step has run (a database the 2.29.1 app hasn't opened
/// yet, or one whose step failed), the WEB's five verses are stored with no
/// text, and every read of verses leaves them out, as it leaves out a verse
/// with no row: a chapter, a range, passages, and a bookmark's or
/// highlighted passage's preview, which neither quotes one nor counts it
/// as numbered. Reading changes nothing: the rows are as they were.
#[tokio::test]
async fn reads_leave_out_a_verse_stored_with_no_text() {
    let (_db, pool) = fresh_database("web_unnumbered_reads").await;
    as_2_29_0_left_it(&pool).await;
    add_bookmark(pool.write(), 42, 17, 36, 36).await.unwrap();
    set_highlights(pool.write(), 42, 17, 35, 37, HighlightColor::Green)
        .await
        .unwrap();
    let before = verses_snapshot(pool.read()).await;
    let numbers = |verses: &[Verse]| verses.iter().map(|v| v.verse).collect::<Vec<_>>();

    let chapter = numbers(&get_chapter_verses(pool.read(), WEB, 42, 17).await.unwrap());
    assert!(!chapter.contains(&36), "{chapter:?}");
    assert!(
        chapter.contains(&35) && chapter.contains(&37),
        "{chapter:?}"
    );
    let range = get_verse_range(pool.read(), WEB, 42, 17, 35, 37)
        .await
        .unwrap();
    assert_eq!(numbers(&range), [35, 37]);
    let passages = verses_of_passages(pool.read(), WEB, &[[42, 17, 35, 37], [44, 8, 37, 37]])
        .await
        .unwrap();
    assert_eq!(
        passages.iter().map(|p| numbers(p)).collect::<Vec<_>>(),
        [vec![35, 37], vec![]]
    );

    let bookmark = &list_bookmarks(pool.read(), WEB).await.unwrap()[0];
    assert_eq!(
        (bookmark.first_verse_text.as_deref(), bookmark.complete),
        (None, false)
    );
    let highlighted = &list_highlight_passages(pool.read(), WEB).await.unwrap()[0];
    assert!(highlighted.first_verse_text.is_some());
    assert!(!highlighted.complete, "verse 36 isn't numbered");
    // The KJV numbers all three.
    let highlighted = &list_highlight_passages(pool.read(), KJV).await.unwrap()[0];
    assert!(highlighted.complete);

    assert_eq!(empty_web_verses(pool.read()).await, WEB_UNNUMBERED);
    assert!(
        verses_snapshot(pool.read()).await == before,
        "no verse changed"
    );
}

/// An upgrade that fails part-way leaves everything as it was, the five
/// empty verses, their index rows and count, and the version unrecorded,
/// and the app starts all the same; the next launch makes it.
#[tokio::test]
async fn a_failed_upgrade_changes_nothing_and_is_made_next_launch() {
    let (db, pool) = fresh_database("web_unnumbered_failed").await;
    as_2_29_0_left_it(&pool).await;
    records_on_the_webs_unnumbered_verses(&pool).await;
    pool.close().await;
    let pool = db.connect().await;
    migrate(pool.write()).await.unwrap();
    let verses_before = verses_snapshot(pool.read()).await;
    let readers_before = readers_tables(pool.read()).await;

    // Its last write, recording the version, fails: on the writer's one
    // connection, a trigger that refuses it.
    sqlx::query(
        "CREATE TEMP TRIGGER refuse_the_version BEFORE INSERT ON translation_data_version \
         BEGIN SELECT RAISE(ABORT, 'refused for the test'); END",
    )
    .execute(pool.write().pool())
    .await
    .unwrap();
    prepare(pool.write()).await.unwrap();
    assert_eq!(verses_snapshot(pool.read()).await, verses_before);
    assert_eq!(empty_web_verses(pool.read()).await, WEB_UNNUMBERED);
    assert_eq!(indexed_web_unnumbered(pool.read()).await, 5);
    assert_eq!(web_index_count(pool.read()).await, Some(31_103));
    assert_eq!(data_versions(pool.read()).await, Vec::<(i64, i64)>::new());
    assert_eq!(readers_tables(pool.read()).await, readers_before);
    assert_eq!(ensure_search_index(&pool).await.unwrap(), Vec::<i64>::new());

    // The next launch, a new connection without the trigger.
    pool.close().await;
    let pool = db.connect().await;
    prepare(pool.write()).await.unwrap();
    assert_eq!(empty_web_verses(pool.read()).await, NONE);
    assert_eq!(indexed_web_unnumbered(pool.read()).await, 0);
    assert_eq!(web_index_count(pool.read()).await, Some(31_098));
    // The KJV's 2.29.2 step is made with it, and only the KJV is indexed
    // again.
    assert_eq!(data_versions(pool.read()).await, [(KJV, 2), (WEB, 2)]);
    assert_eq!(readers_tables(pool.read()).await, readers_before);
    assert_eq!(ensure_search_index(&pool).await.unwrap(), vec![KJV]);
}

/// A translation a newer version of the app has brought past this one's
/// latest data version is left as it is, as `migrate` leaves a newer
/// schema; one recorded at its latest isn't changed either, even with an
/// empty verse in it.
#[tokio::test]
async fn data_past_this_versions_steps_is_left_alone() {
    let (db, pool) = fresh_database("web_unnumbered_newer").await;
    as_2_29_0_left_it(&pool).await;
    pool.close().await;
    let pool = db.connect().await;
    migrate(pool.write()).await.unwrap();
    for version in [3, 2] {
        sqlx::query(
            "INSERT INTO translation_data_version (translation_id, version) VALUES ($1, $2) \
             ON CONFLICT (translation_id) DO UPDATE SET version = excluded.version",
        )
        .bind(WEB)
        .bind(version)
        .execute(pool.write().pool())
        .await
        .unwrap();
        prepare(pool.write()).await.unwrap();
        assert_eq!(empty_web_verses(pool.read()).await, WEB_UNNUMBERED);
        // The KJV, with no version recorded, is brought to its latest.
        assert_eq!(data_versions(pool.read()).await, [(KJV, 2), (WEB, version)]);
    }
}

/// The reader's records where the KJV changes in 2.29.2: a note on
/// Matthew 2:16 (a verse 2.29.1's KJV didn't have, its words then standing
/// at 2:17's number), a note and a highlight on Matthew 2:23's coordinates
/// (2.29.1's KJV ended Matthew 2 at 22), a bookmark and an AI finding on
/// 1 Samuel 20:42–43 (2.29.1's KJV split 20:42 in two; 2.29.2's has no
/// 20:43), a highlight on Revelation 13:1, and the reading place at
/// Revelation 12:18 in the KJV (a verse 2.29.2's doesn't have).
async fn records_where_the_kjv_changes(pool: &Db) {
    let at = |book_id, chapter, verse_start, verse_end| Passage {
        book_id,
        chapter,
        verse_start,
        verse_end,
    };
    add_note(pool.write(), &at(40, 2, 16, 16), "Herod.", None)
        .await
        .unwrap();
    add_note(pool.write(), &at(40, 2, 23, 23), "Nazareth.", None)
        .await
        .unwrap();
    set_highlights(pool.write(), 40, 2, 23, 23, HighlightColor::Yellow)
        .await
        .unwrap();
    add_bookmark(pool.write(), 9, 20, 42, 43).await.unwrap();
    sqlx::query(
        "INSERT INTO ai_findings (book_id, chapter, verse_start, verse_end, agent, request) \
         VALUES (9, 20, 43, 43, 'Claude', 'parting')",
    )
    .execute(pool.write().pool())
    .await
    .unwrap();
    set_highlights(pool.write(), 66, 13, 1, 1, HighlightColor::Pink)
        .await
        .unwrap();
    save_reading_position_at(
        pool.write(),
        KJV,
        66,
        12,
        Some(TopVerse {
            verse: 18,
            offset_px: Some(4.0),
        }),
    )
    .await
    .unwrap();
}

/// The KJV's text of one verse, if it has a row.
async fn kjv_verse(
    pool: &sqlx::SqlitePool,
    (book, chapter, verse): (i64, i64, i64),
) -> Option<String> {
    sqlx::query_scalar(
        "SELECT text FROM verses \
         WHERE translation_id = $1 AND book_id = $2 AND chapter = $3 AND verse = $4",
    )
    .bind(KJV)
    .bind(book)
    .bind(chapter)
    .bind(verse)
    .fetch_optional(pool)
    .await
    .unwrap()
}

/// How many verses the KJV has.
async fn kjv_verses(pool: &sqlx::SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM verses WHERE translation_id = $1")
        .bind(KJV)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// The bundled KJV's text of one verse.
fn bundled_kjv((book, chapter, verse): (i64, i64, i64)) -> String {
    bundled("kjv")[book as usize - 1].chapters[chapter as usize - 1][verse as usize - 1].clone()
}

/// The first launch after upgrading from 2.29.1 seeds the KJV again from
/// its bundled text: 31,102 verses, Matthew 2:23 and Revelation 13:1 as
/// bundled, every verse of every translation its bundled text. Every
/// record of the reader's is as it was, row for row, the ones on verses
/// the KJV no longer numbers too. Its search index is built again, and
/// finds the verses where they are now. The next launch changes nothing.
#[tokio::test]
async fn upgrading_from_2_29_1_seeds_the_kjv_again_once() {
    let (db, pool) = fresh_database("kjv_reseed_upgrade").await;
    as_2_29_1_left_it(&pool).await;
    records_where_the_kjv_changes(&pool).await;
    let readers_before = readers_tables(pool.read()).await;
    assert_eq!(readers_before.len(), 8, "{readers_before:?}");
    let others_before = other_tables_snapshot(pool.read()).await;
    // As 2.29.1 has it: no Matthew 2:23, Revelation 12:18 and 13:1 split.
    assert_eq!(kjv_verse(pool.read(), (40, 2, 23)).await, None);
    assert_eq!(
        kjv_verse(pool.read(), (66, 12, 18)).await.as_deref(),
        Some("And I stood upon the sand of the sea.")
    );
    assert_eq!(found_by(pool.read(), KJV, "Nazarene").await, [(40, 2, 22)]);
    let sand = found_by(pool.read(), KJV, "\"sand of the sea\"").await;
    assert!(
        sand.contains(&(66, 12, 18)) && !sand.contains(&KJV_TAKEN_VERSE),
        "{sand:?}"
    );

    // Exactly what startup does.
    pool.close().await;
    let pool = db.connect().await;
    prepare(pool.write()).await.unwrap();

    assert_eq!(kjv_verses(pool.read()).await, 31_102);
    for verse in [(40, 2, 16), (40, 2, 23), KJV_TAKEN_VERSE] {
        assert_eq!(
            kjv_verse(pool.read(), verse).await,
            Some(bundled_kjv(verse)),
            "{verse:?}"
        );
    }
    assert!(bundled_kjv((40, 2, 23)).contains("Nazarene"));
    assert!(bundled_kjv(KJV_TAKEN_VERSE).starts_with("And I stood upon the sand of the sea"));
    for gone in [(9, 20, 43), (11, 22, 54), (64, 1, 15), (66, 12, 18)] {
        assert_eq!(kjv_verse(pool.read(), gone).await, None, "{gone:?}");
    }
    assert_verses_match_bundled(&verses_snapshot(pool.read()).await);
    // Every record of the reader's, and every other table, as it was.
    assert_eq!(readers_tables(pool.read()).await, readers_before);
    assert_eq!(other_tables_snapshot(pool.read()).await, others_before);
    assert_eq!(
        data_versions(pool.read()).await,
        [(KJV, 2), (WEB, 2), (BSB, 1)]
    );

    // Its search index is built again, and only its.
    assert_eq!(ensure_search_index(&pool).await.unwrap(), vec![KJV]);
    assert_eq!(found_by(pool.read(), KJV, "Nazarene").await, [(40, 2, 23)]);
    let sand = found_by(pool.read(), KJV, "\"sand of the sea\"").await;
    assert!(
        sand.contains(&KJV_TAKEN_VERSE) && !sand.contains(&(66, 12, 18)),
        "{sand:?}"
    );

    // The next launch: every row of every table is as the first left it.
    pool.close().await;
    let snapshot = TempDb::new("kjv_reseed_upgrade_snapshot");
    std::fs::copy(db.path(), snapshot.path()).unwrap();
    let pool = db.connect().await;
    prepare(pool.write()).await.unwrap();
    assert_eq!(ensure_search_index(&pool).await.unwrap(), Vec::<i64>::new());
    assert_same_database(&pool, &snapshot.path()).await;
}

/// A new install seeds the KJV as bundled, past its step: nothing to make.
#[tokio::test]
async fn a_new_install_seeds_the_kjv_past_its_step() {
    let (_db, pool) = prepared_database("kjv_reseed_new").await;
    assert_eq!(kjv_verses(pool.read()).await, 31_102);
    assert_eq!(
        super::upgrade_bundled_data(pool.write()).await.unwrap(),
        Vec::<(String, i64)>::new()
    );
    assert_eq!(
        data_versions(pool.read()).await,
        [(KJV, 2), (WEB, 2), (BSB, 1)]
    );
}

/// A re-seed that fails part-way (an insert refused, after the KJV's rows
/// were cleared and most inserted again) leaves every row as it was: the
/// KJV's 31,100 verses, its index and its version. What's logged names no
/// record of the reader's, and the app starts all the same; the next
/// launch makes it.
#[tokio::test]
async fn a_failed_kjv_reseed_changes_nothing_and_is_made_next_launch() {
    let (db, pool) = fresh_database("kjv_reseed_failed").await;
    as_2_29_1_left_it(&pool).await;
    records_where_the_kjv_changes(&pool).await;
    pool.close().await;
    let pool = db.connect().await;
    migrate(pool.write()).await.unwrap();
    let verses_before = verses_snapshot(pool.read()).await;
    let readers_before = readers_tables(pool.read()).await;

    // On the writer's one connection, a trigger that refuses Matthew 2:23,
    // late in the re-seed.
    sqlx::query(
        "CREATE TEMP TRIGGER refuse_a_verse BEFORE INSERT ON verses \
         WHEN NEW.translation_id = 1 AND NEW.book_id = 40 AND NEW.chapter = 2 \
             AND NEW.verse = 23 \
         BEGIN SELECT RAISE(ABORT, 'refused for the test'); END",
    )
    .execute(pool.write().pool())
    .await
    .unwrap();
    let error = format!(
        "{:#}",
        super::upgrade_bundled_data(pool.write()).await.unwrap_err()
    );
    assert!(error.contains("refused for the test"), "{error}");
    for record in ["Herod.", "Nazareth.", "parting"] {
        assert!(!error.contains(record), "{error}");
    }
    // Startup goes on.
    prepare(pool.write()).await.unwrap();
    assert!(
        verses_snapshot(pool.read()).await == verses_before,
        "every verse as it was"
    );
    assert_eq!(kjv_verses(pool.read()).await, 31_100);
    assert_eq!(
        data_versions(pool.read()).await,
        [(KJV, 1), (WEB, 2), (BSB, 1)]
    );
    assert_eq!(readers_tables(pool.read()).await, readers_before);
    assert_eq!(ensure_search_index(&pool).await.unwrap(), Vec::<i64>::new());
    assert_eq!(found_by(pool.read(), KJV, "Nazarene").await, [(40, 2, 22)]);

    // The next launch, a new connection without the trigger.
    pool.close().await;
    let pool = db.connect().await;
    prepare(pool.write()).await.unwrap();
    assert_eq!(kjv_verses(pool.read()).await, 31_102);
    assert_verses_match_bundled(&verses_snapshot(pool.read()).await);
    assert_eq!(
        data_versions(pool.read()).await,
        [(KJV, 2), (WEB, 2), (BSB, 1)]
    );
    assert_eq!(readers_tables(pool.read()).await, readers_before);
    assert_eq!(ensure_search_index(&pool).await.unwrap(), vec![KJV]);
    assert_eq!(found_by(pool.read(), KJV, "Nazarene").await, [(40, 2, 23)]);
}

/// A record on a verse number 2.29.2's KJV no longer has (`RETIRED_VERSES`:
/// numbered by 2.29.1's KJV, by no translation since) can be saved again,
/// as an undo saves a deleted one, and is listed as not numbered, never
/// with another verse's text; the verse after it is no verse at all.
#[tokio::test]
async fn a_record_on_a_verse_the_kjv_numbered_until_2_29_2_can_be_saved_again() {
    let (_db, pool) = prepared_database("kjv_retired").await;
    for (book, chapter, verse) in super::seed::RETIRED_VERSES {
        assert_eq!(kjv_verse(pool.read(), (book, chapter, verse)).await, None);
        assert_eq!(
            super::get_chapter_last_verse(pool.read(), book, chapter)
                .await
                .unwrap(),
            verse
        );
    }
    let note = add_note(
        pool.write(),
        &Passage {
            book_id: 64,
            chapter: 1,
            verse_start: 15,
            verse_end: 15,
        },
        "Greet the friends.",
        None,
    )
    .await
    .unwrap();
    let deleted = delete_note(pool.write(), note.id).await.unwrap().unwrap();
    let passage = Passage {
        book_id: deleted.book_id,
        chapter: deleted.chapter,
        verse_start: deleted.verse_start,
        verse_end: deleted.verse_end,
    };
    let dates = (deleted.created_at.as_str(), deleted.updated_at.as_str());
    let restored = add_note(pool.write(), &passage, &deleted.body, Some(dates))
        .await
        .unwrap();
    assert_eq!(
        (restored.verse_start, restored.body.as_str()),
        (15, "Greet the friends.")
    );
    add_bookmark(pool.write(), 9, 20, 42, 43).await.unwrap();
    set_highlights(pool.write(), 66, 12, 18, 18, HighlightColor::Blue)
        .await
        .unwrap();
    assert!(matches!(
        add_bookmark(pool.write(), 64, 1, 16, 16).await,
        Err(BookmarkError::NoSuchVerse { last_verse: 15, .. })
    ));

    for translation in [KJV, WEB, BSB] {
        let bookmark = &list_bookmarks(pool.read(), translation).await.unwrap()[0];
        assert!(!bookmark.complete, "1 Samuel 20:43 isn't numbered");
        assert!(bookmark.first_verse_text.is_some(), "20:42 is");
        let highlighted = &list_highlight_passages(pool.read(), translation)
            .await
            .unwrap()[0];
        assert_eq!(
            (
                highlighted.first_verse_text.as_deref(),
                highlighted.complete
            ),
            (None, false)
        );
    }
}

/// `migrate` records the schema's version, which `SCHEMA_VERSION` names,
/// and leaves a newer one (from a later app) as it is.
#[tokio::test]
async fn the_schema_version_is_recorded_and_never_lowered() {
    let db = TempDb::new("schema_version");
    let pool = db.connect().await;
    assert_eq!(user_version(pool.read()).await, 0);
    migrate(pool.write()).await.unwrap();
    assert_eq!(SCHEMA_VERSION, 1);
    assert_eq!(user_version(pool.read()).await, SCHEMA_VERSION);
    migrate(pool.write()).await.unwrap();
    assert_eq!(user_version(pool.read()).await, SCHEMA_VERSION);

    sqlx::query("PRAGMA user_version = 7")
        .execute(pool.write().pool())
        .await
        .unwrap();
    migrate(pool.write()).await.unwrap();
    assert_eq!(user_version(pool.read()).await, 7);
}

#[tokio::test]
async fn upgrading_a_v2_1_0_database_keeps_its_data_and_adds_bookmarks_and_search() {
    let db = TempDb::new("upgrade");
    write_v2_1_0_database(&db.path()).await;

    let pool = db.connect().await;
    let verses_before = verses_snapshot(pool.read()).await;
    let others_before = other_tables_snapshot(pool.read()).await;
    assert_eq!(user_version(pool.read()).await, 0);
    assert!(
        !table_names(pool.read())
            .await
            .iter()
            .any(|t| t == "bookmarks"),
        "fixture should be a pre-bookmarks database"
    );
    assert!(
        others_before
            .iter()
            .any(|r| r == "reading_position: 1|2|45|8"),
        "fixture should have a saved reading position: {others_before:?}"
    );

    // Exactly what startup does.
    prepare(pool.write()).await.unwrap();
    assert_eq!(
        ensure_search_index(&pool).await.unwrap(),
        vec![KJV, WEB, BSB]
    );

    // The KJV's text is as it was (this stand-in seeds today's KJV, which
    // its 2.29.2 step seeds again), and so is the WEB's but for the five
    // verses with no text, which it doesn't number (2.1.0 to 2.29.0 seeded
    // them as empty verses); the BSB, which 2.1.0 didn't have, is added
    // after them, exactly as bundled.
    let numbered_before: Vec<_> = verses_before
        .iter()
        .filter(|(.., text)| !text.is_empty())
        .cloned()
        .collect();
    assert_eq!(verses_before.len() - numbered_before.len(), 5);
    let empty_before: Vec<_> = verses_before
        .iter()
        .filter(|(.., text)| text.is_empty())
        .map(|&(t, b, c, v, _)| (t, b, c, v))
        .collect();
    assert_eq!(empty_before, web_unnumbered());
    assert_verses_match(&numbered_before, &[(KJV, "kjv"), (WEB, "web")]);
    let verses_after = verses_snapshot(pool.read()).await;
    assert_eq!(verses_after[..numbered_before.len()], numbered_before[..]);
    assert_verses_match_bundled(&verses_after);
    // Every other row is as it was; the BSB is listed after the two.
    let mut others_expected = others_before.clone();
    let at = others_expected
        .iter()
        .position(|r| r.starts_with("reading_position"))
        .unwrap();
    others_expected.insert(at, "translations: 3|bsb|Berean Standard Bible".to_string());
    assert_eq!(other_tables_snapshot(pool.read()).await, others_expected);
    assert_eq!(user_version(pool.read()).await, SCHEMA_VERSION);
    let tables = table_names(pool.read()).await;
    for table in [
        "footnotes",
        "bookmarks",
        "highlights",
        "notes",
        "verse_search",
        "search_index_state",
    ] {
        assert!(
            tables.iter().any(|t| t == table),
            "upgrade should add {table}"
        );
    }

    // The new features work on the upgraded database.
    let (total, hits) = search_verses(pool.read(), WEB, &terms(&["shepherd"]), 5, 0)
        .await
        .unwrap();
    assert!(total > 0 && !hits.is_empty());
    add_bookmark(pool.write(), 43, 3, 16, 16).await.unwrap();

    // And upgrading again (the next launch) changes nothing.
    pool.close().await;
    let pool = db.connect().await;
    prepare(pool.write()).await.unwrap();
    assert_eq!(ensure_search_index(&pool).await.unwrap(), Vec::<i64>::new());
    assert_eq!(verses_snapshot(pool.read()).await, verses_after);
    assert_eq!(other_tables_snapshot(pool.read()).await, others_expected);
    assert_eq!(list_bookmarks(pool.read(), KJV).await.unwrap().len(), 1);
}

#[tokio::test]
async fn indexing_leaves_every_verse_byte_for_byte_unchanged() {
    let (_db, pool) = fresh_database("integrity").await;
    let before = verses_snapshot(pool.read()).await;
    assert_verses_match_bundled(&before);

    // Force a full rebuild of every translation.
    sqlx::query("DELETE FROM search_index_state")
        .execute(pool.write().pool())
        .await
        .unwrap();
    assert_eq!(
        ensure_search_index(&pool).await.unwrap(),
        vec![KJV, WEB, BSB]
    );

    assert_eq!(verses_snapshot(pool.read()).await, before);

    // Nothing can write to `verses` behind the index's back.
    let triggers: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE type = 'trigger'")
            .fetch_one(pool.read())
            .await
            .unwrap();
    assert_eq!(triggers, 0);
}

#[tokio::test]
async fn search_returns_stored_text_in_bible_order_per_translation() {
    let (_db, pool) = fresh_database("search").await;

    let (total, hits) = search_verses(
        pool.read(),
        KJV,
        &terms(&["faith", "hope", "charity"]),
        50,
        0,
    )
    .await
    .unwrap();
    assert_eq!(total, hits.len() as i64);
    assert!(
        hits.iter()
            .any(|h| (h.book_id, h.chapter, h.verse) == (46, 13, 13)),
        "1 Corinthians 13:13 should match"
    );
    for h in &hits {
        let lower = h.text.to_lowercase();
        for word in ["faith", "hope", "charity"] {
            assert!(lower.contains(word), "{h:?} should contain {word}");
        }
    }

    // Every hit's text is the stored verse, and hits are in canonical order.
    let (total, page) = search_verses(pool.read(), WEB, &terms(&["love"]), 50, 0)
        .await
        .unwrap();
    assert!(total > 50, "a common word should need paging (got {total})");
    assert_eq!(page.len(), 50);
    let web = bundled("web");
    for h in &page {
        let stored =
            &web[h.book_id as usize - 1].chapters[h.chapter as usize - 1][h.verse as usize - 1];
        assert_eq!(h.text.as_bytes(), stored.as_bytes());
    }
    let keys: Vec<_> = page
        .iter()
        .map(|h| (h.book_id, h.chapter, h.verse))
        .collect();
    let mut sorted = keys.clone();
    sorted.sort();
    assert_eq!(keys, sorted);

    // Paging continues where the last page left off, without overlap.
    let (_, next) = search_verses(pool.read(), WEB, &terms(&["love"]), 50, 50)
        .await
        .unwrap();
    let next_first = next.first().map(|h| (h.book_id, h.chapter, h.verse));
    assert!(next_first > keys.last().copied());

    // Each translation only finds its own text.
    let (kjv_yahweh, _) = search_verses(pool.read(), KJV, &terms(&["yahweh"]), 1, 0)
        .await
        .unwrap();
    let (web_yahweh, _) = search_verses(pool.read(), WEB, &terms(&["yahweh"]), 1, 0)
        .await
        .unwrap();
    assert_eq!(kjv_yahweh, 0);
    assert!(web_yahweh > 1000);

    // Curly apostrophes split words the same way for the index and the query.
    let (_, keeper) = search_verses(pool.read(), WEB, &terms(&["brother", "s", "keeper"]), 5, 0)
        .await
        .unwrap();
    assert!(
        keeper
            .iter()
            .any(|h| (h.book_id, h.chapter, h.verse) == (1, 4, 9))
    );

    // FTS query syntax typed as words is searched for literally, not run.
    for syntax in [
        &["or"][..],
        &["near"],
        &["not"],
        &["\"", "*"],
        &["text:faith"],
    ] {
        search_verses(pool.read(), KJV, &terms(syntax), 5, 0)
            .await
            .unwrap_or_else(|e| panic!("{syntax:?} should be searched literally: {e:#}"));
    }
}

/// Words in double quotes find that phrase: its words in order, side by
/// side (what's between them in the verse, a space or a comma, isn't a
/// word), case and accents folded; mixed with free words, which may be
/// anywhere. Typed FTS syntax, in quotes or out, is only ever words.
#[tokio::test]
async fn a_phrase_in_quotes_finds_its_words_in_order_and_side_by_side() {
    let (_db, pool) = fresh_database("phrase").await;
    let pool = pool.read();
    let john_13_34 = (43, 13, 34);
    let rom_12_10 = (45, 12, 10);

    // The words anywhere: John 13:34 and Romans 12:10 ("one to another
    // with brotherly love") both have all three.
    let anywhere = found_by(pool, KJV, "love one another").await;
    assert!(anywhere.contains(&john_13_34) && anywhere.contains(&rom_12_10));
    // As a phrase, only where they stand together, in that order.
    let phrase = found_by(pool, KJV, "\"love one another\"").await;
    assert!(phrase.contains(&john_13_34));
    assert!(!phrase.contains(&rom_12_10), "apart isn't the phrase");
    assert!(phrase.len() < anywhere.len());
    // Order matters.
    assert!(found_by(pool, KJV, "\"another one love\"").await.is_empty());
    // Every hit has the phrase, as its marks say.
    for &(book, chapter, verse) in &phrase {
        let (_, hits) = search_verses(pool, KJV, &search_query("\"love one another\""), 1000, 0)
            .await
            .unwrap();
        let hit = hits
            .iter()
            .find(|h| (h.book_id, h.chapter, h.verse) == (book, chapter, verse))
            .unwrap();
        let shown = VerseText::new("kjv", &hit.text).text;
        let marks = term_marks(&shown, &search_query("\"love one another\""));
        assert!(
            marks.len() >= 3 && marks.len().is_multiple_of(3),
            "{shown}: {marks:?}"
        );
    }

    // Case and accents fold, as for words: typed in capitals, curly
    // quotes, or with accents the text doesn't have.
    for typed in [
        "\"LOVE ONE ANOTHER\"",
        "\u{201c}Love one another\u{201d}",
        "\"lóve oné anóther\"",
    ] {
        assert_eq!(found_by(pool, KJV, typed).await, phrase, "{typed}");
    }

    // A phrase with free words: the words may be anywhere in the verse.
    let mixed = found_by(pool, KJV, "\"love one another\" commandment").await;
    assert!(mixed.contains(&john_13_34));
    assert!(mixed.iter().all(|v| phrase.contains(v)));
    assert!(mixed.len() < phrase.len());

    // Punctuation between the words isn't a word: "Am I my brother's
    // keeper?" (Genesis 4:9) has "brother s keeper" side by side, and an
    // apostrophe or a single quote typed in the phrase parts the words.
    for typed in [
        "\"my brother's keeper\"",
        "\u{201c}my brother\u{2019}s keeper\u{201d}",
    ] {
        assert_eq!(found_by(pool, KJV, typed).await, [(1, 4, 9)], "{typed}");
    }

    // An unclosed quote's words are words.
    assert_eq!(found_by(pool, KJV, "\"love one another").await, anywhere);

    // Typed FTS syntax is words, searched for as words: it runs, never
    // fails, and finds no more than the plain words would.
    let either = found_by(pool, KJV, "\"faith\" OR \"hope\"").await;
    assert_eq!(either, found_by(pool, KJV, "faith or hope").await);
    for typed in [
        "\"a\" OR \"b\"",
        "NEAR(faith hope)",
        "\"NEAR(faith hope, 2)\"",
        "faith*",
        "\"lov*\"",
        "text:faith",
        "\"faith\" NOT \"works\"",
        "^faith",
        "(\"faith",
        "\"\"\"\"",
    ] {
        let hits = found_by(pool, KJV, typed).await;
        let plain = search_query(typed);
        let words = plain
            .phrases
            .iter()
            .flatten()
            .chain(&plain.words)
            .map(String::as_str)
            .collect::<Vec<_>>();
        let (anywhere, _) = search_verses(pool, KJV, &terms(&words), 1000, 0)
            .await
            .unwrap();
        assert!(
            hits.len() as i64 <= anywhere,
            "{typed}: {} > {anywhere}",
            hits.len()
        );
    }
    assert!(found_by(pool, KJV, "faith*").await.contains(&(46, 13, 13)));
}

/// The version `search_index_version` records, if any.
async fn index_version(pool: &sqlx::SqlitePool) -> Option<i64> {
    sqlx::query_scalar("SELECT version FROM search_index_version WHERE id = 1")
        .fetch_optional(pool)
        .await
        .unwrap()
}

/// The (book, chapter, verse) of every verse in a translation with all of
/// `words`.
async fn found(pool: &sqlx::SqlitePool, translation: i64, words: &[&str]) -> Vec<(i64, i64, i64)> {
    let (_, hits) = search_verses(pool, translation, &terms(words), 10_000, 0)
        .await
        .unwrap();
    hits.iter()
        .map(|h| (h.book_id, h.chapter, h.verse))
        .collect()
}

const PSALM_119_105: (i64, i64, i64) = (19, 119, 105);
const PSALM_23_1: (i64, i64, i64) = (19, 23, 1);

#[tokio::test]
async fn the_kjv_is_searched_as_shown_without_its_margin_notes() {
    let (_db, pool) = fresh_database("search_shown").await;

    // "{lamp: or, candle}" is Psalms 119:105's margin note, not its text.
    let candle = found(pool.read(), KJV, &["candle"]).await;
    assert!(!candle.contains(&PSALM_119_105), "{candle:?}");
    // Other verses do say "candle" (Matthew 5:15).
    assert!(candle.contains(&(40, 5, 15)), "{candle:?}");
    // The verse's own words still find it, supplied words included.
    assert!(
        found(pool.read(), KJV, &["lamp", "feet"])
            .await
            .contains(&PSALM_119_105)
    );
    assert!(
        found(pool.read(), KJV, &["shepherd"])
            .await
            .contains(&PSALM_23_1)
    );
    assert!(
        found(pool.read(), KJV, &["lord", "is", "my", "shepherd"])
            .await
            .contains(&PSALM_23_1)
    );
    // Luke 17:36's note on the Greek copies isn't searchable either.
    assert!(
        !found(pool.read(), KJV, &["greek", "copies"])
            .await
            .contains(&(42, 17, 36))
    );
    // A malformed verse is indexed as stored (Hebrews 10:34's note too).
    assert!(
        found(pool.read(), KJV, &["enduring", "substance"])
            .await
            .contains(&(58, 10, 34))
    );

    // The WEB is indexed exactly as stored.
    assert!(
        found(pool.read(), WEB, &["lamp", "feet"])
            .await
            .contains(&PSALM_119_105)
    );
}

#[tokio::test]
async fn an_index_of_stored_text_is_rebuilt_once() {
    let (_db, pool) = fresh_database("search_reindex").await;
    let verses_before = verses_snapshot(pool.read()).await;

    // As 2.9.1 left it: no version table, and the KJV indexed as stored,
    // so its margin notes' words find their verses.
    sqlx::query("DROP TABLE search_index_version")
        .execute(pool.write().pool())
        .await
        .unwrap();
    sqlx::query("DELETE FROM verse_search WHERE rowid >= 100000000 AND rowid < 200000000")
        .execute(pool.write().pool())
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO verse_search (rowid, text) \
         SELECT 100000000 + book_id * 1000000 + chapter * 1000 + verse, text \
         FROM verses WHERE translation_id = 1",
    )
    .execute(pool.write().pool())
    .await
    .unwrap();
    assert!(
        found(pool.read(), KJV, &["candle"])
            .await
            .contains(&PSALM_119_105)
    );

    // The first launch after upgrading rebuilds everything, once.
    prepare(pool.write()).await.unwrap();
    let started = Instant::now();
    assert_eq!(
        ensure_search_index(&pool).await.unwrap(),
        vec![KJV, WEB, BSB]
    );
    println!("rebuild after upgrading: {:?}", started.elapsed());
    assert_eq!(
        index_version(pool.read()).await,
        Some(SEARCH_CONTENT_VERSION)
    );
    assert!(
        !found(pool.read(), KJV, &["candle"])
            .await
            .contains(&PSALM_119_105)
    );
    assert!(
        found(pool.read(), KJV, &["shepherd"])
            .await
            .contains(&PSALM_23_1)
    );

    // The next launch doesn't.
    prepare(pool.write()).await.unwrap();
    assert_eq!(ensure_search_index(&pool).await.unwrap(), Vec::<i64>::new());

    // An index recorded as an older version is rebuilt again.
    sqlx::query("UPDATE search_index_version SET version = $1")
        .bind(SEARCH_CONTENT_VERSION - 1)
        .execute(pool.write().pool())
        .await
        .unwrap();
    assert_eq!(
        ensure_search_index(&pool).await.unwrap(),
        vec![KJV, WEB, BSB]
    );
    assert_eq!(
        index_version(pool.read()).await,
        Some(SEARCH_CONTENT_VERSION)
    );
    assert!(
        !found(pool.read(), KJV, &["candle"])
            .await
            .contains(&PSALM_119_105)
    );

    // None of it touches the stored text.
    assert_eq!(verses_snapshot(pool.read()).await, verses_before);
    assert_verses_match_bundled(&verses_before);
}

#[tokio::test]
async fn a_reseeded_translation_is_reindexed_on_its_own() {
    let (_db, pool) = fresh_database("reseed").await;
    let footnotes: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM footnotes")
        .fetch_one(pool.read())
        .await
        .unwrap();

    // As if the BSB's rows (its footnotes, verses and listing) were cleared
    // and seeded again by a later launch.
    sqlx::query("DELETE FROM footnotes WHERE translation_id = 3")
        .execute(pool.write().pool())
        .await
        .unwrap();
    sqlx::query("DELETE FROM verses WHERE translation_id = 3")
        .execute(pool.write().pool())
        .await
        .unwrap();
    sqlx::query("DELETE FROM translations WHERE id = 3")
        .execute(pool.write().pool())
        .await
        .unwrap();
    prepare(pool.write()).await.unwrap();
    assert_eq!(ensure_search_index(&pool).await.unwrap(), vec![BSB]);
    let (total, _) = search_verses(pool.read(), BSB, &terms(&["shepherd"]), 1, 0)
        .await
        .unwrap();
    assert!(total > 10);
    assert_verses_match_bundled(&verses_snapshot(pool.read()).await);
    let after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM footnotes")
        .fetch_one(pool.read())
        .await
        .unwrap();
    assert_eq!((footnotes, after), (4_854, 4_854));
}

#[tokio::test]
async fn bookmarks_are_validated_deduplicated_and_survive_a_restart() {
    let (db, pool) = prepared_database("bookmarks").await;

    let john = add_bookmark(pool.write(), 43, 3, 16, 18).await.unwrap();
    let genesis = add_bookmark(pool.write(), 1, 1, 1, 1).await.unwrap();
    assert_eq!((john.verse_start, john.verse_end), (16, 18));

    // The same passage again is the same bookmark, not a second one.
    let again = add_bookmark(pool.write(), 43, 3, 16, 18).await.unwrap();
    assert_eq!(again, john);
    // An overlapping but different passage is its own bookmark.
    let single = add_bookmark(pool.write(), 43, 3, 16, 16).await.unwrap();
    assert_ne!(single.id, john.id);

    for (book, chapter, start, end) in [(43, 3, 0, 1), (43, 3, 5, 4), (43, 3, -2, -1)] {
        assert!(
            matches!(
                add_bookmark(pool.write(), book, chapter, start, end).await,
                Err(BookmarkError::InvalidRange { .. })
            ),
            "{book} {chapter}:{start}-{end}"
        );
    }
    for (book, chapter) in [(43, 22), (43, 0), (67, 1), (0, 1), (65, 2)] {
        assert!(
            matches!(
                add_bookmark(pool.write(), book, chapter, 1, 1).await,
                Err(BookmarkError::NoSuchChapter { .. })
            ),
            "{book} {chapter}"
        );
    }
    assert!(matches!(
        add_bookmark(pool.write(), 43, 3, 36, 37).await,
        Err(BookmarkError::NoSuchVerse {
            verse: 37,
            last_verse: 36,
            ..
        })
    ));
    // The database itself refuses what validation would: the CHECKs back
    // it up.
    assert!(
        sqlx::query(
            "INSERT INTO bookmarks (book_id, chapter, verse_start, verse_end) VALUES (43, 3, 5, 4)"
        )
        .execute(pool.write().pool())
        .await
        .is_err()
    );

    // Newest first.
    let ids: Vec<i64> = list_bookmarks(pool.read(), KJV)
        .await
        .unwrap()
        .into_iter()
        .map(|b| b.bookmark.id)
        .collect();
    assert_eq!(ids, vec![single.id, genesis.id, john.id]);

    // Restart: close and reopen the file, running startup again.
    pool.close().await;
    let pool = db.connect().await;
    prepare(pool.write()).await.unwrap();
    let listed = list_bookmarks(pool.read(), WEB).await.unwrap();
    assert_eq!(listed.len(), 3);
    assert_eq!(listed[2].bookmark, john);
    assert_eq!(
        listed[2].first_verse_text.as_deref(),
        Some(bundled("web")[42].chapters[2][15].as_str())
    );

    // Removing.
    assert!(remove_bookmark(pool.write(), genesis.id).await.unwrap());
    assert!(!remove_bookmark(pool.write(), genesis.id).await.unwrap());
    let ids: Vec<i64> = list_bookmarks(pool.read(), KJV)
        .await
        .unwrap()
        .into_iter()
        .map(|b| b.bookmark.id)
        .collect();
    assert_eq!(ids, vec![single.id, john.id]);
}

#[tokio::test]
async fn bookmarks_follow_each_translations_own_verse_numbering() {
    let (_db, pool) = prepared_database("numbering").await;

    // Romans 14:24 is numbered in the WEB (26 verses) but not the KJV (23),
    // and Romans 16:25 the other way round (the WEB's Romans 16 ends at 24).
    // Both can be bookmarked. (Until 2.29.2 the KJV's misnumbered Matthew 2
    // and 3 John were the examples.)
    let web_only = add_bookmark(pool.write(), 45, 14, 23, 24).await.unwrap();
    let kjv_only = add_bookmark(pool.write(), 45, 16, 25, 25).await.unwrap();
    assert!(matches!(
        add_bookmark(pool.write(), 45, 14, 27, 27).await,
        Err(BookmarkError::NoSuchVerse { last_verse: 26, .. })
    ));

    let in_kjv = list_bookmarks(pool.read(), KJV).await.unwrap();
    let in_web = list_bookmarks(pool.read(), WEB).await.unwrap();
    let find = |list: &[super::BookmarkInTranslation], id: i64| {
        list.iter()
            .find(|b| b.bookmark.id == id)
            .cloned()
            .expect("bookmark listed")
    };

    let m_kjv = find(&in_kjv, web_only.id);
    assert!(!m_kjv.complete, "KJV has no Romans 14:24");
    assert_eq!(
        m_kjv.first_verse_text.as_deref(),
        Some(bundled("kjv")[44].chapters[13][22].as_str()),
        "KJV preview is its own 14:23, unaltered"
    );
    let m_web = find(&in_web, web_only.id);
    assert!(m_web.complete);
    assert_eq!(
        m_web.first_verse_text.as_deref(),
        Some(bundled("web")[44].chapters[13][22].as_str())
    );

    let j_kjv = find(&in_kjv, kjv_only.id);
    assert!(j_kjv.complete);
    let j_web = find(&in_web, kjv_only.id);
    assert!(!j_web.complete, "WEB has no Romans 16:25");
    assert_eq!(j_web.first_verse_text, None, "never another verse's text");

    // The stored coordinates are the same whichever translation views them.
    assert_eq!(m_kjv.bookmark, m_web.bookmark);
}

/// Times representative searches over the full Bible. Ignored by default
/// because timing depends on the machine and build profile; run with
/// `cargo test --release search_timing -- --ignored --nocapture`.
#[tokio::test]
#[ignore = "timing; run explicitly in release mode"]
async fn search_timing() {
    let (_db, pool) = fresh_database("timing").await;

    let db = TempDb::new("timing_index");
    let fresh = db.connect().await;
    prepare(fresh.write()).await.unwrap();
    let started = Instant::now();
    ensure_search_index(&fresh).await.unwrap();
    println!("index build, every translation: {:?}", started.elapsed());

    // The first launch after upgrading to 2.15: the BSB is seeded and
    // indexed; the KJV and WEB are already.
    let upgrade = TempDb::new("timing_upgrade");
    write_v2_1_0_database(&upgrade.path()).await;
    let upgraded = upgrade.connect().await;
    prepare(upgraded.write()).await.unwrap();
    ensure_search_index(&upgraded).await.unwrap();
    sqlx::query("DELETE FROM footnotes")
        .execute(upgraded.write().pool())
        .await
        .unwrap();
    sqlx::query("DELETE FROM verses WHERE translation_id = 3")
        .execute(upgraded.write().pool())
        .await
        .unwrap();
    sqlx::query("DELETE FROM translations WHERE id = 3")
        .execute(upgraded.write().pool())
        .await
        .unwrap();
    let started = Instant::now();
    prepare(upgraded.write()).await.unwrap();
    let seeded = started.elapsed();
    assert_eq!(ensure_search_index(&upgraded).await.unwrap(), vec![BSB]);
    println!(
        "upgrade to the BSB: seeding {seeded:?}, then indexing {:?}",
        started.elapsed() - seeded
    );

    let mut worst = std::time::Duration::ZERO;
    for query in [
        &["faith", "hope", "love"][..],
        &["love"],
        &["the"],
        &["and"],
        &["shepherd"],
        &["in", "the", "beginning"],
        &["yahweh"],
    ] {
        for translation in [KJV, WEB] {
            let started = Instant::now();
            let (total, hits) = search_verses(pool.read(), translation, &terms(query), 50, 0)
                .await
                .unwrap();
            let elapsed = started.elapsed();
            worst = worst.max(elapsed);
            println!(
                "{query:?} in {translation}: {total} matches, first page of {} in {elapsed:?}",
                hits.len()
            );
        }
    }
    println!("slowest: {worst:?}");
    assert!(worst.as_millis() < 100, "slowest search took {worst:?}");
}

/// (book, chapter, verse, color) of every highlight, in Bible order.
async fn highlight_colors(pool: &sqlx::SqlitePool) -> Vec<(i64, i64, i64, HighlightColor)> {
    list_highlights(pool)
        .await
        .unwrap()
        .into_iter()
        .map(|h| (h.book_id, h.chapter, h.verse, h.color))
        .collect()
}

#[tokio::test]
async fn highlights_are_set_recolored_and_removed_over_ranges() {
    use HighlightColor::{Blue, Green, Pink, Yellow};
    let (db, pool) = prepared_database("highlights").await;

    // A range: every verse in it gets the color.
    set_highlights(pool.write(), 43, 3, 16, 18, Yellow)
        .await
        .unwrap();
    assert_eq!(
        highlight_colors(pool.read()).await,
        vec![
            (43, 3, 16, Yellow),
            (43, 3, 17, Yellow),
            (43, 3, 18, Yellow)
        ]
    );

    // Recoloring part of it replaces those verses' color and their
    // updated_at, and leaves the rest alone.
    let before = list_highlights(pool.read()).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    set_highlights(pool.write(), 43, 3, 17, 18, Green)
        .await
        .unwrap();
    let after = list_highlights(pool.read()).await.unwrap();
    assert_eq!(after[0], before[0]);
    assert_eq!((after[1].color, after[2].color), (Green, Green));
    assert_eq!(after[1].created_at, before[1].created_at);
    assert!(after[1].updated_at > before[1].updated_at);

    // Setting the color a verse already has changes nothing at all.
    set_highlights(pool.write(), 43, 3, 17, 17, Green)
        .await
        .unwrap();
    assert_eq!(list_highlights(pool.read()).await.unwrap(), after);

    // Every color, and a verse only one translation numbers (Matthew 2:23
    // is WEB-only, like bookmarks).
    set_highlights(pool.write(), 19, 23, 1, 1, Blue)
        .await
        .unwrap();
    set_highlights(pool.write(), 40, 2, 23, 23, Pink)
        .await
        .unwrap();

    // Removing a range removes only highlighted verses in it.
    assert_eq!(
        remove_highlights(pool.write(), 43, 3, 15, 17)
            .await
            .unwrap(),
        2
    );
    assert_eq!(
        remove_highlights(pool.write(), 43, 3, 15, 17)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        highlight_colors(pool.read()).await,
        vec![(19, 23, 1, Blue), (40, 2, 23, Pink), (43, 3, 18, Green)]
    );

    // Invalid passages are refused, and change nothing.
    let kept = highlight_colors(pool.read()).await;
    for (book, chapter, start, end) in [
        (43, 3, 0, 1),
        (43, 3, 5, 4),
        (43, 22, 1, 1),
        (99, 1, 1, 1),
        (43, 3, 36, 37),
    ] {
        let error = set_highlights(pool.write(), book, chapter, start, end, Yellow)
            .await
            .unwrap_err();
        assert!(
            !matches!(error, BookmarkError::Database(_)),
            "{book} {chapter}:{start}-{end}: {error}"
        );
    }
    assert!(matches!(
        set_highlights(pool.write(), 43, 3, 36, 37, Yellow).await,
        Err(BookmarkError::NoSuchVerse { last_verse: 36, .. })
    ));
    assert_eq!(highlight_colors(pool.read()).await, kept);

    // The table itself refuses a color that isn't one of the four.
    let bad = sqlx::query(
        "INSERT INTO highlights (book_id, chapter, verse, color) VALUES (1, 1, 1, 'purple')",
    )
    .execute(pool.write().pool())
    .await;
    assert!(bad.is_err(), "the CHECK should refuse purple");

    // And highlights survive a restart.
    pool.close().await;
    let pool = db.connect().await;
    prepare(pool.write()).await.unwrap();
    assert_eq!(highlight_colors(pool.read()).await, kept);
}

#[tokio::test]
async fn the_highlights_table_is_added_to_an_existing_database() {
    // A database from before highlights: everything else, with a reader's
    // bookmark and position, but no highlights table.
    let (db, pool) = fresh_database("add_highlights").await;
    add_bookmark(pool.write(), 43, 3, 16, 16).await.unwrap();
    super::save_reading_position(pool.write(), 2, 45, 8)
        .await
        .unwrap();
    sqlx::query("DROP TABLE highlights")
        .execute(pool.write().pool())
        .await
        .unwrap();
    pool.close().await;

    let pool = db.connect().await;
    let others_before = other_tables_snapshot(pool.read()).await;
    assert!(
        !table_names(pool.read())
            .await
            .iter()
            .any(|t| t == "highlights")
    );

    // Exactly what startup does.
    prepare(pool.write()).await.unwrap();
    assert!(
        table_names(pool.read())
            .await
            .iter()
            .any(|t| t == "highlights")
    );
    assert_eq!(other_tables_snapshot(pool.read()).await, others_before);
    assert_eq!(list_bookmarks(pool.read(), KJV).await.unwrap().len(), 1);
    set_highlights(pool.write(), 43, 3, 16, 16, HighlightColor::Yellow)
        .await
        .unwrap();

    // Running it again (the next launch) keeps the highlight.
    prepare(pool.write()).await.unwrap();
    assert_eq!(
        highlight_colors(pool.read()).await,
        vec![(43, 3, 16, HighlightColor::Yellow)]
    );
}

#[tokio::test]
async fn highlighted_passages_preview_each_translations_own_verses() {
    let (_db, pool) = prepared_database("highlight_passages").await;
    assert_eq!(list_highlight_passages(pool.read(), KJV).await.unwrap(), []);

    // John 3:16-18 green with 3:17 recolored pink; Romans 14:23-24 (14:24
    // is numbered in the WEB only); Romans 16:25 (the KJV only). (Until
    // 2.29.2 the KJV's misnumbered Matthew 2 and 3 John were the examples.)
    set_highlights(pool.write(), 43, 3, 16, 18, HighlightColor::Green)
        .await
        .unwrap();
    set_highlights(pool.write(), 43, 3, 17, 17, HighlightColor::Pink)
        .await
        .unwrap();
    set_highlights(pool.write(), 45, 14, 23, 24, HighlightColor::Yellow)
        .await
        .unwrap();
    set_highlights(pool.write(), 45, 16, 25, 25, HighlightColor::Blue)
        .await
        .unwrap();

    let summary = |list: &[super::HighlightPassageInTranslation]| -> Vec<_> {
        list.iter()
            .map(|h| {
                let p = h.passage;
                (
                    p.book_id,
                    p.chapter,
                    p.verse_start,
                    p.verse_end,
                    p.color,
                    h.complete,
                )
            })
            .collect()
    };
    let in_kjv = list_highlight_passages(pool.read(), KJV).await.unwrap();
    assert_eq!(
        summary(&in_kjv),
        [
            (43, 3, 16, 16, HighlightColor::Green, true),
            (43, 3, 17, 17, HighlightColor::Pink, true),
            (43, 3, 18, 18, HighlightColor::Green, true),
            (45, 14, 23, 24, HighlightColor::Yellow, false),
            (45, 16, 25, 25, HighlightColor::Blue, true),
        ]
    );
    let texts: Vec<Option<&str>> = in_kjv
        .iter()
        .map(|h| h.first_verse_text.as_deref())
        .collect();
    assert_eq!(
        texts,
        [
            Some(bundled("kjv")[42].chapters[2][15].as_str()),
            Some(bundled("kjv")[42].chapters[2][16].as_str()),
            Some(bundled("kjv")[42].chapters[2][17].as_str()),
            Some(bundled("kjv")[44].chapters[13][22].as_str()),
            Some(bundled("kjv")[44].chapters[15][24].as_str()),
        ],
        "previews are the stored text, unaltered"
    );

    let in_web = list_highlight_passages(pool.read(), WEB).await.unwrap();
    assert!(in_web[3].complete, "the WEB numbers Romans 14:24");
    let kjv_only = &in_web[4];
    assert!(!kjv_only.complete, "the WEB has no Romans 16:25");
    assert_eq!(
        kjv_only.first_verse_text, None,
        "never another verse's text"
    );
}

fn john_3(verse_start: i64, verse_end: i64) -> Passage {
    Passage {
        book_id: 43,
        chapter: 3,
        verse_start,
        verse_end,
    }
}

/// (passage, body) of every note, in Bible order.
async fn note_rows(pool: &sqlx::SqlitePool) -> Vec<(i64, i64, i64, i64, String)> {
    list_notes(pool)
        .await
        .unwrap()
        .into_iter()
        .map(|n| (n.book_id, n.chapter, n.verse_start, n.verse_end, n.body))
        .collect()
}

#[tokio::test]
async fn notes_are_added_edited_listed_and_deleted() {
    let (_db, pool) = fresh_database("notes").await;
    assert_eq!(
        notes_overlapping(pool.read(), &john_3(1, 36))
            .await
            .unwrap(),
        []
    );

    let first = add_note(
        pool.write(),
        &john_3(16, 16),
        "  God so loved\r\nthe world. \n",
        None,
    )
    .await
    .unwrap();
    assert_eq!(first.body, "God so loved\nthe world.");
    assert_eq!(first.created_at, first.updated_at);
    // Any number of notes on one passage, even with the same text.
    let second = add_note(
        pool.write(),
        &john_3(16, 16),
        "God so loved\nthe world.",
        None,
    )
    .await
    .unwrap();
    assert_ne!(first.id, second.id);
    let range = add_note(pool.write(), &john_3(16, 18), "Belief and judgment.", None)
        .await
        .unwrap();
    let later = add_note(pool.write(), &john_3(21, 21), "Come to the light.", None)
        .await
        .unwrap();
    // Luke 17:36 is numbered only in the KJV; it can still have a note.
    // (Until 2.29.2 the KJV's Matthew 2:23, numbered only in the WEB, was
    // the example.)
    let kjv_only = Passage {
        book_id: 42,
        chapter: 17,
        verse_start: 36,
        verse_end: 36,
    };
    add_note(pool.write(), &kjv_only, "Two in the field.", None)
        .await
        .unwrap();

    let ids = |notes: Vec<super::Note>| notes.into_iter().map(|n| n.id).collect::<Vec<_>>();
    assert_eq!(
        ids(notes_overlapping(pool.read(), &john_3(16, 21))
            .await
            .unwrap()),
        [first.id, second.id, range.id, later.id]
    );
    assert_eq!(
        ids(notes_overlapping(pool.read(), &john_3(17, 17))
            .await
            .unwrap()),
        [range.id]
    );
    assert_eq!(
        ids(notes_overlapping(pool.read(), &john_3(19, 20))
            .await
            .unwrap()),
        Vec::<i64>::new()
    );
    let everything = note_rows(pool.read()).await;
    assert_eq!(everything[0].0, 42, "Bible order: Luke before John");
    assert_eq!(everything.len(), 5);

    // Editing changes the text and updated_at only.
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    let edited = update_note(pool.write(), range.id, "Belief, judgment, light.")
        .await
        .unwrap();
    assert_eq!(edited.body, "Belief, judgment, light.");
    assert_eq!(
        (
            edited.created_at.as_str(),
            edited.verse_start,
            edited.verse_end
        ),
        (range.created_at.as_str(), 16, 18)
    );
    assert!(edited.updated_at > range.updated_at);
    // Saving the same text again leaves it as it was.
    assert_eq!(
        update_note(pool.write(), range.id, " Belief, judgment, light. ")
            .await
            .unwrap(),
        edited
    );

    let everything = note_rows(pool.read()).await;

    // Deleting returns the note as it was; adding it back with its dates
    // restores the same passage, text and created_at.
    let deleted = delete_note(pool.write(), edited.id).await.unwrap().unwrap();
    assert_eq!(deleted, edited);
    assert_eq!(delete_note(pool.write(), edited.id).await.unwrap(), None);
    assert!(matches!(
        update_note(pool.write(), edited.id, "gone").await,
        Err(NoteError::NotFound { .. })
    ));
    let restored = add_note(
        pool.write(),
        &deleted.passage(),
        &deleted.body,
        Some((&deleted.created_at, &deleted.updated_at)),
    )
    .await
    .unwrap();
    assert_eq!(
        (
            restored.passage(),
            &restored.body,
            &restored.created_at,
            &restored.updated_at
        ),
        (
            deleted.passage(),
            &deleted.body,
            &deleted.created_at,
            &deleted.updated_at
        )
    );
    assert_eq!(note_rows(pool.read()).await, everything);

    // Notes survive a restart.
    pool.close().await;
    let pool = _db.connect().await;
    prepare(pool.write()).await.unwrap();
    assert_eq!(note_rows(pool.read()).await, everything);
}

#[tokio::test]
async fn invalid_notes_are_refused_and_change_nothing() {
    let (_db, pool) = fresh_database("notes_invalid").await;
    add_note(pool.write(), &john_3(16, 16), "Kept.", None)
        .await
        .unwrap();
    let before = note_rows(pool.read()).await;

    for (passage, body, expected) in [
        (john_3(16, 16), "   \n ", "A note can't be empty."),
        (
            john_3(16, 16),
            "ring\u{7}",
            "A note can only hold plain text",
        ),
        (
            john_3(16, 16),
            &"x".repeat(MAX_NOTE_CHARS + 1),
            "A note can hold up to 10,000 characters; this one has 10,001.",
        ),
        (john_3(18, 16), "backwards", "invalid verse range 18-16"),
        (john_3(0, 1), "verse zero", "invalid verse range 0-1"),
        (
            john_3(36, 37),
            "past the end",
            "has no verse 37 (last is 36)",
        ),
        (
            Passage {
                book_id: 43,
                chapter: 22,
                verse_start: 1,
                verse_end: 1,
            },
            "no chapter",
            "book 43 has no chapter 22",
        ),
        (
            Passage {
                book_id: 67,
                chapter: 1,
                verse_start: 1,
                verse_end: 1,
            },
            "no book",
            "book 67 has no chapter 1",
        ),
    ] {
        let error = add_note(pool.write(), &passage, body, None)
            .await
            .unwrap_err();
        assert!(
            error.to_string().contains(expected),
            "{passage:?} {body:?}: {error}"
        );
    }
    assert!(matches!(
        add_note(pool.write(), &john_3(16, 16), "", None).await,
        Err(NoteError::Body(NoteBodyError::Empty))
    ));
    assert!(matches!(
        add_note(pool.write(), &john_3(16, 40), "x", None).await,
        Err(NoteError::Passage(BookmarkError::NoSuchVerse { .. }))
    ));
    let id = list_notes(pool.read()).await.unwrap()[0].id;
    assert!(matches!(
        update_note(pool.write(), id, " ").await,
        Err(NoteError::Body(NoteBodyError::Empty))
    ));
    assert_eq!(note_rows(pool.read()).await, before);

    // The longest note fits, counted in characters, not bytes.
    let longest = "\u{e9}".repeat(MAX_NOTE_CHARS);
    add_note(pool.write(), &john_3(1, 1), &longest, None)
        .await
        .unwrap();

    // The table enforces the same rules if anything gets past them.
    for (body, verse_start, verse_end) in [
        (String::new(), 1, 1),
        ("x".repeat(MAX_NOTE_CHARS + 1), 1, 1),
        ("x".to_string(), 2, 1),
        ("x".to_string(), 0, 0),
    ] {
        let inserted = sqlx::query(
            "INSERT INTO notes (book_id, chapter, verse_start, verse_end, body) \
             VALUES (43, 3, $1, $2, $3)",
        )
        .bind(verse_start)
        .bind(verse_end)
        .bind(&body)
        .execute(pool.write().pool())
        .await;
        assert!(
            inserted.is_err(),
            "{verse_start}-{verse_end} {}",
            body.len()
        );
    }
}

/// Every row of the reader's bookmarks and highlights, as text.
async fn reader_rows(pool: &sqlx::SqlitePool) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT 'bookmark: ' || book_id || '|' || chapter || '|' || verse_start || '|' || \
             verse_end || '|' || created_at FROM bookmarks \
         UNION ALL \
         SELECT 'highlight: ' || book_id || '|' || chapter || '|' || verse || '|' || color || \
             '|' || created_at || '|' || updated_at FROM highlights \
         ORDER BY 1",
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn the_notes_table_is_added_to_an_existing_database() {
    // A database as v2.7.0 left it: the current schema without notes, with
    // a reader's bookmarks, highlights and position in it.
    let (db, pool) = fresh_database("add_notes").await;
    add_bookmark(pool.write(), 43, 3, 16, 18).await.unwrap();
    add_bookmark(pool.write(), 19, 23, 1, 1).await.unwrap();
    set_highlights(pool.write(), 43, 3, 16, 17, HighlightColor::Yellow)
        .await
        .unwrap();
    set_highlights(pool.write(), 1, 1, 1, 1, HighlightColor::Pink)
        .await
        .unwrap();
    super::save_reading_position(pool.write(), 2, 45, 8)
        .await
        .unwrap();
    sqlx::query("DROP TABLE notes")
        .execute(pool.write().pool())
        .await
        .unwrap();
    pool.close().await;

    let pool = db.connect().await;
    let others_before = other_tables_snapshot(pool.read()).await;
    let readers_before = reader_rows(pool.read()).await;
    let verses_before = verses_snapshot(pool.read()).await;
    assert_eq!(readers_before.len(), 2 + 3);
    assert!(!table_names(pool.read()).await.iter().any(|t| t == "notes"));

    // Exactly what startup does.
    prepare(pool.write()).await.unwrap();
    assert!(table_names(pool.read()).await.iter().any(|t| t == "notes"));
    let index: Option<String> = sqlx::query_scalar(
        "SELECT sql FROM sqlite_master WHERE type = 'index' AND name = 'idx_notes_chapter'",
    )
    .fetch_optional(pool.read())
    .await
    .unwrap();
    assert!(
        index.is_some_and(|sql| sql.contains("notes (book_id, chapter)")),
        "notes are indexed by chapter"
    );
    assert_eq!(other_tables_snapshot(pool.read()).await, others_before);
    assert_eq!(reader_rows(pool.read()).await, readers_before);
    assert_eq!(verses_snapshot(pool.read()).await, verses_before);
    add_note(pool.write(), &john_3(16, 16), "A first note.", None)
        .await
        .unwrap();

    // Running it again (the next launch) keeps everything, the note too.
    pool.close().await;
    let pool = db.connect().await;
    prepare(pool.write()).await.unwrap();
    assert_eq!(reader_rows(pool.read()).await, readers_before);
    assert_eq!(
        note_rows(pool.read()).await,
        vec![(43, 3, 16, 16, "A first note.".to_string())]
    );
}

/// (count, total characters) of the notes, as the limits count them.
async fn note_totals(pool: &sqlx::SqlitePool) -> (i64, i64) {
    sqlx::query_as("SELECT COUNT(*), COALESCE(SUM(length(body)), 0) FROM notes")
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn no_note_is_added_past_the_count_limit() {
    let (_db, pool) = fresh_database("notes_count_limit").await;
    fill_notes(pool.write(), MAX_NOTES - 1, 10).await;
    // The last one fits…
    let last = add_note(pool.write(), &john_3(16, 16), "The last.", None)
        .await
        .unwrap();
    assert_eq!(note_totals(pool.read()).await.0, MAX_NOTES as i64);
    // …and then nothing more, not even a restored one.
    for dates in [
        None,
        Some((last.created_at.as_str(), last.updated_at.as_str())),
    ] {
        let error = add_note(pool.write(), &john_3(17, 17), "One too many.", dates)
            .await
            .unwrap_err();
        assert!(
            matches!(error, NoteError::Limit(NoteLimit::Count)),
            "{error:?}"
        );
        assert_eq!(
            error.to_string(),
            "You've reached the limit of 10,000 notes."
        );
    }
    assert_eq!(note_totals(pool.read()).await.0, MAX_NOTES as i64);
    // Room again once one goes.
    delete_note(pool.write(), last.id).await.unwrap();
    add_note(pool.write(), &john_3(17, 17), "Fits again.", None)
        .await
        .unwrap();
}

#[tokio::test]
async fn notes_stay_within_their_total_size() {
    let (_db, pool) = fresh_database("notes_text_limit").await;
    let full = MAX_TOTAL_NOTE_CHARS / MAX_NOTE_CHARS;
    fill_notes(pool.write(), full, MAX_NOTE_CHARS).await;
    assert_eq!(
        note_totals(pool.read()).await,
        (full as i64, MAX_TOTAL_NOTE_CHARS as i64)
    );
    let error = add_note(pool.write(), &john_3(16, 16), "x", None)
        .await
        .unwrap_err();
    assert!(
        matches!(error, NoteError::Limit(NoteLimit::TotalText)),
        "{error:?}"
    );
    assert_eq!(
        error.to_string(),
        "Your notes have reached the total size limit (25,000,000 characters)."
    );

    // Shortening is always allowed, even at the limit; the room it makes
    // can be used again.
    let first = list_notes(pool.read()).await.unwrap()[0].clone();
    update_note(pool.write(), first.id, &"s".repeat(9_000))
        .await
        .unwrap();
    let used = add_note(pool.write(), &john_3(16, 16), &"y".repeat(1_000), None)
        .await
        .unwrap();
    assert_eq!(
        note_totals(pool.read()).await.1,
        MAX_TOTAL_NOTE_CHARS as i64
    );
    // Growing a note past the total isn't, and changes nothing.
    let before = note_rows(pool.read()).await;
    let error = update_note(pool.write(), first.id, &"s".repeat(9_001))
        .await
        .unwrap_err();
    assert!(
        matches!(error, NoteError::Limit(NoteLimit::TotalText)),
        "{error:?}"
    );
    assert_eq!(note_rows(pool.read()).await, before);
    // The same length, or shorter, is fine.
    update_note(pool.write(), first.id, &"t".repeat(9_000))
        .await
        .unwrap();
    update_note(pool.write(), used.id, "z").await.unwrap();
    assert_eq!(
        note_totals(pool.read()).await.1,
        (MAX_TOTAL_NOTE_CHARS - 999) as i64
    );
}

/// What 2.19.1 runs to save the chapter on screen (its
/// `save_reading_position`, word for word): it knows nothing of
/// `reading_place`.
async fn save_as_2_19_1(pool: &Db, translation_id: i64, book_id: i64, chapter: i64) {
    sqlx::query(
        "INSERT INTO reading_position (id, translation_id, book_id, chapter) \
         VALUES (1, $1, $2, $3) \
         ON CONFLICT(id) DO UPDATE SET \
             translation_id = excluded.translation_id, \
             book_id = excluded.book_id, \
             chapter = excluded.chapter",
    )
    .bind(translation_id)
    .bind(book_id)
    .bind(chapter)
    .execute(pool.write().pool())
    .await
    .unwrap();
}

fn place(translation_id: i64, book_id: i64, chapter: i64, at: Option<TopVerse>) -> ReadingPlace {
    ReadingPlace {
        translation_id,
        book_id,
        chapter,
        at,
    }
}

/// The verse at the top of the reading pane is saved with its chapter and
/// read back with it; a chapter saved at its top forgets it, and saving
/// that chapter again doesn't bring it back.
#[tokio::test]
async fn the_reading_place_is_saved_and_read_with_its_chapter() {
    let (_db, pool) = fresh_database("reading_place").await;
    assert_eq!(get_reading_place(pool.read()).await.unwrap(), None);

    let at = TopVerse {
        verse: 16,
        offset_px: Some(-12.5),
    };
    save_reading_position_at(pool.write(), WEB, 43, 3, Some(at))
        .await
        .unwrap();
    assert_eq!(
        get_reading_place(pool.read()).await.unwrap(),
        Some(place(WEB, 43, 3, Some(at)))
    );
    // Without an offset: the verse at the bar.
    let at_the_bar = TopVerse {
        verse: 17,
        offset_px: None,
    };
    save_reading_position_at(pool.write(), WEB, 43, 3, Some(at_the_bar))
        .await
        .unwrap();
    assert_eq!(
        get_reading_place(pool.read()).await.unwrap(),
        Some(place(WEB, 43, 3, Some(at_the_bar)))
    );

    save_reading_position(pool.write(), WEB, 43, 4)
        .await
        .unwrap();
    assert_eq!(
        get_reading_place(pool.read()).await.unwrap(),
        Some(place(WEB, 43, 4, None))
    );
    save_reading_position(pool.write(), WEB, 43, 3)
        .await
        .unwrap();
    assert_eq!(
        get_reading_place(pool.read()).await.unwrap(),
        Some(place(WEB, 43, 3, None))
    );
    // The position alone reads as it always has.
    assert_eq!(
        super::get_reading_position(pool.read()).await.unwrap(),
        Some((WEB, 43, 3))
    );
}

/// A place is used only for the translation, book and chapter it was saved
/// with: once an older app has saved another chapter or translation over
/// the position, the chapter opens at its top.
#[tokio::test]
async fn a_place_for_another_chapter_or_translation_is_never_used() {
    let (_db, pool) = fresh_database("reading_place_guard").await;
    let at = TopVerse {
        verse: 16,
        offset_px: Some(-3.0),
    };
    for (translation_id, book_id, chapter) in [(KJV, 43, 3), (WEB, 43, 4), (WEB, 44, 3)] {
        save_reading_position_at(pool.write(), WEB, 43, 3, Some(at))
            .await
            .unwrap();
        save_as_2_19_1(&pool, translation_id, book_id, chapter).await;
        assert_eq!(
            get_reading_place(pool.read()).await.unwrap(),
            Some(place(translation_id, book_id, chapter, None)),
            "{translation_id} {book_id}:{chapter}"
        );
    }
}

/// 2.19.1 opening a database this version saved a place in: its schema is
/// there as it knows it (the new table is one more, and the version stays
/// 1, so its `migrate` changes nothing), it reads and saves the position as
/// it always has, and this version then opens its chapter at the top.
#[tokio::test]
async fn an_older_app_opens_and_saves_a_database_with_a_place_in_it() {
    let (db, pool) = fresh_database("reading_place_2_19_1").await;
    save_reading_position_at(
        pool.write(),
        WEB,
        43,
        3,
        Some(TopVerse {
            verse: 16,
            offset_px: Some(-12.5),
        }),
    )
    .await
    .unwrap();
    pool.close().await;

    // 2.19.1: its `migrate` (every statement is `CREATE ... IF NOT EXISTS`,
    // and it writes the version only when it's below 1), then its read and
    // its save.
    let pool = db.connect().await;
    assert_eq!(user_version(pool.read()).await, 1);
    assert_eq!(SCHEMA_VERSION, 1);
    let read: Option<(i64, i64, i64)> = sqlx::query_as(
        "SELECT translation_id, book_id, chapter FROM reading_position WHERE id = 1",
    )
    .fetch_optional(pool.read())
    .await
    .unwrap();
    assert_eq!(read, Some((WEB, 43, 3)));
    save_as_2_19_1(&pool, WEB, 45, 8).await;
    pool.close().await;

    // This version again: Romans 8, from its top.
    let pool = db.connect().await;
    prepare(pool.write()).await.unwrap();
    assert_eq!(user_version(pool.read()).await, SCHEMA_VERSION);
    assert_eq!(
        get_reading_place(pool.read()).await.unwrap(),
        Some(place(WEB, 45, 8, None))
    );
}

/// A chapter's length is read with one seek into the verses' index per
/// translation, never a walk through every translation's verses (the index
/// starts with the translation), and it's still the highest verse any
/// translation has there.
#[tokio::test]
async fn a_chapter_s_last_verse_is_one_index_seek_per_translation() {
    use super::queries::{CHAPTER_LAST_VERSE_SQL, chapter_last_verse};
    let (_db, pool) = prepared_database("chapter_last_verse").await;
    let plan: Vec<(i64, i64, i64, String)> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "EXPLAIN QUERY PLAN {CHAPTER_LAST_VERSE_SQL}"
    )))
    .bind(43_i64)
    .bind(3_i64)
    .fetch_all(pool.read())
    .await
    .expect("explain the chapter's last verse");
    let details: Vec<&str> = plan.iter().map(|(_, _, _, d)| d.as_str()).collect();
    assert!(
        details
            .iter()
            .any(|d| d.starts_with("SEARCH v USING COVERING INDEX")
                && d.ends_with("(translation_id=? AND book_id=? AND chapter=?)")),
        "{details:#?}"
    );
    assert!(
        !details
            .iter()
            .any(|d| d.starts_with("SCAN v") || d.contains("SCAN verses")),
        "{details:#?}"
    );

    // The same answer a walk through every verse gives, here and on the
    // writing connection, and 0 for a chapter no translation has.
    for (book_id, chapter) in [(1, 1), (19, 119), (43, 3), (65, 1), (66, 22), (65, 2)] {
        let walked: Option<i64> =
            sqlx::query_scalar("SELECT MAX(verse) FROM verses WHERE book_id = $1 AND chapter = $2")
                .bind(book_id)
                .bind(chapter)
                .fetch_one(pool.read())
                .await
                .expect("walk the chapter");
        let walked = walked.unwrap_or(0);
        let read = chapter_last_verse(pool.read(), book_id, chapter)
            .await
            .expect("read the chapter's last verse");
        let mut conn = pool.write().pool().acquire().await.expect("the writer");
        let written = chapter_last_verse(&mut *conn, book_id, chapter)
            .await
            .expect("read it on the writer");
        assert_eq!((read, written), (walked, walked), "{book_id} {chapter}");
        assert_eq!(
            read == 0,
            (book_id, chapter) == (65, 2),
            "{book_id} {chapter}"
        );
    }
}

/// Every note's place is its book, chapter and date, as the whole note
/// has them, in book and chapter order, read without its text.
#[tokio::test]
async fn note_places_are_each_notes_book_chapter_and_date() {
    use super::notes::{NotePlace, note_places};
    let (_db, pool) = prepared_database("note_places").await;
    assert_eq!(note_places(pool.read()).await.unwrap(), []);
    for (book_id, chapter, verse) in [(45, 8, 28), (43, 3, 16), (1, 1, 1), (43, 1, 1), (43, 3, 5)] {
        let passage = Passage {
            book_id,
            chapter,
            verse_start: verse,
            verse_end: verse,
        };
        add_note(
            pool.write(),
            &passage,
            &format!("{book_id} {chapter}:{verse}"),
            None,
        )
        .await
        .unwrap();
    }
    let mut whole: Vec<(i64, i64, i64, String)> = list_notes(pool.read())
        .await
        .unwrap()
        .into_iter()
        .map(|n| (n.book_id, n.chapter, n.id, n.created_at))
        .collect();
    whole.sort();
    let expected: Vec<NotePlace> = whole
        .into_iter()
        .map(|(book_id, chapter, _, created_at)| NotePlace {
            book_id,
            chapter,
            created_at,
        })
        .collect();
    assert_eq!(note_places(pool.read()).await.unwrap(), expected);
}
