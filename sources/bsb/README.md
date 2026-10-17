# The Berean Standard Bible's sources

The app's Berean Standard Bible (BSB), `src-tauri/data/bsb.json`, is derived
from the two files here. Both are Berean's own, kept exactly as downloaded.
The BSB is in the public domain (CC0) since April 30, 2023
([berean.bible/terms.htm](https://berean.bible/terms.htm)). Berean invites
apps that keep its text verbatim to use the Berean name, and asks those that
change it not to. The app keeps it verbatim.

| File | From | Server date | Downloaded | SHA-256 |
|---|---|---|---|---|
| `bsb.txt` | <https://bereanbible.com/bsb.txt> | 2026-07-31 | 2026-10-16 | `2ac3af1de52d4e68261cba91d85c320b7eadc6560e830d99e591767b8ff5ca96` |
| `bsb_usfm.zip` | <https://bereanbible.com/bsb_usfm.zip> | 2026-10-02 (its files are dated 2026-03-08) | 2026-10-16 | `f0c56438eb89c98e8f62c5c00461e25cc35b098ad5b179bc350cede759ee5f87` |

Both are linked from Berean's downloads page,
[berean.bible/downloads.htm](https://berean.bible/downloads.htm).
`src-tauri/tests/bsb_source.rs` checks both hashes.

## What comes from where

- **The verse text is `bsb.txt`'s, byte for byte.** It's Berean's plain text,
  one verse per line ("Genesis 1:1", a tab, the verse), and it matches
  their typeset ePub. Reading it only decodes it:
  - the UTF-8 byte-order mark at the start is dropped;
  - lines are split at their CRLF line breaks.

  Neither is part of any verse, and both are put back by writing the lines
  out again with a BOM and CRLFs. Nothing else changes.
- **The footnotes are the USFM's, exactly.** Each is stored as its
  characters from `\f` to `\f*` (for example
  `\f + \fr 1:3 \ft Cited in \ref 2 Corinthians 4:6|2CO 4:6\ref*\f*`), with
  where it stands: a number of characters into its verse's text. The text
  has no trace of it. The app reads the markers only to show the note: its
  words, its alternative readings (`\fqa`) in italics, and its verse numbers
  (`\fv`) small. The note's own reference (`\fr`) and the part of a `\ref`
  after `|` aren't shown.
- **Where a note stands** is where the USFM puts it.
  1. The USFM's wording of the verse is lined up with the text's by a
     shortest edit script.
  2. The note goes right after the last character before it that both
     share.

  All 4,854 notes have such a place. Where the wording differs next to a
  note (31 notes), the shared character is the word or punctuation it
  follows, as in Exodus 38:28, after "silver" (the USFM has "[of silver]").
  The USFM's wording is only used for this; it's never stored.
- **A verse the BSB doesn't number** is one `bsb.txt` lists with nothing
  after its tab, and that the USFM has no `\v` for. There are 16, all in
  mid-chapter:
  - Matthew 17:21, 18:11, 23:14;
  - Mark 7:16, 9:44, 9:46, 11:26, 15:28;
  - Luke 17:36, 23:17;
  - John 5:4;
  - Acts 8:37, 15:34, 24:7, 28:29;
  - Romans 16:24.

  They're `null` in `bsb.json` and have no row in the database. The
  footnote on the verse before each gives its wording ("BYZ and TR include
  ²¹But this kind …").
- **The Psalm titles** are in verse 1, as both files have them ("A Psalm of
  David. The LORD is my shepherd; …").
- **Not used:** the USFM's section headings (`\s1`, `\s2`), parallel
  passages (`\r`), the Psalter's book divisions (`\ms` "BOOK I" with `\d`
  "Psalms 1–41"), the acrostic letters (`\qa`), paragraph and poetry
  markers, and the words of Christ (`\wj`). They're all still in the zip.

## Where the USFM and the text differ: `artifacts.tsv`

1,375 verses read differently in the USFM than in `bsb.txt`. The app shows
`bsb.txt`'s wording, so none of the USFM's differences is shown. The
owner chose not to report them to Berean. Each is listed in
`artifacts.tsv`, one verse per row:
- its reference;
- its kinds;
- each difference, as `«USFM» → «text»` with a little of the wording
  around it.

`tests/bsb_source.rs` checks the list is exactly what the two files give,
and pins its counts:

| Kind | Verses | For example |
|---|---|---|
| spaced dash | 1,253 | Genesis 1:5: the USFM's "morning — the first day", the text's "morning—the first day" |
| stray [’’] | 59 | Exodus 20:26: the USFM ends "exposed on it.’[’’]" |
| spacing | 57 | Jeremiah 22:24: the USFM's "evenif"; 1 Samuel 8:19 "Nevertheless ,"; Acts 26:14 "Hebrew,‘Saul" |
| ellipsis | 11 | Numbers 26:18: the USFM's "numbered . . . 40,500" |
| brackets | 2 | Exodus 38:28 "[of silver]", Judges 16:14 "[it]" |
| stray letters | 1 | Genesis 35:18: the USFM's "she named vvv him Ben-oni" |
| text outside the verse in the USFM | 1 | Habakkuk 3:19: "For the choirmaster. With stringed instruments." is in the text's verse; the USFM marks it `\mr`, after the verse |

A verse can be of more than one kind, so the counts add up to more than
1,375. Berean's USX (`bsb_usx.zip`, from the same generator, not kept
here) reads as the USFM does in every verse but Genesis 35:18, where it has
no "vvv".

## Remaking `bsb.json`

From `src-tauri/`:

```sh
cargo run --example bsb_ingest
```

It writes `data/bsb.json` and `artifacts.tsv` from the two files here
(`ingest/bsb.rs`). It doesn't download anything. To take a newer BSB:
1. Replace the two files, and update their dates and hashes here and in
   `tests/bsb_source.rs`.
2. Run the command.
3. Review the new `artifacts.tsv`.

An existing install only seeds a translation it doesn't have yet, so a
corrected BSB also needs its rows cleared (see the README's
"Translations").
