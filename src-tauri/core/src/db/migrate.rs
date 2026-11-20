//! The database's schema, and the steps that bring an older one up to it.
//!
//! The schema's version is SQLite's `PRAGMA user_version`. `migrate` reads
//! it, and whether the database has any tables, before it changes anything:
//!
//! - No tables: a new database. It gets [`SCHEMA_VERSION`]'s schema
//!   (`CURRENT`) directly, and that number, in one transaction.
//! - Tables, at 0: a database from before the version was kept (2.1.0's,
//!   say), whose tables are some of version 1's. Like one at 1 (which can
//!   also lack a table added later in version 1's life, such as 2.29.1's
//!   `translation_data_version`), it first gets each object of version 1
//!   it lacks, from `schema_1.sql`, the frozen copy of version 1, and 1 is
//!   recorded with them in one transaction.
//! - Then every step past its version runs, in order (`STEPS`), each in
//!   one write transaction together with its `user_version` write, so a
//!   step and its number land together or not at all. Apart, a crash
//!   between the two would leave a changed schema recorded as the old one,
//!   and the step would run again on tables it had already changed (an
//!   `ALTER TABLE ... ADD COLUMN` then fails with "duplicate column name"),
//!   or a number recorded for a change never made.
//! - A version above [`SCHEMA_VERSION`] is a newer app's: it's kept, and
//!   nothing here changes, so that app doesn't take the database for an
//!   older one.
//!
//! A step that fails leaves the database exactly as it was before that
//! step, at the version before it, and the next launch tries it again. The
//! app goes on if it still reads that version
//! ([`OLDEST_READABLE_SCHEMA_VERSION`]); otherwise starting fails.
//!
//! How to add a step: append a `Step` to `STEPS` whose `to` is the new
//! version, written with its own SQL, frozen like `schema_1.sql` (a later
//! step that changes the same table again writes its own, and never edits
//! an earlier one: databases out there have already run it); change
//! `CURRENT` so a new database gets the same schema directly; and raise
//! [`SCHEMA_VERSION`] to `to`. `migrate_tests`' comparison of an upgraded
//! database with a new one fails until the two agree. A table whose
//! constraints change is rebuilt (`Rebuild`: SQLite can't alter a
//! constraint in place).
//!
//! Every step so far is compatible both ways: an app or CLI of an earlier
//! version reads and writes a database this one has brought up to date,
//! and this one reads one the step hasn't reached yet. The first step that
//! isn't must wait for a released max-writer check (an app or CLI refusing
//! to write a database whose version is above the newest it knows to be
//! safe: this version and every earlier one write a database of any higher
//! version) and raise [`OLDEST_READABLE_SCHEMA_VERSION`] with it.

use anyhow::{Context, bail, ensure};
use sqlx::{AssertSqlSafe, Connection, SqliteConnection};

use crate::db::{WritePool, begin_write};

/// The schema `migrate` brings a database to, kept in SQLite's
/// `PRAGMA user_version` (see the module's doc).
pub const SCHEMA_VERSION: i64 = 1;

/// The oldest schema this version of the app and the CLI can use: the
/// version a database is left at if a step past it fails is still read
/// and written, and `gospel-getter-cli` reads any database at it or above.
/// It rises only with a step that isn't compatible (see the module's doc).
pub const OLDEST_READABLE_SCHEMA_VERSION: i64 = 1;

/// Version 1's schema, frozen (see its header).
const SCHEMA_1: &str = include_str!("migrate/schema_1.sql");

/// The steps past version 1, in order: `STEPS[i]` brings a database from
/// version `i + 1` to `i + 2`.
pub(crate) const STEPS: &[Step] = &[];

/// One step: what brings a database from version `to - 1` to `to`, in one
/// write transaction with its `user_version` write. Its tables' foreign
/// keys aren't enforced while it runs (a rebuild drops a table and makes it
/// again, see [`Rebuild`]), and it must leave no foreign key broken that
/// wasn't before (`PRAGMA foreign_key_check`).
#[derive(Debug)]
pub(crate) struct Step {
    /// The version it brings a database to.
    pub(crate) to: i64,
    /// The tables it rebuilds, in order.
    pub(crate) rebuilds: &'static [Rebuild],
}

