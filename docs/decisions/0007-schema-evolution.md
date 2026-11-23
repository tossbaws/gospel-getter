# 0007. How the database schema changes

- **Status:** accepted, 2026-11-19 (shipped in 2.33.0, schema 2)

## Decision

- The schema's version is SQLite's `user_version`. A step runner brings an
  older database up to date: each step runs in one write transaction
  together with its version write, so a step and its number land together
  or not at all. A step's SQL is frozen once released; a later change to the
  same table is a new step.
- Schema 2 stays readable and writable by 2.32.x apps and CLIs. Every step
  so far is compatible both ways.
- Before the first step that isn't compatible, a max-writer check ships in
  a release of its own: an app or CLI refuses to write a database whose
  version is above the newest it knows to be safe. Only then can an
  incompatible step land, raising `OLDEST_READABLE_SCHEMA_VERSION` with it.
- Rust owns the product limits (note length, highlight colours, the agent
  and request labels' lengths). SQL keeps only structural constraints:
  `NOT NULL`, foreign keys, `UNIQUE`, `chapter >= 1`,
  `verse_end >= verse_start`.

## Why

Readers go back a version (About's Go back, or a manual install of the
previous release), so an older app has to keep working on a database a newer
one upgraded. A crash between a schema change and its version write would
leave a changed schema recorded as the old one. Limits written as SQL
`CHECK`s can only be changed by rebuilding a table, and they duplicated the
Rust checks that give the reader a sentence instead of a constraint error.

## Consequences

- `src-tauri/core/src/db/migrate.rs`'s `//!` says how to add a step; its
  `compatible_both_ways` flag drives About's Go back (`older_app_can_use`).
- `src-tauri/core/src/db/migrate_tests.rs` holds it: an upgraded database
  matches a new one, a failed step changes nothing, a newer version is left
  alone, 2.32.2's own statements read and write an upgraded database, and
  the limits are Rust's on an upgraded database.
