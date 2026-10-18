# ui-leptos: the page's Rust pieces

Gospel Getter's page (`ui/index.html`) is being moved to Rust a piece at a
time. This crate holds the pieces so far, written with [Leptos] and compiled
to WebAssembly; they run inside the same Tauri app, next to the page's
JavaScript, which stays as it is everywhere else. The first piece is the
Export notes dialog (`src/export_notes.rs`).

[Leptos]: https://leptos.dev

## Layout

- `ui-leptos/` is its own crate (Rust 2024, `cdylib`), separate from
  `src-tauri/`: one targets `wasm32-unknown-unknown`, the other the desktop.
  They share no code yet; `src/dto.rs` mirrors the DTOs of the three commands
  the dialog calls, with camelCase serde names, and says what a shared crate
  would replace later.
- `src/ipc.rs`: Tauri commands through the same global the page uses,
  `window.__TAURI__.core.invoke` (a `wasm_bindgen` extern plus
  `serde-wasm-bindgen`), with the same command names and argument objects.
- `src/bridge.rs`: everything the Rust side and the page share, in one
  place. The page's half is the short block in `ui/index.html` that
  dispatches `gospel-getter:export-notes` and listens for
  `gospel-getter:toast`.
- `src/text.rs`: the dialog's words, numbers and filter, free of the DOM,
  unit-tested natively (`cargo test`).
- `src/lib.rs`: mounts the pieces when the module loads.

## Building and running

[Trunk] builds the page: `trunk build --release` at the repository root
reads `Trunk.toml`, compiles this crate, runs wasm-bindgen (`no-modules`
target) and wasm-opt (`-Oz`), copies the fonts and favicons, and writes
`dist/`, which Tauri embeds (`frontendDist` in `src-tauri/tauri.conf.json`).
Tauri runs it for you: `cargo tauri dev` and `cargo tauri build` call
`trunk build` and `trunk build --release` first (`beforeDevCommand`,
`beforeBuildCommand`). Run them from `src-tauri/`, as CI does. Trunk
downloads the wasm-bindgen and wasm-opt versions it needs into its own
cache the first time.

You need Trunk (`cargo install trunk --locked`, or a release binary) and the
target (`rustup target add wasm32-unknown-unknown`). Anything that builds the
app crate, `cargo test` in `src-tauri/` included, needs `dist/` to exist, so
run `trunk build` once first.

The page loads the pieces itself: Trunk inlines `ui/leptos-loader.js` at the
end of `<body>`, which loads the glue (`ui-leptos.js`) and the module
(`ui-leptos_bg.wasm`). Trunk's own loader is an ES module import, which the
`no-modules` glue cannot be loaded by, so `Trunk.toml` turns it off. If the
module fails to load, the page is as it was without it: only Export notes…
is unavailable, and says so.

The Content Security Policy allows exactly what this needs beyond the page's
own code: `'wasm-unsafe-eval'` in `script-src` (compiling WebAssembly) and
`'self'` in `connect-src` (fetching the module). The glue and the inlined
loader are covered as the page's scripts are: `'self'` and Tauri's
build-time hashes.

Size matters: the module is fetched on every launch. The release profile is
`opt-level = "z"`, `lto`, one codegen unit and `panic = "abort"`, and Trunk
runs wasm-opt `-Oz`.

## Tests

The frontend tests (`tests/frontend/`) drive the real page in jsdom against
the real backend, and the dialog with it, as a black box: the same ids,
classes, ARIA, `hidden` and `disabled`, text and focus order as the
JavaScript version, so the dialog's tests (`notes-export.test.mjs`) are the
ones it had. jsdom runs no module scripts and has no `fetch`, so the test
harness runs the glue as a classic script and hands it the `.wasm` bytes
itself (`loadLeptos` in `harness.mjs`), the same files the app ships.
`npm test` builds `dist/` first.

Leptos updates the DOM after the current task, where the page's scripts
update it within the click or key itself, so the harness's `app.click` and
`app.key` return the event as before and can also be awaited:
`await app.click(el)` resolves once pending DOM updates have flushed. Tests
that look at the dialog right after a click or key await it; assertions did
not change.

## What a shared crate would replace

`src/dto.rs` in full: `NotedBook`, `DaySpan`, `DatePresets`,
`ExportOptions` (`commands::NotesExportOptions`), `Filter`
(`notes_export::NotesFilter`) and `ExportOutcome`
(`commands::NotesExportOutcome`). The backend's types carry `chrono` dates
and `deny_unknown_fields`; the shared crate would keep them serde-only with
the dates as `YYYY-MM-DD` strings, as the page always sent and received them.
