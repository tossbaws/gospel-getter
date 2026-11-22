//! `migrate`'s tests: a new database, every older one (version 1, before
//! versions, a newer app's), a step that fails part-way, and running twice.
//! Version-1 databases are built from the frozen schema
//! (`migrate/schema_1.sql`), never by old code.

use sqlx::SqlitePool;

use super::bookmarks::{add_bookmark, remove_bookmark};
use super::findings::{
    FindingsError, MAX_REQUEST_CHARS, MAX_STORED_AGENT_CHARS, NewFinding, RemovedFinding,
    RequestError, add_findings, all_findings, remove_finding, restore_findings,
};
use super::highlights::set_highlights;
use super::migrate::{
    HEAD_CHARS, OLDEST_READABLE_SCHEMA_VERSION, Rebuild, SCHEMA_VERSION, STEPS, Step, can_use_with,
    complete_schema_1, head, if_not_exists, migrate, migrate_with, older_app_can_use, schema_1,
};
use super::notes::{MAX_NOTE_CHARS, NoteBodyError, NoteError, Passage, add_note, delete_note};
use super::test_support::{TempDb, fresh_database, insert_old_rule_findings, schema_1_database};
use super::{Db, HighlightColor, open_read_only, prepare};

/// Each object in `sqlite_master` (type, name, table, SQL), by type and
/// name.
pub(super) async fn schema(pool: &SqlitePool) -> Vec<(String, String, String, Option<String>)> {
    sqlx::query_as("SELECT type, name, tbl_name, sql FROM sqlite_master ORDER BY type, name")
        .fetch_all(pool)
        .await
        .expect("read sqlite_master")
}

/// [`schema`] with each statement's whitespace made single spaces, and the
/// quotes taken from around a table's name, which SQLite's `ALTER TABLE
/// ... RENAME` puts there (`CREATE TABLE "notes" (...`): what two
/// databases with the same schema can differ by.
pub(super) async fn normalized_schema(pool: &SqlitePool) -> Vec<(String, String, String, String)> {
    schema(pool)
        .await
        .into_iter()
        .map(|(kind, name, table, sql)| {
            let sql = sql
                .unwrap_or_default()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .replacen(
                    &format!("CREATE TABLE \"{name}\" "),
                    &format!("CREATE TABLE {name} "),
                    1,
                );
            (kind, name, table, sql)
        })
        .collect()
}

/// The database's `PRAGMA user_version`.
pub(super) async fn user_version(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar("PRAGMA user_version")
        .fetch_one(pool)
        .await
        .expect("read user_version")
}

/// Every row of `table`, rowid first where it has one, each value as SQLite's
/// `quote()` writes it (its type and every byte), in order.
pub(super) async fn rows(pool: &SqlitePool, table: &str) -> Vec<String> {
    let columns: Vec<String> = sqlx::query_scalar("SELECT name FROM pragma_table_info($1)")
        .bind(table)
        .fetch_all(pool)
        .await
        .expect("read the columns");
    let without_rowid: bool =
        sqlx::query_scalar("SELECT sql LIKE '%WITHOUT ROWID' FROM sqlite_master WHERE name = $1")
            .bind(table)
            .fetch_one(pool)
            .await
            .expect("read the table");
    let mut values: Vec<String> = columns.iter().map(|c| format!("quote(\"{c}\")")).collect();
    // A table without rowids in order of its whole row.
    let order = if without_rowid {
        "1"
    } else {
        values.insert(0, "quote(rowid)".to_string());
        "rowid"
    };
    // Names from `sqlite_master` and `pragma_table_info`.
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT {} FROM \"{table}\" ORDER BY {order}",
        values.join(" || '|' || ")
    )))
    .fetch_all(pool)
    .await
    .expect("read the rows")
}

/// The whole database: its schema exactly, its version, and every row of
/// every table but the search index's (a virtual table, which holds no
/// rows of its own: its tables are listed with the rest).
pub(super) async fn snapshot(
    pool: &SqlitePool,
) -> (
    Vec<(String, String, String, Option<String>)>,
    i64,
    Vec<(String, Vec<String>)>,
) {
    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type = 'table' \
         AND sql NOT LIKE 'CREATE VIRTUAL TABLE%' ORDER BY name",
    )
    .fetch_all(pool)
    .await
    .expect("list the tables");
    let mut all = Vec::new();
    for table in tables {
        let rows = rows(pool, &table).await;
        all.push((table, rows));
    }
    (schema(pool).await, user_version(pool).await, all)
}

/// The reader's tables, every row of each (see [`rows`]).
pub(super) async fn readers_rows(pool: &SqlitePool) -> Vec<(&'static str, Vec<String>)> {
    let mut all = Vec::new();
    for table in READERS_TABLES {
        all.push((table, rows(pool, table).await));
    }
    all
}

/// Every table of the reader's data.
pub(super) const READERS_TABLES: [&str; 7] = [
    "bookmarks",
    "highlights",
    "notes",
    "ai_findings",
    "app_settings",
    "reading_position",
    "reading_place",
];

