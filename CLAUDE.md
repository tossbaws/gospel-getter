# Gospel Getter: instructions for Claude Code

Gospel Getter is a Tauri 2 Bible reader. The Rust backend lives in `src-tauri/`, a workspace: the app, `core/` (the database and domain logic, free of Tauri) and `cli/` (`gospel-getter-cli`, a CLI and MCP server built on `core/` that only reads, except for saving AI-flagged findings). The UI is the `ui-leptos/` crate (Rust, Leptos), which Trunk (`Trunk.toml`) compiles to WebAssembly and builds with `ui/index.html` into `dist/`, the folder Tauri embeds. `ui/index.html` still holds the page's markup, its stylesheet and three small inline scripts (the pre-paint script, boot's prefetch and the plain-text fallback), with `ui/leptos-loader.js`, the fonts and the favicon beside it. `tests/gui/` drives the real app through tauri-driver (`scripts/gui-check.sh`), and `tests/frontend/` loads the built page into jsdom against the backend's real commands.

## Rust

- Use the `rust-coding-standards` skill (`.claude/skills/rust-coding-standards/`) for all Rust coding decisions.
- Don't use `unwrap()` or `panic!()` on recoverable errors in library code.
- Use `anyhow::Context` for error propagation.
- After every change, run `cargo fmt` and Clippy. A build that compiles but fails Clippy isn't done.
- Before a change is done, run the gates CI runs (`.github/workflows/ci.yml`), as CI writes them. CI adds `.github/ci/cargo-config.toml` to cargo's config; locally, pass `--config ../.github/ci/cargo-config.toml` to each cargo command but `fmt` for the same builds.
  1. From the repository root, first: `trunk build --release`. The backend embeds `dist/`, and the frontend tests load it, so everything below that builds `src-tauri/` needs it.
  2. In `ui-leptos/` (Clippy runs on the WebAssembly target, the tests natively): `cargo fmt --all -- --check`, `cargo clippy --locked --target wasm32-unknown-unknown --all-targets -- -D warnings`, `cargo test --locked`.
  3. In `src-tauri/`: `cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo clippy --all-targets -- -D warnings`, `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps`, `cargo build --all-targets`, `cargo test --all-targets`, `cargo test --doc`. On `main` only, CI also runs `cargo test -p gospel-getter-cli -- --ignored`, the CLI's long sweeps.
  4. The jsdom suite: `cd tests/frontend && npm ci && npm test`. `npm test` runs the Trunk build and builds the bridge (`cargo build --example frontend_bridge`), then the tests; CI builds both first and runs `npm run test:built`, in two shards.
  5. The Python tools and the install script, from the root: `python3 -m unittest discover -s tools/release -v`, `python3 -m unittest discover -s tools/ci -v`, `scripts/test-install.sh`.
  6. In `tests/gui/` (CI runs these on Windows): `cargo clippy --locked --all-targets -- -D warnings`, `cargo test --locked`.
  7. The GUI suite, for any change a reader can see or reach: `scripts/gui-check.sh <release binary or AppImage>`, which needs a full release build of the app, plus `GUI_CHECK_UPDATER_APP=<build>` for the updater checks (`cargo build --release --locked -p gospel_getter --features gui-test-updater`). On Linux it runs the app on a private Xvfb. CI runs it in `dist.yml`: on Linux in every run, and on Windows in a release's run and on pull requests with the `ci:full` label (see `RELEASING.md`).

## Scripture text

- Use the `biblical-text-integrity` skill (`.claude/skills/biblical-text-integrity/`) for any work that touches Bible text: seed data, ingestion, translations, verse rendering.
- Never alter verse wording for any reason, including cleanup or fixing an apparent typo in a source dataset.

## Comments

- Comments explain why: the reason, alternatives that were rejected, the incident behind a rule, invariants, and couplings between files ("move both together; X's test fails if they differ").
- Don't restate what the code plainly does.
- Each reason lives once, next to the code it governs. Elsewhere, write a one-line pointer ("see ci.yml's header").
- No inventories of modules or files inside source comments. Each module's own `//!` says what that module does; `ui-leptos/README.md` holds the UI's module map.
- No migration narration ("so far", "now", "what X was") once a move is finished. Keep a history sentence only if it guards a rule, and phrase it as the rule.
- When behavior changes, update the comment at its canonical home in the same commit.
- Doc comments that compile into user- or agent-facing text (clap help and MCP schemas in `src-tauri/cli/`) are product text, not comments.
