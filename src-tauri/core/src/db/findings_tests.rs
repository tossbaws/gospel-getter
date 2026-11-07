//! AI-flagged findings against real files: the table's rules, the reader's
//! switch, "Keep as mine" and the one narrow writer a program beside the app
//! has. Each test gets its own disposable database (see `test_support`).

use std::time::Duration;

use super::findings::{
    DEFAULT_AGENT, FindingsWriter, KeepError, NewFinding, add_findings, findings_enabled,
    keep_as_mine, list_findings, list_requests, remove_finding, remove_request,
    set_findings_enabled,
};
use super::notes::{MAX_NOTES, NoteLimit, Passage, add_note, list_notes};
use super::test_support::{TempDb, fill_notes, fresh_database};
use super::{
    Db, FindingsError, HighlightColor, NoteBodyError, NoteError, begin_write, bookmarks, highlights,
};

const JOHN: i64 = 43;

fn verse(chapter: i64, verse: i64) -> Passage {
    Passage {
        book_id: JOHN,
        chapter,
        verse_start: verse,
        verse_end: verse,
    }
}

fn new(passage: Passage, request: &str) -> NewFinding {
    NewFinding {
        passage,
        agent: "Claude".to_string(),
        request: request.to_string(),
        created_at: None,
    }
}

/// Every row of the reader's own tables, as text: what no finding may touch.
async fn readers_own(pool: &Db) -> Vec<String> {
    let mut rows = Vec::new();
    for sql in [
        "SELECT 'note ' || id || '|' || book_id || '|' || chapter || '|' || verse_start || '|' \
         || verse_end || '|' || body || '|' || created_at || '|' || updated_at FROM notes \
         ORDER BY id",
        "SELECT 'bookmark ' || id || '|' || book_id || '|' || chapter || '|' || verse_start \
         || '|' || verse_end || '|' || created_at FROM bookmarks ORDER BY id",
        "SELECT 'highlight ' || book_id || '|' || chapter || '|' || verse || '|' || color \
         || '|' || created_at || '|' || updated_at FROM highlights ORDER BY 1",
    ] {
        let found: Vec<String> = sqlx::query_scalar(sql)
            .fetch_all(pool.read())
            .await
            .expect("read the reader's own rows");
        rows.extend(found);
    }
    rows
}

async fn add_readers_data(pool: &Db) {
    bookmarks::add_bookmark(pool.write(), JOHN, 11, 35, 35)
        .await
        .expect("a bookmark");
    highlights::set_highlights(pool.write(), JOHN, 11, 35, 35, HighlightColor::Yellow)
        .await
        .expect("a highlight");
    add_note(pool.write(), &verse(11, 35), "He wept anyway.", None)
        .await
        .expect("a note");
}

#[tokio::test]
async fn saving_a_verse_again_for_the_same_request_is_a_no_op() {
    let (_db, pool) = fresh_database("findings_unique").await;
    let first = add_findings(
        pool.write(),
        &[new(verse(11, 33), "grief"), new(verse(11, 35), "grief")],
    )
    .await
    .expect("save");
    assert_eq!(first.saved, [verse(11, 33), verse(11, 35)]);
    assert!(first.already.is_empty());

    let again = add_findings(
        pool.write(),
        &[new(verse(11, 35), "grief"), new(verse(11, 35), "tears")],
    )
    .await
    .expect("save again");
    assert_eq!(again.already, [verse(11, 35)]);
    assert_eq!(
        again.saved,
        [verse(11, 35)],
        "another request is another finding"
    );

    let chapter = list_findings(pool.read(), JOHN, 11).await.expect("list");
    let saved: Vec<(i64, &str)> = chapter
        .iter()
        .map(|f| (f.verse_start, f.request.as_str()))
        .collect();
    assert_eq!(saved, [(33, "grief"), (35, "grief"), (35, "tears")]);
    // A plain INSERT of the same verses and request is refused by the index,
    // whatever agent says it.
    let duplicate = sqlx::query(
        "INSERT INTO ai_findings (book_id, chapter, verse_start, verse_end, agent, request) \
         VALUES (43, 11, 35, 35, 'Someone else', 'grief')",
    )
    .execute(pool.write().pool())
    .await;
    assert!(duplicate.is_err(), "the UNIQUE index holds");
}

