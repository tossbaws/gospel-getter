# 0002. The AI rule: agents find and quote scripture, never interpret it

- **Status:** accepted, 2026-11-07 (agent access first shipped read-only in 2.24.0; findings in 2.26.0; the agent's name rule on 2026-11-19)

## Decision

`gospel-getter-cli` (CLI and MCP server) lets an agent find, quote and
locate scripture and read the reader's own data. No tool, prompt, skill or
agent-facing text may have an agent summarise, paraphrase or interpret
scripture; agent-facing copy tells it to quote exactly and point the reader
to the place. The reader's own notes are theirs, so an agent may summarise
those, quoting any verse they point to exactly.

The one thing an agent can write is an **AI-flagged finding**, and:

- only while the reader has turned on **Let AI agents save findings** (off
  until then; while off, the tool stays listed and answers with how to turn
  it on), and only when the reader asked it to save, note or mark verses;
- it is labelled as the agent's work: "Found by {agent} for your request
  '{request}'", with its own look in the app;
- it marks a place and never describes the verse: the agent's name and the
  request are the only words it saves, both short labels held to one
  character rule (no `: ; — –`, line breaks or invisible characters; at most
  40 characters for the name, 60 for the request). The name comes from an
  explicit `agent` argument (refused if it breaks the rule), else the
  client's `title` if it passes as given, else the client's `name`, cleaned,
  else "AI agent";
- it is add-only: an agent never edits or deletes the reader's notes,
  bookmarks or highlights, or a finding. Removing findings, and **Keep as
  mine**, are the reader's. **Keep as mine** opens the note editor with the
  finding's text in it, all selected; saving a non-empty note makes it the
  reader's own and removes the finding, and cancelling changes nothing.

Any tool that takes a note or finding ID back from an agent checks the
record's identity (its ID and `created_at`), never the ID alone.

## Why

The AI features exist so readers find and read the words of God for
themselves. If someone gets a passage wrong, it should be through their own
reading, not because an agent took liberties with the text. Findings meet a
real request ("note every verse that deals with grief") while leaving the
judging to the reader. The identity check exists because an agent holding an
old list could otherwise act on a newer record that took a deleted one's ID.

## Consequences

- `src-tauri/core/src/db/findings.rs` (its `//!` and `FindingsWriter`) is
  the only write path beside the app; `findings_tests.rs` and
  `read_only_tests.rs` beside it, and `src-tauri/cli/tests/e2e.rs`, hold
  the rule. `README.md`'s "Use with AI agents" and the site's guide say the
  same in readers' words.
- The CLI's clap help and MCP schemas and instructions (`src-tauri/cli/`)
  are agent-facing product text: a change there is checked against this
  rule first.
- A backup always restores: older findings whose labels break today's rule
  are cleaned on import, never refused.
- Every new AI-access proposal is checked against this record first.
