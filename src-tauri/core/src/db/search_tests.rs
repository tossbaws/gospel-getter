//! The search index built by more than one copy of the app at once (two
//! launches, or the app and an older version): it's only ever marked
//! indexed when it holds every verse, and a build stopped part-way takes
//! nothing away. Each "copy" is a connection of its own to one database
//! file, as a second process would have. The interleavings are made step by
//! step ([`TranslationBuild`]), with no timing involved.

use sqlx::SqlitePool;

use super::search::{
    Published, SEARCH_CONTENT_VERSION, TranslationBuild, search_index_is_current,
    search_verses_in_book,
};
use super::test_support::fresh_database;
use super::{Db, ensure_search_index};
use crate::domain::query::SearchQuery;

/// The WEB's id: the translation these tests index.
async fn web(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT id FROM translations WHERE code = 'web'")
        .fetch_one(pool)
        .await
        .expect("the WEB")
}

/// The WEB's index and its record cleared, as on the first launch after
/// it's seeded: what a build starts from.
async fn clear_index(db: &Db, translation_id: i64) {
    let lo = translation_id * 100_000_000;
    sqlx::query("DELETE FROM verse_search WHERE rowid >= $1 AND rowid < $2")
        .bind(lo)
        .bind(lo + 100_000_000)
        .execute(db.write().pool())
        .await
        .expect("clear the index");
    sqlx::query("DELETE FROM search_index_state WHERE translation_id = $1")
        .bind(translation_id)
        .execute(db.write().pool())
        .await
        .expect("clear its record");
}

/// What 2.28.0 and older did first when indexing a translation: remove its
/// record and every row it had, in one transaction.
async fn older_versions_reset(db: &Db, translation_id: i64) {
    clear_index(db, translation_id).await;
}

/// The verses of `translation_id` (book, chapter, verse) the index has no
/// row for, and how many rows it has besides: asked here, independently of
/// the build's own check.
async fn unindexed(pool: &SqlitePool, translation_id: i64) -> (Vec<(i64, i64, i64)>, i64) {
    let lo = translation_id * 100_000_000;
    let missing = sqlx::query_as(
        "SELECT book_id, chapter, verse FROM verses WHERE translation_id = $1 \
         AND $2 + book_id * 1000000 + chapter * 1000 + verse NOT IN \
             (SELECT rowid FROM verse_search WHERE rowid >= $2 AND rowid < $3) \
         ORDER BY book_id, chapter, verse",
    )
    .bind(translation_id)
    .bind(lo)
    .bind(lo + 100_000_000)
    .fetch_all(pool)
    .await
    .expect("the unindexed verses");
    let extra: i64 = sqlx::query_scalar(
        "SELECT (SELECT COUNT(*) FROM verse_search WHERE rowid >= $2 AND rowid < $3) \
              - (SELECT COUNT(*) FROM verses WHERE translation_id = $1)",
    )
    .bind(translation_id)
    .bind(lo)
    .bind(lo + 100_000_000)
    .fetch_one(pool)
    .await
    .expect("the rows besides");
    (missing, extra)
}

/// Every verse of the translation is indexed, nothing else is, and a search
/// in each book for the first words of its first verse finds that verse.
async fn assert_every_verse_indexed(pool: &SqlitePool, translation_id: i64) {
    let (missing, extra) = unindexed(pool, translation_id).await;
    assert!(
        missing.is_empty(),
        "{} verses aren't indexed, from {:?}",
        missing.len(),
        missing.first()
    );
    assert_eq!(extra, 0, "rows besides the verses'");
    let firsts: Vec<(i64, String)> = sqlx::query_as(
        "SELECT book_id, text FROM verses WHERE translation_id = $1 AND chapter = 1 \
         AND verse = 1 ORDER BY book_id",
    )
    .bind(translation_id)
    .fetch_all(pool)
    .await
    .expect("each book's first verse");
    assert!(!firsts.is_empty());
    for (book_id, text) in firsts {
        let words: Vec<String> = text
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .take(5)
            .map(str::to_lowercase)
            .collect();
        let query = SearchQuery {
            words: Vec::new(),
            phrases: vec![words],
        };
        let (_, hits) = search_verses_in_book(pool, translation_id, book_id, &query, 200, 0)
            .await
            .expect("search");
        assert!(
            hits.iter().any(|h| (h.chapter, h.verse) == (1, 1)),
            "book {book_id}'s first verse isn't found by its first words"
        );
    }
}

/// An index an older version left marked indexed while a book is missing
/// (two of its builds overlapping could), is noticed at launch and indexed
/// again, not taken at its word.
#[tokio::test]
async fn an_index_marked_indexed_but_missing_a_book_is_indexed_again() {
    let (_db, pool) = fresh_database("index_marked_missing").await;
    let web = web(pool.read()).await;
    sqlx::query("DELETE FROM verse_search WHERE rowid >= $1 AND rowid < $2")
        .bind(web * 100_000_000 + 1_000_000)
        .bind(web * 100_000_000 + 2_000_000)
        .execute(pool.write().pool())
        .await
        .expect("lose Genesis");
    assert!(search_index_is_current(pool.read(), web).await.unwrap());

    assert_eq!(ensure_search_index(&pool).await.unwrap(), vec![web]);

    assert_every_verse_indexed(pool.read(), web).await;
    assert!(search_index_is_current(pool.read(), web).await.unwrap());
    // And the next launch has nothing to do.
    assert_eq!(ensure_search_index(&pool).await.unwrap(), Vec::<i64>::new());
}