/// Records of every kind in a version-1 database, written with SQL as
/// 2.32.2 wrote them, with gaps in the ids (a record deleted from the
/// middle and from the end): notes, bookmarks, highlights, findings (the
/// old-rule labels among them), a setting and the reading place.
pub(super) async fn readers_records(pool: &Db) {
    for sql in [
        "INSERT INTO bookmarks (book_id, chapter, verse_start, verse_end, created_at) VALUES \
         (43, 3, 16, 16, '2026-01-02T03:04:05.006Z'), (19, 23, 1, 6, '2026-02-02T03:04:05.006Z'), \
         (45, 8, 28, 28, '2026-03-02T03:04:05.006Z'), (1, 1, 1, 1, '2026-04-02T03:04:05.006Z')",
        "DELETE FROM bookmarks WHERE id IN (2, 4)",
        "INSERT INTO highlights (book_id, chapter, verse, color, created_at, updated_at) VALUES \
         (43, 3, 16, 'yellow', '2026-01-02T03:04:05.006Z', '2026-01-03T03:04:05.006Z'), \
         (43, 3, 17, 'pink', '2026-01-02T03:04:05.006Z', '2026-01-02T03:04:05.006Z'), \
         (19, 23, 4, 'blue', '2026-05-02T03:04:05.006Z', '2026-05-02T03:04:05.006Z')",
        "DELETE FROM highlights WHERE verse = 17",
        "INSERT INTO notes (book_id, chapter, verse_start, verse_end, body, created_at, \
         updated_at) VALUES \
         (43, 3, 16, 16, 'For God so loved — “the world”.', '2026-01-02T03:04:05.006Z', \
          '2026-01-05T03:04:05.006Z'), \
         (43, 3, 17, 18, 'Deleted.', '2026-01-02T03:04:05.006Z', '2026-01-02T03:04:05.006Z'), \
         (19, 23, 1, 1, 'Line one\nline two\n\nפסוק 𝄞', '2026-02-02T03:04:05.006Z', \
          '2026-02-02T03:04:05.006Z'), \
         (45, 8, 28, 28, 'Newest, deleted.', '2026-06-02T03:04:05.006Z', \
          '2026-06-02T03:04:05.006Z')",
        "DELETE FROM notes WHERE id IN (2, 4)",
        "INSERT INTO app_settings (key, value) VALUES ('ai_findings_enabled', 'true')",
        "INSERT INTO reading_position (id, translation_id, book_id, chapter) VALUES (1, 2, 19, 23)",
        "INSERT INTO reading_place (id, translation_id, book_id, chapter, verse, offset_px) \
         VALUES (1, 2, 19, 23, 4, 12.5)",
    ] {
        sqlx::query(sql)
            .execute(pool.write().pool())
            .await
            .unwrap_or_else(|e| panic!("{sql}: {e}"));
    }
    insert_old_rule_findings(pool.write()).await;
    sqlx::query("DELETE FROM ai_findings WHERE id = (SELECT MAX(id) FROM ai_findings)")
        .execute(pool.write().pool())
        .await
        .expect("delete the newest finding");
}

/// A new database gets the current schema and version at once, in one go;
/// running again changes nothing.
#[tokio::test]
async fn a_new_database_gets_the_current_schema_and_version() {
    let db = TempDb::new("migrate_new");
    let pool = db.connect().await;
    assert_eq!(user_version(pool.read()).await, 0);
    migrate(pool.write()).await.unwrap();
    assert_eq!(user_version(pool.read()).await, SCHEMA_VERSION);
    let once = snapshot(pool.read()).await;
    migrate(pool.write()).await.unwrap();
    assert_eq!(snapshot(pool.read()).await, once);
}

/// `STEPS` runs up to `SCHEMA_VERSION`, which a new database gets.
#[test]
fn the_steps_end_at_the_schema_version() {
    for (i, step) in super::migrate::STEPS.iter().enumerate() {
        assert_eq!(step.to, i as i64 + 2, "step {i}");
    }
    let last = super::migrate::STEPS.last().map_or(1, |step| step.to);
    assert_eq!(last, SCHEMA_VERSION);
    assert!((1..=SCHEMA_VERSION).contains(&OLDEST_READABLE_SCHEMA_VERSION));
}

/// A statement `schema_1.sql` mustn't hold is named by its first line,
/// cut short, not given whole: the error can reach the "couldn't start"
/// dialog, which must stay readable.
#[test]
fn a_bad_statement_is_named_in_one_short_line() {
    let long = format!(
        "DROP TABLE {} -- a comment\nCREATE TABLE books (\n    id INTEGER\n)",
        "x".repeat(200)
    );
    let message = if_not_exists(&long).unwrap_err().to_string();
    assert!(!message.contains('\n'), "{message:?}");
    assert!(!message.contains("CREATE TABLE books"), "{message:?}");
    assert!(message.ends_with('…'), "{message:?}");
    assert!(message.chars().count() < HEAD_CHARS + 80, "{message:?}");
    assert_eq!(head("  SELECT 1;\nSELECT 2;"), "SELECT 1;");
    assert_eq!(head(&"é".repeat(HEAD_CHARS)), "é".repeat(HEAD_CHARS));
}

/// What `schema_1.sql` makes is what it says: `sqlite_master` holds each
/// of its statements, in its order, and besides them only the tables the
/// search index makes for itself and the indexes SQLite makes for a
/// UNIQUE or PRIMARY KEY (with no SQL).
#[tokio::test]
async fn schema_1_makes_what_it_says() {
    let db = TempDb::new("migrate_schema_1");
    let pool = db.connect().await;
    complete_schema_1(pool.write()).await.unwrap();
    assert_eq!(user_version(pool.read()).await, 1);
    let made: Vec<String> = sqlx::query_scalar(
        "SELECT sql FROM sqlite_master WHERE sql IS NOT NULL \
         AND name NOT LIKE 'verse\\_search\\_%' ESCAPE '\\' ORDER BY rowid",
    )
    .fetch_all(pool.read())
    .await
    .unwrap();
    assert_eq!(made, schema_1().collect::<Vec<_>>());
    let own: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE name LIKE 'verse\\_search\\_%' ESCAPE '\\' \
         ORDER BY name",
    )
    .fetch_all(pool.read())
    .await
    .unwrap();
    assert_eq!(
        own,
        [
            "verse_search_config",
            "verse_search_data",
            "verse_search_docsize",
            "verse_search_idx"
        ]
    );
}

/// A version-1 database brought up to date has a new database's schema
/// (each object's SQL the same but for whitespace, and the quotes a
/// rebuild's rename leaves around a table's name), the same version, and
/// every one of the reader's rows as it was; a second run changes nothing.
#[tokio::test]
async fn a_version_1_database_is_brought_up_to_date_with_every_row() {
    let (_db, pool) = schema_1_database("migrate_v1").await;
    readers_records(&pool).await;
    let before = readers_rows(pool.read()).await;
    prepare(pool.write()).await.unwrap();
    assert_eq!(user_version(pool.read()).await, SCHEMA_VERSION);
    assert_eq!(readers_rows(pool.read()).await, before);

    let new = TempDb::new("migrate_v1_new");
    let new_pool = new.connect().await;
    migrate(new_pool.write()).await.unwrap();
    assert_eq!(
        normalized_schema(pool.read()).await,
        normalized_schema(new_pool.read()).await
    );

    let once = snapshot(pool.read()).await;
    prepare(pool.write()).await.unwrap();
    assert_eq!(snapshot(pool.read()).await, once);
}

