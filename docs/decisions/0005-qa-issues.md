# 0005. Each release gets one QA issue

- **Status:** accepted, 2026-11-19 (D22)

## Decision

Each release's QA issue holds a standing short checklist and one observable
check per user-visible fix or feature in that release. The standing four:

1. Self-update from the previous release.
2. Restore a backup into a clean profile.
3. Restart with two panes open and an unsaved note.
4. Keyboard and focus, on Windows and on Linux.

When a new QA issue opens, the previous one closes as superseded, with a
comment pointing at the new one. Nothing is rolled over into the new issue.

## Why

The earlier QA issue rolled every unchecked item forward into the next
release's, until it held about 90 items. A short standing list (update,
restore, restart, keyboard) covers the paths every release touches, and one
check per change covers what's new, so the whole issue can be run each time.

## Consequences

- [`RELEASING.md`](../../RELEASING.md)'s last step opens the issue.
- An observable check says what to do and what to see, so anyone can run it.
