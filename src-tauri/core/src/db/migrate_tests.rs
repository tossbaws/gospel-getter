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
    OLDEST_READABLE_SCHEMA_VERSION, Rebuild, SCHEMA_VERSION, Step, complete_schema_1, migrate,
    migrate_with, schema_1,
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