/// A database from before versions were kept (version 0 with tables, some
/// of version 1's missing, as a 2.25.x one lacks `ai_findings` and
/// `app_settings`) gets the rest of version 1 and then every step, its
/// rows kept.
#[tokio::test]
async fn a_database_from_before_versions_gets_version_1_then_the_steps() {
    let (_db, pool) = schema_1_database("migrate_pre_versions").await;
    readers_records(&pool).await;
    for sql in [
        "DROP TABLE ai_findings",
        "DROP TABLE app_settings",
        "DROP TABLE translation_data_version",
        "PRAGMA user_version = 0",
    ] {
        sqlx::query(sql).execute(pool.write().pool()).await.unwrap();
    }
    let mut before = Vec::new();
    for table in READERS_TABLES {
        if !["ai_findings", "app_settings"].contains(&table) {
            before.push((table, rows(pool.read(), table).await));
        }
    }
    migrate(pool.write()).await.unwrap();
    assert_eq!(user_version(pool.read()).await, SCHEMA_VERSION);
    let after = readers_rows(pool.read()).await;
    for (table, rows) in &before {
        assert_eq!(
            after.iter().find(|(t, _)| t == table).map(|(_, r)| r),
            Some(rows),
            "{table}"
        );
    }

    let new = TempDb::new("migrate_pre_versions_new");
    let new_pool = new.connect().await;
    migrate(new_pool.write()).await.unwrap();
    assert_eq!(
        normalized_schema(pool.read()).await,
        normalized_schema(new_pool.read()).await
    );
}

/// A newer app's version is kept, and nothing at all is changed: not even
/// a table version 1 had that this database lacks.
#[tokio::test]
async fn a_newer_version_is_kept_and_nothing_changes() {
    let (_db, pool) = schema_1_database("migrate_newer").await;
    readers_records(&pool).await;
    for sql in ["DROP TABLE app_settings", "PRAGMA user_version = 7"] {
        sqlx::query(sql).execute(pool.write().pool()).await.unwrap();
    }
    let before = snapshot(pool.read()).await;
    migrate(pool.write()).await.unwrap();
    assert_eq!(snapshot(pool.read()).await, before);
    assert_eq!(user_version(pool.read()).await, 7);
}

/// `notes` again as it is, then `highlights` with a definition its rows
/// can't be copied into (no `chapter` column), for a step that fails
/// part-way.
const FAILS_PART_WAY: [Rebuild; 2] = [
    Rebuild {
        table: "notes",
        create: "CREATE TABLE notes (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            book_id INTEGER NOT NULL REFERENCES books(id),
            chapter INTEGER NOT NULL,
            verse_start INTEGER NOT NULL,
            verse_end INTEGER NOT NULL,
            body TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        )",
        columns: &[
            "id",
            "book_id",
            "chapter",
            "verse_start",
            "verse_end",
            "body",
            "created_at",
            "updated_at",
        ],
        autoincrement: true,
    },
    Rebuild {
        table: "highlights",
        create: "CREATE TABLE highlights (book_id INTEGER, verse INTEGER, color TEXT)",
        columns: &[
            "rowid",
            "book_id",
            "chapter",
            "verse",
            "color",
            "created_at",
            "updated_at",
        ],
        autoincrement: false,
    },
];

/// A step that fails part-way, after one table is rebuilt, leaves the
/// database exactly as it was, at the version before it, every row, index
/// and number; the app goes on (that version is one it reads), with the
/// writing connection's foreign keys on again.
#[tokio::test]
async fn a_step_that_fails_part_way_changes_nothing() {
    let (db, pool) = schema_1_database("migrate_step_fails").await;
    readers_records(&pool).await;
    let before = snapshot(pool.read()).await;
    let fails = [Step {
        to: 2,
        keep_copy: false,
        compatible_both_ways: true,
        rebuilds: &FAILS_PART_WAY,
    }];
    migrate_with(pool.write(), &fails).await.unwrap();
    assert_eq!(snapshot(pool.read()).await, before);
    assert_eq!(user_version(pool.read()).await, 1);
    let foreign_keys: bool = sqlx::query_scalar("PRAGMA foreign_keys")
        .fetch_one(pool.write().pool())
        .await
        .unwrap();
    assert!(foreign_keys);

    // And as the next launch sees it.
    pool.close().await;
    let pool = db.connect().await;
    assert_eq!(snapshot(pool.read()).await, before);
}

/// John 3:`verse`.
fn john_3(verse: i64) -> Passage {
    Passage {
        book_id: 43,
        chapter: 3,
        verse_start: verse,
        verse_end: verse,
    }
}

/// `table`'s next-id record (`sqlite_sequence`), if it has one.
async fn sequence(pool: &SqlitePool, table: &str) -> Option<i64> {
    sqlx::query_scalar("SELECT seq FROM sqlite_sequence WHERE name = $1")
        .bind(table)
        .fetch_optional(pool)
        .await
        .expect("read sqlite_sequence")
}

/// Where `migrate` keeps the copy of the database at `path` from before
/// schema 2.
fn copy_before_schema_2(path: &std::path::Path) -> std::path::PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".before-schema-2");
    name.into()
}

