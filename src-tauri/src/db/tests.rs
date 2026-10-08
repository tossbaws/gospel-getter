//! Database tests against real files: fresh installs, upgrades from
//! v2.1.0, bookmarks, highlights and the search index. Each test gets its own
//! disposable database (see `test_support`).

use std::time::Instant;

use super::bookmarks::{add_bookmark, list_bookmarks, remove_bookmark};
use super::highlights::{list_highlights, remove_highlights, set_highlights};
use super::search::search_verses;
use super::test_support::{
    TempDb, bundled, fresh_database, other_tables_snapshot, table_names, verses_snapshot,
    write_v2_1_0_database,
};
use super::{BookmarkError, HighlightColor, ensure_search_index, prepare};

const KJV: i64 = 1;
const WEB: i64 = 2;

fn terms(words: &[&str]) -> Vec<String> {
    words.iter().map(|w| (*w).to_string()).collect()
}

/// `verses` must hold exactly the bundled text, byte for byte, numbered as
/// the bundled files number it.
fn assert_verses_match_bundled(snapshot: &[(i64, i64, i64, i64, Vec<u8>)]) {
    let mut expected = Vec::new();
    for (translation_id, code) in [(KJV, "kjv"), (WEB, "web")] {
        for (b, book) in bundled(code).into_iter().enumerate() {
            for (c, verses) in book.chapters.into_iter().enumerate() {
                for (v, text) in verses.into_iter().enumerate() {
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
    assert_eq!(snapshot.len(), expected.len(), "verse row count");
    assert!(snapshot == expected, "verses differ from the bundled text");
}

#[tokio::test]
async fn fresh_database_has_every_table_and_a_complete_search_index() {
    let (_db, pool) = fresh_database("fresh").await;

    let tables = table_names(&pool).await;
    for table in [
        "books",
        "translations",
        "verses",
        "reading_position",
        "cross_references",
        "bookmarks",
        "highlights",
        "verse_search",
        "search_index_state",
    ] {
        assert!(tables.iter().any(|t| t == table), "missing table {table}");
    }

    let state: Vec<(i64, i64)> =
        sqlx::query_as("SELECT translation_id, verse_count FROM search_index_state ORDER BY 1")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(state, vec![(KJV, 31_100), (WEB, 31_103)]);

    let bookmarks: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM bookmarks")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(bookmarks, 0);

    // A normal later launch finds nothing to (re)index.
    prepare(&pool).await.unwrap();
    assert_eq!(ensure_search_index(&pool).await.unwrap(), Vec::<i64>::new());

    assert_verses_match_bundled(&verses_snapshot(&pool).await);
}

#[tokio::test]
async fn upgrading_a_v2_1_0_database_keeps_its_data_and_adds_bookmarks_and_search() {
    let db = TempDb::new("upgrade");
    write_v2_1_0_database(&db.path()).await;

    let pool = db.connect().await;
    let verses_before = verses_snapshot(&pool).await;
    let others_before = other_tables_snapshot(&pool).await;
    assert!(
        !table_names(&pool).await.iter().any(|t| t == "bookmarks"),
        "fixture should be a pre-bookmarks database"
    );
    assert!(
        others_before
            .iter()
            .any(|r| r == "reading_position: 1|2|45|8"),
        "fixture should have a saved reading position: {others_before:?}"
    );

    // Exactly what startup does.
    prepare(&pool).await.unwrap();
    assert_eq!(ensure_search_index(&pool).await.unwrap(), vec![KJV, WEB]);

    assert_eq!(verses_snapshot(&pool).await, verses_before);
    assert_eq!(other_tables_snapshot(&pool).await, others_before);
    assert_verses_match_bundled(&verses_before);
    let tables = table_names(&pool).await;
    for table in [
        "bookmarks",
        "highlights",
        "verse_search",
        "search_index_state",
    ] {
        assert!(
            tables.iter().any(|t| t == table),
            "upgrade should add {table}"
        );
    }

    // The new features work on the upgraded database.
    let (total, hits) = search_verses(&pool, WEB, &terms(&["shepherd"]), 5, 0)
        .await
        .unwrap();
    assert!(total > 0 && !hits.is_empty());
    add_bookmark(&pool, 43, 3, 16, 16).await.unwrap();

    // And upgrading again (the next launch) changes nothing.
    pool.close().await;
    let pool = db.connect().await;
    prepare(&pool).await.unwrap();
    assert_eq!(ensure_search_index(&pool).await.unwrap(), Vec::<i64>::new());
    assert_eq!(verses_snapshot(&pool).await, verses_before);
    assert_eq!(other_tables_snapshot(&pool).await, others_before);
    assert_eq!(list_bookmarks(&pool, KJV).await.unwrap().len(), 1);
}

#[tokio::test]
async fn indexing_leaves_every_verse_byte_for_byte_unchanged() {
    let (_db, pool) = fresh_database("integrity").await;
    let before = verses_snapshot(&pool).await;
    assert_verses_match_bundled(&before);

    // Force a full rebuild of both translations.
    sqlx::query("DELETE FROM search_index_state")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(ensure_search_index(&pool).await.unwrap(), vec![KJV, WEB]);

    assert_eq!(verses_snapshot(&pool).await, before);

    // Nothing can write to `verses` behind the index's back.
    let triggers: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE type = 'trigger'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(triggers, 0);
}

#[tokio::test]
async fn search_returns_stored_text_in_bible_order_per_translation() {
    let (_db, pool) = fresh_database("search").await;

    let (total, hits) = search_verses(&pool, KJV, &terms(&["faith", "hope", "charity"]), 50, 0)
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
    let (total, page) = search_verses(&pool, WEB, &terms(&["love"]), 50, 0)
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
    let (_, next) = search_verses(&pool, WEB, &terms(&["love"]), 50, 50)
        .await
        .unwrap();
    let next_first = next.first().map(|h| (h.book_id, h.chapter, h.verse));
    assert!(next_first > keys.last().copied());

    // Each translation only finds its own text.
    let (kjv_yahweh, _) = search_verses(&pool, KJV, &terms(&["yahweh"]), 1, 0)
        .await
        .unwrap();
    let (web_yahweh, _) = search_verses(&pool, WEB, &terms(&["yahweh"]), 1, 0)
        .await
        .unwrap();
    assert_eq!(kjv_yahweh, 0);
    assert!(web_yahweh > 1000);

    // Curly apostrophes split words the same way for the index and the query.
    let (_, keeper) = search_verses(&pool, WEB, &terms(&["brother", "s", "keeper"]), 5, 0)
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
        search_verses(&pool, KJV, &terms(syntax), 5, 0)
            .await
            .unwrap_or_else(|e| panic!("{syntax:?} should be searched literally: {e:#}"));
    }
}

#[tokio::test]
async fn a_reseeded_translation_is_reindexed_on_its_own() {
    let (_db, pool) = fresh_database("reseed").await;

    // As if WEB's rows were cleared and seeded again by a later launch.
    sqlx::query("DELETE FROM verses WHERE translation_id = 2")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM translations WHERE id = 2")
        .execute(&pool)
        .await
        .unwrap();
    prepare(&pool).await.unwrap();
    assert_eq!(ensure_search_index(&pool).await.unwrap(), vec![WEB]);
    let (total, _) = search_verses(&pool, WEB, &terms(&["yahweh"]), 1, 0)
        .await
        .unwrap();
    assert!(total > 1000);
    assert_verses_match_bundled(&verses_snapshot(&pool).await);
}

#[tokio::test]
async fn bookmarks_are_validated_deduplicated_and_survive_a_restart() {
    let db = TempDb::new("bookmarks");
    let pool = db.connect().await;
    prepare(&pool).await.unwrap();

    let john = add_bookmark(&pool, 43, 3, 16, 18).await.unwrap();
    let genesis = add_bookmark(&pool, 1, 1, 1, 1).await.unwrap();
    assert_eq!((john.verse_start, john.verse_end), (16, 18));

    // The same passage again is the same bookmark, not a second one.
    let again = add_bookmark(&pool, 43, 3, 16, 18).await.unwrap();
    assert_eq!(again, john);
    // An overlapping but different passage is its own bookmark.
    let single = add_bookmark(&pool, 43, 3, 16, 16).await.unwrap();
    assert_ne!(single.id, john.id);

    for (book, chapter, start, end) in [(43, 3, 0, 1), (43, 3, 5, 4), (43, 3, -2, -1)] {
        assert!(
            matches!(
                add_bookmark(&pool, book, chapter, start, end).await,
                Err(BookmarkError::InvalidRange { .. })
            ),
            "{book} {chapter}:{start}-{end}"
        );
    }
    for (book, chapter) in [(43, 22), (43, 0), (67, 1), (0, 1), (65, 2)] {
        assert!(
            matches!(
                add_bookmark(&pool, book, chapter, 1, 1).await,
                Err(BookmarkError::NoSuchChapter { .. })
            ),
            "{book} {chapter}"
        );
    }
    assert!(matches!(
        add_bookmark(&pool, 43, 3, 36, 37).await,
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
        .execute(&pool)
        .await
        .is_err()
    );

    // Newest first.
    let ids: Vec<i64> = list_bookmarks(&pool, KJV)
        .await
        .unwrap()
        .into_iter()
        .map(|b| b.bookmark.id)
        .collect();
    assert_eq!(ids, vec![single.id, genesis.id, john.id]);

    // Restart: close and reopen the file, running startup again.
    pool.close().await;
    let pool = db.connect().await;
    prepare(&pool).await.unwrap();
    let listed = list_bookmarks(&pool, WEB).await.unwrap();
    assert_eq!(listed.len(), 3);
    assert_eq!(listed[2].bookmark, john);
    assert_eq!(
        listed[2].first_verse_text.as_deref(),
        Some(bundled("web")[42].chapters[2][15].as_str())
    );

    // Removing.
    assert!(remove_bookmark(&pool, genesis.id).await.unwrap());
    assert!(!remove_bookmark(&pool, genesis.id).await.unwrap());
    let ids: Vec<i64> = list_bookmarks(&pool, KJV)
        .await
        .unwrap()
        .into_iter()
        .map(|b| b.bookmark.id)
        .collect();
    assert_eq!(ids, vec![single.id, john.id]);
}

#[tokio::test]
async fn bookmarks_follow_each_translations_own_verse_numbering() {
    let db = TempDb::new("numbering");
    let pool = db.connect().await;
    prepare(&pool).await.unwrap();

    // Matthew 2:23 is numbered in the WEB (23 verses) but not the KJV (22),
    // and 3 John 1:15 the other way round. Both can be bookmarked.
    let matthew = add_bookmark(&pool, 40, 2, 22, 23).await.unwrap();
    let third_john = add_bookmark(&pool, 64, 1, 15, 15).await.unwrap();
    assert!(matches!(
        add_bookmark(&pool, 40, 2, 24, 24).await,
        Err(BookmarkError::NoSuchVerse { last_verse: 23, .. })
    ));

    let in_kjv = list_bookmarks(&pool, KJV).await.unwrap();
    let in_web = list_bookmarks(&pool, WEB).await.unwrap();
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
    prepare(&fresh).await.unwrap();
    let started = Instant::now();
    ensure_search_index(&fresh).await.unwrap();
    println!("index build, both translations: {:?}", started.elapsed());

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
            let (total, hits) = search_verses(&pool, translation, &terms(query), 50, 0)
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
    let db = TempDb::new("highlights");
    let pool = db.connect().await;
    prepare(&pool).await.unwrap();

    // A range: every verse in it gets the color.
    set_highlights(&pool, 43, 3, 16, 18, Yellow).await.unwrap();
    assert_eq!(
        highlight_colors(&pool).await,
        vec![
            (43, 3, 16, Yellow),
            (43, 3, 17, Yellow),
            (43, 3, 18, Yellow)
        ]
    );

    // Recoloring part of it replaces those verses' color and their
    // updated_at, and leaves the rest alone.
    let before = list_highlights(&pool).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    set_highlights(&pool, 43, 3, 17, 18, Green).await.unwrap();
    let after = list_highlights(&pool).await.unwrap();
    assert_eq!(after[0], before[0]);
    assert_eq!((after[1].color, after[2].color), (Green, Green));
    assert_eq!(after[1].created_at, before[1].created_at);
    assert!(after[1].updated_at > before[1].updated_at);

    // Setting the color a verse already has changes nothing at all.
    set_highlights(&pool, 43, 3, 17, 17, Green).await.unwrap();
    assert_eq!(list_highlights(&pool).await.unwrap(), after);

    // Every color, and a verse only one translation numbers (Matthew 2:23
    // is WEB-only, like bookmarks).
    set_highlights(&pool, 19, 23, 1, 1, Blue).await.unwrap();
    set_highlights(&pool, 40, 2, 23, 23, Pink).await.unwrap();

    // Removing a range removes only highlighted verses in it.
    assert_eq!(remove_highlights(&pool, 43, 3, 15, 17).await.unwrap(), 2);
    assert_eq!(remove_highlights(&pool, 43, 3, 15, 17).await.unwrap(), 0);
    assert_eq!(
        highlight_colors(&pool).await,
        vec![(19, 23, 1, Blue), (40, 2, 23, Pink), (43, 3, 18, Green)]
    );

    // Invalid passages are refused, and change nothing.
    let kept = highlight_colors(&pool).await;
    for (book, chapter, start, end) in [
        (43, 3, 0, 1),
        (43, 3, 5, 4),
        (43, 22, 1, 1),
        (99, 1, 1, 1),
        (43, 3, 36, 37),
    ] {
        let error = set_highlights(&pool, book, chapter, start, end, Yellow)
            .await
            .unwrap_err();
        assert!(
            !matches!(error, BookmarkError::Database(_)),
            "{book} {chapter}:{start}-{end}: {error}"
        );
    }
    assert!(matches!(
        set_highlights(&pool, 43, 3, 36, 37, Yellow).await,
        Err(BookmarkError::NoSuchVerse { last_verse: 36, .. })
    ));
    assert_eq!(highlight_colors(&pool).await, kept);

    // The table itself refuses a color that isn't one of the four.
    let bad = sqlx::query(
        "INSERT INTO highlights (book_id, chapter, verse, color) VALUES (1, 1, 1, 'purple')",
    )
    .execute(&pool)
    .await;
    assert!(bad.is_err(), "the CHECK should refuse purple");

    // And highlights survive a restart.
    pool.close().await;
    let pool = db.connect().await;
    prepare(&pool).await.unwrap();
    assert_eq!(highlight_colors(&pool).await, kept);
}

#[tokio::test]
async fn the_highlights_table_is_added_to_an_existing_database() {
    // A database from before highlights: everything else, with a reader's
    // bookmark and position, but no highlights table.
    let (db, pool) = fresh_database("add_highlights").await;
    add_bookmark(&pool, 43, 3, 16, 16).await.unwrap();
    super::save_reading_position(&pool, 2, 45, 8).await.unwrap();
    sqlx::query("DROP TABLE highlights")
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;

    let pool = db.connect().await;
    let others_before = other_tables_snapshot(&pool).await;
    assert!(!table_names(&pool).await.iter().any(|t| t == "highlights"));

    // Exactly what startup does.
    prepare(&pool).await.unwrap();
    assert!(table_names(&pool).await.iter().any(|t| t == "highlights"));
    assert_eq!(other_tables_snapshot(&pool).await, others_before);
    assert_eq!(list_bookmarks(&pool, KJV).await.unwrap().len(), 1);
    set_highlights(&pool, 43, 3, 16, 16, HighlightColor::Yellow)
        .await
        .unwrap();

    // Running it again (the next launch) keeps the highlight.
    prepare(&pool).await.unwrap();
    assert_eq!(
        highlight_colors(&pool).await,
        vec![(43, 3, 16, HighlightColor::Yellow)]
    );
}
