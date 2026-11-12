# Changelog

What changed in each release of Gospel Getter, newest first. The format
follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and
versions follow [Semantic Versioning](https://semver.org/).

## [2.29.2] - 2026-11-12

Fixes for the KJV's verse numbers in ten chapters, a failed start that
said nothing, updates that can't be installed, Copy as Markdown and
dialogs, and search words kept out of the log. Updating keeps your
bookmarks, highlights, notes, findings, reading position and settings.

### Fixed
- The KJV: ten chapters had verses at the wrong numbers, so there a verse
  number could show another verse's words. These are 1 Samuel 20, 1 Kings
  22, Matthew 2, 22 and 26, Mark 4, 7 and 8, 3 John and Revelation 12,
  and with them Revelation 13:1, whose start was numbered as Revelation
  12:18. They're now numbered as in every KJV, taken from eBible.org's KJV
  as it has them, so they use its typography (’ and ¶, for example);
  every other verse is exactly as before. Updating makes the change once,
  then builds the KJV's search index again in the background. **About
  Gospel Getter** credits eBible.org too.
- Your notes, bookmarks, highlights and findings in those chapters are
  kept as they were, on the same verse numbers. Because the numbers were
  wrong before, one may now sit beside different words: the ones that
  number has in the KJV. One on a verse number the KJV doesn't have any
  more (1 Samuel 20:43, 1 Kings 22:54, 3 John 1:15 and Revelation 12:18)
  is kept, listed as not numbered, and still exports and imports; a
  reading place there opens its chapter at the top. The 28 New Testament
  margin notes in those chapters are gone, because eBible.org's KJV
  doesn't have them.
- If Gospel Getter can't start, it now says why and where its log is,
  instead of closing without a word.
- AI agents and `gospel-getter-cli`: the WEB's verses it doesn't number
  read as not numbered even before the app has started once since
  updating, and `save_findings` and `verify_quote` say when a passage has
  verses that aren't numbered.
- A search that fails no longer writes the words you searched for in the
  log.
- Windows: if an update's installer doesn't start, Gospel Getter stays
  open and usable and says the update didn't start.
- Linux (`.deb`): when an update can't be installed, for example because
  the password prompt was cancelled, the message says nothing was changed.
- **Copy as Markdown** always copies the note you clicked, with its own
  passage, and copies at the first click on a verse with many notes.
- Dialogs no longer open over each other: **Keyboard shortcuts** or
  **About** opened while **Export notes** is getting ready keeps it from
  opening on top. Tab stays inside About, and the mouse's Back and Forward
  buttons don't move the page behind a dialog.
- Smaller fixes: switching translation while a chapter loads opens the
  chapter you asked for; changing a highlight while the highlights reload
  no longer loses either change; **Check now** stays disabled while any
  update check runs; a click on **Export notes**' **Export…** isn't lost
  when it also leaves a chapter field; a note editor's **Other notes** are
  always its own passage's; and a link inside a note link's text shows as
  text.
- Files, the log and AI agents: saving a backup or export also writes
  the folder's change to disk on Linux, and clears Gospel Getter's own
  temporary files over an hour old from that folder; the log keeps to its
  size even when it can't start a new file; and an AI agent's read made
  just as the app starts is made again, so it sees what the app has saved.
- Backups keep the same data formats: 3, or 4 with AI findings or the
  switch on, restored by the same versions as before.

Behind the scenes: a release's signed installers are kept as artifacts of
their own, with each installer's checksum (`INSTALLER-SHA256SUMS`), and
the unsigned ones are never overwritten; `packaging/uninstall.sh` names
your data and the menu to export it from, and asks before it removes
anything; and GUI checks that can't run on Windows say SKIP instead of
passing.

## [2.29.1] - 2026-11-10

Fixes for verse ranges in one-chapter books, the WEB's verses it doesn't
number, note drafts, search, backups and the data folder, and a small log
file for when something goes wrong. Updating keeps your bookmarks,
highlights, notes, findings, reading position and settings.

### Fixed
- **Import data…**: after an import, choosing a file that can't be
  imported no longer leaves its dialog impossible to close.
- Note drafts: pressing **Restore** while a note was being saved or
  deleted could lose the restored text; Restore and Discard now wait for
  it. A draft saved while the computer's clock ran fast is no longer
  deleted when the clock is put right.
- Search: **Show more** after changing the words or the translation no
  longer mixes two searches' results.
- Verse ranges in one-chapter books ("Jude 5-7", "3 John 2-4") work in Go
  to, in links in your notes and for AI agents, as do "Gen 1:1-1:3" and a
  reference ending in a period ("John 3:16.").
- The WEB's verses it doesn't number (Luke 17:36, Acts 8:37, 15:34 and
  24:7, and Romans 16:25) say so, as the BSB's do, instead of showing an
  empty verse. Updating takes the five out once; your notes, bookmarks,
  highlights and findings on them stay as they are.
- Backups and exports: "Saved …" counts AI findings; a backup or notes
  export holds what changed while its save dialog was open; and file
  dialogs open over Gospel Getter's window and stay with it.
- The data folder and the database are yours alone on Linux, as backups
  and exports already are, and a data folder whose path has `%`, `?` or `#`
  in it opens the database in it. A path that isn't valid UTF-8 is refused
  instead of opening a database at a similar name.
- Pressing **Bookmark** twice quickly changes the bookmark once; `[` and
  `]` work on keyboards that type them with AltGr; and opening a place
  from **Recent places** in another translation no longer loads the
  chapter you're on again first.
- The window's size and place are kept when Gospel Getter updates itself
  on Windows, and a window brought back from off the screen no longer
  grows by its frame.
- AI agents: `list_findings --request` lists that request only, exactly
  as it was saved (capitals count), and a request counts each verse once
  where its findings overlap. `list_bookmarks`, `list_highlights` and
  `list_notes` answer faster.
- Smaller fixes: a file with a very long name can be saved; finding a
  chapter's last verse is one quick lookup; the notes export's count reads
  no note text; **My notes** no longer opens over a note editor opened
  while it was getting ready; and timers and the note-copy cache no longer
  keep memory they don't need.
- The app keeps a small log of what it did and what went wrong (never your
  notes or what you search for): `gospel-getter.log`, up to 1 MB with the
  two before it, in `~/.local/share/com.tossbaws.gospel-getter/logs` on
  Linux and `%LOCALAPPDATA%\com.tossbaws.gospel-getter\logs` on Windows.
  `RUST_LOG` sets how much it says.
- Backups keep the same data formats: 3, or 4 with AI findings or the
  switch on, restored by the same versions as before.

Behind the scenes, nothing you'll see changes: release installers are
signed only in a job of their own that runs only for a release, so no
build has the signing key; CI's actions are pinned to exact commits; CI
runs more tests (doc tests, Clippy without all features, the GUI checks'
own tests, locked builds of the page); each Windows build installs and
removes the MSI as well as the setup.exe; and the app's page is given no
Tauri permissions, since it uses none.

## [2.29.0] - 2026-11-10

One Gospel Getter at a time, a safe move from the browser-based version,
and a search index and quote checks that leave nothing out. Updating keeps
your bookmarks, highlights, notes, findings, reading position and settings.

### Changed
- Opening Gospel Getter while it's already open brings the window that's
  open forward, unminimised and focused, instead of starting a second copy
  on the same data.

### Fixed
- Coming from the old browser-based Gospel Getter, the first launch brings
  its database over as a snapshot it has checked, with everything it has
  saved, even while it's still running, and never half a copy. If that
  can't be done, the old file is left as it was, and Gospel Getter says
  what went wrong and what to do (close the earlier Gospel Getter if it's
  open and open Gospel Getter again, or move that file somewhere else to
  start without it), and doesn't start. The next launch tries again.
- The search index is marked complete only when every verse is in it,
  even if two launches build it at once. An index an older version left
  short is built again.
- Cross-references are added once, even when two launches start together.
  Before, both could add them, leaving every cross-reference twice. An
  install where that already happened stays as it is.
- Quote checks (`verify_quote`, in `gospel-getter-cli` and over MCP) list
  every place a quote is, each once and in Bible order, including where it
  runs across the end of a verse. Before, a quote whole in one verse could
  hide the same words elsewhere across two. With more than 200 places, it
  lists 200 and says "at least".
- Backups and Markdown exports are readable only by you on Linux, from the
  moment they're written, and replacing one keeps it so. Before, they could
  be readable by anyone on the computer.
- AI agents save findings only when you ask them to save, note or mark
  verses. Asked only to find, list or quote verses, they answer and can
  offer to save them. That's what Gospel Getter asks of the agent; what the
  app enforces is still **Let AI agents save findings**.
- `packaging/install.sh` installs the AppImage it has just built, never an
  older one left beside it. If there's none, more than one, or one of
  another version, it stops, says what's there and deletes nothing.
- The draft Arch recipe (`packaging/PKGBUILD`) builds the app's page before
  the app, with Trunk and its tools pinned and checksummed, and the build
  itself runs offline. It has been built with `makepkg` in a clean Arch
  container, and it's still not on the AUR.
- Backups keep the same data formats: 3, or 4 with AI findings or the
  switch on, restored by the same versions as before.

Behind the scenes, nothing you'll see changes: CI keeps the tools it
builds with cached and checks each against its pinned checksum before
using it, so a build with them cached downloads none of them, and
`gospel-getter-cli`'s MCP library is rmcp 3.9.0, for RUSTSEC-2026-0462. On
Linux, one app at a time works over the session bus, so the app again
carries D-Bus libraries that three advisories mark as unmaintained, with no
known vulnerability (RUSTSEC-2026-0370 to 0372, as before 2.16.2).

## [2.28.0] - 2026-11-09

A switch for the keys that work pressed on their own, imports that name
everything they change, and fixes for slow answers that undid what you'd
done since. Updating keeps your bookmarks, highlights, notes, findings,
reading position and settings.

### Added
- Single-key shortcuts can be turned off: choose **Aa**, then under
  **Keyboard** turn off **Single-key shortcuts** (it's on by default).
  The keys that work pressed on their own (`G`, `T`, `C`, `N`, `j`, `k`,
  `[`, `]`, `/` and `?`) then do nothing, so they can't get in the way of
  speech input or a screen reader. The shortcuts with Ctrl or Alt, the
  arrow keys and Esc still work, and Ctrl+K still opens the search box and
  the command palette. **Keyboard shortcuts**, in the ☰ menu or the
  palette, says when they're off. The switch is kept on this computer, not
  in backups.

### Fixed
- Import names AI findings. The preview says how many AI findings the
  file has and how many are new. **Replace**, its warning and the Import
  button name the AI findings replacing would delete, and afterwards the
  message says what happened to them. A file holding only AI findings now
  offers **Replace** too.
- A backup no longer turns **Let AI agents save findings** on or off by
  itself. When the file's setting differs from yours, the import offers a
  tick to set it as in the file: unticked when it would turn saving on,
  ticked when it would turn it off. Without the tick the switch stays as it
  is, adding or replacing.
- Once you press **Import** it can't be cancelled midway: Cancel, Escape
  and the dialog's choices wait for it, and then it says what it did. It no
  longer says "Nothing was changed" for an import that went through. If
  the import fails, the choices come back, to try again or cancel.
- A backup that holds AI findings now records **Let AI agents save
  findings** whether it's on or off, so restoring one made with it off
  turns it off again. A backup without findings, made with the setting
  off, is still format 3, as before.
- A note's **Edit** in the Library no longer opens the note over another
  note you opened while it was on its way. An open note with text you
  haven't saved is never replaced by another: the app says you're still
  writing, and your text stays.
- Enter in the search box runs only what you pressed it on. Pressing
  Enter straight after typing and then Escape no longer goes to the
  passage when the search answers, and nor does typing on, switching
  translation or closing the box.
- Switching Compare's translation while the first comparison is still
  loading no longer drops the page back to one column.
- Aa's **Let AI agents save findings** switch shows the setting as last
  saved: a slow read from opening Aa, or an older save that failed, no
  longer flips it back.
- Removing a bookmark or highlight in the Library no longer pulls focus
  away from a note or the search box you've opened since.
- Backups keep the same data formats: 3, or 4 with AI findings or the
  switch on, restored by the same versions as before.

## [2.27.0] - 2026-11-08

One place to export, in the ☰ menu, and a limit on AI findings. Updating
keeps your bookmarks, highlights, notes, findings, reading position and
settings.

### Changed
- One place to export: **☰ → Export data…** now opens a small chooser.
  **Everything (backup)** saves the same backup as before, and **My
  notes** opens the same Export notes window as before. My notes is
  greyed out while there are no notes, and Everything while an export or
  import runs. The Library's Notes tab no longer has an **Export notes…**
  button, so its search box runs the full width of the tab. The command
  palette (/ or Ctrl+K) is unchanged: its **Export notes…** and **Export
  data…** still go straight to each, with no chooser in between.
- Gospel Getter keeps at most 10,000 AI findings, as it does notes. A
  save or import that would go past that changes nothing and says so. A
  verse already saved for the same request doesn't count again.
- `gospel-getter-cli`'s package description no longer says read-only: it
  reads your install, and its one write is saving findings.
- Backups are unchanged: Everything (backup) saves the same file, in the
  same data formats, as 2.26.0's Export data.

## [2.26.0] - 2026-11-07

An AI agent can now mark every verse on a topic for you to read yourself.
It's off until you turn it on, and updating keeps your bookmarks,
highlights, notes, reading position and settings.

### Added
- AI-flagged findings. Ask your agent something like "note every verse
  that deals with grief so I can read them", and it can mark each verse
  it finds, so you can read each one for yourself and judge whether it
  got them right. It's off until you turn it on: choose **Aa**, then under
  **AI agents** turn on **Let AI agents save findings**. Until you do,
  the agent's tool answers with how to turn it on, and saves nothing.
- A finding says only who found the verse and for which of your
  requests, "Found by Claude for your request “grief”.", and never what
  the verse means: there's nowhere to store anything that would. The
  request is a short topic label, at most 60 characters, with no colon,
  semicolon, dash or line break, so it can't carry commentary either.
- In reading mode, a finding sits in the margin beside its verse, after
  your own notes there, collapsed as a violet **AI** badge and the verse
  (**AI ×2** when there are two). Click it, or press Enter or Space, to
  open it, with **Keep as mine** and **Remove** (with Undo); click again
  to fold it. It opens collapsed every time. A flagged verse has a dotted
  violet underline. Compare's My notes column shows findings the same
  way. In a narrow window, a violet AI marker follows the verse (after
  your own note's pencil, if it has one) and opens them in the text.
- **Keep as mine** opens your note editor with the finding's words in
  it, all selected, so typing anything replaces them. Saving makes it
  your own note and removes the finding; cancelling changes nothing.
- The Library's Notes tab lists findings under **Found by AI agents**,
  apart from your notes and grouped by request. Each group says who found
  it, how many verses and when, links each verse to its passage, and has
  **Remove all** (with Undo).
- An agent can add findings, never edit or remove them, and never touch
  your notes, bookmarks or highlights. Saving the same verse for the same
  request again changes nothing. Findings an agent saves while the app is
  open show the next time a chapter is shown or the Library is opened.
  Export notes leaves them out.
- In `gospel-getter-cli`, `save_findings` saves findings, up to 500 verses
  a call (a reference that fails gets its own error, and the rest still
  save), and `list_findings` lists what's saved, with no text. In a
  terminal they're `save-findings` and `list-findings`. A finding names
  the agent it's given, or else the MCP client's name, or "AI agent" in a
  terminal. Saving a finding is the CLI's one write: everything else it
  still reads through a read-only connection. Open 2.26.0 once before the
  first save.
- One MCP prompt, `summarise-my-notes`, asks an agent to summarise and
  organise your own notes on a book. It works only with your words,
  quotes any verse they point to exactly, and never summarises the
  verses.
- Every chapter of every translation is an MCP resource,
  `gg://<translation>/<book>/<chapter>` (such as `gg://kjv/john/3`), so an
  agent can attach a chapter as context. Its text is exactly what
  `get_passage` gives.

### Changed
- The MCP server's instructions now tell an agent to quote scripture
  exactly, and never to summarise, paraphrase or interpret it.
- Export data keeps your findings and the switch. A backup with findings,
  or made with the switch on, is data format 4: 2.26.0 and newer restore
  it, and 2.25.x and older refuse it with their "made by a newer version"
  message. A backup with neither stays format 3, exactly as 2.25.x writes
  it, so 2.8.0 through 2.25.x still restore it in full.

### Fixed
- In a list of references, a part of only numbers after a range across
  chapters now goes on from the chapter the range ends in: "John
  3:16-4:2, 5" is John 4:5, not John 3:5.
- `verify_quote` gives its look-alike note in one more case. Where the
  closest match ties between two places (Exodus 9:29 has "the LORD"
  twice), it now reports the difference at the place where the
  characters only look alike: a quote with a curly apostrophe in "the
  LORD'S" gets the note about it. The verdict doesn't change.

## [2.25.1] - 2026-11-07

The app and `gospel-getter-cli` are unchanged from 2.25.0: updating keeps
your bookmarks, highlights, notes, reading position and settings.

### Fixed
- On release day, the automatic build no longer cancels the release's own
  build, so a release commit's checks on GitHub show green instead of a red
  X. (#191)

## [2.25.0] - 2026-11-07

An AI agent can now check its own quotations and read many passages at
once. The app's window doesn't change, and updating keeps your bookmarks,
highlights, notes, reading position and settings.

### Added
- `verify_quote`, a new tool in `gospel-getter-cli`, checks a quotation
  against the real text of one translation, and no other, character for
  character. Only whitespace at either end of the quote is trimmed; case,
  punctuation, the kind of apostrophe and every space inside it count. It
  answers `exact` (the whole passage), `exact_part` (an unbroken part of
  it), `differs`, `elsewhere` (not in the passage given, but in another)
  or `not_found`. Where the quote differs, it says at which character and
  which word it first does, and gives the real text from there. When the
  two characters there only look alike, such as a straight and a curly
  apostrophe or a space and a no-break space, a note names both. Without
  a reference, or when the quote isn't in the one given, a phrase search
  in the same translation looks for the real reference. The search only
  suggests where to look: each passage it leads to is then compared
  character for character, so the tool never reports a match that isn't
  one. A quotation can be up to 10,000 characters.
- `get_passages`, another new tool, reads many passages in one call, all
  in one translation: at most 50 references and 500 verses, or the call
  is refused. Each passage comes back as `get_passage` gives it, in the
  order asked, and a reference that fails gets its error in its place
  without stopping the rest.
- `get_passage` takes several references separated by commas or
  semicolons: "John 3:16, 18; Rom 5:8" is John 3:16, John 3:18 and
  Romans 5:8. Several come back as `get_passages` gives them, with the
  same limits.
- In a terminal, every subcommand also answers to a kebab-case name, such
  as `get-passage`, `reading-place` or `verify-quote`. The old names still
  work.

## [2.24.2] - 2026-11-05

The app is unchanged from 2.24.1. This release makes one frontend test
reliable, and you won't see a difference: updating keeps your bookmarks,
highlights, notes, reading position and settings.

The welcome's history test no longer picks a random chapter that can match
the one already open. Picking the open chapter records no history, so the
test would fail: a flake seen in CI about once in 1,200 runs.

## [2.24.1] - 2026-11-05

A fix for `gospel-getter-cli` on Windows, and nothing else changes: updating
keeps your bookmarks, highlights, notes, reading position and settings.

### Fixed
- On Windows, `gospel-getter-cli` needed `VCRUNTIME140.dll`, part of
  Microsoft's Visual C++ runtime, which not every PC has. Without it, the
  CLI and its connection to an AI agent wouldn't start. The CLI now has the
  runtime built in. The app itself never needed that file.

## [2.24.0] - 2026-11-05

An AI agent on your computer can now read your install. Updating keeps your
bookmarks, highlights, notes, reading position and settings.

### Added
- `gospel-getter-cli`, a small separate program that lets an AI agent on
  your computer (Claude Code, Claude Desktop or any MCP client), or you in
  a terminal, read your own install. It only reads: it opens the database
  with a read-only connection, so it can't change anything, and it has no
  network port. Its nine tools list the translations, get a passage's text,
  search a translation, compare a passage across translations, get
  cross-references, tell where you last were reading, and list your
  bookmarks, your highlights and your notes. The `.deb` puts it on your
  `PATH`, the Windows installers put it beside the app, and AppImage users
  download it on its own from the release. Setup is in the README, under
  [Use with AI agents](README.md#use-with-ai-agents). What it reads goes
  to the agent's AI service, so connect only agents you trust. The app's
  window doesn't change.

### Changed
- The copyright holder is now Chi Rho Code LLC.

## [2.23.0] - 2026-10-31

Your notes beside the text, from Compare. Updating keeps your bookmarks,
highlights, notes, reading position and settings.

### Added
- Your notes beside the text: Compare's **with** choices end in
  **Notes**, which shows your notes on the chapter in a **My notes**
  column where the second translation would be. The text stays as you
  read it, in Verses or Flowing, and never moves as notes come and go.
  Each note sits beside its first verse, labelled with its verses (a thin
  line beside a range), below the one above where they'd overlap, and
  folds after about ten lines with **more**. Clicking a note selects its
  verses and opens it in the card, where you edit it, and the column
  follows every change. In a narrow window each note sits under its first
  verse. Reading mode still shows the margin notes, printing leaves the
  column out, and the choice is remembered on this computer but not put
  in backups, so backups are unchanged and still import into 2.22.

## [2.22.1] - 2026-10-31

Fixes for holding Shift to select verses, and nothing else changes:
updating keeps your bookmarks, highlights, notes, reading position and
settings.

### Fixed
- On Linux, after you held Shift to Shift-click more verses, the selection
  card could stay faint and unclickable once you let go of Shift. On
  Wayland with an input method such as fcitx5, the app may never hear
  Shift come up; it now also goes by the mouse and keyboard themselves,
  so the card comes back at the latest when the mouse moves. Clicking
  the card before you've moved the mouse brings it back too, and leaves
  your selection as it was, instead of going through to the verse
  underneath.
- With both Shift keys held, letting go of one brought the card back while
  the other was still down, so the next Shift-click landed on the card.
  It stays faint until you let go of both.
- A Shift-click with nothing selected opened the card at full strength
  while Shift was still held, so the next Shift-click landed on it. A card
  opened that way is faint at once.
- Letting go of Shift a moment before the mouse button cleared the
  selection instead of extending it: the button came up on the card that
  had just come back. It now counts as the Shift-click you meant, and the
  selection stays.

## [2.22.0] - 2026-10-30

Back, Forward and Recent places in the bar, chapter arrows, exact-phrase
search, a commentary of your notes to export or print, and panels that
grow with Text size. Updating keeps your bookmarks, highlights, notes,
reading position and settings.

### Added
- Back and Forward buttons in the top bar, left of the reference. They do
  what Alt+← and Alt+→ do, and each one's tooltip says where it goes
  ("Back to Psalms 23"); with nowhere to go, it's dimmed. Between them, ▾
  opens Recent places: the last 10 places you went to on purpose (from
  the picker, search, a link, the Library or Back and Forward), newest
  first, one per chapter, each with its translation, and the one you're
  on marked "you are here". Choosing one opens it in the translation you
  read it in. Turning chapters adds nothing. Unlike Back and Forward,
  the list lasts after the app closes. It's kept on this computer only,
  and neither backups nor exports include it. In a narrow window Forward
  is the first item in that menu.
- ‹ and › either side of the reference turn to the previous and next
  chapter, as ← and → do, and their tooltips name it ("Next chapter: John
  4"). At Genesis 1 and Revelation 22 the one with nowhere to go is
  dimmed. The bar's new buttons aren't shown in reading mode.
- Exact-phrase search: put words in quotes, such as "love one another",
  to find verses with those words together, in that order. Straight or
  curly quotes both work, and a phrase mixes with other words: `"love one
  another" commandment` finds verses with the phrase and the word. A
  quote that isn't closed counts as ordinary words, and `"John 3:16"` in
  quotes searches for those words instead of going to the verse.
- Export notes has a Commentary layout, "Commentary (notes by verse
  number)": your notes under book and chapter headings, each after its
  verse numbers ("3:16–18"), in Bible order, without the Bible text. It
  takes the same choices of books, chapters and dates, and works with one
  file per book. With it chosen, Print… prints the commentary.

### Changed
- The selection card, the menus and popovers, the Library, the picker,
  search and the notes in the margin now grow with Text size, a little
  less than the text does. At Medium they're as before, and the top bar
  keeps its size.
- The neighboring chapters on either side are a little fainter, and
  lighten more gently on hover.

## [2.21.0] - 2026-10-28

Search your notes, drafts kept while you write, Markdown and linked
references in notes, Copy as Markdown, and exporting one file per book.
Updating keeps your bookmarks, highlights, notes, reading position and
settings.

### Added
- Search your notes: a box at the top of the Library's Notes tab
  filters the list as you type. A note is found when every word you
  type is in its text or its reference (the book's name or its usual
  abbreviation, like "Rom" or "Mk", the chapter, a verse), whatever the
  capitals or accents. It says how many of your notes it shows, marks
  the words it found, and Escape clears it.
- Notes keep what you're typing as a draft: if the app quits, or you
  close the note with Cancel and the new "Keep for later", the text is
  kept on this computer, and when you open that note again (or add a
  note on the same verses) it says "You have unsaved text from …", with
  Restore and Discard. Nothing is restored or saved without you asking.
  Saving or discarding the note forgets the draft, and so does deleting
  the note. Drafts stay on this computer: they're in neither a backup
  nor Export notes, and they're kept for 30 days (the 20 newest).
- Notes understand a little Markdown: **bold**, *italics*, bulleted and
  numbered lists, `code`, and links (`[text](https://…)` or a bare
  https:// address), in the card, the Library and the margin. A web link
  opens in your browser; nothing else opens, and HTML in a note shows as
  you typed it. Your notes are still stored, backed up and edited as the
  plain text you typed.
- References you type in a note ("see Rom 5:8", "John 3:16-18", "1 Cor
  13", "Ps 23") become links that go there, as Go to does: Back returns.
  A verse the translation you're reading doesn't number opens its
  chapter.
- Copy as Markdown, on each note in the card and the Library: the note's
  reference as a heading, its verses quoted in the translation you're
  reading (as Copy as… quotes them), then your note as you wrote it.
- Export notes can write one file per book into a folder you choose
  (for a notes app such as Obsidian): "43 John.md" and so on, each like
  that book's part of the single file, with the same choices of books,
  chapters and dates. It asks before replacing any file already there.
  One file is still the default, and unchanged.

### Changed
- In reading mode, the notes in the margin keep a little more room from
  the window's edge, and still fit beside the text in a 1280 px window.

## [2.20.0] - 2026-10-27

Your notes in the margin in reading mode, and reopening at the verse you
were reading. Updating keeps your bookmarks, highlights, notes, reading
position and settings.

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