/// Schema 2 on a version-1 database: every row of the reader's, byte for
/// byte (ids, dates and the hidden rowids of highlights included), every
/// bundled row too, nothing a foreign key or `integrity_check` would
/// object to, each table's next id past the largest it holds, and, beside
/// the database, a copy of it as it was, at version 1, which the next
/// launch leaves alone.
#[tokio::test]
async fn schema_2_keeps_every_row_and_a_copy_of_the_database_before_it() {
    let (db, pool) = schema_1_database("schema_2_rows").await;
    readers_records(&pool).await;
    let before = snapshot(pool.read()).await;
    let copy = copy_before_schema_2(&db.path());
    assert!(!copy.exists());

    migrate(pool.write()).await.unwrap();
    assert_eq!(user_version(pool.read()).await, 2);
    let after = snapshot(pool.read()).await;
    let changed = [
        "bookmarks",
        "highlights",
        "notes",
        "ai_findings",
        "sqlite_sequence",
    ];
    for (table, rows) in &before.2 {
        let now = after.2.iter().find(|(t, _)| t == table).map(|(_, r)| r);
        assert_eq!(now, Some(rows), "{table}");
    }
    // The only table that's new is SQLite's record of the next ids.
    let tables = |snapshot: &(_, _, Vec<(String, Vec<String>)>)| -> Vec<String> {
        snapshot.2.iter().map(|(t, _)| t.clone()).collect()
    };
    let mut expected = tables(&before);
    expected.push("sqlite_sequence".to_string());
    expected.sort();
    assert_eq!(tables(&after), expected);
    // Every object but the four tables is as it was, SQL and all.
    let unchanged = |schema: &Vec<(String, String, String, Option<String>)>| {
        schema
            .iter()
            .filter(|(kind, name, ..)| !(kind == "table" && changed.contains(&name.as_str())))
            .cloned()
            .collect::<Vec<_>>()
    };
    assert_eq!(unchanged(&after.0), unchanged(&before.0));

    for (table, largest) in [("bookmarks", 3), ("notes", 3), ("ai_findings", 10)] {
        assert_eq!(sequence(pool.read(), table).await, Some(largest), "{table}");
    }
    let broken: Vec<(String, Option<i64>, String, i64)> =
        sqlx::query_as("SELECT * FROM pragma_foreign_key_check")
            .fetch_all(pool.read())
            .await
            .unwrap();
    assert_eq!(broken, []);
    let integrity: Vec<String> = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_all(pool.read())
        .await
        .unwrap();
    assert_eq!(integrity, ["ok"]);

    // The copy is the database as it was: version 1, every row.
    let copied = open_read_only(&copy).await.unwrap();
    assert_eq!(snapshot(&copied).await, before);
    copied.close().await;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&copy).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
    let mut partial = copy.as_os_str().to_owned();
    partial.push(".partial");
    assert!(!std::path::Path::new(&partial).exists());

    let written = std::fs::metadata(&copy).unwrap().modified().unwrap();
    prepare(pool.write()).await.unwrap();
    assert_eq!(snapshot(pool.read()).await, after);
    assert_eq!(
        std::fs::metadata(&copy).unwrap().modified().unwrap(),
        written
    );
}

/// A copy an earlier launch kept (its step then failed) is the database as
/// it was before; it's never written over.
#[tokio::test]
async fn a_copy_already_there_is_kept() {
    let (db, pool) = schema_1_database("schema_2_copy_there").await;
    let copy = copy_before_schema_2(&db.path());
    std::fs::write(&copy, b"an earlier launch's copy").unwrap();
    migrate(pool.write()).await.unwrap();
    assert_eq!(user_version(pool.read()).await, 2);
    assert_eq!(std::fs::read(&copy).unwrap(), b"an earlier launch's copy");
}

/// The collision GG32-08 describes, after schema 2: the newest note,
/// bookmark and finding deleted, then one added, gets an id of its own,
/// never the deleted one's, on a database upgraded from version 1 and on a
/// new one alike.
#[tokio::test]
async fn an_id_is_never_handed_out_twice() {
    let (_v1, upgraded) = schema_1_database("schema_2_ids_upgraded").await;
    readers_records(&upgraded).await;
    migrate(upgraded.write()).await.unwrap();
    let (_new, new) = fresh_database("schema_2_ids_new").await;
    for pool in [&upgraded, &new] {
        let a = add_note(pool.write(), &john_3(20), "A.", None)
            .await
            .unwrap();
        delete_note(pool.write(), a.id, &a.created_at)
            .await
            .unwrap()
            .unwrap();
        let b = add_note(pool.write(), &john_3(21), "B.", None)
            .await
            .unwrap();
        assert!(b.id > a.id, "note B {} after A {}", b.id, a.id);

        let a = add_bookmark(pool.write(), 43, 3, 20, 20).await.unwrap();
        assert!(
            remove_bookmark(pool.write(), a.id, &a.created_at)
                .await
                .unwrap()
        );
        let b = add_bookmark(pool.write(), 43, 3, 21, 21).await.unwrap();
        assert!(b.id > a.id, "bookmark B {} after A {}", b.id, a.id);

        let finding = |verse, request: &str| NewFinding {
            passage: john_3(verse),
            agent: "Claude".to_string(),
            request: request.to_string(),
            created_at: None,
        };
        add_findings(pool.write(), &[finding(20, "love")])
            .await
            .unwrap();
        let a = all_findings(pool.read())
            .await
            .unwrap()
            .into_iter()
            .max_by_key(|f| f.id)
            .unwrap();
        remove_finding(pool.write(), a.id, &a.created_at)
            .await
            .unwrap()
            .unwrap();
        add_findings(pool.write(), &[finding(21, "light")])
            .await
            .unwrap();
        let b = all_findings(pool.read())
            .await
            .unwrap()
            .into_iter()
            .max_by_key(|f| f.id)
            .unwrap();
        assert!(b.id > a.id, "finding B {} after A {}", b.id, a.id);
        assert_eq!(b.request, "light");
    }
}