/// A table made again with a new definition, by the procedure SQLite
/// documents for a change `ALTER TABLE` can't make (a constraint, say;
/// <https://www.sqlite.org/lang_altertable.html#otheralter>): the new table
/// made beside the old under `<table>_new`, every row copied into it, the
/// old dropped, the new renamed to the old name, and the old table's own
/// indexes and triggers made again exactly as they were.
#[derive(Debug)]
pub(crate) struct Rebuild {
    /// The table.
    pub(crate) table: &'static str,
    /// Its new definition, a whole `CREATE TABLE <table> ...` statement.
    pub(crate) create: &'static str,
    /// Every column the old table has, in its order, copied as it is: with
    /// `rowid` first for a table that has no `INTEGER PRIMARY KEY`, so each
    /// row keeps its rowid too. A table with any other column is refused,
    /// rather than rebuilt without it.
    pub(crate) columns: &'static [&'static str],
    /// Whether its `id` is `AUTOINCREMENT`: then SQLite's next id for it
    /// (`sqlite_sequence`) starts past the largest id copied.
    pub(crate) autoincrement: bool,
}

/// A new database's schema, version [`SCHEMA_VERSION`]: each object and
/// what it is (for an error's words), in the order they're made.
const CURRENT: [(&str, &str); 21] = [
    (
        "books table",
        "CREATE TABLE books (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            testament TEXT NOT NULL CHECK (testament IN ('OT', 'NT')),
            chapter_count INTEGER NOT NULL
        )",
    ),
    (
        "translations table",
        "CREATE TABLE translations (
            id INTEGER PRIMARY KEY,
            code TEXT NOT NULL UNIQUE,
            name TEXT NOT NULL
        )",
    ),
    // Book structure (names, testament, chapter counts) is shared across
    // translations; only the verse text itself is per-translation.
    (
        "verses table",
        "CREATE TABLE verses (
            translation_id INTEGER NOT NULL REFERENCES translations(id),
            book_id INTEGER NOT NULL REFERENCES books(id),
            chapter INTEGER NOT NULL,
            verse INTEGER NOT NULL,
            text TEXT NOT NULL,
            PRIMARY KEY (translation_id, book_id, chapter, verse)
        )",
    ),
    // Single-row table (id is always 1) remembering the last chapter read,
    // so the app can reopen where the reader left off instead of always
    // starting at Genesis 1.
    (
        "reading_position table",
        "CREATE TABLE reading_position (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            translation_id INTEGER NOT NULL REFERENCES translations(id),
            book_id INTEGER NOT NULL REFERENCES books(id),
            chapter INTEGER NOT NULL
        )",
    ),
    // Single-row table (id is always 1): where in that chapter the reader
    // was, so the app reopens at the verse at the top of the reading pane,
    // `offset_px` below the bar (`None`: at the bar). It names its own
    // translation, book and chapter, and is used only while they're
    // `reading_position`'s (`queries::get_reading_place`): an older app
    // that saves another chapter there, not knowing this table, never
    // makes the app open on a verse of the wrong chapter. Only for
    // reopening on this computer: an export doesn't carry it.
    (
        "reading_place table",
        "CREATE TABLE reading_place (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            translation_id INTEGER NOT NULL REFERENCES translations(id),
            book_id INTEGER NOT NULL REFERENCES books(id),
            chapter INTEGER NOT NULL,
            verse INTEGER NOT NULL CHECK (verse >= 1),
            offset_px REAL
        )",
    ),
    // Cross-references are translation-independent (they're citations
    // between passages, not text), so they're keyed only by book/chapter/
    // verse. `ref_end_verse` is set when the reference is to a verse range
    // rather than a single verse.
    (
        "cross_references table",
        "CREATE TABLE cross_references (
            book_id INTEGER NOT NULL REFERENCES books(id),
            chapter INTEGER NOT NULL,
            verse INTEGER NOT NULL,
            ref_book_id INTEGER NOT NULL REFERENCES books(id),
            ref_chapter INTEGER NOT NULL,
            ref_verse INTEGER NOT NULL,
            ref_end_verse INTEGER,
            score INTEGER NOT NULL
        )",
    ),
    (
        "cross_references index",
        "CREATE INDEX idx_cross_references_verse
            ON cross_references (book_id, chapter, verse)",
    ),
    // The reader's bookmarks. A bookmark is a passage's coordinates —
    // book, chapter and an inclusive verse range — with no translation, so
    // it opens in whichever translation is selected. The CHECKs and the
    // UNIQUE constraint back up the validation in `db::bookmarks`.
    (
        "bookmarks table",
        "CREATE TABLE bookmarks (
            id INTEGER PRIMARY KEY,
            book_id INTEGER NOT NULL REFERENCES books(id),
            chapter INTEGER NOT NULL CHECK (chapter >= 1),
            verse_start INTEGER NOT NULL CHECK (verse_start >= 1),
            verse_end INTEGER NOT NULL CHECK (verse_end >= verse_start),
            created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
            UNIQUE (book_id, chapter, verse_start, verse_end)
        )",
    ),
    // The reader's highlights: one color per verse, like a highlighter
    // pen in a paper Bible. Keyed by translation-independent coordinates,
    // like bookmarks, so a highlight shows in whichever translation numbers
    // that verse. The CHECKs back up the validation in `db::highlights`;
    // `updated_at` changes when a verse's color does.
    (
        "highlights table",
        "CREATE TABLE highlights (
            book_id INTEGER NOT NULL REFERENCES books(id),
            chapter INTEGER NOT NULL CHECK (chapter >= 1),
            verse INTEGER NOT NULL CHECK (verse >= 1),
            color TEXT NOT NULL CHECK (color IN ('yellow', 'green', 'blue', 'pink')),
            created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
            updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
            PRIMARY KEY (book_id, chapter, verse)
        )",
    ),
    // The reader's notes: their own words on a passage within one
    // chapter. Like bookmarks they have no translation, but a passage can
    // have any number of them. The CHECKs back up the validation in
    // `db::notes` (`MAX_NOTE_CHARS`); `updated_at` changes when the text
    // does. Notes are looked up a chapter at a time, hence the index.
    (
        "notes table",
        "CREATE TABLE notes (
            id INTEGER PRIMARY KEY,
            book_id INTEGER NOT NULL REFERENCES books(id),
            chapter INTEGER NOT NULL CHECK (chapter >= 1),
            verse_start INTEGER NOT NULL CHECK (verse_start >= 1),
            verse_end INTEGER NOT NULL CHECK (verse_end >= verse_start),
            body TEXT NOT NULL CHECK (length(body) BETWEEN 1 AND 10000),
            created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
            updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
        )",
    ),
    (
        "notes index",
        "CREATE INDEX idx_notes_chapter ON notes (book_id, chapter)",
    ),
    // AI-flagged findings (2.26.0, `db::findings`): where an AI agent, at the
    // reader's request, marked a verse ("every verse that deals with
    // grief"), so the reader can read each one themselves. A marker, never
    // commentary (C3): there's no column for free text. What the app shows
    // is built from `agent` and `request` alone ("Found by Claude for your
    // request “grief”."), two short labels held to one rule in Rust
    // (`db::findings::normalize_agent`), so neither can carry commentary.
    // The passage's CHECKs are the notes'; the lengths back up
    // `db::findings::{MAX_STORED_AGENT_CHARS, MAX_REQUEST_CHARS}`. Saving
    // the same verse for the same request again is a no-op (the UNIQUE
    // index); they're looked up a chapter at a time and a request at a
    // time, hence the other two.
    (
        "ai_findings table",
        "CREATE TABLE ai_findings (
            id INTEGER PRIMARY KEY,
            book_id INTEGER NOT NULL REFERENCES books(id),
            chapter INTEGER NOT NULL CHECK (chapter >= 1),
            verse_start INTEGER NOT NULL CHECK (verse_start >= 1),
            verse_end INTEGER NOT NULL CHECK (verse_end >= verse_start),
            agent TEXT NOT NULL CHECK (length(agent) BETWEEN 1 AND 60),
            request TEXT NOT NULL CHECK (length(request) BETWEEN 1 AND 60),
            created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
        )",
    ),
    (
        "ai_findings unique index",
        "CREATE UNIQUE INDEX idx_ai_findings_unique \
         ON ai_findings (book_id, chapter, verse_start, verse_end, request)",
    ),
    (
        "ai_findings chapter index",
        "CREATE INDEX idx_ai_findings_chapter ON ai_findings (book_id, chapter)",
    ),
    (
        "ai_findings request index",
        "CREATE INDEX idx_ai_findings_request ON ai_findings (request)",
    ),
    // The app's own settings that something outside the page must read, a
    // key and its value each: `ai_findings_enabled` (`db::findings`), which
    // `gospel-getter-cli` checks before it saves a finding. A key that's
    // absent has its default (for that one, off: C6). The page's display
    // settings stay in the page's own storage (`shared/preferences.rs`).
    (
        "app_settings table",
        "CREATE TABLE app_settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        )",
    ),
    // A translation's footnotes, where it keeps them apart from its verse
    // text (the BSB's): each exactly as its source has it (`usfm`), and
    // where it stands, as a number of characters into its verse's `text`.
    // The KJV's margin notes are in its text instead (`kjv_markup`).
    (
        "footnotes table",
        "CREATE TABLE footnotes (
            translation_id INTEGER NOT NULL REFERENCES translations(id),
            book_id INTEGER NOT NULL REFERENCES books(id),
            chapter INTEGER NOT NULL,
            verse INTEGER NOT NULL,
            ordinal INTEGER NOT NULL,
            position INTEGER NOT NULL CHECK (position >= 0),
            usfm TEXT NOT NULL,
            PRIMARY KEY (translation_id, book_id, chapter, verse, ordinal)
        )",
    ),
    // Full-text search index over `verses`. It's derived data only: built
    // from `verses` by `db::search`, never written back, and contentless —
    // it stores no copy of the text, only the tokens and a rowid encoding
    // each verse's coordinates (see `db::search`). Results
    // are always read from `verses` itself.
    (
        "verse_search index",
        "CREATE VIRTUAL TABLE verse_search USING fts5(
            text,
            content = '',
            contentless_delete = 1
        )",
    ),
    // Which translations `verse_search` covers, and how many verses each
    // had when indexed, so startup only (re)indexes a translation that's
    // missing or has changed instead of rebuilding everything every launch.
    // Deliberately no foreign key: it mustn't stop a translation's rows
    // being cleared out for re-seeding (see the README).
    (
        "search_index_state table",
        "CREATE TABLE search_index_state (
            translation_id INTEGER PRIMARY KEY,
            verse_count INTEGER NOT NULL
        )",
    ),
    // What `verse_search` holds for each verse, as a number
    // (`db::search::SEARCH_CONTENT_VERSION`): when it isn't the current
    // one, startup rebuilds the whole index once. One row, or none in a
    // database whose index predates it (2.9.1 and older).
    (
        "search_index_version table",
        "CREATE TABLE search_index_version (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            version INTEGER NOT NULL
        )",
    ),
    // Which version of its bundled data each translation's rows are
    // (`seed::DATA_STEPS`, 2.29.1), so a change to a translation installs
    // have already seeded is made on each of them once. None for rows
    // seeded before it: version 1. Deliberately no foreign key, like
    // `search_index_state`: it mustn't stop a translation's rows being
    // cleared out for re-seeding, which records the version again.
    (
        "translation_data_version table",
        "CREATE TABLE translation_data_version (
            translation_id INTEGER PRIMARY KEY,
            version INTEGER NOT NULL CHECK (version >= 1)
        )",
    ),
];

