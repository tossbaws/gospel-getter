//! A backup's snapshot of the reader's data against real files: one moment
//! of the database, whatever the writer commits while it's read. The
//! snapshot stops at [`between_notes_and_findings`] only inside a test's
//! [`Pause`], so the race "Keep as mine" could win is played out in order,
//! without sleeps. Each test gets its own disposable database (see
//! `test_support`).

use std::sync::Arc;

use tokio::sync::Notify;
use tokio::task::JoinHandle;

use super::findings::{all_findings, findings_enabled, keep_as_mine, set_findings_enabled};
use super::highlights::{list_highlights, set_highlights};
use super::notes::{Passage, add_note, list_notes};
use super::reader_data::{ReaderSnapshot, all_bookmarks, snapshot};
use super::test_support::fresh_database;
use super::{Db, Finding, HighlightColor, NewFinding, bookmarks, get_reading_position};

const JOHN: i64 = 43;

/// Where a test holds the snapshot: it says when it's there (`reached`)
/// and waits to be let go (`resume`). `Notify` keeps a permit given before
/// anyone waits, so neither side can miss the other.
#[derive(Default)]
struct Pause {
    reached: Notify,
    resume: Notify,
}

tokio::task_local! {
    static PAUSE: Arc<Pause>;
}

/// The snapshot's pause point, between its notes and its findings. Outside
/// a test's [`PAUSE`] scope (every other test's snapshot) it does nothing.
pub(super) async fn between_notes_and_findings() {
    if let Ok(pause) = PAUSE.try_with(Arc::clone) {
        pause.reached.notify_one();
        pause.resume.notified().await;
    }
}

/// A snapshot of `pool`, started and held at its pause point.
async fn paused_snapshot(pool: &Db) -> (Arc<Pause>, JoinHandle<anyhow::Result<ReaderSnapshot>>) {
    let pause = Arc::new(Pause::default());
    let read = pool.read().clone();
    let taking =
        tokio::spawn(PAUSE.scope(Arc::clone(&pause), async move { snapshot(&read).await }));
    pause.reached.notified().await;
    (pause, taking)
}

fn verse(chapter: i64, verse: i64) -> Passage {
    Passage {
        book_id: JOHN,
        chapter,
        verse_start: verse,
        verse_end: verse,
    }
}

/// The one finding a test keeps, saved by an agent on John 11:35.
async fn one_finding(pool: &Db) -> Finding {
    super::findings::add_findings(
        pool.write(),
        &[NewFinding {
            passage: verse(11, 35),
            agent: "Claude".to_string(),
            request: "grief".to_string(),
            created_at: None,
        }],
    )
    .await
    .expect("a finding");
    let mut all = all_findings(pool.read()).await.expect("the findings");
    assert_eq!(all.len(), 1);
    all.remove(0)
}

async fn keep(pool: &Db, finding: &Finding) {
    keep_as_mine(pool.write(), finding.id, &finding.created_at, "Jesus wept.")
        .await
        .expect("kept as mine");
}

/// The live database once the finding is kept: its note, and no finding.
async fn assert_kept(pool: &Db) {
    let notes = list_notes(pool.read()).await.expect("the notes");
    assert_eq!(
        notes.iter().map(|n| n.body.as_str()).collect::<Vec<_>>(),
        ["Jesus wept."]
    );
    assert!(
        all_findings(pool.read())
            .await
            .expect("the findings")
            .is_empty()
    );
}

/// "Keep as mine" committed while the snapshot is between its notes and its
/// findings: the snapshot is the moment before (the finding, no note), not
/// neither. The live database has the note and no finding.
#[tokio::test]
async fn a_finding_kept_mid_snapshot_is_in_it_once_as_it_was() {
    let (_db, pool) = fresh_database("snapshot_keep_mid").await;
    let finding = one_finding(&pool).await;

    let (pause, taking) = paused_snapshot(&pool).await;
    keep(&pool, &finding).await;
    pause.resume.notify_one();
    let snap = taking.await.expect("the snapshot ran").expect("a snapshot");

    assert_eq!(
        (snap.findings.as_slice(), snap.notes.as_slice()),
        (std::slice::from_ref(&finding), &[][..]),
        "the finding as it was, and not yet its note"
    );
    assert_kept(&pool).await;
}