/// The product's limits are Rust's on a database upgraded from version 1:
/// its tables hold no CHECK for them (SQL alone takes a 10,001-character
/// note, an unknown color and a 61-character request), and the app refuses
/// each before any SQL, while taking everything up to the limit.
#[tokio::test]
async fn the_limits_are_rusts_on_an_upgraded_database() {
    let (_db, pool) = schema_1_database("schema_2_limits").await;
    migrate(pool.write()).await.unwrap();
    for (table, gone) in [
        ("notes", "length(body)"),
        ("highlights", "color IN"),
        ("ai_findings", "length(agent)"),
        ("ai_findings", "length(request)"),
    ] {
        let sql: String = sqlx::query_scalar("SELECT sql FROM sqlite_master WHERE name = $1")
            .bind(table)
            .fetch_one(pool.read())
            .await
            .unwrap();
        assert!(!sql.contains(gone), "{table}: {sql}");
    }
    let mut tx = pool.write().pool().begin().await.unwrap();
    for sql in [
        "INSERT INTO notes (book_id, chapter, verse_start, verse_end, body) \
         VALUES (43, 3, 16, 16, substr(hex(zeroblob(5001)), 1, 10001))",
        "INSERT INTO highlights (book_id, chapter, verse, color) VALUES (43, 3, 16, 'orange')",
        "INSERT INTO ai_findings (book_id, chapter, verse_start, verse_end, agent, request) \
         VALUES (43, 3, 16, 16, 'Claude', substr(hex(zeroblob(31)), 1, 61))",
    ] {
        sqlx::query(sql).execute(&mut *tx).await.unwrap();
    }
    tx.rollback().await.unwrap();

    let longest = "\u{1d11e}".repeat(MAX_NOTE_CHARS);
    assert!(matches!(
        add_note(pool.write(), &john_3(16), &format!("{longest}x"), None).await,
        Err(NoteError::Body(NoteBodyError::TooLong { chars })) if chars == MAX_NOTE_CHARS + 1
    ));
    add_note(pool.write(), &john_3(16), &longest, None)
        .await
        .unwrap();

    assert!("orange".parse::<HighlightColor>().is_err());
    for color in HighlightColor::ALL {
        set_highlights(pool.write(), 43, 3, 16, 16, color)
            .await
            .unwrap();
    }

    let request = |chars| NewFinding {
        passage: john_3(16),
        agent: "Claude".to_string(),
        request: "r".repeat(chars),
        created_at: None,
    };
    assert!(matches!(
        add_findings(pool.write(), &[request(MAX_REQUEST_CHARS + 1)]).await,
        Err(FindingsError::Request(RequestError::TooLong { chars })) if chars == MAX_REQUEST_CHARS + 1
    ));
    add_findings(pool.write(), &[request(MAX_REQUEST_CHARS)])
        .await
        .unwrap();
    // An Undo puts back a stored record without today's label rule, but
    // never one longer than any stored record can be.
    let removed = |agent: usize, request: usize| RemovedFinding {
        passage: john_3(17),
        agent: "a".repeat(agent),
        request: "r".repeat(request),
        created_at: "2026-01-02T03:04:05.006Z".to_string(),
    };
    for (agent, request) in [
        (MAX_STORED_AGENT_CHARS + 1, 5),
        (5, MAX_REQUEST_CHARS + 1),
        (0, 5),
        (5, 0),
    ] {
        assert!(
            restore_findings(pool.write(), &[removed(agent, request)])
                .await
                .is_err(),
            "{agent}, {request}"
        );
    }
    assert_eq!(
        restore_findings(
            pool.write(),
            &[removed(MAX_STORED_AGENT_CHARS, MAX_REQUEST_CHARS)]
        )
        .await
        .unwrap(),
        1
    );
}

/// An import that replaces everything (`reader_data::apply_import`) brings
/// back no old id: the file's records are new ones, numbered past every
/// record the database has held, even with the same passages and dates.
#[tokio::test]
async fn an_import_that_replaces_everything_brings_back_no_old_id() {
    use super::reader_data::{FindingsImport, NewBookmark, NewNote, apply_import};
    let (_v1, pool) = schema_1_database("schema_2_replace").await;
    readers_records(&pool).await;
    migrate(pool.write()).await.unwrap();
    let largest = |table: &'static str| {
        let pool = pool.clone();
        async move {
            sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(format!(
                "SELECT MAX(id) FROM {table}"
            )))
            .fetch_one(pool.read())
            .await
            .unwrap()
        }
    };
    let before = (
        largest("bookmarks").await,
        largest("notes").await,
        largest("ai_findings").await,
    );
    let at = Some("2026-01-02T03:04:05.006Z".to_string());
    let bookmarks = [NewBookmark {
        book_id: 43,
        chapter: 3,
        verse_start: 16,
        verse_end: 16,
        created_at: at.clone(),
    }];
    let notes = [NewNote {
        book_id: 43,
        chapter: 3,
        verse_start: 16,
        verse_end: 16,
        body: "For God so loved.".to_string(),
        created_at: at.clone(),
        updated_at: at.clone(),
    }];
    let findings = [NewFinding {
        passage: john_3(16),
        agent: "Claude".to_string(),
        request: "love".to_string(),
        created_at: at,
    }];
    apply_import(
        pool.write(),
        &bookmarks,
        Some(&[]),
        Some(&notes),
        Some(FindingsImport {
            findings: &findings,
            enabled: None,
        }),
        true,
        None,
    )
    .await
    .unwrap();
    let after = (
        largest("bookmarks").await,
        largest("notes").await,
        largest("ai_findings").await,
    );
    assert_eq!(
        (after.0 - before.0, after.1 - before.1, after.2 - before.2),
        (1, 1, 1),
        "{after:?} after {before:?}"
    );
}

