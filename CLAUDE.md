# Gospel Getter: instructions for Claude Code

Gospel Getter is a Tauri 2 Bible reader. The Rust backend lives in `src-tauri/`. The UI is the `ui-leptos/` crate (Rust, Leptos), which Trunk (`Trunk.toml`) compiles to WebAssembly and builds with `ui/index.html` into `dist/`, the folder Tauri embeds. `ui/index.html` still holds the page's markup, its stylesheet and three small inline scripts (the pre-paint script, boot's prefetch and the plain-text fallback), with `ui/leptos-loader.js`, the fonts and the favicons beside it. `tests/gui/` drives the real app through tauri-driver (`scripts/gui-check.sh`), and `tests/frontend/` loads the built page into jsdom against the backend's real commands.

## Rust

- Use the `rust-coding-standards` skill for all Rust coding decisions.
- Don't use `unwrap()` or `panic!()` on recoverable errors in library code.
- Use `anyhow::Context` for error propagation.
- After every change, run `cargo fmt` and `cargo clippy -- -D warnings`. A build that compiles but fails Clippy isn't done.
- Run `trunk build --release` from the repository root first: the backend embeds `dist/`, and the frontend tests load it.
- In `ui-leptos/`, Clippy runs on the WebAssembly target: `cargo clippy --target wasm32-unknown-unknown --all-targets -- -D warnings`.

## Scripture text

- Use the `biblical-text-integrity` skill for any work that touches Bible text: seed data, ingestion, translations, verse rendering.
- Never alter verse wording for any reason, including cleanup or fixing an apparent typo in a source dataset.