/// Bring the database's schema up to [`SCHEMA_VERSION`], as the module's
/// doc describes. Safe to run on every launch: once a database is up to
/// date it changes nothing.
pub async fn migrate(writer: &WritePool) -> anyhow::Result<()> {
    migrate_with(writer, STEPS).await
}

/// [`migrate`], with `steps` for [`STEPS`] (a test's, which can fail).
pub(crate) async fn migrate_with(writer: &WritePool, steps: &[Step]) -> anyhow::Result<()> {
    if create_new(writer).await? {
        return Ok(());
    }
    let found = version(writer.pool()).await?;
    if found > SCHEMA_VERSION {
        tracing::info!(
            "The database's schema is version {found}, a newer app's (this one's is \
             {SCHEMA_VERSION}); it's left as it is"
        );
        return Ok(());
    }
    if found <= 1 {
        complete_schema_1(writer).await?;
    }
    for step in steps {
        let at = version(writer.pool()).await?;
        if step.to <= at {
            continue;
        }
        ensure!(
            step.to == at + 1,
            "The database is at schema {at}, and the next step is to {}",
            step.to
        );
        let started = std::time::Instant::now();
        match run_step(writer, step).await {
            Ok(()) => tracing::info!(
                "Brought the database's schema to version {} in {:?}",
                step.to,
                started.elapsed()
            ),
            Err(e) if at >= OLDEST_READABLE_SCHEMA_VERSION => {
                tracing::warn!(
                    "Couldn't bring the database's schema to version {}; it's as it was, at \
                     version {at}, which this version reads, and the next launch tries again: \
                     {e:#}",
                    step.to
                );
                return Ok(());
            }
            Err(e) => {
                return Err(e.context(format!(
                    "Failed to bring the database's schema from version {at} to {}",
                    step.to
                )));
            }
        }
    }
    Ok(())
}