/// The table has no column for free text (C3), and its CHECKs back up the
/// rules for each field.
#[tokio::test]
async fn the_table_holds_only_a_marker_and_checks_it() {
    let (_db, pool) = fresh_database("findings_checks").await;
    let columns: Vec<String> =
        sqlx::query_scalar("SELECT name FROM pragma_table_info('ai_findings')")
            .fetch_all(pool.read())
            .await
            .expect("columns");
    assert_eq!(
        columns,
        [
            "id",
            "book_id",
            "chapter",
            "verse_start",
            "verse_end",
            "agent",
            "request",
            "created_at"
        ]
    );
    let insert = |chapter: i64, start: i64, end: i64, agent: String, request: String| {
        let pool = pool.clone();
        async move {
            sqlx::query(
                "INSERT INTO ai_findings (book_id, chapter, verse_start, verse_end, agent, request) \
                 VALUES (43, $1, $2, $3, $4, $5)",
            )
            .bind(chapter)
            .bind(start)
            .bind(end)
            .bind(agent)
            .bind(request)
            .execute(pool.write().pool())
            .await
        }
    };
    let ok = || ("Claude".to_string(), "grief".to_string());
    for (what, (chapter, start, end), (agent, request)) in [
        ("chapter 0", (0, 1, 1), ok()),
        ("verse 0", (11, 0, 1), ok()),
        ("a range that ends first", (11, 5, 4), ok()),
        ("no agent", (11, 1, 1), (String::new(), "grief".to_string())),
        (
            "a long agent",
            (11, 1, 1),
            ("a".repeat(61), "grief".to_string()),
        ),
        (
            "no request",
            (11, 1, 1),
            ("Claude".to_string(), String::new()),
        ),
        (
            "a long request",
            (11, 1, 1),
            ("Claude".to_string(), "r".repeat(201)),
        ),
    ] {
        let result = insert(chapter, start, end, agent, request).await;
        let message = format!("{:?}", result.expect_err(what));
        assert!(message.contains("CHECK"), "{what}: {message}");
    }
    insert(11, 1, 1, "a".repeat(60), "r".repeat(200))
        .await
        .expect("the longest of each is allowed");
}

#[tokio::test]
async fn the_switch_is_off_until_the_reader_turns_it_on() {
    let (_db, pool) = fresh_database("findings_switch").await;
    assert!(!findings_enabled(pool.read()).await.expect("read"));
    set_findings_enabled(pool.write(), true).await.expect("on");
    assert!(findings_enabled(pool.read()).await.expect("read"));
    set_findings_enabled(pool.write(), false)
        .await
        .expect("off");
    assert!(!findings_enabled(pool.read()).await.expect("read"));
}

/// C10: the note is the reader's, saved as any note is, and the finding goes
/// only once it's saved; a verse's own notes are never touched.
#[tokio::test]
async fn keep_as_mine_makes_a_note_and_removes_the_finding_together() {
    let (_db, pool) = fresh_database("findings_keep").await;
    add_readers_data(&pool).await;
    let before = list_notes(pool.read()).await.expect("notes");
    add_findings(
        pool.write(),
        &[new(verse(11, 35), "grief"), new(verse(11, 35), "tears")],
    )
    .await
    .expect("save");
    let found = list_findings(pool.read(), JOHN, 11).await.expect("list");
    let grief = found.iter().find(|f| f.request == "grief").expect("grief");

    // Nothing to keep: nothing changes.
    for empty in ["", "  \n "] {
        let result = keep_as_mine(pool.write(), grief.id, empty).await;
        assert!(
            matches!(
                result,
                Err(KeepError::Note(NoteError::Body(NoteBodyError::Empty)))
            ),
            "{result:?}"
        );
    }
    assert_eq!(list_findings(pool.read(), JOHN, 11).await.unwrap(), found);
    assert_eq!(list_notes(pool.read()).await.unwrap(), before);

    // Kept unchanged, the text still says who found it; it's the reader's.
    let note = keep_as_mine(pool.write(), grief.id, &grief.text())
        .await
        .expect("keep");
    assert_eq!(
        note.body,
        "Found by Claude for your request \u{201c}grief\u{201d}."
    );
    assert_eq!(note.passage(), verse(11, 35));
    let after = list_notes(pool.read()).await.expect("notes");
    assert_eq!(after.len(), before.len() + 1, "a second note on the verse");
    assert!(
        after.contains(&before[0]),
        "the reader's own note is untouched"
    );
    let left = list_findings(pool.read(), JOHN, 11).await.unwrap();
    let requests: Vec<&str> = left.iter().map(|f| f.request.as_str()).collect();
    assert_eq!(requests, ["tears"], "only that one finding went");

    // Gone already: no note.
    let again = keep_as_mine(pool.write(), grief.id, "Mine.").await;
    assert!(matches!(again, Err(KeepError::FindingGone)), "{again:?}");
    assert_eq!(list_notes(pool.read()).await.unwrap(), after);
}

