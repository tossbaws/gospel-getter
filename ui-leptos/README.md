# ui-leptos: the page's Rust pieces

Gospel Getter's page (`ui/index.html`) is being moved to Rust a piece at a
time. This crate holds the pieces so far, written with [Leptos] and compiled
to WebAssembly; they run inside the same Tauri app, next to the page's
JavaScript, which stays as it is everywhere else. The pieces are the page's
modal dialogs: Export notes (`src/export_notes.rs`, the first), the
keyboard shortcuts sheet (`src/shortcuts.rs`), the welcome
(`src/welcome.rs`), the import preview (`src/import_preview.rs`) and the
note editor (`src/note_editor.rs`); and About (`src/about.rs`) with the
updater (`src/updates.rs`: Check now, the launch check, the banner, the
update shown in About, and installing with its progress); and the
Library's tabs and lists (`src/library.rs`: Bookmarks, Highlights with
its color filter, and Notes, each loaded from the backend here, once per
reload); and search with the command palette (`src/search.rs`: the box,
its Go to, Commands and Verses, Show more, and running a command), with
the table of keyboard shortcuts (`src/shortcut_table.rs`, which the
shortcuts sheet lists and the palette's key hints come from) and the
palette's commands (`src/palette.rs`). About and the Library stay the head
script's surfaces: the head script opens and closes them.

[Leptos]: https://leptos.dev

## Layout

- `ui-leptos/` is its own crate (Rust 2024, `cdylib`), separate from
  `src-tauri/`: one targets `wasm32-unknown-unknown`, the other the desktop.
  They share no code yet; `src/dto.rs` mirrors the DTOs of the commands the
  pieces call, with the names the JSON carries, and says what a shared crate
  would replace later.
- `src/ipc.rs`: Tauri commands through the same global the page uses,
  `window.__TAURI__.core.invoke` (a `wasm_bindgen` extern plus
  `serde-wasm-bindgen`), with the same command names and argument objects.
- `src/bridge.rs`: everything the Rust side and the page share, in one
  place: the custom events each way (requests to open with a snapshot in
  their `detail`, the page's one request to reload the Library's lists;
  toasts, jumps, refreshes and the lists' answers back), the page globals
  it calls, and the page controls it uses. The page's half is the short
  blocks in `ui/index.html` that dispatch and listen for those names.
- `src/dom.rs`: the DOM helpers the pieces share (focus that never
  scrolls, the modals' Tab trap, listening).
- `src/text.rs`, `src/import_text.rs`, `src/note_text.rs`,
  `src/install.rs`, `src/update_text.rs`, `src/library_text.rs`,
  `src/shortcut_table.rs`, `src/palette.rs`, `src/search_text.rs`: the
  pieces' words, numbers and rules (Export notes' filter, the import's
  sentences, the note's length rule, the welcome's fresh-install rule, the
  updater's sentences, `megabytes` and download progress, the Library's
  rows, color names, filter and focus arithmetic; the shortcuts; the
  palette's commands, their reasons and its word matching; the search's
  sentences, active option and marked words), free of the DOM,
  unit-tested natively
  (`cargo test`). The words a search matched are the backend's to find
  (`query::term_marks`, beside the search's own terms); the box only draws
  them. The update check's time limit and its "couldn't reach
  GitHub" message are the backend's (`check_for_update`).
- `src/lib.rs`: mounts the pieces when the module loads (About and the
  banner are moved to where the page had them, for focus order, and the
  Library's tabs and lists into its panel), then tells the page they're
  listening (`gospel-getter:pieces-ready`).

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
module fails to load, the page is as it was without it, except that the
five dialogs, About, the Library and search are unavailable, and say so
when asked for, no update is looked for, and the reader marks no
bookmarks (the Library loads them). The module may also load after the page has booted:
the Library's lists, then the welcome's startup question, the launch check
and About's credits wait for `gospel-getter:pieces-ready`.

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
the real backend, and the dialogs with it, as a black box: the same ids,
classes, ARIA, `hidden` and `disabled`, text and focus order as the
JavaScript version, so the dialogs' tests (`notes-export.test.mjs`,
`shortcuts.test.mjs`, `welcome.test.mjs`, `reader-data.test.mjs`,
`notes.test.mjs` and others) are the ones they had. jsdom runs no module
scripts and has no `fetch`, so the test harness runs the glue as a classic
script and hands it the `.wasm` bytes itself (`loadLeptos` in
`harness.mjs`), the same files the app ships.
`npm test` builds `dist/` first.

Leptos updates the DOM after the current task, where the page's scripts
update it within the click or key itself, so the harness's `app.click` and
`app.key` return the event as before and can also be awaited:
`await app.click(el)` resolves once pending DOM updates have flushed. Tests
that look at a dialog right after a click or key await it; assertions did
not change. Where the page answered within the event itself and a test
looks at once with nothing to await (typing in the note editor, its discard
prompt, the import's add-or-replace choice, and the updater throughout:
Check now's line while it checks, the views as an install starts, Later;
and the Library's tabs), the piece writes to those elements within the
event, as the page did. The search box does so throughout: it opens and
closes, and draws its list (static Leptos views, built and put in place
at once) and its active option, within the key or click. `openApp({ leptos: 'late' })` loads the pieces only
when the test calls `app.loadPieces()`, and `leptos: false` never
(`pieces.test.mjs`); boot waits for the Library's bookmark list only when
the pieces load with the page.

## What a shared crate would replace

`src/dto.rs` in full: `NotedBook`, `DaySpan`, `DatePresets`,
`ExportOptions` (`commands::NotesExportOptions`), `Filter`
(`notes_export::NotesFilter`) and `ExportOutcome`
(`commands::NotesExportOutcome`); for the import, `ImportChoice`,
`ImportPreview` (`commands::ImportPreviewDto`), `PositionPreview`
(`commands::PositionPreviewDto`), `ImportResult`
(`commands::ImportResultDto`) and `Preferences` (`reader_data::Preferences`,
whose enums travel as their kebab-case names); for the updater,
`UpdateInfo` and `DownloadEvent` (`updater.rs`) and `NoteBlock` and
`NoteSpan` (`release_notes::Block` and `Span`); for the Library,
`Bookmark`, `HighlightPassage` and `Note` (`commands::BookmarkDto`,
`HighlightPassageDto` and `NoteDto`, of which they read only what the lists
show) and `library_text::COLORS` (`HighlightColor`); for search,
`SearchAnswer` and `SearchVerse` (`commands::SearchDto` and
`SearchResultDto`); and the commands' argument objects. The backend's types carry `chrono` dates
and `deny_unknown_fields`; the shared crate would keep them serde-only with
the dates as `YYYY-MM-DD` strings, as the page always sent and received them.