/// Give a database with no tables [`SCHEMA_VERSION`]'s schema and number,
/// in one transaction. Whether it did: `false` for a database that has
/// tables (looked at under the write lock, so another copy of the app
/// starting at the same moment can't make them twice).
async fn create_new(writer: &WritePool) -> anyhow::Result<bool> {
    let mut tx = begin_write(writer)
        .await
        .context("Failed to start creating the schema")?;
    let has_tables: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM sqlite_master \
         WHERE type = 'table' AND name NOT LIKE 'sqlite\\_%' ESCAPE '\\')",
    )
    .fetch_one(&mut *tx)
    .await
    .context("Failed to look for the database's tables")?;
    if has_tables {
        return Ok(false);
    }
    for (what, sql) in CURRENT {
        sqlx::query(sql)
            .execute(&mut *tx)
            .await
            .with_context(|| format!("Failed to create the {what}"))?;
    }
    set_version(&mut tx, SCHEMA_VERSION).await?;
    tx.commit().await.context("Failed to create the schema")?;
    Ok(true)
}

/// Give a database at version 0 or 1 each object of version 1's schema
/// (`schema_1.sql`) it lacks, and record version 1, in one transaction.
/// Also how the tests make a version-1 database (`test_support`).
pub(crate) async fn complete_schema_1(writer: &WritePool) -> anyhow::Result<()> {
    let mut tx = begin_write(writer)
        .await
        .context("Failed to start completing schema 1")?;
    let found = version(&mut *tx).await?;
    if found > 1 {
        return Ok(());
    }
    for sql in schema_1() {
        sqlx::query(AssertSqlSafe(if_not_exists(sql)?))
            .execute(&mut *tx)
            .await
            .with_context(|| format!("Failed to complete schema 1 with {sql}"))?;
    }
    if found < 1 {
        set_version(&mut tx, 1).await?;
    }
    tx.commit().await.context("Failed to complete schema 1")
}

