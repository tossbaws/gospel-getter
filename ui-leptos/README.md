# ui-leptos: the page's Rust pieces

Gospel Getter's page (`ui/index.html`) has been moved to Rust a piece at a
time. This crate holds the pieces, written with [Leptos] and compiled to
WebAssembly; they run inside the same Tauri app. The page keeps only three
small inline scripts of its own (the pre-paint script, boot's prefetch and
the plain-text fallback) and the loader. What crosses between them and the
pieces, and the events the pieces send each other, are in `src/bridge.rs`.

The saved settings' one table, their keys, values and names (in the page
and in an export file), is `../shared/preferences.rs`, a file the backend
(`src-tauri`) compiles too, as its `preferences` module: its export file's
display settings are that file's enums. The page's pre-paint script, the
first in its `<head>`, applies the saved look before the first frame from a
table generated from it (`prepaint_script`); `src-tauri/tests/frontend_markup.rs`
keeps the two the same.

[Leptos]: https://leptos.dev

## Layout

`ui-leptos/` is its own crate (Rust 2024, `cdylib`), separate from
`src-tauri/`: one targets `wasm32-unknown-unknown`, the other the desktop.
They share the files under `../shared/`, compiled into both by `#[path]`.

## Module map

The one list of the crate's modules: every `mod` in `src/lib.rs`, each
`src/<name>.rs` unless it's shared. Each module's own `//!` says the rest.
The `_text` modules, and `text`, `install`, `shortcut_table`, `palette`,
`note_markdown` and the top of `export_choice`, hold the pieces' words,
numbers and rules, free of the DOM and unit-tested natively (`cargo test`).

The root and what the pieces share:

- `lib`: the crate root; mounts the pieces when the module loads (below).
- `bridge`: what crosses to the page's remaining scripts, the globals set for the tests and the GUI checks, the custom events the pieces send each other, and the page controls they use.
- `ipc`: Tauri commands through the global the page uses, `window.__TAURI__.core.invoke` (a `wasm_bindgen` extern plus `serde-wasm-bindgen`), with the same command names and argument objects.
- `dto`: the other shapes the commands exchange, mirrored from the backend with the names the JSON carries (see "What a shared crate would replace").
- `dom`: the DOM helpers the pieces share (focus that never scrolls, the modals' Tab trap, listening, timers).
- `intent`: whose an answer is once it's back, after the reader has done something newer.

The page's glue:

- `boot`: startup, from the translations to the launch check; it takes over the `get_home` and `get_reading` the page's prefetch script sent from `<head>`.
- `page`: the toast, Export and Import data…, the welcome, the shortcuts sheet and search opened, the palette's page commands, the launch check.
- `page_text`: the toast's words, Export data…'s summary and Compare's partner.
- `keyboard`: the keyboard dispatcher, on `document`.
- `keyboard_text`: which key does what, j and k's steps, and how far a verse is scrolled into view.
- `clicks`: the click dispatcher, on the body.

The dialogs:

- `export_choice`: ☰ → Export data…'s chooser: Everything (the backup), or My notes, which opens Export notes.
- `export_notes`: the Export notes dialog.
- `text`: Export notes' words, numbers and filter.
- `import_preview`: Import data…: the file's preview, then confirm or cancel.
- `import_text`: the import's sentences and its add-or-replace choice.
- `note_editor`: the note editor.
- `note_text`: a note's length rule and its drafts.
- `shortcuts`: the keyboard shortcuts sheet.
- `welcome`: the welcome.
- `install`: the welcome's fresh-install rule.
- `about`: About, with the updater's section.
- `updates`: the updater: Check now, the launch check, the banner, the update shown in About, and installing with its progress.
- `update_text`: the updater's sentences, `megabytes` and download progress.

The header, the Library and search:

- `surfaces`: the header's popovers and panels and the picker's two shapes, the sidebar or the popover: one open at a time, their buttons, focus in and back, a click outside, Escape, the menu's keys and the window crossing 1200px.
- `surfaces_text`: the surfaces' table, the sidebar's rule and the ☰ menu's keys.
- `aa`: Aa and the translation button's popover: the translations, Compare, text size, line spacing, width, layout, theme, reading mode and the launch check's setting.
- `aa_text`: the size keys and stepping, and what Compare shows beside the text.
- `prefs`: the saved settings' store, by the one table in `preferences`.
- `nav`: the top bar's Back, Forward and Recent places, and the chapter arrows.
- `nav_text`: their words and rules, and the Recent places list as it's kept.
- `picker`: the book and chapter picker's books and chapters, the marks for the chapter being read, Random chapter, and its clicks and keys.
- `picker_text`: the compact grid, Random chapter's pick, the keyboard's moves and how far the picker scrolls.
- `library`: the Library's tabs and lists: Bookmarks, Highlights with its color filter, and Notes, each loaded from the backend here, once per reload.
- `library_text`: the lists' rows, color names, filter, notes search and focus arithmetic.
- `search`: search, Go to and the command palette, in one box.
- `search_text`: the search's sentences, active option and marked words.
- `palette`: the palette's commands, their reasons and its word matching.
- `shortcut_table`: the one table of keyboard shortcuts, which the sheet lists and the palette's key hints come from.

The reader, the selection card and notes:

- `reader`: the chapter on screen, how it gets there, and the passage history.
- `reader_text`: the chapter's markup, byte for byte the page's former templates (`fixtures/genesis-1.html` is their output, which a test compares), Compare's rows, the reference, and the passage history's bookkeeping.
- `card`: the selection card: the verse selection, Copy and Copy as…, the card's bookmark, highlight and notes, the verses' marks and the Shift ghost.
- `card_text`: the card's markup and placement.
- `margin`: the notes in the margin in reading mode.
- `margin_text`: where a margin note sits, how notes stack, and the margin's markup.
- `findings`: the AI-flagged findings an agent saved at the reader's request, in the margin, Compare's notes column and the Library, with Aa's switch for them.
- `findings_text`: the findings' rules and markup.
- `note_markdown`: a note's text as Markdown wherever it shows: the subset, everything else escaped, the references the backend found in it as links.
- `note_links`: the links in a note: a reference goes to its passage, a web link opens through the backend.
- `note_copy`: a note's Copy as Markdown.

Shared with the backend, from `../shared/`:

- `preferences`: the saved settings' one table (above).
- `verse_parts`: a verse's parts and footnotes as the backend shows them, which the reader draws.
- `passage_reference`: how a passage is named.
- `book_aliases`: the books' abbreviations, by which the backend reads a typed reference and the Library's notes search finds a note's book.

`src/lib.rs` mounts the pieces when the module loads: the page's own
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
cache the first time: wasm-bindgen's is this crate's in `Cargo.lock`, and
wasm-opt's is pinned in `Trunk.toml` (`[tools] wasm_opt`). CI doesn't let
Trunk download them: it fetches those releases and Trunk's own itself
(`tools/ci/build-tools.txt`), and puts them on `PATH`. Trunk builds from
`Cargo.lock` as committed (`locked = true` in `Trunk.toml`), so after
changing this crate's dependencies, update the lock file first (`cargo
update -p <crate>`, or a plain `cargo build` here) and commit it with them.

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
