---
name: biblical-text-integrity
description: >
  REQUIRED whenever handling Bible/scripture text in any form — ingesting,
  seeding, parsing, storing, cleaning, formatting, migrating, or displaying
  verse text from any translation (KJV, WEB, ESV, NIV, or any other).
  Governs what may and may not be done to the actual wording of scripture,
  how to handle versification differences between translations, how to
  source translation data responsibly, and how to handle suspected errors
  in a source dataset. Triggers: Bible, scripture, verse, chapter, book of
  the Bible, translation (KJV/WEB/ESV/NIV/NASB/etc.), versification,
  USFM/USX, seed data for a Bible app, Bible API, Bible database schema.
---

# Biblical Text Integrity

The central rule, from which everything else follows:

**Never alter the wording of scripture text, for any reason, under any
justification — including "cleanup," "formatting," "fixing a typo,"
"removing clutter," or "modernizing."** You are not a translator, an editor,
or a textual critic. Ingesting, storing, and displaying a translation
faithfully is the entire job. Getting this wrong isn't a normal bug — it
misrepresents what a translation actually says, to someone using it to
read Scripture.

This applies **regardless of source quality**. Even when a dataset is a
community scrape rather than an official publisher edition, and even when
part of it looks like an obvious artifact (a stray unmatched brace, a
duplicated word, odd markup) — the fix is never to silently edit it. See
"Suspected errors in source data" below.

## What counts as "the wording" (all of it is off-limits to edit)

- The verse text itself, in full, including archaic spelling/grammar
  ("thee," "thou," "-eth" verb endings) — that is the translation, not an
  error.
