# Gospel Getter

[![CI](https://github.com/tossbaws/gospel-getter/actions/workflows/ci.yml/badge.svg)](https://github.com/tossbaws/gospel-getter/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-c9a227.svg)](LICENSE)

A Bible reader with a vaporwave soul: browse all 66 books, pick a chapter,
and read — with the chapter before and after it shown in full, at reduced
opacity, to the left and right of the one you're reading, scrolling along
with you as one continuous page.

![Gospel Getter — book list and reading view](screenshots/hero-full-page.png)

## Features

- **Two full translations** bundled and embedded in the binary — no
  network access, no API keys, works completely offline.
- **Continuous side-by-side reading**: the chapter before and after the
  current one are shown in full, faded, and scroll with you — not a
  paginated "next page" click, and not a cropped preview.
- **Arrow-key navigation** leafs through the entire Bible a chapter at a
  time, correctly crossing book boundaries (Genesis 50 → Exodus 1, and so
  on) without you needing to touch the mouse.
- **Remembers your place** — closing and reopening the app returns you to
  the exact chapter (and translation) you were last reading.
- **Installable as a real desktop app**: one script sets it up as an
  always-running background service with its own taskbar icon.

## Screenshots

| Reading with translation switching | Responsive on narrow screens |
|---|---|
| ![Translation switching](screenshots/translations-web.png) | ![Mobile layout](screenshots/mobile-view.png) |

## Running it

```bash
cargo run
```

Then open http://localhost:3002.

## Installing as a desktop app

`packaging/install.sh` builds a release binary, installs it as an
always-running `systemd --user` service, and creates a real taskbar
launcher (via `omarchy webapp install` on Omarchy, or a plain `.desktop`
file elsewhere). See `packaging/` for details; `packaging/uninstall.sh`
reverses it.

**This same script is also how updates ship.** After a code change:

```bash
packaging/install.sh
```

rebuilds and **restarts** the running service in place — no separate
"update" command. (Re-running it is safe to do any time; if nothing
changed, you just get the same binary back.)

Two things worth knowing about what "update" actually means here:

- **Code changes** (routes, navigation, templates, styling) take effect on
  the next restart — always, since Askama templates are compiled into the
  binary, there's no live reload for the installed instance.
- **Bundled Bible data** only gets *added to*, not *overwritten*: adding a
  new translation to `TRANSLATIONS` and reinstalling seeds just that new
  one into your existing database (verified: it detects exactly what's
  missing by translation code, not "is the database empty"). But fixing a
  typo in a translation that's *already* seeded won't reach an existing
  install on its own — that translation's rows (or the whole database at
  `~/.local/share/gospel-getter/data/gospel_getter.db`) need to be cleared
  first so it re-seeds.

## Navigating

- **Books list** (always visible) → click a book to jump straight to its
  first chapter (and bring up its chapter grid, in case you want a
  different one) → click a chapter number to read that one instead.
- **Arrow keys** (&larr; / &rarr;) move one chapter at a time through the
  whole Bible, including across book boundaries (e.g. the end of Genesis
  leads into the start of Exodus) — same as clicking the previous/next
  chapter's heading in the faded side columns.
- **Translation** dropdown switches translations for whatever you're
  currently reading, and is remembered across visits.

## Translations

Bundled: **King James Version** (`kjv`) and **World English Bible**
(`web`) — both public domain, so both are embedded in the binary like
everything else, no API key or network access needed.

**NIV and ESV are not included**, on purpose:

- **NIV** has no free API or downloadable text. Real access goes through
  API.Bible and starts around $10/month per translation for commercial
  use; the free tier explicitly excludes NIV.
- **ESV** has a free API (`api.esv.org`, non-commercial) but its terms cap
  cached/downloaded text at 500 verses (and no more than half of any one
  book) — it can't be bundled offline the way KJV and WEB are. Adding it
  would mean a second translation module type (a live API call per
  request, with a reader-supplied API key) rather than an embedded one.

### Adding a new translation

For anything with a compatible license (public domain, or terms that
permit an offline/bulk copy):

1. Build a JSON file shaped like `data/kjv.json` — an array of 66 objects,
   `{"name": "...", "testament": "OT"|"NT", "chapters": [[verse, verse, ...], ...]}`,
   in canonical Genesis-to-Revelation order.
2. Drop it in `data/`.
3. Add one entry to the `TRANSLATIONS` list in `src/db/seed.rs`
   (`code`, `name`, `include_str!(...)` for the new file).

That's it — the seed step, schema, and UI translation picker all pick it up
automatically. (A translation that can only be accessed live, like ESV,
would need a small amount of translation-specific fetch code rather than
just a data file — not yet built here, since neither bundled translation
needs it.)

## How it's put together

- `axum` + `askama` + `sqlx`/SQLite.
- `books` (name/testament/chapter count) is shared across translations;
  `verses` is keyed by `(translation_id, book_id, chapter, verse)`, so each
  translation has its own verse text and — correctly — its own verse
  *counts* per chapter (translations occasionally split/number a verse
  differently; chapter counts line up across KJV/WEB, individual verse
  counts don't always, and that's expected, not a bug).
- `src/domain/bible.rs` holds the only interesting logic: computing the
  chapter before/after any given one, crossing book boundaries as needed.
  It's pure and unit-tested (no database needed to test it), and doesn't
  care which translation is selected.
- The reading pane (current chapter + faded neighbors) is rendered from a
  single template (`templates/reading_pane.html`) used both for the initial
  page load and for the AJAX fragment endpoint that arrow keys and chapter
  clicks hit — so there's exactly one copy of that markup to keep correct.

## Data provenance and textual integrity

**No verse text is ever altered, cleaned up, or "corrected" here.** Both
translations are stored exactly as their source provides them:

- **KJV**: sourced from a public-domain JSON dataset (thiagobodruk/bible).
  That source renders KJV translator-supplied-word italics and Hebrew/Greek
  marginal notes inline as `{...}` groups (affecting ~56% of verses) — this
  is preserved verbatim, braces and all, exactly as received, including in
  the handful of spots where the source's own markup is internally
  malformed (e.g. Hebrews 10:34 has a stray unmatched `}` in the upstream
  data — it's kept exactly as-is rather than "fixed"). The only
  transformations applied are non-textual: stripping the file's UTF-8 BOM
  and reshaping the JSON into this app's schema (splitting into
  book/testament/chapters — a restructuring of the container, not the
  verse text inside it).
- **WEB**: sourced from TehShrike/world-english-bible (public domain, via
  ebible.org), one file per book. The only transformation is reassembling
  verse fragments that the source itself splits across multiple entries
  (needed for poetic books like Psalms, where one verse is often several
  "line" entries) — joined in the source's own order, nothing added,
  removed, or reworded.

## License

[MIT](LICENSE).