/// Two copies of the app index the WEB at once. The first is part-way when
/// the second starts; the second gets part-way too, then stops (its app
/// quits). The first finishes: every verse is indexed, and it's marked so.
/// The second stopping after the first has finished takes nothing away
/// either.
#[tokio::test]
async fn two_builds_interleaved_one_stopped_part_way_leave_every_verse_indexed() {
    let (db, one) = fresh_database("index_two_builds").await;
    let two = db.connect().await;
    let web = web(one.read()).await;
    clear_index(&one, web).await;

    let first = TranslationBuild::plan(&one, web).await.unwrap();
    let books = first.books.clone();
    assert!(books.len() > 20);
    for &book in &books[..10] {
        first.index_book(&one, book).await.unwrap();
    }
    let second = TranslationBuild::plan(&two, web).await.unwrap();
    for &book in &books[..5] {
        second.index_book(&two, book).await.unwrap();
    }
    // The second app quits.
    drop(second);
    two.close().await;
    for &book in &books[10..] {
        first.index_book(&one, book).await.unwrap();
    }
    let published = first.publish(&one).await.unwrap();
    assert!(
        matches!(published, Published::Complete { .. }),
        "{published:?}"
    );
    assert_every_verse_indexed(one.read(), web).await;
    assert!(search_index_is_current(one.read(), web).await.unwrap());

    // A build that started before that one finished, and stops part-way
    // after it: the index is still whole, and still marked so.
    let two = db.connect().await;
    let late = TranslationBuild::plan(&two, web).await.unwrap();
    clear_index(&one, web).await;
    for &book in &books {
        first.index_book(&one, book).await.unwrap();
    }
    for &book in &books[..3] {
        late.index_book(&two, book).await.unwrap();
    }
    assert!(matches!(
        first.publish(&one).await.unwrap(),
        Published::Complete { .. }
    ));
    for &book in &books[3..7] {
        late.index_book(&two, book).await.unwrap();
    }
    drop(late);
    two.close().await;
    assert_every_verse_indexed(one.read(), web).await;
    assert!(search_index_is_current(one.read(), web).await.unwrap());
}

/// The review's interleaving, with the second copy an older version (whose
/// build starts by clearing the translation's rows): the first has indexed
/// Genesis, the older one clears it all and quits, the first indexes the
/// rest. It isn't marked indexed then; Genesis is indexed again, and then
/// it is.
#[tokio::test]
async fn a_build_never_marks_an_index_missing_verses_as_indexed() {
    let (db, one) = fresh_database("index_never_incomplete").await;
    let older = db.connect().await;
    let web = web(one.read()).await;
    clear_index(&one, web).await;

    let build = TranslationBuild::plan(&one, web).await.unwrap();
    let books = build.books.clone();
    build.index_book(&one, books[0]).await.unwrap();
    older_versions_reset(&older, web).await;
    older.close().await;
    for &book in &books[1..] {
        build.index_book(&one, book).await.unwrap();
    }

    match build.publish(&one).await.unwrap() {
        Published::Incomplete(coverage) => {
            assert_eq!(coverage.books_missing_verses, vec![books[0]]);
        }
        complete => panic!("published an index missing Genesis: {complete:?}"),
    }
    assert!(!search_index_is_current(one.read(), web).await.unwrap());
    let (missing, _) = unindexed(one.read(), web).await;
    assert!(missing.iter().all(|&(book, _, _)| book == books[0]));
    assert!(!missing.is_empty());

    build.index_book(&one, books[0]).await.unwrap();
    assert!(matches!(
        build.publish(&one).await.unwrap(),
        Published::Complete { .. }
    ));
    assert_every_verse_indexed(one.read(), web).await;
    assert!(search_index_is_current(one.read(), web).await.unwrap());
}

/// A newer version of the app takes the index over to rebuild it with its
/// own kind of content: this one's build stops writing at its next step,
/// and marks nothing. When this version is opened again, it rebuilds it as
/// its own.
#[tokio::test]
async fn a_build_stops_once_another_version_takes_the_index_over() {
    let (db, one) = fresh_database("index_taken_over").await;
    let newer = db.connect().await;
    let web = web(one.read()).await;
    clear_index(&one, web).await;

    let build = TranslationBuild::plan(&one, web).await.unwrap();
    let books = build.books.clone();
    build.index_book(&one, books[0]).await.unwrap();
    sqlx::query("UPDATE search_index_version SET version = $1")
        .bind(SEARCH_CONTENT_VERSION + 1)
        .execute(newer.write().pool())
        .await
        .unwrap();
    let before = unindexed(one.read(), web).await;

    let refused = build.index_book(&one, books[1]).await.unwrap_err();
    assert!(
        format!("{refused:#}").contains("another version of the app is building it"),
        "{refused:#}"
    );
    assert!(build.publish(&one).await.is_err());
    assert_eq!(unindexed(one.read(), web).await, before);
    let marked: Option<i64> =
        sqlx::query_scalar("SELECT verse_count FROM search_index_state WHERE translation_id = $1")
            .bind(web)
            .fetch_optional(one.read())
            .await
            .unwrap();
    assert_eq!(marked, None);
    newer.close().await;

    // This version again: it takes the index back and rebuilds all of it.
    let rebuilt = ensure_search_index(&one).await.unwrap();
    assert!(rebuilt.contains(&web), "{rebuilt:?}");
    assert_every_verse_indexed(one.read(), web).await;
    assert!(search_index_is_current(one.read(), web).await.unwrap());
}
