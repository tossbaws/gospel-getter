# The King James Version's source

The app's King James Version (KJV), `src-tauri/data/kjv.json`, is derived
from the file here, kept exactly as downloaded. It's eBible.org's
"King James (Authorized) Version": the standardized text of 1769, the 66
books only, in USFM, with the translators' supplied words marked (`\add`)
and the KJV's margin notes as footnotes (`\f`).

| File | From | Server date | Downloaded | SHA-256 |
|---|---|---|---|---|
| `eng-kjv2006_usfm.zip` | <https://ebible.org/Scriptures/eng-kjv2006_usfm.zip> | 2026-11-10 06:31 GMT (its `copr.htm`: "from source files dated 3 Nov 2026") | 2026-11-10 | `dce1ec3ea45a155ac05e975fc8d817da403a817713d554eb9cc3a2177a6cfd1c` |

It's linked from the translation's page,
[ebible.org/find/details.php?id=eng-kjv2006](https://ebible.org/find/details.php?id=eng-kjv2006).
`src-tauri/tests/kjv_source.rs` checks the hash.

**Licence.** Public domain. The zip's `copr.htm`: "You may copy the King
James Version of the Holy Bible freely." In the United Kingdom the Crown's
letters patent restrict printing it; that has no effect elsewhere. eBible.org
credits the text to the CrossWire Bible Society.

## Why this edition

eBible.org has two KJV downloads, and their 66 books are the same, byte
for byte:
- `eng-kjv2006` ("protocanon only, with Strong's numbers added"), this one;
- `eng-kjv` ("King James Version + Apocrypha"), which adds the Apocrypha
  and the 1611 preface.

This one has nothing the app doesn't read. Both have the 31,102 verses of
the KJV, with every chapter's count the standard one (the same as
Berean's `bsb.txt`, which lists every verse of the same English
numbering).

## What comes from where

`src-tauri/ingest/kjv.rs` maps the USFM onto the markup the app has always
read (`core/src/domain/kjv_markup.rs`), copying the source's characters.
Its module docs list every rule; in short:
- supplied words, `\add …\add*`, are `{…}`;
- each margin note is a `{…}` span after the verse: the note's text after
  `\ft`;
- a Psalm's title, `\d`, is a `[…]` prefix on its verse 1;
- Psalm 119's acrostic headings, `\s1`, are a prefix on the verse after
  them;
- an epistle's subscription, the `\s1` after its last verse, is a `«…»`
  span after that verse's text;
- the Strong's numbers, the words-of-Christ and divine-name markers, the
  paragraph breaks and the book titles are taken out, their words kept.

## Remaking `kjv.json`

From `src-tauri/`:

```sh
cargo run --example kjv_ingest
```

It writes `data/kjv.json` from the zip here. It doesn't download anything.
To take a newer download:
1. Replace the zip, and update its dates and hash here and in
   `tests/kjv_source.rs`.
2. Run the command.
3. Review what changed in `data/kjv.json`.