/// A snapshot started once the finding is kept has the note alone.
#[tokio::test]
async fn a_snapshot_after_keeping_has_the_note_alone() {
    let (_db, pool) = fresh_database("snapshot_keep_before").await;
    let finding = one_finding(&pool).await;
    keep(&pool, &finding).await;

    let snap = snapshot(pool.read()).await.expect("a snapshot");
    assert!(snap.findings.is_empty(), "{:?}", snap.findings);
    assert_eq!(
        snap.notes
            .iter()
            .map(|n| n.body.as_str())
            .collect::<Vec<_>>(),
        ["Jesus wept."]
    );
}

/// The hazard the snapshot is for: the same reads made one pool call at a
/// time, with "Keep as mine" committed between the notes and the findings,
/// see neither the note nor the finding. If this stops holding, the test
/// above no longer plays out the race it guards against.
#[tokio::test]
async fn separate_reads_can_miss_a_kept_finding_both_ways() {
    let (_db, pool) = fresh_database("snapshot_split_reads").await;
    let finding = one_finding(&pool).await;

    let notes = list_notes(pool.read()).await.expect("the notes");
    keep(&pool, &finding).await;
    let findings = all_findings(pool.read()).await.expect("the findings");

    assert!(notes.is_empty() && findings.is_empty());
    assert_kept(&pool).await;
}

/// With nothing writing, the snapshot is exactly what the separate reads
/// give, record for record and in their order, with the switch and the
/// reading position too.
#[tokio::test]
async fn a_snapshot_is_what_the_separate_reads_give() {
    let (_db, pool) = fresh_database("snapshot_same_reads").await;
    let empty = snapshot(pool.read()).await.expect("a snapshot");
    assert_eq!(
        empty,
        ReaderSnapshot {
            bookmarks: Vec::new(),
            highlights: Vec::new(),
            notes: Vec::new(),
            findings: Vec::new(),
            findings_enabled: false,
            reading_position: None,
        }
    );

    for (chapter, v) in [(11, 35), (3, 16), (1, 1)] {
        bookmarks::add_bookmark(pool.write(), JOHN, chapter, v, v)
            .await
            .expect("a bookmark");
        set_highlights(pool.write(), JOHN, chapter, v, v, HighlightColor::Green)
            .await
            .expect("a highlight");
        add_note(pool.write(), &verse(chapter, v), "Mine.", None)
            .await
            .expect("a note");
    }
    one_finding(&pool).await;
    set_findings_enabled(pool.write(), true)
        .await
        .expect("the switch on");
    super::save_reading_position_at(pool.write(), 1, JOHN, 11, None)
        .await
        .expect("a position");

    let snap = snapshot(pool.read()).await.expect("a snapshot");
    let read = pool.read();
    assert_eq!(
        snap,
        ReaderSnapshot {
            bookmarks: all_bookmarks(read).await.expect("bookmarks"),
            highlights: list_highlights(read).await.expect("highlights"),
            notes: list_notes(read).await.expect("notes"),
            findings: all_findings(read).await.expect("findings"),
            findings_enabled: findings_enabled(read).await.expect("the switch"),
            reading_position: get_reading_position(read).await.expect("the position"),
        }
    );
    assert_eq!(
        (
            snap.bookmarks.len(),
            snap.highlights.len(),
            snap.notes.len()
        ),
        (3, 3, 3)
    );
    assert!(snap.findings_enabled);
    assert_eq!(snap.reading_position, Some((1, JOHN, 11)));
}