/// Each statement of [`SCHEMA_1`], without its header or `;`.
pub(crate) fn schema_1() -> impl Iterator<Item = &'static str> {
    SCHEMA_1.split(";\n").filter_map(|chunk| {
        let start = chunk
            .lines()
            .take_while(|line| line.starts_with("--") || line.trim().is_empty())
            .map(|line| line.len() + 1)
            .sum::<usize>()
            .min(chunk.len());
        let sql = chunk[start..].trim();
        (!sql.is_empty()).then_some(sql)
    })
}

/// `sql`, a `CREATE` statement as `sqlite_master` keeps it, made a no-op
/// for an object that's already there. SQLite keeps the statement without
/// the `IF NOT EXISTS`, so what it keeps for an object made this way is
/// `sql` itself.
fn if_not_exists(sql: &str) -> anyhow::Result<String> {
    for kind in [
        "CREATE TABLE ",
        "CREATE VIRTUAL TABLE ",
        "CREATE INDEX ",
        "CREATE UNIQUE INDEX ",
    ] {
        if let Some(rest) = sql.strip_prefix(kind) {
            return Ok(format!("{kind}IF NOT EXISTS {rest}"));
        }
    }
    bail!("schema_1.sql holds a statement that isn't a CREATE TABLE or INDEX: {sql}")
}

