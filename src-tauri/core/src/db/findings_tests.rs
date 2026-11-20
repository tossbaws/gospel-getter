//! AI-flagged findings against real files: the table's rules, the reader's
//! switch, "Keep as mine" and the one narrow writer a program beside the app
//! has. Each test gets its own disposable database (see `test_support`).

use std::time::Duration;

use super::findings::{
    Finding, FindingsWriter, KeepError, MAX_FINDINGS, NewFinding, RemovedFinding, add_findings,
    all_findings, findings_enabled, keep_as_mine, list_findings, list_requests, remove_finding,
    remove_request, restore_findings, set_findings_enabled,
};
use super::notes::{MAX_NOTES, NoteLimit, Passage, add_note, list_notes};
use super::test_support::{
    OLD_RULE_REQUESTS, TempDb, fill_findings, fill_notes, fresh_database, insert_old_rule_findings,
};
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
/// passage's rules. The labels' limits are Rust's alone
/// (`migrate_tests::the_limits_are_rusts_on_an_upgraded_database`).
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
    ] {
        let result = insert(chapter, start, end, agent, request).await;
        let message = format!("{:?}", result.expect_err(what));
        assert!(message.contains("CHECK"), "{what}: {message}");
    }
    insert(11, 1, 1, "a".repeat(60), "r".repeat(60))
        .await
        .expect("a finding on a real passage is allowed");
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
        let result = keep_as_mine(pool.write(), grief.id, &grief.created_at, empty).await;
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
    let note = keep_as_mine(pool.write(), grief.id, &grief.created_at, &grief.text())
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
    let again = keep_as_mine(pool.write(), grief.id, &grief.created_at, "Mine.").await;
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
    let result = keep_as_mine(pool.write(), found[0].id, &found[0].created_at, "Mine.").await;
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
    let back: Vec<RemovedFinding> = removed.iter().cloned().map(RemovedFinding::from).collect();
    assert_eq!(
        restore_findings(pool.write(), &back)
            .await
            .expect("restore"),
        2
    );
    let restored = list_requests(pool.read()).await.unwrap();
    let grief = restored.iter().find(|g| g.request == "grief").unwrap();
    let dates: Vec<&str> = grief
        .findings
        .iter()
        .map(|f| f.created_at.as_str())
        .collect();
    let was: Vec<&str> = removed.iter().map(|f| f.created_at.as_str()).collect();
    assert_eq!(dates, was);

    let one = &grief.findings[0];
    assert_eq!(
        remove_finding(pool.write(), one.id, &one.created_at)
            .await
            .unwrap()
            .map(|f| f.id),
        Some(one.id)
    );
    assert_eq!(
        remove_finding(pool.write(), one.id, &one.created_at)
            .await
            .unwrap(),
        None
    );
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
        .add(" Claude Code ", " grief ", &[verse(11, 33), verse(11, 35)])
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
    assert_eq!(found[0].agent, "Claude Code");
    assert_eq!(found[0].request, "grief");

    let bad_request = writer.add("Claude", "", &[verse(11, 1)]).await;
    assert!(
        matches!(bad_request, Err(FindingsError::Request(_))),
        "{bad_request:?}"
    );
    // An agent's name is a label too (C11): commentary, a blank name or one
    // past 40 characters saves nothing.
    for agent in [
        "Your pastor: this verse proves grief is sin",
        "  ",
        &"a".repeat(41),
    ] {
        let bad_agent = writer.add(agent, "grief", &[verse(11, 1)]).await;
        assert!(
            matches!(bad_agent, Err(FindingsError::Agent(_))),
            "{agent:?}: {bad_agent:?}"
        );
    }
    assert_eq!(list_findings(pool.read(), JOHN, 11).await.unwrap().len(), 3);
    writer.close().await;
    assert_eq!(readers_own(&pool).await, own, "no other table changed");
}

/// How many findings there are.
async fn finding_count(pool: &Db) -> usize {
    all_findings(pool.read()).await.expect("findings").len()
}