/// A note that can't be saved leaves the finding where it was: the two are
/// one transaction.
#[tokio::test]
async fn a_kept_note_that_cant_be_saved_leaves_the_finding() {
    let (_db, pool) = fresh_database("findings_keep_limit").await;
    add_findings(pool.write(), &[new(verse(11, 35), "grief")])
        .await
        .expect("save");
    fill_notes(pool.write(), MAX_NOTES, 7).await;
    let found = list_findings(pool.read(), JOHN, 11).await.unwrap();
    let result = keep_as_mine(pool.write(), found[0].id, "Mine.").await;
    assert!(
        matches!(
            result,
            Err(KeepError::Note(NoteError::Limit(NoteLimit::Count)))
        ),
        "{result:?}"
    );
    assert_eq!(list_findings(pool.read(), JOHN, 11).await.unwrap(), found);
    assert_eq!(list_notes(pool.read()).await.unwrap().len(), MAX_NOTES);
}

#[tokio::test]
async fn a_request_is_removed_alone_and_comes_back_with_its_dates() {
    let (_db, pool) = fresh_database("findings_remove").await;
    add_findings(
        pool.write(),
        &[
            new(verse(11, 35), "grief"),
            new(verse(11, 33), "grief"),
            new(verse(11, 35), "tears"),
        ],
    )
    .await
    .expect("save");
    let groups = list_requests(pool.read()).await.expect("groups");
    assert_eq!(groups.len(), 2);

    let removed = remove_request(pool.write(), "grief").await.expect("remove");
    let verses: Vec<i64> = removed.iter().map(|f| f.verse_start).collect();
    assert_eq!(verses, [33, 35]);
    let left = list_requests(pool.read()).await.unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].request, "tears");

    // Undo brings them back as they were, but for their ids.
    let back: Vec<NewFinding> = removed
        .iter()
        .map(|f| NewFinding {
            passage: f.passage(),
            agent: f.agent.clone(),
            request: f.request.clone(),
            created_at: Some(f.created_at.clone()),
        })
        .collect();
    add_findings(pool.write(), &back).await.expect("restore");
    let restored = list_requests(pool.read()).await.unwrap();
    let grief = restored.iter().find(|g| g.request == "grief").unwrap();
    let dates: Vec<&str> = grief
        .findings
        .iter()
        .map(|f| f.created_at.as_str())
        .collect();
    let was: Vec<&str> = removed.iter().map(|f| f.created_at.as_str()).collect();
    assert_eq!(dates, was);

    let one = grief.findings[0].id;
    assert_eq!(
        remove_finding(pool.write(), one)
            .await
            .unwrap()
            .map(|f| f.id),
        Some(one)
    );
    assert_eq!(remove_finding(pool.write(), one).await.unwrap(), None);
}

#[tokio::test]
async fn a_passage_that_isnt_there_saves_nothing() {
    let (_db, pool) = fresh_database("findings_passage").await;
    for bad in [verse(22, 1), verse(11, 58), verse(11, 0)] {
        let result = add_findings(
            pool.write(),
            &[new(verse(11, 35), "grief"), new(bad, "grief")],
        )
        .await;
        assert!(
            matches!(result, Err(FindingsError::Passage(_))),
            "{bad:?}: {result:?}"
        );
    }
    assert!(
        list_findings(pool.read(), JOHN, 11)
            .await
            .unwrap()
            .is_empty()
    );
}