/// 2.32.2's own statements on the reader's tables, verbatim from its source
/// (git 4024a26: `db/bookmarks.rs`, `notes.rs`, `highlights.rs`,
/// `findings.rs` and `reader_data.rs`, each `note_columns!()` and
/// `finding_columns!()` written out), with the parameters a test gives
/// them. What an app or CLI of 2.32.x runs on a database this version has
/// brought to schema 2.
mod as_2_32_2 {
    pub const ADD_BOOKMARK: &str = "INSERT INTO bookmarks (book_id, chapter, verse_start, verse_end) VALUES ($1, $2, $3, $4) ON CONFLICT (book_id, chapter, verse_start, verse_end) DO NOTHING";
    pub const FIND_BOOKMARK: &str = "SELECT id, book_id, chapter, verse_start, verse_end, created_at FROM bookmarks WHERE book_id = $1 AND chapter = $2 AND verse_start = $3 AND verse_end = $4";
    pub const LIST_BOOKMARKS: &str = "SELECT id, book_id, chapter, verse_start, verse_end, created_at FROM bookmarks ORDER BY created_at DESC, id DESC";
    pub const REMOVE_BOOKMARK: &str = "DELETE FROM bookmarks WHERE id = $1 AND created_at = $2";
    pub const NOTE_TOTALS: &str =
        "SELECT COUNT(*), COALESCE(SUM(length(body)), 0) FROM notes WHERE $1 IS NULL OR id <> $1";
    pub const ADD_NOTE: &str = "INSERT INTO notes (book_id, chapter, verse_start, verse_end, body, created_at, updated_at) VALUES ($1, $2, $3, $4, $5, COALESCE($6, strftime('%Y-%m-%dT%H:%M:%fZ', 'now')), COALESCE($7, $6, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))) RETURNING id, book_id, chapter, verse_start, verse_end, body, created_at, updated_at";
    pub const NOTE_LENGTH: &str =
        "SELECT length(body) FROM notes WHERE id = $1 AND created_at = $2";
    pub const UPDATE_NOTE: &str = "UPDATE notes SET updated_at = CASE WHEN body = $2 THEN updated_at ELSE strftime('%Y-%m-%dT%H:%M:%fZ', 'now') END, body = $2 WHERE id = $1 RETURNING id, book_id, chapter, verse_start, verse_end, body, created_at, updated_at";
    pub const DELETE_NOTE: &str = "DELETE FROM notes WHERE id = $1 AND created_at = $2 RETURNING id, book_id, chapter, verse_start, verse_end, body, created_at, updated_at";
    pub const NOTE: &str = "SELECT id, book_id, chapter, verse_start, verse_end, body, created_at, updated_at FROM notes WHERE id = $1 AND created_at = $2";
    pub const CHAPTER_NOTES: &str = "SELECT id, book_id, chapter, verse_start, verse_end, body, created_at, updated_at FROM notes WHERE book_id = $1 AND chapter = $2";
    pub const NOTE_PLACES: &str =
        "SELECT book_id, chapter, created_at FROM notes ORDER BY book_id, chapter, id";
    pub const SET_HIGHLIGHT: &str = "INSERT INTO highlights (book_id, chapter, verse, color) VALUES ($1, $2, $3, $4) ON CONFLICT (book_id, chapter, verse) DO UPDATE SET color = excluded.color, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE highlights.color <> excluded.color";
    pub const REMOVE_HIGHLIGHTS: &str =
        "DELETE FROM highlights WHERE book_id = $1 AND chapter = $2 AND verse BETWEEN $3 AND $4";
    pub const LIST_HIGHLIGHTS: &str = "SELECT book_id, chapter, verse, color, created_at, updated_at FROM highlights ORDER BY book_id, chapter, verse";
    pub const FINDINGS_READY: &str = "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name IN ('ai_findings', 'app_settings')";
    pub const SET_ENABLED: &str = "INSERT INTO app_settings (key, value) VALUES ($1, $2) ON CONFLICT (key) DO UPDATE SET value = excluded.value";
    pub const FINDING_COUNT: &str = "SELECT COUNT(*) FROM ai_findings";
    pub const ADD_FINDING: &str = "INSERT INTO ai_findings (book_id, chapter, verse_start, verse_end, agent, request, created_at) VALUES ($1, $2, $3, $4, $5, $6, COALESCE($7, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))) ON CONFLICT (book_id, chapter, verse_start, verse_end, request) DO NOTHING";
    pub const RESTORE_FINDING: &str = "INSERT INTO ai_findings (book_id, chapter, verse_start, verse_end, agent, request, created_at) VALUES ($1, $2, $3, $4, $5, $6, $7) ON CONFLICT (book_id, chapter, verse_start, verse_end, request) DO NOTHING";
    pub const ALL_FINDINGS: &str = "SELECT id, book_id, chapter, verse_start, verse_end, agent, request, created_at FROM ai_findings ORDER BY book_id, chapter, verse_start, verse_end, created_at, id";
    pub const REMOVE_FINDING: &str = "DELETE FROM ai_findings WHERE id = $1 AND created_at = $2 RETURNING id, book_id, chapter, verse_start, verse_end, agent, request, created_at";
    pub const REMOVE_REQUEST: &str = "DELETE FROM ai_findings WHERE request = $1 RETURNING id, book_id, chapter, verse_start, verse_end, agent, request, created_at";
    pub const REPLACE_BOOKMARKS: &str = "DELETE FROM bookmarks";
    pub const IMPORT_BOOKMARK: &str = "INSERT INTO bookmarks (book_id, chapter, verse_start, verse_end, created_at) VALUES ($1, $2, $3, $4, COALESCE($5, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))) ON CONFLICT (book_id, chapter, verse_start, verse_end) DO NOTHING";
    pub const REPLACE_HIGHLIGHTS: &str = "DELETE FROM highlights";
    pub const IMPORT_HIGHLIGHT: &str = "INSERT INTO highlights (book_id, chapter, verse, color, created_at, updated_at) VALUES ($1, $2, $3, $4, COALESCE($5, strftime('%Y-%m-%dT%H:%M:%fZ', 'now')), COALESCE($6, $5, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))) ON CONFLICT (book_id, chapter, verse) DO NOTHING";
    pub const REPLACE_NOTES: &str = "DELETE FROM notes";
    pub const IMPORT_NOTE: &str = "INSERT INTO notes (book_id, chapter, verse_start, verse_end, body, created_at, updated_at) VALUES ($1, $2, $3, $4, $5, COALESCE($6, strftime('%Y-%m-%dT%H:%M:%fZ', 'now')), COALESCE($7, $6, strftime('%Y-%m-%dT%H:%M:%fZ', 'now')))";
    pub const REPLACE_FINDINGS: &str = "DELETE FROM ai_findings";
}

