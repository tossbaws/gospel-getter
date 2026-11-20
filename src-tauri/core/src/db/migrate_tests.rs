//! `migrate`'s tests: a new database, every older one (version 1, before
//! versions, a newer app's), a step that fails part-way, and running twice.
//! Version-1 databases are built from the frozen schema
//! (`migrate/schema_1.sql`), never by old code.

use sqlx::SqlitePool;

use super::migrate::{
    OLDEST_READABLE_SCHEMA_VERSION, Rebuild, SCHEMA_VERSION, Step, complete_schema_1, migrate,
    migrate_with, schema_1,
};
use super::test_support::{TempDb, insert_old_rule_findings, schema_1_database};
use super::{Db, prepare};

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