// ---- The writer a program beside the app has.

#[tokio::test]
async fn the_writer_saves_only_while_the_switch_is_on() {
    let (db, pool) = fresh_database("findings_writer").await;
    add_readers_data(&pool).await;
    let own = readers_own(&pool).await;
    let writer = FindingsWriter::open(&db.path()).await.expect("open");

    let off = writer.add("Claude", "grief", &[verse(11, 35)]).await;
    assert!(matches!(off, Err(FindingsError::Off)), "{off:?}");
    assert!(
        list_findings(pool.read(), JOHN, 11)
            .await
            .unwrap()
            .is_empty()
    );

    set_findings_enabled(pool.write(), true).await.expect("on");
    let saved = writer
        .add("  ", " grief ", &[verse(11, 33), verse(11, 35)])
        .await
        .expect("save");
    assert_eq!(saved.saved.len(), 2);
    let again = writer
        .add("Claude", "grief", &[verse(11, 35), verse(11, 36)])
        .await
        .expect("save again");
    assert_eq!(
        (again.saved, again.already),
        (vec![verse(11, 36)], vec![verse(11, 35)])
    );
    let found = list_findings(pool.read(), JOHN, 11).await.unwrap();
    assert_eq!(found[0].agent, DEFAULT_AGENT);
    assert_eq!(found[0].request, "grief");

    let bad_request = writer.add("Claude", "", &[verse(11, 1)]).await;
    assert!(
        matches!(bad_request, Err(FindingsError::Request(_))),
        "{bad_request:?}"
    );
    writer.close().await;
    assert_eq!(readers_own(&pool).await, own, "no other table changed");
}

/// A database the app hasn't opened since 2.25.x has no tables for
/// findings, and the writer never makes them.
#[tokio::test]
async fn the_writer_refuses_a_database_without_the_tables() {
    let (db, pool) = fresh_database("findings_not_ready").await;
    for table in ["ai_findings", "app_settings"] {
        sqlx::query(sqlx::AssertSqlSafe(format!("DROP TABLE {table}")))
            .execute(pool.write().pool())
            .await
            .expect("drop");
    }
    let writer = FindingsWriter::open(&db.path()).await.expect("open");
    let result = writer.add("Claude", "grief", &[verse(11, 35)]).await;
    assert!(matches!(result, Err(FindingsError::NotReady)), "{result:?}");
    assert_eq!(
        result.unwrap_err().to_string(),
        "Open Gospel Getter 2.26.0 or newer once, then try again."
    );
    writer.close().await;
    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE name IN ('ai_findings', 'app_settings')",
    )
    .fetch_all(pool.read())
    .await
    .unwrap();
    assert!(tables.is_empty(), "{tables:?}");

    let missing = TempDb::new("findings_no_file");
    assert!(FindingsWriter::open(&missing.path()).await.is_err());
    assert!(!missing.path().exists(), "no database is created");
}

/// While the app holds the write lock, the writer waits its turn rather
/// than failing.
#[tokio::test]
async fn the_writer_waits_for_the_apps_write() {
    let (db, pool) = fresh_database("findings_wait").await;
    set_findings_enabled(pool.write(), true).await.expect("on");
    let writer = FindingsWriter::open(&db.path()).await.expect("open");
    let mut tx = begin_write(pool.write()).await.expect("the app's write");
    sqlx::query(
        "INSERT INTO bookmarks (book_id, chapter, verse_start, verse_end) VALUES (43, 3, 16, 16)",
    )
    .execute(&mut *tx)
    .await
    .expect("a bookmark");
    let saving = tokio::spawn(async move {
        let saved = writer.add("Claude", "love", &[verse(3, 16)]).await;
        writer.close().await;
        saved
    });
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(!saving.is_finished(), "it waits for the lock");
    tx.commit().await.expect("commit");
    let saved = saving
        .await
        .expect("join")
        .expect("saved once the app's write is done");
    assert_eq!(saved.saved, [verse(3, 16)]);
}