/// 2.32.x on a database this version brought to schema 2 (AF8), with its
/// own SQL ([`as_2_32_2`]): its `migrate` (version 1's statements, each `IF
/// NOT EXISTS`, and the version written only below 1) changes nothing; it
/// reads every record; it adds, changes and removes bookmarks, notes,
/// highlights, findings and the switch; an import that replaces everything
/// works; and the ids it's given are new ones. Then this version opens the
/// database again and finds everything 2.32.2 left, as it left it.
#[tokio::test]
async fn version_2_32_2_reads_and_writes_an_upgraded_database() {
    use as_2_32_2::*;
    // A note's columns, or a finding's: five numbers, then three strings.
    type Row = (i64, i64, i64, i64, i64, String, String, String);
    let (db, pool) = schema_1_database("compat_2_32_2").await;
    readers_records(&pool).await;
    migrate(pool.write()).await.unwrap();
    pool.close().await;

    // 2.32.2 starts: its `migrate`.
    let old = db.connect().await;
    let w = old.write().pool();
    let upgraded = snapshot(old.read()).await;
    for sql in schema_1() {
        sqlx::query(sqlx::AssertSqlSafe(if_not_exists(sql).unwrap()))
            .execute(w)
            .await
            .unwrap();
    }
    // It writes the version only when it's below 1.
    assert!(user_version(old.read()).await >= 1);
    assert_eq!(snapshot(old.read()).await, upgraded);
    assert_eq!(user_version(old.read()).await, 2);

    // Its reads.
    let bookmarks: Vec<(i64, i64, i64, i64, i64, String)> =
        sqlx::query_as(LIST_BOOKMARKS).fetch_all(w).await.unwrap();
    assert_eq!(bookmarks.iter().map(|b| b.0).collect::<Vec<_>>(), [3, 1]);
    let highlights: Vec<(i64, i64, i64, String, String, String)> =
        sqlx::query_as(LIST_HIGHLIGHTS).fetch_all(w).await.unwrap();
    assert_eq!(highlights.len(), 2);
    let places: Vec<(i64, i64, String)> = sqlx::query_as(NOTE_PLACES).fetch_all(w).await.unwrap();
    assert_eq!(places.len(), 2);
    let ready: i64 = sqlx::query_scalar(FINDINGS_READY)
        .fetch_one(w)
        .await
        .unwrap();
    assert_eq!(ready, 2);
    let findings: Vec<Row> = sqlx::query_as(ALL_FINDINGS).fetch_all(w).await.unwrap();
    assert_eq!(findings.len(), 10);
    let count: i64 = sqlx::query_scalar(FINDING_COUNT)
        .fetch_one(w)
        .await
        .unwrap();
    assert_eq!(count, 10);

    // Its writes. A bookmark added, found, listed and removed; another kept.
    for verse in [19, 20] {
        let added = sqlx::query(ADD_BOOKMARK)
            .bind(43)
            .bind(3)
            .bind(verse)
            .bind(verse)
            .execute(w)
            .await
            .unwrap();
        assert_eq!(added.rows_affected(), 1);
    }
    let found: (i64, i64, i64, i64, i64, String) = sqlx::query_as(FIND_BOOKMARK)
        .bind(43)
        .bind(3)
        .bind(19)
        .bind(19)
        .fetch_one(w)
        .await
        .unwrap();
    assert!(found.0 > 3, "a new id: {}", found.0);
    let removed = sqlx::query(REMOVE_BOOKMARK)
        .bind(found.0)
        .bind(&found.5)
        .execute(w)
        .await
        .unwrap();
    assert_eq!(removed.rows_affected(), 1);

    // A note added, read, changed and deleted; another kept.
    let totals: (i64, i64) = sqlx::query_as(NOTE_TOTALS)
        .bind(None::<i64>)
        .fetch_one(w)
        .await
        .unwrap();
    assert_eq!(totals.0, 2);
    let mut added: Vec<Row> = Vec::new();
    for body in ["2.32.2's note.", "2.32.2's other note."] {
        added.push(
            sqlx::query_as(ADD_NOTE)
                .bind(43)
                .bind(3)
                .bind(19)
                .bind(19)
                .bind(body)
                .bind(None::<String>)
                .bind(None::<String>)
                .fetch_one(w)
                .await
                .unwrap(),
        );
    }
    // Past the largest id there (3): 4 was deleted before the upgrade,
    // which no view still open can hold.
    assert!(added[0].0 > 3 && added[1].0 > added[0].0, "{added:?}");
    let (id, created_at) = (added[0].0, added[0].6.clone());
    let length: i64 = sqlx::query_scalar(NOTE_LENGTH)
        .bind(id)
        .bind(&created_at)
        .fetch_one(w)
        .await
        .unwrap();
    assert_eq!(length, 14);
    let changed: Row = sqlx::query_as(UPDATE_NOTE)
        .bind(id)
        .bind("2.32.2's note, changed.")
        .fetch_one(w)
        .await
        .unwrap();
    assert_eq!(changed.5, "2.32.2's note, changed.");
    let one: Option<Row> = sqlx::query_as(NOTE)
        .bind(id)
        .bind(&created_at)
        .fetch_optional(w)
        .await
        .unwrap();
    assert_eq!(one, Some(changed.clone()));
    let deleted: Row = sqlx::query_as(DELETE_NOTE)
        .bind(id)
        .bind(&created_at)
        .fetch_one(w)
        .await
        .unwrap();
    assert_eq!(deleted, changed);
    let chapter: Vec<Row> = sqlx::query_as(CHAPTER_NOTES)
        .bind(43)
        .bind(3)
        .fetch_all(w)
        .await
        .unwrap();
    assert_eq!(chapter.len(), 2);

    // Highlights set, recolored and removed.
    for (verse, color) in [(19, "green"), (19, "blue"), (20, "yellow")] {
        sqlx::query(SET_HIGHLIGHT)
            .bind(43)
            .bind(3)
            .bind(verse)
            .bind(color)
            .execute(w)
            .await
            .unwrap();
    }
    let removed = sqlx::query(REMOVE_HIGHLIGHTS)
        .bind(43)
        .bind(3)
        .bind(20)
        .bind(20)
        .execute(w)
        .await
        .unwrap();
    assert_eq!(removed.rows_affected(), 1);

    // Findings: the switch, a save, a removal, its Undo, a request removed.
    sqlx::query(SET_ENABLED)
        .bind("ai_findings_enabled")
        .bind("on")
        .execute(w)
        .await
        .unwrap();
    for (verse, request) in [(19, "light"), (20, "light"), (21, "darkness")] {
        let saved = sqlx::query(ADD_FINDING)
            .bind(43)
            .bind(3)
            .bind(verse)
            .bind(verse)
            .bind("Claude")
            .bind(request)
            .bind(None::<String>)
            .execute(w)
            .await
            .unwrap();
        assert_eq!(saved.rows_affected(), 1);
    }
    let all: Vec<Row> = sqlx::query_as(ALL_FINDINGS).fetch_all(w).await.unwrap();
    let newest = all.iter().max_by_key(|f| f.0).unwrap().clone();
    assert!(newest.0 > 10, "a new id: {}", newest.0);
    let gone: Row = sqlx::query_as(REMOVE_FINDING)
        .bind(newest.0)
        .bind(&newest.7)
        .fetch_one(w)
        .await
        .unwrap();
    assert_eq!(gone, newest);
    let back = sqlx::query(RESTORE_FINDING)
        .bind(gone.1)
        .bind(gone.2)
        .bind(gone.3)
        .bind(gone.4)
        .bind(&gone.5)
        .bind(&gone.6)
        .bind(&gone.7)
        .execute(w)
        .await
        .unwrap();
    assert_eq!(back.rows_affected(), 1);
    let light: Vec<Row> = sqlx::query_as(REMOVE_REQUEST)
        .bind("light")
        .fetch_all(w)
        .await
        .unwrap();
    assert_eq!(light.len(), 2);
    old.close().await;

    // This version opens it again: everything 2.32.2 left, as it left it,
    // and still at schema 2.
    let pool = db.connect().await;
    let left = readers_rows(pool.read()).await;
    prepare(pool.write()).await.unwrap();
    assert_eq!(user_version(pool.read()).await, 2);
    assert_eq!(readers_rows(pool.read()).await, left);
    let restored = all_findings(pool.read()).await.unwrap();
    assert!(
        restored
            .iter()
            .any(|f| f.request == "darkness" && f.id > newest.0)
    );
    pool.close().await;

    // 2.32.2 again: an import that replaces everything, then this version.
    let old = db.connect().await;
    let w = old.write().pool();
    let largest: (i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT MAX(id) FROM bookmarks), (SELECT MAX(id) FROM notes), \
         (SELECT MAX(id) FROM ai_findings)",
    )
    .fetch_one(w)
    .await
    .unwrap();
    let mut tx = w.begin().await.unwrap();
    for sql in [
        REPLACE_BOOKMARKS,
        REPLACE_HIGHLIGHTS,
        REPLACE_NOTES,
        REPLACE_FINDINGS,
    ] {
        sqlx::query(sql).execute(&mut *tx).await.unwrap();
    }
    sqlx::query(IMPORT_BOOKMARK)
        .bind(1)
        .bind(1)
        .bind(1)
        .bind(1)
        .bind("2026-01-01T00:00:00.000Z")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query(IMPORT_HIGHLIGHT)
        .bind(1)
        .bind(1)
        .bind(1)
        .bind("pink")
        .bind("2026-01-01T00:00:00.000Z")
        .bind(None::<String>)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query(IMPORT_NOTE)
        .bind(1)
        .bind(1)
        .bind(1)
        .bind(1)
        .bind("In the beginning.")
        .bind("2026-01-01T00:00:00.000Z")
        .bind(None::<String>)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query(ADD_FINDING)
        .bind(1)
        .bind(1)
        .bind(1)
        .bind(1)
        .bind("Claude")
        .bind("creation")
        .bind("2026-01-01T00:00:00.000Z")
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let ids: (i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT id FROM bookmarks), (SELECT id FROM notes), (SELECT id FROM ai_findings)",
    )
    .fetch_one(w)
    .await
    .unwrap();
    assert!(
        ids.0 > largest.0 && ids.1 > largest.1 && ids.2 > largest.2,
        "{ids:?} after {largest:?}: a replace brings back no old id"
    );
    let left = readers_rows(old.read()).await;
    old.close().await;

    let pool = db.connect().await;
    prepare(pool.write()).await.unwrap();
    assert_eq!(readers_rows(pool.read()).await, left);
    assert_eq!(user_version(pool.read()).await, 2);
}

