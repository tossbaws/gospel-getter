# 0001. Scripture text is never altered

- **Status:** accepted, 2026-10-23 (written into `CLAUDE.md` that day; it has held since the first release)

## Decision

The wording of a bundled translation is never changed, for any reason:
not to clean it up, not to fix what looks like a typo, not to drop a stray
character a source dataset carries. Ingestion copies verse text through
byte for byte; anything shown differently (italics, footnotes, margin
notes) is a presentation choice made from the stored text, which keeps
everything. A suspected error in a source is raised with the maintainer
and fixed, if at all, by re-sourcing the translation, never by hand.

## Why

People read this app as scripture. A silent edit, however well meant,
misrepresents what a translation says, and a reader has no way to notice.

## Consequences

- `CLAUDE.md`'s Scripture text section and the
  [`biblical-text-integrity`](../../.claude/skills/biblical-text-integrity/SKILL.md)
  skill apply to any change that touches Bible text.
- The raw sources live in `sources/` with their own READMEs.
  `src-tauri/tests/kjv_source.rs` and `src-tauri/tests/bsb_source.rs` pin
  the raw files by SHA-256, check that the bundled `data/kjv.json` and
  `data/bsb.json` are exactly what `src-tauri/ingest/` derives from them,
  and pin `data/kjv.json` and `data/web.json` by SHA-256. A change to a
  bundled translation fails them until the pins are changed on purpose.
- Versification differences between translations are kept as they are
  (each translation keeps its own verse rows).
