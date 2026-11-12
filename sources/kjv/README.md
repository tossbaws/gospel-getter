# The King James Version's second source

The app's King James Version (KJV), `src-tauri/data/kjv.json`, is the
thiagobodruk/bible dataset it has always been, except for ten chapters and
one verse, which since 2.29.2 come from the file here, kept exactly as
downloaded: eBible.org's "King James (Authorized) Version", the
standardized text of 1769, the 66 books only, in USFM, with the
translators' supplied words marked (`\add`) and the KJV's margin notes as
footnotes (`\f`).

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

## What's taken from it, and why

Only these, the parts thiagobodruk/bible numbered wrongly, so that a
verse's words stood under another verse's number:

- **Six chapters a verse short**, each missing one verse, so every later
  verse in it stood one number early: Matthew 2 (2:16 missing), Matthew 22
  (22:1), Matthew 26 (26:38), Mark 4 (4:40), Mark 7 (7:11) and Mark 8
  (8:8).
- **Four chapters with a verse split in two**, so every later verse stood
  one number late: 1 Samuel 20 (20:42), 1 Kings 22 (22:43), 3 John (1:14)
  and Revelation 12, whose extra verse 18 ("And I stood upon the sand of
  the sea.") is the first words of Revelation 13:1.
- **Revelation 13:1**, which began "And saw a beast" without them.

Each is replaced whole, with what `src-tauri/ingest/kjv.rs` reads in the
USFM there (its `TAKEN`): 386 verses in the ten chapters, where there were
384, and Revelation 13:1. Every other verse of the file is
thiagobodruk/bible's, byte for byte as before; `tests/kjv_source.rs` pins
them with one SHA-256.

What the file had there before, every verse exactly as 2.29.1 bundled it,
is kept in `kjv_2_29_1_replaced.tsv` ("book, chapter, verse, text", one
verse a line), so the change can be read and undone: putting those verses
back gives 2.29.1's `kjv.json`, byte for byte, which the test checks.

The whole file isn't taken from eBible.org. Its New Testament has no
margin notes (thiagobodruk/bible has 905), and its typography differs in
about 8,500 verses (`’`, `…`, the 1769 text's `¶`, hyphenated names), with
a few edition differences in wording; so the rest stays as it was. Within
the verses taken, the source's characters are copied as they are, `’`,
`¶`, `…` and hyphens included, so those chapters read slightly differently
from the others: that's the source's text, and changing it would be
editing it. The 28 New Testament margin notes thiagobodruk/bible had in
those chapters are gone, since the source doesn't have them; the Old
Testament chapters' notes are the source's.

## How it's read

`src-tauri/ingest/kjv.rs` maps the USFM onto the markup the app has always
read (`core/src/domain/kjv_markup.rs`), copying the source's characters.
Its module docs list every rule; in short:
- supplied words, `\add …\add*`, are `{…}`, with the closing punctuation
  they end with after the `}` ("thine only {son}:"), as the rest of the
  file has it;
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
cargo run --example kjv_splice
```

It reads `data/kjv.json` and the zip here, and writes `data/kjv.json` with
the verses above replaced, and `kjv_2_29_1_replaced.tsv` with what it
replaced. It doesn't download anything. Run on the file it made, it changes
nothing. To take a newer download:
1. Replace the zip, and update its dates and hash here and in
   `tests/kjv_source.rs`.
2. Run the command on 2.29.1's `kjv.json`
   (`git show fa7c1ab:src-tauri/data/kjv.json > data/kjv.json` first).
3. Review what changed in `data/kjv.json`.