- Translator-supplied-word markers (e.g. KJV/NASB italics for words added
  by translators that aren't in the original Hebrew/Greek but are implied
  by it) — these are meaningful editorial apparatus, not decoration.
  [GotQuestions: why some words are italicized](https://www.gotquestions.org/words-in-the-Bible-italicized.html)
- Marginal notes / alternate-reading footnotes bundled with the text
  (e.g. "Heb.: ..." / "or, ...") — part of the translation's critical
  apparatus. [Critical apparatus](https://en.wikipedia.org/wiki/Critical_apparatus)
- Punctuation, capitalization, and section headings, if they're part of
  the licensed/sourced text.
- Verse and chapter boundaries (versification) — see below, this is its
  own topic with its own standard, not something to renumber ad hoc.

## What's actually allowed: presentation, never storage

You may make **presentation-layer** decisions — as long as the stored text
is left completely intact and every choice is reversible from it:

- Rendering italics markup as visual italics, or showing/hiding footnotes
  behind a toggle, is fine — **as long as nothing is deleted from the
  stored/underlying text to do it.** Style it, wrap it in a `<span>`,
  collapse it behind a "show notes" control — don't strip it out.
- If displaying raw markup (e.g. literal `{...}` from a particular source
  format) looks bad and you don't yet have time to build real toggle/italics
  rendering, the correct fallback is to **display it as-is, verbatim** —
  not to delete it "for now." Ugly-but-faithful beats pretty-but-wrong.
- Typography, verse-number badges, layout, fonts, translation switchers,
  search, cross-references as *additional* metadata (not a rewrite of the
  base text) are all fine.

## Versification: differences between translations are real, not bugs

Chapter/verse divisions are not perfectly universal across translations
and manuscript traditions (Hebrew/Masoretic, Greek/Septuagint, Latin
Vulgate, and English traditions all have documented differences — e.g.
Catholic Bibles' Esther has different chapter divisions than Protestant
ones). This is well-studied and has real tooling behind it:

- **TVTMS** (Tyndale Versification Traditions with Methodology for
  Standardisation) documents versification differences across Hebrew,
  Latin, and Greek traditions against an English/NRSV~KJV baseline.
- CrossWire's SWORD project supports **"Alternate Versification"**
  explicitly so texts with differing canon/versification can coexist and
  map cleanly to each other via cross-references, rather than forcing one
  scheme onto another.
  [Alternate Versification](https://wiki.crosswire.org/Alternate_Versification)

Practical rules:
- Never force one translation's verse/chapter numbering onto another, and
  never merge or split verses to make counts match across translations.
  Real versification differences exist and are expected: e.g. English
  Malachi 4:1-6 is Malachi 3:19-24 in Hebrew-numbered editions, and English
  Joel 2:28-32 is Joel 3:1-5 there. Store each translation's own actual
  verse rows; don't normalize across them.
- But a verse count that differs from the translation's OWN standard
  edition is a dataset defect, not versification. Check counts against
  that edition (the KJV has 31,102 verses; Matthew 2 has 23 in every
  English Bible) and re-source a defective dataset; never hand-edit it.
- If cross-translation verse alignment is ever genuinely needed, use or
  reference an established mapping (TVTMS, or a library that implements
  it) — don't invent an ad hoc renumbering scheme.
- Chapter *counts* are far more stable across English translations than
  individual verse splits within a chapter — don't assume this generalizes
  to non-English or historically distant traditions without checking.

## Sourcing translation data

Prefer, in order:
1. An official, licensed distribution from the translation's own
   copyright holder or an authorized aggregator — the
   **Digital Bible Library (DBL)**, run by the United Bible Societies, is
   the standard licensed-text distribution/validation platform used by
   major Bible agencies (UBS, Biblica, Wycliffe, and others via the ETEN
   Alliance); submissions go through a review process before publication.
   [DBL](https://www.tools.bible/tools/digital-bible-library-dbl) ·
   [DBL review process](https://care.library.bible/article/138-our-review-process)
2. **USFM/USX** (Unified Standard Format Markers / their XML form) is the
   standard interchange format for Scripture text, maintained via
   Paratext/UBS — the format DBL itself stores translations in. Prefer a
   source already in this format, or a well-established derivative of it,
   over an arbitrary ad hoc scrape when one is available.
   [USFM](https://paratext.org/usfm/)
3. CrossWire/SWORD modules — a long-established, actively maintained
   corpus with real submission standards (Unicode/UTF-8 requirements,
   versification declarations).
4. A community JSON/text dump (e.g. from GitHub) **only** when nothing
   above is realistically available for a public-domain or appropriately
   licensed text — and even then, treat it as unvalidated: ingest it
   losslessly (see below), and don't assume it's error-free.

Whatever the source, respect the license: display required copyright/
attribution notices, and never redistribute or alter licensed text beyond
what the license actually permits (e.g. the ESV API caps how much can be
cached/stored at once — that's a hard limit, not a suggestion).

## Ingestion must be lossless

When parsing a source format into your own schema:
- Preserve every markup element present (italics spans, footnote/marginal
  markers, headers) even if the current feature set doesn't render all of
  it yet. Losing information during ingestion because "we don't use that
  part yet" means it's gone — don't discard now what you might want later,
  and don't discard it just because you don't want it *displayed* now
  either (see "presentation, never storage," above).
- Reshaping a source's *container* format (e.g. converting its file/array
  structure into your own JSON shape, splitting into book/chapter arrays,
  tagging testament) is fine — that's structural, not textual. The
  distinction is: did any actual verse *characters* change? If yes, stop.
- Keep the ingestion step itself simple and mechanical (parse the source's
  own structure, copy text through byte-for-byte) precisely so it's
  obviously not doing anything interpretive. If an ingestion script has
  logic that could plausibly change a verse's wording (regex substitution,
  "cleanup" passes, manual overrides), that's a sign it's doing too much.
- Where practical, keep the untouched raw source file alongside/underneath
  your ingested version so the transformation is auditable and diffable.

## Suspected errors in source data

If ingested text looks wrong — a typo, a stray character, a duplicated
word, mismatched formatting — **do not fix it based on your own
judgment**, even when you're fairly confident it's a dataset artifact
rather than the actual translation. In order:

1. Check whether it's present in the *original* source you ingested from
   (not a downstream copy) — if so, it's not your ingestion pipeline's
   bug, it's a property of that source.
2. If possible, compare against a second, independent edition/distribution
   of the *same* translation to see whether the anomaly is specific to one
   digitization (in which case a better-sourced dataset is the right fix,
   not hand-editing text) or is genuinely how that edition reads.
3. If you still conclude it's a transcription/formatting artifact rather
   than the translation's actual wording, **surface it to the user
   explicitly and let them decide** — don't silently patch it, even for a
   single character. Real Bible publishers run this as a formal process
   (e.g. the CSB's own text-error reporting form, reviewed by their team,
   with corrections shipping in future editions) — the pattern is "report
   and defer to the people with actual authority over the text," not
   "the developer noticed and changed it."
   [CSB text error reporting](https://csbible.com/texterrorreporting/)
4. Never let uncertainty push you toward "just remove the weird part" as
   a middle ground. Deleting an anomaly you don't understand is still
   altering the text — display it as received, and ask.

Word choice in Scripture carries real theological weight — the Exodus
20:13 "murder" vs. "kill" translation choice is a well-known example of
how much a single word matters — which is exactly why this isn't a
judgment call to make unilaterally, even over what looks like a trivial
formatting glitch.

## Before touching any Bible text in this kind of project, check

- [ ] Am I about to change any verse's actual characters? If yes: stop,
      this isn't allowed regardless of the reason.
- [ ] Is this a presentation choice (rendering, styling, show/hide) that
      leaves the stored text 100% intact and recoverable? That's fine.
- [ ] Am I treating a versification difference between translations as a
      bug to reconcile? It probably isn't — store each translation's own
      actual verse structure.
- [ ] Did I source this translation as responsibly as was practical (DBL/
      USFM/an established corpus over an arbitrary scrape), and am I
      respecting its license/attribution requirements?
- [ ] Did my ingestion code do anything beyond mechanically reshaping the
      source's container format? If it has a "cleanup" or "fix" step,
      remove it.
- [ ] If something in the source looks wrong, have I left it untouched
      and raised it with the user instead of editing it myself?
