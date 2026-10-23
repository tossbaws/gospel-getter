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
    TempDb, bundled, bundled_bsb, fill_notes, fresh_database, other_tables_snapshot,
    prepared_database, table_names, verses_snapshot, write_v2_1_0_database,
};
use super::{
    BookmarkError, Db, HighlightColor, NoteBodyError, NoteError, ensure_search_index, migrate,
    prepare,
};

const KJV: i64 = 1;
const WEB: i64 = 2;
/// Seeded third (see `seed::SEED_ORDER`), on a new install or an upgrade.
const BSB: i64 = 3;

fn terms(words: &[&str]) -> Vec<String> {
    words.iter().map(|w| (*w).to_string()).collect()
}

/// `verses` must hold exactly the bundled text, byte for byte, numbered as
/// the bundled files number it: every translation's.
fn assert_verses_match_bundled(snapshot: &[(i64, i64, i64, i64, Vec<u8>)]) {
    assert_verses_match(snapshot, &[(KJV, "kjv"), (WEB, "web"), (BSB, "bsb")]);
}

/// `verses` must hold exactly these translations' bundled text, byte for
/// byte, and nothing else. A verse a translation doesn't number (`null` in
/// its file) has no row.
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
                    let Some(text) = text else { continue };
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
        "cross_references",
        "bookmarks",
        "highlights",
        "notes",
        "verse_search",
        "search_index_state",
        "search_index_version",
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
    assert_eq!(state, vec![(KJV, 31_100), (WEB, 31_103), (BSB, 31_086)]);

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

    // The KJV and WEB rows are untouched; the BSB, which 2.1.0 didn't
    // have, is added after them, exactly as bundled.
    assert_verses_match(&verses_before, &[(KJV, "kjv"), (WEB, "web")]);
    let verses_after = verses_snapshot(pool.read()).await;
    assert_eq!(verses_after[..verses_before.len()], verses_before[..]);
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

    // Matthew 2:23 is numbered in the WEB (23 verses) but not the KJV (22),
    // and 3 John 1:15 the other way round. Both can be bookmarked.
    let matthew = add_bookmark(pool.write(), 40, 2, 22, 23).await.unwrap();
    let third_john = add_bookmark(pool.write(), 64, 1, 15, 15).await.unwrap();
    assert!(matches!(
        add_bookmark(pool.write(), 40, 2, 24, 24).await,
        Err(BookmarkError::NoSuchVerse { last_verse: 23, .. })
    ));

    let in_kjv = list_bookmarks(pool.read(), KJV).await.unwrap();
    let in_web = list_bookmarks(pool.read(), WEB).await.unwrap();
    let find = |list: &[super::BookmarkInTranslation], id: i64| {
        list.iter()
            .find(|b| b.bookmark.id == id)
            .cloned()
            .expect("bookmark listed")
    };

    let m_kjv = find(&in_kjv, matthew.id);
    assert!(!m_kjv.complete, "KJV has no Matthew 2:23");
    assert_eq!(
        m_kjv.first_verse_text.as_deref(),
        Some(bundled("kjv")[39].chapters[1][21].as_str()),
        "KJV preview is its own 2:22, unaltered"
    );
    let m_web = find(&in_web, matthew.id);
    assert!(m_web.complete);
    assert_eq!(
        m_web.first_verse_text.as_deref(),
        Some(bundled("web")[39].chapters[1][21].as_str())
    );

    let j_kjv = find(&in_kjv, third_john.id);
    assert!(j_kjv.complete);
    let j_web = find(&in_web, third_john.id);
    assert!(!j_web.complete, "WEB has no 3 John 1:15");
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

    // John 3:16-18 green with 3:17 recolored pink; Matthew 2:22-23 (2:23
    // is numbered in the WEB only); 3 John 1:15 (the KJV only).
    set_highlights(pool.write(), 43, 3, 16, 18, HighlightColor::Green)
        .await
        .unwrap();
    set_highlights(pool.write(), 43, 3, 17, 17, HighlightColor::Pink)
        .await
        .unwrap();
    set_highlights(pool.write(), 40, 2, 22, 23, HighlightColor::Yellow)
        .await
        .unwrap();
    set_highlights(pool.write(), 64, 1, 15, 15, HighlightColor::Blue)
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
            (40, 2, 22, 23, HighlightColor::Yellow, false),
            (43, 3, 16, 16, HighlightColor::Green, true),
            (43, 3, 17, 17, HighlightColor::Pink, true),
            (43, 3, 18, 18, HighlightColor::Green, true),
            (64, 1, 15, 15, HighlightColor::Blue, true),
        ]
    );
    let texts: Vec<Option<&str>> = in_kjv
        .iter()
        .map(|h| h.first_verse_text.as_deref())
        .collect();
    assert_eq!(
        texts,
        [
            Some(bundled("kjv")[39].chapters[1][21].as_str()),
            Some(bundled("kjv")[42].chapters[2][15].as_str()),
            Some(bundled("kjv")[42].chapters[2][16].as_str()),
            Some(bundled("kjv")[42].chapters[2][17].as_str()),
            Some(bundled("kjv")[63].chapters[0][14].as_str()),
        ],
        "previews are the stored text, unaltered"
    );

    let in_web = list_highlight_passages(pool.read(), WEB).await.unwrap();
    assert!(in_web[0].complete, "the WEB numbers Matthew 2:23");
    let third_john = &in_web[4];
    assert!(!third_john.complete, "the WEB has no 3 John 1:15");
    assert_eq!(
        third_john.first_verse_text, None,
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
    // Matthew 2:23 is numbered only in the WEB; it can still have a note.
    let web_only = Passage {
        book_id: 40,
        chapter: 2,
        verse_start: 23,
        verse_end: 23,
    };
    add_note(pool.write(), &web_only, "Nazarene.", None)
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
    assert_eq!(everything[0].0, 40, "Bible order: Matthew before John");
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