/// There are at most `MAX_FINDINGS` findings, every request's together.
/// A save that would make more saves none of its findings (all or none,
/// as a passage that isn't there stops them all), and says so plainly;
/// one that only finds its verses already saved adds nothing, so it's
/// answered as before even at the limit. An agent's save through the writer
/// and `add_findings` (the same save through the app's connection) obey it
/// alike; Undo doesn't (`undo_puts_back_past_the_limit`).
#[tokio::test]
async fn findings_stop_at_the_limit_all_or_none() {
    assert_eq!(MAX_FINDINGS, 10_000);
    let (db, pool) = fresh_database("findings_limit").await;
    fill_findings(pool.write(), MAX_FINDINGS - 2).await;
    assert_eq!(finding_count(&pool).await, MAX_FINDINGS - 2);

    // Three new ones would be one over: none is saved.
    let over = add_findings(
        pool.write(),
        &[
            new(verse(11, 33), "grief"),
            new(verse(11, 34), "grief"),
            new(verse(11, 36), "grief"),
        ],
    )
    .await;
    assert!(matches!(over, Err(FindingsError::Limit)), "{over:?}");
    assert_eq!(
        over.unwrap_err().to_string(),
        "Gospel Getter holds at most 10,000 AI findings; remove some in the Library first."
    );
    assert_eq!(finding_count(&pool).await, MAX_FINDINGS - 2);
    assert!(
        list_requests(pool.read())
            .await
            .unwrap()
            .iter()
            .all(|r| r.request != "grief")
    );

    // Two reach it exactly.
    let at = add_findings(
        pool.write(),
        &[new(verse(11, 33), "grief"), new(verse(11, 34), "grief")],
    )
    .await
    .expect("up to the limit");
    assert_eq!(at.saved.len(), 2);
    assert_eq!(finding_count(&pool).await, MAX_FINDINGS);

    // At the limit, the same verses again add nothing, and are answered
    // as already saved; a new one is refused.
    let again = add_findings(pool.write(), &[new(verse(11, 33), "grief")])
        .await
        .expect("already saved");
    assert_eq!((again.saved, again.already), (vec![], vec![verse(11, 33)]));
    let one_more = add_findings(pool.write(), &[new(verse(11, 36), "grief")]).await;
    assert!(
        matches!(one_more, Err(FindingsError::Limit)),
        "{one_more:?}"
    );

    // An agent's save, through the writer, likewise.
    set_findings_enabled(pool.write(), true).await.expect("on");
    let writer = FindingsWriter::open(&db.path()).await.expect("open");
    let agent = writer.add("Claude", "tears", &[verse(11, 35)]).await;
    assert!(matches!(agent, Err(FindingsError::Limit)), "{agent:?}");
    let same = writer
        .add("Claude", "grief", &[verse(11, 34)])
        .await
        .expect("already saved");
    assert_eq!(same.already, [verse(11, 34)]);

    // Room again once some are removed.
    let gone = remove_request(pool.write(), "grief").await.expect("remove");
    assert_eq!(gone.len(), 2);
    let saved = writer
        .add("Claude", "tears", &[verse(11, 35), verse(11, 36)])
        .await
        .expect("room for two");
    writer.close().await;
    assert_eq!(saved.saved.len(), 2);
    assert_eq!(finding_count(&pool).await, MAX_FINDINGS);
}

/// Every finding, as its columns are stored, but for its id.
async fn stored(pool: &Db) -> Vec<(i64, i64, i64, i64, String, String, String)> {
    sqlx::query_as(
        "SELECT book_id, chapter, verse_start, verse_end, agent, request, created_at \
         FROM ai_findings ORDER BY book_id, chapter, verse_start, request",
    )
    .fetch_all(pool.read())
    .await
    .expect("read the findings")
}

/// Undo puts back exactly what was removed, a finding whose label an older
/// rule saved and today's refuses included: its agent, request and date byte
/// for byte, one finding or a whole request (GG32-01, AF1).
#[tokio::test]
async fn undo_puts_back_a_finding_an_older_rule_saved_exactly() {
    let (_db, pool) = fresh_database("findings_undo_old_rule").await;
    insert_old_rule_findings(pool.write()).await;
    let before = stored(&pool).await;
    assert_eq!(before.len(), OLD_RULE_REQUESTS.len());

    for found in all_findings(pool.read()).await.unwrap() {
        let removed: Finding = remove_finding(pool.write(), found.id, &found.created_at)
            .await
            .unwrap()
            .expect("removed");
        assert_eq!(stored(&pool).await.len(), before.len() - 1);
        let back = restore_findings(pool.write(), &[removed.into()])
            .await
            .expect("Undo");
        assert_eq!(back, 1, "{:?}", found.request);
        assert_eq!(stored(&pool).await, before, "{:?}", found.request);
    }

    for (request, _) in OLD_RULE_REQUESTS {
        let removed = remove_request(pool.write(), request).await.unwrap();
        assert_eq!(removed.len(), 1, "{request:?}");
        let back: Vec<RemovedFinding> = removed.into_iter().map(RemovedFinding::from).collect();
        assert_eq!(restore_findings(pool.write(), &back).await.unwrap(), 1);
        assert_eq!(stored(&pool).await, before, "{request:?}");
    }
}

/// Undo isn't an agent's save: it puts back what was removed even when
/// agents have since filled the findings to the limit. An agent's save
/// afterwards is still refused at the limit, as ever.
#[tokio::test]
async fn undo_puts_back_past_the_limit() {
    let (db, pool) = fresh_database("findings_undo_limit").await;
    add_findings(pool.write(), &[new(verse(11, 33), "grief")])
        .await
        .expect("save");
    let found = all_findings(pool.read()).await.unwrap().remove(0);
    let removed = remove_finding(pool.write(), found.id, &found.created_at)
        .await
        .unwrap()
        .expect("removed");
    fill_findings(pool.write(), MAX_FINDINGS).await;

    assert_eq!(
        restore_findings(pool.write(), &[removed.into()])
            .await
            .expect("Undo past the limit"),
        1
    );
    assert_eq!(finding_count(&pool).await, MAX_FINDINGS + 1);
    assert!(all_findings(pool.read()).await.unwrap().iter().any(|f| {
        (f.request.as_str(), f.created_at.as_str()) == ("grief", found.created_at.as_str())
    }));

    set_findings_enabled(pool.write(), true).await.expect("on");
    let writer = FindingsWriter::open(&db.path()).await.expect("open");
    let agent = writer.add("Claude", "tears", &[verse(11, 35)]).await;
    writer.close().await;
    assert!(matches!(agent, Err(FindingsError::Limit)), "{agent:?}");
    assert_eq!(
        agent.unwrap_err().to_string(),
        "Gospel Getter holds at most 10,000 AI findings; remove some in the Library first."
    );
}

