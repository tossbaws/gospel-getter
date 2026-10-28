# ui-leptos: the page's Rust pieces

Gospel Getter's page (`ui/index.html`) has been moved to Rust a piece at a
time. This crate holds the pieces, written with [Leptos] and compiled to
WebAssembly; they run inside the same Tauri app. The page keeps only three
small inline scripts of its own (the pre-paint script, boot's prefetch and
the plain-text fallback) and the loader. The pieces are the page's
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
palette's commands (`src/palette.rs`); and the book and chapter picker's
books and chapters (`src/picker.rs`: the two testaments' lists, a book's
chapters, the marks for the chapter being read, Random chapter, and the
picker's clicks and keys, with its rules in `src/picker_text.rs`); and Aa
with the translation button's popover (`src/aa.rs`: the translations,
Compare and what it compares with, text size with its stepper and keys,
line spacing, width, layout, theme, reading mode and the launch check's
setting, with its rules in `src/aa_text.rs`), which keep the saved settings
(`src/prefs.rs`); and the header's popovers and panels themselves
(`src/surfaces.rs`: the translations, Aa, the Library, the ☰ menu, About
and the picker as the sidebar or the popover; one open at a time, their
buttons, focus in and back, a click outside, Escape, the menu's keys and
the window crossing 1200px, with its rules in `src/surfaces_text.rs`),
and the reader itself (`src/reader.rs`: each chapter read from the backend and
drawn into the reading pane, with Compare's grid, the chapter's end and its
Footnotes and Cross references; the reference button, the window title and
the translation button; the reading position; a chapter that couldn't be
read, with Try again; and the passage history, Back and Forward, with the
mouse's buttons; its markup and rules in `src/reader_text.rs`); and the
selection card (`src/card.rs`: the verse selection, Copy and Copy as…, the
card's bookmark, highlight and notes, the verses' marks and the Shift
ghost, with its markup and rules in `src/card_text.rs`); and the notes in
the margin in reading mode (`src/margin.rs`: placed beside their verses,
the markers in a narrow window, with their rules and markup in
`src/margin_text.rs`); and what was the
page's main script: the keyboard dispatcher (`src/keyboard.rs`, its rules
in `src/keyboard_text.rs`), the click dispatcher (`src/clicks.rs`), boot
(`src/boot.rs`, which takes over the `get_home` and `get_reading` the
page's prefetch script sent from `<head>`) and the page's glue
(`src/page.rs`, its words and rules in `src/page_text.rs`: the toast,
Export and Import data…, the welcome, the shortcuts sheet and search
opened, the palette's page commands, the launch check). What the page's
remaining scripts share with this, and the events the pieces send each
other, are in `src/bridge.rs`.

The saved settings' one table, their keys, values and names (in the page
and in an export file), is `../shared/preferences.rs`, a file the backend
(`src-tauri`) compiles too, as its `preferences` module: its export file's
display settings are that file's enums. The page's pre-paint script, the
first in its `<head>`, applies the saved look before the first frame from a
table generated from it (`prepaint_script`); `src-tauri/tests/frontend_markup.rs`
keeps the two the same.

[Leptos]: https://leptos.dev

## Layout

- `ui-leptos/` is its own crate (Rust 2024, `cdylib`), separate from
  `src-tauri/`: one targets `wasm32-unknown-unknown`, the other the desktop.
  They share files, compiled into both by `#[path]`:
  `../shared/preferences.rs` (below), `../shared/verse_parts.rs`, a
  verse's parts and footnotes as the backend shows them, which the reader
  draws, `../shared/passage_reference.rs`, how a passage is named, and
  `../shared/book_aliases.rs`, the books' abbreviations, by which the
  backend reads a typed reference and the Library's notes search finds a
  note's book. `src/dto.rs` mirrors the other DTOs of the commands the pieces
  call, with the names the JSON carries, and says what a shared crate would
  replace later.
- `src/ipc.rs`: Tauri commands through the same global the page uses,
  `window.__TAURI__.core.invoke` (a `wasm_bindgen` extern plus
  `serde-wasm-bindgen`), with the same command names and argument objects.
- `src/bridge.rs`: what crosses to the page's remaining scripts (boot's
  prefetch, `gospel-getter:boot`; the loader's
  `gospel-getter:pieces-failed`, heard by the fallback alone), the globals
  set for the tests, the custom events the pieces send each other (requests
  to open with a snapshot in their `detail`, the one request to reload the
  Library's lists and their answers back), and the page controls they use.
- `src/dom.rs`: the DOM helpers the pieces share (focus that never
  scrolls, the modals' Tab trap, listening).
- `src/text.rs`, `src/import_text.rs`, `src/note_text.rs`,
  `src/install.rs`, `src/update_text.rs`, `src/library_text.rs`,
  `src/shortcut_table.rs`, `src/palette.rs`, `src/search_text.rs`,
  `src/picker_text.rs`, `src/aa_text.rs`, `src/surfaces_text.rs`,
  `src/reader_text.rs`, `src/card_text.rs`, `src/margin_text.rs`,
  `src/keyboard_text.rs`, `src/page_text.rs`: the
  pieces' words, numbers and rules (Export notes' filter, the import's
  sentences, the note's length rule and its drafts, the welcome's
  fresh-install rule, the updater's sentences, `megabytes` and download
  progress, the Library's rows, color names, filter, notes search and
  focus arithmetic; the shortcuts; the palette's commands, their reasons and its word matching; the search's
  sentences, active option and marked words; the picker's compact grid,
  Random chapter's pick, its keyboard moves and how far it scrolls; the
  size keys; the surfaces' table, the sidebar's rule and the ☰ menu's
  keys; the chapter's markup, byte for byte the page's former templates
  (`fixtures/genesis-1.html` is their output, which a test compares),
  Compare's rows, the reference, and the passage history's bookkeeping; the
  card's markup and placement; where a margin note sits and how notes
  stack, and the margin's markup; which key does what, j and k's steps and
  how far a verse is scrolled into view; the toast's words, Export data…'s
  summary and Compare's partner),
  free
  of the DOM,
  unit-tested natively
  (`cargo test`). The words a search matched are the backend's to find
  (`query::term_marks`, beside the search's own terms); the box only draws
  them. The update check's time limit and its "couldn't reach
  GitHub" message are the backend's (`check_for_update`).
- `src/lib.rs`: mounts the pieces when the module loads: the page's own
  listeners first (`page`, `clicks`, `keyboard`, where the main script's
  were), then the surfaces, Aa, the card and the reader, then boot (its
  chapter drawn at once if its answer is in), then the rest in the next task
  (About and the banner are moved to where the page had them, for focus
  order, and the Library's tabs and lists into its panel), then tells the
  pieces every one is listening (`gospel-getter:pieces-ready`).

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

The page loads the pieces itself: Trunk inlines `ui/leptos-loader.js` in
`<head>`, which loads the glue (`ui-leptos.js`) and fetches and compiles the
module (`ui-leptos_bg.wasm`) while the page loads, then starts it once the
page has been read (`DOMContentLoaded`). Boot's first two commands don't
wait for it: the prefetch script in `<head>` sends `get_home` as the page
loads and, with its answer, the chapter's `get_reading` (unless comparing),
which boot takes over (`gospel-getter:boot`). The reader draws the chapter,
so the module is started early, and the reader with the first pieces, the
rest mounting in the next task; the chapter's face is preloaded (its text
waits for it). Trunk's own loader is an ES module import, which the
`no-modules` glue cannot be loaded by, so `Trunk.toml` turns it off. If the
module fails to load, the loader says so (`gospel-getter:pieces-failed`),
and the page's small inline fallback shows the chapter as plain text: the
saved position (or Genesis 1), read once, in the saved look (the pre-paint
script applies it), with the line "This chapter is shown as plain text: the
rest of the reader isn’t available." That line is all a reader is told.
Nothing in the chapter can be selected, turned or changed, and nothing
else answers: no button, menu or key opens anything or says anything (the
page has no script left to), no setting can be changed (reading mode can't
be left), and no update is looked for. The module may also load after the
page has read: boot's two commands are on their way (or answered) by then,
and the chapter is drawn as it starts; the Library's lists, the welcome's
startup question and the launch check are asked again once every piece
has mounted (`gospel-getter:pieces-ready`), and the picker's books are
sent again then.

The Content Security Policy allows exactly what this needs beyond the page's
own code: `'wasm-unsafe-eval'` in `script-src` (compiling WebAssembly) and
`'self'` in `connect-src` (fetching the module). The glue and the inlined
loader are covered as the page's three inline scripts are: `'self'` and
Tauri's build-time hashes.

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
at once) and its active option, within the key or click; and so does the
picker, for its books, a book's chapters, the marks and the focus. `openApp({ leptos: 'late' })` loads the pieces only
when the test calls `app.loadPieces()` (which then waits for the rest of
boot, the chapter included), and `leptos: false` never, the harness then
saying they failed as the loader would, so the page shows its plain-text
fallback and nothing else answers (`pieces.test.mjs`); boot's prefetch
runs either way, as it does in the app, and the harness waits for the
chapter and the Library's bookmark list only when the pieces load with the
page.

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
`SearchResultDto`); for the picker, `picker_text::Book` (`commands::BookDto`);
and the commands' argument objects. The backend's types carry `chrono` dates
and `deny_unknown_fields`; the shared crate would keep them serde-only with
the dates as `YYYY-MM-DD` strings, as the page always sent and received them.