/// Go back's data check, as a table: an app built for schema P can use a
/// database at schema D if D isn't past P, or if every step past P up to
/// D is compatible both ways. With today's steps, every release back to
/// schema 1 can; a step marked incompatible (injected here) makes every
/// app from before it a no, and one past it still a yes; a step this
/// build doesn't know is a no.
#[test]
fn an_older_app_can_use_the_database_only_past_compatible_steps() {
    assert!(STEPS.iter().all(|step| step.compatible_both_ways));
    for (app, database, can) in [
        (2, 2, true),
        (1, 2, true),
        (2, 1, true),
        (1, 1, true),
        // Before versions were kept: its step to 1 isn't a `Step`.
        (0, 2, false),
        // A newer app's database, past every step this build knows.
        (2, SCHEMA_VERSION + 1, false),
    ] {
        assert_eq!(
            older_app_can_use(app, database),
            can,
            "app at {app}, database at {database}"
        );
    }

    let step = |to, compatible_both_ways| Step {
        to,
        keep_copy: false,
        compatible_both_ways,
        rebuilds: &[],
    };
    let steps = [step(2, true), step(3, false), step(4, true)];
    for (app, database, can) in [
        (1, 2, true),
        (2, 2, true),
        (1, 3, false),
        (2, 3, false),
        (2, 4, false),
        (1, 4, false),
        (3, 3, true),
        (3, 4, true),
        (4, 4, true),
        (4, 3, true),
        (1, 1, true),
        (4, 5, false),
    ] {
        assert_eq!(
            can_use_with(&steps, app, database),
            can,
            "app at {app}, database at {database}"
        );
    }
}
