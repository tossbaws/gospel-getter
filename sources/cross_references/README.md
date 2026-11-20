# The cross-references' source

The app's cross-references, `src-tauri/data/cross_references.json`, are
OpenBible.info's, selected and reformatted. This records what the
repository can show about where the file came from and what was done to it,
and says plainly what it can't.

| File | SHA-256 | Entries | Added |
|---|---|---|---|
| `src-tauri/data/cross_references.json` | `b6b60521542fc6e625040d7296c2eaa688ec7a28f8a2ff7b448490b863bc8a0a` | 133,770 | 2026-09-12 (commit `833352b`) |

`src-tauri/tests/cross_references_source.rs` checks the hash. The file is
the same bytes in every commit since `833352b` (git blob `80caa17`), moved
from `data/` to `src-tauri/data/` in `ea5d010`.

## What's known

- **Source.** OpenBible.info's cross-references
  (<https://www.openbible.info/labs/cross-references/>), "sourced from
  OpenBible.info via the Free Use Bible API", as the commit that added the
  full set (`5b6a7cf`, 2026-09-12) says. That commit's file is still in
  git: 344,799 references from 29,364 verses, which it says match the
  source dataset's counts, with Genesis 1:1 spot-checked against the live
  API. `git show 5b6a7cf:data/cross_references.json` gives it (SHA-256
  `a4d5e35356b903ff356f8884edd078ab5d62b5fc23031a4d2eefa4c208e45776`).
- **Licence.** CC BY 4.0, as the app's and the README's credits say. The
  change is stated here and in the credits ("selected and reformatted"),
  as CC BY 4.0 §3(a)(1)(B) asks.
- **Reformatting.** Each verse's list holds `{book, chapter, verse,
  endVerse?, score}`, with the books numbered 1 to 66 in canonical order;
  the file is 66 books of chapters of verses, in order.
- **Selection.** `833352b` kept each verse's five highest-scored references
  (`score` is the source dataset's own). Checked against `5b6a7cf`'s file:
  every verse's list there is already in descending score order, and every
  bundled list is exactly the first five of it (or all, when there are
  fewer), in the same order. Where the fifth and sixth have the same score
  (14,137 verses), the one kept is the one the source listed first; no other
  tie-break was applied.

## What isn't known

- **Which file, and its date.** The Free Use Bible API's exact URL or
  version, and the date of OpenBible.info's data it carried, weren't
  recorded. Nothing in the repository pins the upstream file.
- **A range's ending chapter.** The file gives a range's ending verse
  (`endVerse`) but not its ending chapter. So a range that crosses into the
  next chapter can't be told from one within its chapter: 196 entries end
  before they start ("Hebrews 6:20–3", from Genesis 14:18) and 18 end past
  their chapter's last verse ("Job 40:15–34"; Job 40 ends at 24). Whether
  the upstream file has the ending chapter, and the intermediary dropped
  it, isn't checked. Until the upstream file is identified and pinned, the
  app shows such an entry as its start verse, "(full range not
  available)", and never guesses its end (`core/src/domain/cross_reference.rs`).
  Some ranges that cross a chapter may also look valid where the ending
  verse happens to fall inside the starting chapter; how many isn't known.
- **Upgrades.** The table is seeded only when it's empty, so a change to the
  file wouldn't reach an existing install without a step of its own.