/// A new agent save keeps today's rule: a label an older rule allowed is
/// refused with today's words, and nothing is saved.
#[tokio::test]
async fn an_agent_save_with_a_label_an_older_rule_allowed_is_still_refused() {
    let (db, pool) = fresh_database("findings_old_rule_agent").await;
    set_findings_enabled(pool.write(), true).await.expect("on");
    let writer = FindingsWriter::open(&db.path()).await.expect("open");
    for (request, _) in OLD_RULE_REQUESTS {
        let result = writer.add("Claude", request, &[verse(11, 35)]).await;
        let Err(e @ FindingsError::Request(_)) = result else {
            panic!("{request:?}: {result:?}");
        };
        assert_eq!(
            e.to_string(),
            "The request is a short topic label, such as \u{201c}grief\u{201d} or \u{201c}the \
             sun\u{201d}, so it can't have a colon, a semicolon, a dash (\u{2014} or \u{2013}), \
             a line break or a character that can't be seen.",
            "{request:?}"
        );
    }
    writer.close().await;
    assert_eq!(finding_count(&pool).await, 0);
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

/// A finding saved well before the one that takes its id.
const LONG_AGO: &str = "2026-01-01T00:00:00.000Z";

/// Finding A on John 11:35, removed, then finding B on John 11:33, with
/// A's id (GG-06): (A as it was, B). Schema 2 gives B an id of its own; B
/// is moved to A's, as a database still at version 1 would have given it,
/// for the second guard.
async fn reused_finding_id(pool: &Db) -> (super::findings::Finding, super::findings::Finding) {
    let mut first = new(verse(11, 35), "grief");
    first.created_at = Some(LONG_AGO.to_string());
    add_findings(pool.write(), &[first]).await.expect("save A");
    let a = all_findings(pool.read()).await.expect("list").remove(0);
    remove_finding(pool.write(), a.id, &a.created_at)
        .await
        .expect("remove A")
        .expect("A was there");
    add_findings(pool.write(), &[new(verse(11, 33), "tears")])
        .await
        .expect("save B");
    let mut b = all_findings(pool.read()).await.expect("list").remove(0);
    assert_ne!(b.id, a.id, "schema 2 never hands out A's id again");
    sqlx::query("UPDATE ai_findings SET id = $1 WHERE id = $2")
        .bind(a.id)
        .bind(b.id)
        .execute(pool.write().pool())
        .await
        .expect("give B A's id");
    b.id = a.id;
    assert_ne!(b.created_at, a.created_at);
    (a, b)
}

/// GG-06: Keep as mine from a view still holding finding A doesn't turn
/// finding B, which took A's id and is on another verse, into a note; it's
/// refused as A's would be, A being gone. With B's own `created_at` it
/// keeps B.
#[tokio::test]
async fn a_stale_keep_leaves_the_finding_that_took_its_id() {
    let (_db, pool) = fresh_database("findings_stale_keep").await;
    let (a, b) = reused_finding_id(&pool).await;
    let stale = keep_as_mine(pool.write(), a.id, &a.created_at, "Mine.").await;
    assert!(matches!(stale, Err(KeepError::FindingGone)), "{stale:?}");
    assert!(list_notes(pool.read()).await.unwrap().is_empty());
    assert_eq!(
        all_findings(pool.read()).await.unwrap(),
        std::slice::from_ref(&b)
    );

    let note = keep_as_mine(pool.write(), b.id, &b.created_at, "Mine.")
        .await
        .expect("keep B");
    assert_eq!(note.passage(), b.passage());
    assert_eq!(list_notes(pool.read()).await.unwrap(), [note]);
    assert!(all_findings(pool.read()).await.unwrap().is_empty());
}

/// GG-06: Remove from a view still holding finding A leaves finding B,
/// which took A's id, and answers as for a finding already gone. With B's
/// own `created_at` it removes B.
#[tokio::test]
async fn a_stale_remove_leaves_the_finding_that_took_its_id() {
    let (_db, pool) = fresh_database("findings_stale_remove").await;
    let (a, b) = reused_finding_id(&pool).await;
    assert_eq!(
        remove_finding(pool.write(), a.id, &a.created_at)
            .await
            .unwrap(),
        None
    );
    assert_eq!(
        all_findings(pool.read()).await.unwrap(),
        std::slice::from_ref(&b)
    );

    assert_eq!(
        remove_finding(pool.write(), b.id, &b.created_at)
            .await
            .unwrap(),
        Some(b)
    );
    assert!(all_findings(pool.read()).await.unwrap().is_empty());
}
