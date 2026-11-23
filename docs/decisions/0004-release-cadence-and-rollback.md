# 0004. Release cadence and rollback

- **Status:** accepted, 2026-11-19 (D21)

## Decision

A version ships only when the installed app or the CLI changes. Test, CI
and docs-only pull requests merge unreleased and ride the next app release,
with no version bump and no CHANGELOG entry of their own. GitHub keeps two
releases: Latest and the one before it. When a release is published, the
oldest is deleted; every tag is kept.

## Why

Each release costs a reader an update and costs the maintainer a QA pass,
so one that changes nothing a reader installs is pure cost. The previous
release stays downloadable as the way back: About's Go back offers it, and
a reader can install it by hand. Tags stay so every published version can
still be rebuilt from its source.

## Consequences

- [`RELEASING.md`](../../RELEASING.md) carries the procedure, including the
  housekeeping step.
- `.github/workflows/pages.yml` deploys the site on a merge to `main` that
  touches `site/**` only while the site's version is the published one
  (`tools/release/site_version_gate.py`), so site corrections go out at once
  and an unreleased version never appears there.