/// Run `step` on a database at version `step.to - 1`, in one write
/// transaction with its version, on the writing connection with its
/// foreign keys off. SQLite's procedure for a rebuild turns them off for
/// the rebuild (`DROP TABLE` with them on deletes every row first, and a
/// rename rewrites other tables' references), outside any transaction,
/// since `PRAGMA foreign_keys` does nothing inside one; they're on again
/// before the connection goes back to the pool, or it's closed (a new one
/// has them on, as every connection `create_pool` makes does).
async fn run_step(writer: &WritePool, step: &Step) -> anyhow::Result<()> {
    let mut conn = writer
        .pool()
        .acquire()
        .await
        .context("Failed to get the writing connection")?;
    sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(&mut *conn)
        .await
        .context("Failed to turn foreign keys off")?;
    let outcome = step_transaction(&mut conn, step).await;
    if let Err(e) = sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&mut *conn)
        .await
    {
        tracing::warn!("Couldn't turn foreign keys on again; closing that connection: {e}");
        conn.close_on_drop();
    }
    outcome
}

/// [`run_step`]'s transaction: the step's changes, the check that they
/// broke no foreign key, and its version, committed together or not at
/// all.
async fn step_transaction(conn: &mut SqliteConnection, step: &Step) -> anyhow::Result<()> {
    let foreign_keys: bool = sqlx::query_scalar("PRAGMA foreign_keys")
        .fetch_one(&mut *conn)
        .await
        .context("Failed to check foreign keys are off")?;
    ensure!(!foreign_keys, "Foreign keys are still on");
    let mut tx = conn
        .begin_with("BEGIN IMMEDIATE")
        .await
        .context("Failed to start the step")?;
    let at = version(&mut *tx).await?;
    ensure!(
        at == step.to - 1,
        "The database is at schema {at}, not {}",
        step.to - 1
    );
    // Rows a reader's database broke a foreign key with before (none that
    // the app made) are kept as they are; the step mustn't add any.
    let before = foreign_key_violations(&mut tx).await?;
    for rebuild in step.rebuilds {
        rebuild_table(&mut tx, rebuild)
            .await
            .with_context(|| format!("Failed to rebuild {}", rebuild.table))?;
    }
    let after = foreign_key_violations(&mut tx).await?;
    ensure!(
        after == before,
        "The step would break a foreign key: {after:?}, where there were {before:?}"
    );
    set_version(&mut tx, step.to).await?;
    tx.commit().await.context("Failed to commit the step")
}

