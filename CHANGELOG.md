# Changelog

What changed in each release of Gospel Getter, newest first. The format
follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and
versions follow [Semantic Versioning](https://semver.org/).

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
