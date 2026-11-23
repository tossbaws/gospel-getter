# 0008. Errors: `anyhow`, with one typed seam

- **Status:** accepted, 2026-11-19

## Decision

`anyhow` is the error type for faults everywhere in the Rust code, with
`anyhow::Context` saying what was being attempted. The one place a fault is
matched by type is the retry seam (`src-tauri/src/retry.rs`), which tells a
busy or locked database (worth trying again) from any other fault (not) by
the `sqlx` error in the chain. Tauri commands keep `Result<_, String>`: a
fault goes through `failed()` (`src-tauri/src/commands/lookup.rs`), which
logs the whole chain and answers with the one sentence the reader is shown.
A refusal the reader caused (a bad range, a note over a limit) is a plain
enum the command words for the reader (`NoteError`, `BookmarkError` and the
like), not a fault.

## Why

Nothing but the retry loop needs to branch on what kind of fault happened,
and typed fault enums elsewhere would be ceremony that `CLAUDE.md`'s
`anyhow::Context` rule already covers. The reader never sees an error chain;
the log keeps it.

## Consequences

- New fault paths use `anyhow` with context; a new typed error needs a
  caller that matches on it.
- `retry.rs`'s tests hold the busy-versus-fault split against a real locked
  database; `lookup.rs`'s tests hold what `failed()` answers and logs.