/// SQLite's procedure for `rebuild` (see [`Rebuild`]), inside a step's
/// transaction.
async fn rebuild_table(conn: &mut SqliteConnection, rebuild: &Rebuild) -> anyhow::Result<()> {
    let table = rebuild.table;
    let new = format!("{table}_new");
    let definition = rebuild
        .create
        .strip_prefix(&format!("CREATE TABLE {table} "))
        .with_context(|| format!("{table}'s new definition doesn't create {table}"))?;

    let columns: Vec<String> = sqlx::query_scalar("SELECT name FROM pragma_table_xinfo($1)")
        .bind(table)
        .fetch_all(&mut *conn)
        .await
        .context("Failed to read the table's columns")?;
    let copied: Vec<&str> = rebuild
        .columns
        .iter()
        .copied()
        .filter(|&c| c != "rowid")
        .collect();
    ensure!(
        columns == copied,
        "{table} has the columns {columns:?}, not the {copied:?} the step copies"
    );
    let kept: Vec<String> = sqlx::query_scalar(
        "SELECT sql FROM sqlite_master \
         WHERE tbl_name = $1 AND type IN ('index', 'trigger') AND sql IS NOT NULL \
         ORDER BY rowid",
    )
    .bind(table)
    .fetch_all(&mut *conn)
    .await
    .context("Failed to read the table's indexes and triggers")?;
    let rows: i64 = sqlx::query_scalar(AssertSqlSafe(format!("SELECT COUNT(*) FROM {table}")))
        .fetch_one(&mut *conn)
        .await
        .context("Failed to count the table's rows")?;

    let column_list = rebuild.columns.join(", ");
    for (what, sql) in [
        ("create", format!("CREATE TABLE {new} {definition}")),
        (
            "copy",
            format!("INSERT INTO {new} ({column_list}) SELECT {column_list} FROM {table}"),
        ),
        ("drop", format!("DROP TABLE {table}")),
        ("rename", format!("ALTER TABLE {new} RENAME TO {table}")),
    ] {
        let done = sqlx::query(AssertSqlSafe(sql))
            .execute(&mut *conn)
            .await
            .with_context(|| format!("Failed to {what} the table"))?;
        if what == "copy" {
            let copied = i64::try_from(done.rows_affected()).context("too many rows")?;
            ensure!(
                copied == rows,
                "{copied} of {table}'s {rows} rows were copied"
            );
        }
    }
    for sql in kept {
        sqlx::query(AssertSqlSafe(sql.clone()))
            .execute(&mut *conn)
            .await
            .with_context(|| format!("Failed to make {sql} again"))?;
    }
    if rebuild.autoincrement {
        sqlx::query("DELETE FROM sqlite_sequence WHERE name = $1")
            .bind(table)
            .execute(&mut *conn)
            .await
            .context("Failed to clear the table's next id")?;
        sqlx::query(AssertSqlSafe(format!(
            "INSERT INTO sqlite_sequence (name, seq) \
             SELECT $1, MAX(id) FROM {table} HAVING MAX(id) IS NOT NULL"
        )))
        .bind(table)
        .execute(&mut *conn)
        .await
        .context("Failed to set the table's next id")?;
    }
    Ok(())
}

/// Every row `PRAGMA foreign_key_check` finds breaking a foreign key, as
/// (table, rowid, parent table), in order.
async fn foreign_key_violations(
    conn: &mut SqliteConnection,
) -> anyhow::Result<Vec<(String, Option<i64>, String)>> {
    let mut found: Vec<(String, Option<i64>, String)> =
        sqlx::query_as("SELECT \"table\", rowid, parent FROM pragma_foreign_key_check")
            .fetch_all(&mut *conn)
            .await
            .context("Failed to check foreign keys")?;
    found.sort();
    Ok(found)
}

/// The database's `PRAGMA user_version`.
async fn version<'c>(conn: impl sqlx::SqliteExecutor<'c>) -> anyhow::Result<i64> {
    sqlx::query_scalar("PRAGMA user_version")
        .fetch_one(conn)
        .await
        .context("Failed to read the schema version")
}

/// Record `version` as the database's schema version, in `conn`'s
/// transaction (`user_version` is in the database's header, which commits
/// and rolls back with it). PRAGMA takes no bound parameters, so the number
/// is written out.
async fn set_version(conn: &mut SqliteConnection, version: i64) -> anyhow::Result<()> {
    sqlx::query(AssertSqlSafe(format!("PRAGMA user_version = {version}")))
        .execute(&mut *conn)
        .await
        .context("Failed to record the schema version")?;
    Ok(())
}
