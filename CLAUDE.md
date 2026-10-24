# Gospel Getter: instructions for Claude Code

Gospel Getter is a Tauri 2 Bible reader. The Rust backend lives in `src-tauri/`. On `main`, the whole UI is `ui/index.html`.

## Rust

- Use the `rust-coding-standards` skill for all Rust coding decisions.
- Don't use `unwrap()` or `panic!()` on recoverable errors in library code.
- Use `anyhow::Context` for error propagation.
- After every change, run `cargo fmt` and `cargo clippy -- -D warnings`. A build that compiles but fails Clippy isn't done.

## Scripture text

- Use the `biblical-text-integrity` skill for any work that touches Bible text: seed data, ingestion, translations, verse rendering.
- Never alter verse wording for any reason, including cleanup or fixing an apparent typo in a source dataset.
