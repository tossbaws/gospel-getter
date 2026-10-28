# Changelog

What changed in each release of Gospel Getter, newest first. The format
follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and
versions follow [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added
- Your notes in the margin, in reading mode: the notes on the chapter
  sit in the open space to the right of the text, each beside the verse
  it's on, like notes in a printed Bible's margin, with a thin line
  beside a note on several verses. A long note shows its first lines and
  "more"; clicking a note selects its verses and opens it in the card,
  where you edit it as before. In a window too narrow for the margin, a
  small marker after each noted verse opens it instead. Turning the
  margin on or off never moves the text. It's on by default; Aa's "Notes
  in the margin", with reading mode's controls, turns it off. Not while
  comparing, and not printed.
- Gospel Getter reopens at the verse you were reading, not just the
  chapter: the verse at the top of the page, saved as you stop scrolling
  (and as the window is hidden or loses focus), goes back to the top,
  just where it was. A translation that doesn't number that verse opens
  the chapter at its top. It's kept on this computer only: backups are
  unchanged, and restoring one's reading position opens its chapter at
  the top.

### Changed
- Internal cleanup, no change in use.

## [2.19.1] - 2026-10-27

Four fixes from an outside review, and nothing else changes: updating
keeps your bookmarks, highlights, notes, reading position and settings.

### Fixed
- Restoring a backup could leave out some of your notes. Two notes on
  the same passage with the same words, written on different days, came
  back as one: after Replace, or on a new install, the other note and
  its dates were gone. Every note in the backup now comes back, each with
  its own dates, and importing the same backup again still adds nothing.
  Backups from earlier versions import as before. (Found in Codex's
  review, #152.)
- After Back or Forward couldn't open a chapter, trying again skipped a
  place: from Romans 8, a Back to Psalm 23 that failed went to John 3
  the next time. A Back or Forward that fails now changes nothing, so
  pressing it again, or Try again, opens the chapter it was going to.
  (Found in Codex's review, #152.)
- Deleting a note had no Undo when the selection card wasn't there: a
  note you edited from the Library on a verse the translation you're
  reading doesn't number, such as Matthew 17:21 in the BSB. The message
  at the foot of the window now has the same Undo, for as long as the
  card's, and it brings the note back with its words and dates. (Found
  in Codex's review, #152.)
- If the text for Copy or Copy chapter couldn't be read once, copying
  the same verses or chapter kept failing after the problem had passed,
  until you chose other verses or went to another chapter. The next copy
  now asks for the text again. (Found in Codex's review, #152.)

## [2.19.0] - 2026-10-26

2.19.0-beta.1, the pre-release, updates itself to this release.

### Changed
- **The reader now runs in Rust.** The whole interface, from the chapter
  you read to search, the books and chapters, Library, Aa, the selection
  card, the keyboard and About, now runs in Rust (Leptos, built to
  WebAssembly) instead of JavaScript. It looks and works as it did, so
  there's nothing to change or relearn. Your notes, bookmarks,
  highlights, reading place and settings carry over as they are, with no
  change to the database, and so do your exports: export is still
  format 3, and every backup imports as before. (#86, #89, #99, #100,
  #105, #107, #114, #115, #138, #139, #140)
- If that part of the app ever fails to load, you still see the chapter
  you were reading, as plain text, with the line "This chapter is shown
  as plain text: the rest of the reader isn't available." Nothing in it
  can be selected, and it doesn't look as if it can. (#138, #140, #142)

### Added
- **Support Gospel Getter** in About, next to Release notes. It opens
  the project's Ko-fi page in your browser. It's only a link: nothing
  pops up or reminds you, and nothing changes in the reading view or the
  menus. (#135)

### Fixed
- On Windows, moving through the chapter just before making the window
  wider or narrower than 1200 px could send your reading place back to
  where it was before, often the chapter's heading. That happened after
  a Go to, a search result, Back or Forward, or a quick scroll, right
  before a snap or a resize across 1200 px. The place you moved to now
  stays at the top. Linux wasn't affected. (#147)

## [2.19.0-beta.1] - 2026-10-26

A pre-release, for testing 2.19.0 before it ships. No installed copy
is offered it as an update: install it by hand from its release page.
It never offers to take you back to 2.18.1, and once 2.19.0 is out, it
updates itself to that.

### Changed
- **The reader now runs in Rust.** The whole interface, from the chapter
  you read to search, the books and chapters, Library, Aa, the selection
  card, the keyboard and About, now runs in Rust (Leptos, built to
  WebAssembly) instead of JavaScript. It looks and works as it did, so
  there's nothing to change or relearn. Your notes, bookmarks,
  highlights, reading place and settings carry over as they are, and so
  do your exports: export is still format 3, and every backup imports as
  before. (#86, #89, #99, #100, #105, #107, #114, #115, #138, #139, #140)
- If that part of the app ever fails to load, you still see the chapter
  you were reading, as plain text, with the line "This chapter is shown
  as plain text: the rest of the reader isn't available." Nothing in it
  can be selected, and it doesn't look as if it can. (#138, #140, #142)
- On Windows, the MSI installer lists this version as 2.18.1901 in
  Installed apps, since Windows Installer can only number a version with
  digits. The app, its About and the .exe installer say 2.19.0-beta.1.

### Added
- **Support Gospel Getter** in About, next to Release notes. It opens
  the project's Ko-fi page in your browser. It's only a link: nothing
  pops up or reminds you, and nothing changes in the reading view or the
  menus. (#135)

## [2.18.1] - 2026-10-23

### Fixed
- On Windows, making the window wider or narrower than 1200 px, where the
  books and chapters switch between the sidebar and the popover, could
  still move your reading place when you were partway into a chapter:
  the text jumped, and the verse you were reading moved down a few lines
  or another verse took its place at the top. The verse you were reading
  now stays at the top. This was on Windows only, since 2.17.2; Linux
  wasn't affected. (#130)

### Security
- tokio 1.53.3, the library the app uses for background work, for
  RUSTSEC-2026-0386, which marks part of 1.53.1 as unsound. Nothing
  you'll see changes. (#129)

The project's Claude Code settings (#128) change nothing you'll see.

## [2.18.0] - 2026-10-22

### Added
- **A book's name in search goes to its first chapter.** Type a book's
  name or its short form alone, such as `Psalms`, `Mark` or `1 John`
  (`/` or Ctrl+K), and search offers **Go to Psalms 1** first, so Enter
  goes there. The verses that mention the word are still listed below,
  and ↓ reaches them. (#124)

### Fixed
- A chapter the app couldn't read from its database did nothing and said
  nothing: the page just stayed as it was. Now the app says so, such as
  "John 4 couldn't be loaded.", with a **Try again** button, and the
  chapter you were reading stays on screen. (#121)
- Scrolling and, at almost the same moment, making the window wider or
  narrower than 1200 px could still send your reading place back to the
  chapter's heading. It now keeps the place you scrolled to. (#121)
- A backup with more than 10,000 bookmarks or 40,000 highlights couldn't
  be imported, even by the version that made it ("the most one file can
  hold is 10000"). Every backup now imports again, however many
  bookmarks or highlights it holds. (#122)
- Text you typed in a note while it was saving was lost: the panel closed
  and the note kept only what was there when you chose Save. The note
  now holds still until it's saved, so what's saved is what you see. If
  the save fails, you can edit it again, with your text kept. (#122)
- Switching chapters or translations quickly, before the first had
  loaded (WEB to BSB, say, or John 4 to Acts 1), could make the app
  reopen next time on the one you'd left. It now reopens on the chapter
  and translation that were on screen. (#122)

The faster tests (#123), and the rest of #121 (the database now records
its schema version, the tests also run on Windows, and the Windows app
embeds its manifest another way), change nothing you'll see.

## [2.17.2] - 2026-10-21

### Fixed
- **Aa** no longer says **Display** beside it. The bar is back to just
  **Aa**, beside the translation button. It still opens text size, line
  spacing, width, layout, theme and reading mode, and its hover text
  still reads "Text and display". (#111)
- Making the window wider or narrower than 1200 px, where the books and
  chapters switch between the sidebar and the popover, could move your
  reading place on Windows: the page jumped by a few lines, or to another
  verse. Your place now stays where you saw it, on Windows and on Linux,
  as it already did with the sidebar's own button. (#116, #118)

## [2.17.1] - 2026-10-20

### Fixed
- **Show more** at the end of search's results was easy to miss: a small
  button under the 50th verse. It now spans the width of the results and
  says how many more it shows, of how many, such as "Show 50 more (of
  235)". (#106)

### Security
- The updater's secure connection to GitHub, which it uses to check for
  and download updates, could in theory accept a certificate from an
  authority that has since been distrusted (RUSTSEC-2026-0382). The
  library that checks those certificates is updated to fix it. Updates
  were, and still are, checked against Gospel Getter's signature before
  they're installed, so this couldn't have let a forged update in.
  Nothing else changes. (#108)

## [2.17.0] - 2026-10-20

### Added
- **The translation button** in the top bar, just before **Aa**, shows the
  translation you're reading (**KJV**, **BSB** or **WEB**) and opens the
  translations and **Compare**. T opens it too.
- On a wider window, **Aa** says **Display** beside it.

### Changed
- The translations and **Compare** (with its "with" choices) moved from
  **Aa** to the new translation button. **Aa** keeps text size, line
  spacing, width, layout, theme and reading mode.
- **Export notes** says what it makes more plainly: a Markdown (.md) file
  of your notes, plain text that opens in Notepad or any text editor.
- **Export data…** and **Import data…** in the ☰ menu say what they do
  when you hover over them: save a backup of your bookmarks, highlights,
  notes and settings, and load a Gospel Getter backup from this or
  another computer.

## [2.16.3] - 2026-10-19

### Fixed
- Saving could still fail with "database is locked" on a busy or slow
  computer: your reading position, a bookmark, a highlight, a note, an
  import, or a book of the first launch's search index. The app's saves
  raced each other for the database, and one that kept losing gave up
  after 5 seconds. Now they take turns, one at a time, so none of them
  gives up, and reading never waits for a save. (#96)

## [2.16.2] - 2026-10-18

### Fixed
- On a fresh install, search could stay unavailable until you restarted
  the app: building the search index ran into the app's first saves (your
  reading position) and failed with "database is locked". It no longer
  does, it's tried again if it ever fails, and the first launch's index
  build is faster.

### Security
- The Windows app no longer contains the zip library the updater carried,
  with its three known advisories (RUSTSEC-2026-0367 to 0369): updates
  are the installers, published as they are, so there's nothing to unzip.
- The Linux app no longer carries the D-Bus libraries that came with the
  link opener, which three advisories mark as unmaintained
  (RUSTSEC-2026-0370 to 0372). Links (the credits, the release notes and
  the download page) still open in your browser as before.

## [2.16.1] - 2026-10-17

### Fixed
- The selection card no longer stands in the way of extending a
  selection: hold Shift and the card fades to a faint outline that clicks
  go through, so you can Shift-click a verse it covers. Letting go of
  Shift brings it back at once.

### Security
- A Content Security Policy for the app's webview: it loads only the
  app's own scripts, styles, fonts and images, and nothing in the page
  can reach the network.

## [2.16.0] - 2026-10-17

### Added
- **A command palette in the search box.** `/` or Ctrl+K opens the same
  box as before; as you type, it lists **Go to**, **Commands** and
  **Verses**, and each command shows its key, if it has one. Start with
  `>` to list commands only.
- **Copy as…** in the selection card, or Ctrl+Shift+C: text with its
  reference, with verse numbers, a Markdown quote, or plain text. The
  formats change no words.
- **Copy chapter…** at the end of each chapter copies the whole chapter in
  any of those formats.
- **Keyboard shortcuts**, on `?` or in the ☰ menu: every key the app
  answers, in one sheet.
- **j and k** select the next or previous verse, and Shift+j and Shift+k
  extend the selection. **[ and ]** go to the previous or next book.
- **Passage history:** Alt+← and Alt+→ go back and forward through the
  passages you jumped to (the picker, search, cross-references, the
  Library), each where you left it. It lasts until the app closes.

### Changed
- A chapter chosen in the picker, or Random chapter, opens at its top.
- **Check now** gives up after 20 seconds and says GitHub couldn't be
  reached, instead of waiting on a connection that never answers.
- The cross-references' credit links (OpenBible.info and CC BY 4.0) open
  in your browser through the app, as Release notes does; the page never
  opens a web page itself.

### Fixed
- Switching compare on or off while a chapter was loading could leave the
  wrong layout on screen.
- **Check now** while the banner was installing an update no longer hides
  the download's progress; it says the update is already downloading.
- Quick changes could show a stale bookmark list or highlight marks.
- An error with no message now says "Something went wrong." instead of
  leaving a blank status.
- Import messages cut long names from a file short.
- A passage that runs far past its chapter's end is no longer checked
  verse by verse.

### Security
- rustls 0.23.46, for RUSTSEC-2026-0346 to 0349, in the updater's
  connection to GitHub.

## [2.15.0] - 2026-10-16

### Added
- **The Berean Standard Bible (BSB)**, built in and offline like the KJV
  and WEB, in the public domain (CC0). Its text and translators' footnotes
  are word for word Berean's own. The 16 verses the BSB doesn't number are
  left out where they fall, and the footnote on the verse before each
  gives its wording.
- **Aa → Translation** lists the translations, a line each, with each
  one's date and licence. The KJV stays the default.
- **Compare any two translations:** **Compare (C) with** under **Aa**
  chooses the other one, and it's remembered.
- **About Gospel Getter** credits every text and the cross-references.

### Changed
- The installers are about 3–3.7 MB smaller, even with the BSB added.
- Backups carry compare's translation; 2.12.0 to 2.14.0 skip it with a
  notice.

## [2.14.0] - 2026-10-16

### Added
- **Aa → Layout:** **Verses** (the default) or **Flowing**, which runs each
  chapter on as prose with small raised verse numbers.
- **Cross-reference previews:** click a reference in the selection card to
  read that passage right there, with **Go to** to open it.
- The chapter-end Cross references are links, each opening its passage,
  selected.

### Changed
- **One welcome card replaces the four-page tour** on a new install: pick a
  look, or a place to start. ☰ → **Show welcome** opens it again.
- Backups carry the layout; 2.12.0 and 2.13.0 skip it with a notice.

## [2.13.0] - 2026-10-15

### Added
- Previous and next chapter buttons at the end of each chapter.
- **Selecting verses opens a card** that floats under them, with Copy,
  Bookmark, Add note, the highlight colors, footnotes, notes and
  cross-references. The text never moves when you select.
- **Aa → Width:** Narrow, Normal and Wide.
- Ctrl+=, Ctrl+- and Ctrl+0 for the text size.
- The window's title says where you are, and the window reopens at its
  size and place.
- **Check now** in About shows an update right there, with its release
  notes and **Update now**.

### Changed
- A chapter's Footnotes and Cross references are collapsed until you open
  them.
- The side chapters give way on narrower windows; a roomier Psalms grid;
  click anywhere on a Library row to open it.
- Backups carry the width; 2.12.0 skips it with a notice.

### Fixed
- Matrix's yellow highlight is amber, and easy to read.
- **Export notes…** doesn't offer a chapter range when the book has notes
  in only one chapter.

## [2.12.0] - 2026-10-14

### Added
- **A new look:** scripture set in the bundled Literata typeface, and three
  new themes: **Chi Rho** (the new default), **Vellum** and **System**.
- **The reference in the bar** ("John 3 · KJV"), or G, opens a book and
  chapter picker with Random chapter: a sidebar on wide windows, a popover
  on narrow ones.

### Changed
- The chapter is the page: the title banner and the grid of books are gone.
- If you never picked a theme, the app opens in Chi Rho; Vaporwave is
  still under **Aa**.
- The smallest window is 640×480.
- **Import data…** skips a display setting it doesn't know, and names it,
  instead of refusing the whole file. A backup made in 2.12.0 needs 2.12.0
  or later to import.

## [2.11.0] - 2026-10-12

### Added
- **Choose which notes to export:** **Export notes…** filters by books, a
  chapter range for one book, and when the notes were written, with a live
  count of the notes that match. The file's title and name describe the
  selection. An unfiltered export is unchanged.

---

Older versions (back to 1.0.0) remain as git tags only, and only the
latest release and the one before it stay on GitHub's releases page;
2.6.0 and later update themselves to the newest version.
