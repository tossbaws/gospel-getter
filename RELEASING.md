# Releasing Gospel Getter

How a version goes out, from deciding to ship to the QA issue, as the
repository does it. The asset half (the signed bundle, `latest.json`,
`previous.json`) is in [tools/release/README.md](tools/release/README.md);
this file links to it rather than repeating it.

## When a release happens

A version ships only when the installed app or the CLI changes. Test, CI
and docs-only pull requests merge unreleased and ride the next app release:
no version bump, no CHANGELOG entry of their own. GitHub keeps Latest and
the one release before it; every tag is kept. See
[decision 0004](docs/decisions/0004-release-cadence-and-rollback.md).

## The version

Versions follow [Semantic Versioning](https://semver.org/), as
`CHANGELOG.md` says:

- **MINOR** (2.37.0): something new a reader, or an agent through the CLI,
  can do or see, even if the rest is fixes. 2.33.0 was a MINOR because
  agents could page search results.
- **PATCH** (2.33.1): fixes only, nothing new to learn.
- **MAJOR**: not used in the CHANGELOG's history; one is the maintainer's
  call.

Pre-releases (`2.19.0-beta.1`) have been made once; the tooling below
handles a plain `X.Y.Z` only.

## 1. The release-prep pull request

Branch `release/vX.Y.Z` from `main`. One commit, and the pull request's
title, exactly `chore(release): vX.Y.Z`.

**The bump.** `python3 tools/release/bump.py X.Y.Z --check` prints the
edits; without `--check` it makes them. It moves the version strings and
nothing else, and refuses a version that isn't above the current one:

| File | What changes |
|---|---|
| `src-tauri/Cargo.toml` | `[package]` `version` |
| `src-tauri/cli/Cargo.toml` | `[package]` `version` |
| `src-tauri/tauri.conf.json` | `"version"` |
| `src-tauri/Cargo.lock` | the `gospel_getter` and `gospel-getter-cli` entries only (core stays 0.0.0), as `cargo update --workspace --offline` writes them |
| `packaging/PKGBUILD` | `_pkgver`, `pkgver`, and `pkgrel=1`; the checksums stay as they are |
| `site/index.html` | the two "Version X.Y.Z" lines |
| `README.md` | the update path under Updates: "through" moves to the version before, and the two "updates itself to" / "Install X.Y.Z by hand" lines name the new one |

These are checked against each other: `src-tauri/tests/packaging_config.rs`
(tauri.conf.json, the CLI and the PKGBUILD against `Cargo.toml`) and
`tests/frontend/site.test.mjs` (every version the site shows is
tauri.conf.json's). Afterwards, `cargo update --workspace --offline` in
`src-tauri/` should change nothing; if it moves anything else in the lock,
stop.

Then, by hand:

- **CHANGELOG.md**: a new `## [X.Y.Z] - YYYY-MM-DD` at the top, dated the
  day it will be published (if the merge slips to another day, change the
  date before merging). A one-line
  summary of the release, then `### Added`, `### Changed`, `### Fixed` (and
  `### Security` when it applies) in reader terms: no crate names, PR
  numbers or CI. Say anything the update does once (a database upgrade, an
  index rebuild) and anything a reader must do. End with the standard
  line, adjusted only if it stops being true: "No verse changes. Backups
  keep the same format. Updating keeps your bookmarks, highlights, notes,
  findings, reading position and settings."
- Any sentence in `README.md`, `site/` or the app's guide that the
  release's changes made untrue.
- Images: recapture only the README or site images that the changes made
  wrong ([site/README.md](site/README.md) and `tools/site/` have the tools).
- The commit body lists each file changed and why, as the earlier
  `chore(release):` commits do, and says which version literals were left
  alone on purpose (the CHANGELOG's history, test fixtures, "from 2.36.0
  on").

**Why the subject must be exact.** `dist.yml` skips its build jobs on a
push whose commit message starts with `chore(release):`. The installers
for a release come from a run dispatched on the merge commit (step 2); the
merge's own push run would be a second, unused build of the same commit,
and a push run can be cancelled by the next push, which a release build
must never be. Squash-merge without editing the subject, so the merge
commit is `chore(release): vX.Y.Z (#N)`.

**ci:full.** Release-prep pull requests carry the `ci:full` label (`gh pr
edit N --add-label ci:full`). It runs the Windows GUI checks and the
signing and bundle rehearsal (with a throwaway key) in the pull request's
own run, which a plain pull request skips; a pull request from a `release/`
branch runs them too. Use the label on any other pull request whose change
a Windows build, an installer or the signing could notice; a docs or
Linux-only change doesn't need it. See `dist.yml`'s header.

Before merging: every check green, including the Distribution run's GUI
checks on Linux and Windows and the rehearsal.

## 2. Merge and build

1. Squash-merge, keeping the subject (see above). Delete the branch.
2. Check the merge commit's tree equals the pull request head's tree.
3. Dispatch both workflows on the merge commit, and check each run's
   `headSha` is that commit:

   ```sh
   gh workflow run dist.yml --ref main
   gh workflow run audit.yml --ref main
   gh run list --workflow dist.yml --event workflow_dispatch --limit 3 \
       --json databaseId,headSha,status
   gh run list --workflow audit.yml --event workflow_dispatch --limit 3 \
       --json databaseId,headSha,status
   ```

4. Wait for both. The Distribution run must be green on every job that ran
   (Linux and Windows builds, both GUI checks, both upgrade checks, **Sign
   the installers (release)** and **Assemble the release files**); the
   rehearsal and supersede jobs are skipped on a dispatch, as expected. The
   audit must be green on the same commit; its run is named in the notes.

Take the release's files only from that run's `gospel-getter-release-bundle`
artifact, never from a pull request's run or a failed attempt. If `main`
moves before you publish, the release is still that merge commit.

## 3. Tag and draft

1. Make an annotated tag `vX.Y.Z` on the merge commit, message "Gospel
   Getter X.Y.Z", and check it peels to that commit (`git rev-parse
   vX.Y.Z^{commit}`).
2. Create the release as a draft: `gh release create vX.Y.Z --verify-tag
   --draft --notes-file NOTES.md`. Follow the previous release's notes:
   the summary line; "What's new" from the CHANGELOG entry; Upgrading
   (which versions update themselves, any by-hand cases, Go back, backups);
   Standalone CLI (what changed for agents, or that nothing did); Checked
   (the CI, audit and Distribution runs on the tagged commit, the GUI and
   upgrade results); Not yet verified by hand, linking the QA issue
   (step 5); Downloads; and the provenance line naming the commit, the
   Distribution run and the audit run.
3. Download the bundle, check it, and write `latest.json` and
   `previous.json` into it: [tools/release/README.md](tools/release/README.md),
   steps 1 and 2.
4. Upload everything except `latest.json` to the draft. Download them back
   from the draft and run `sha256sum -c SHA256SUMS` on them. Upload
   `latest.json` last, and only once that passes: it's what installed apps
   read, and it must never name a file that isn't there.

## 4. Publish

1. `gh release edit vX.Y.Z --draft=false --latest`, and check it shows as
   Latest.
2. The website: publishing starts `pages.yml`, which deploys `main`'s
   `site/` once `tools/release/site_version_gate.py` sees the site's
   version is published (the release-prep merge's own push deployed
   nothing). Check the run's deploy job and that the site shows the new
   version.
3. Housekeeping: delete the oldest of the three releases, keeping its tag,
   and the kept release's stale `previous.json`:
   [tools/release/README.md](tools/release/README.md), step 4. `gh release
   list` then shows exactly the new release (Latest) and the one before.
4. Check the published release from outside: a fresh download passes
   `sha256sum -c SHA256SUMS`, every `releases/latest/download/<name>` link
   resolves to the new tag, and `latest.json` names the new version.

## 5. The QA issue

Open "QA: X.Y.Z", as
[decision 0005](docs/decisions/0005-qa-issues.md) says: the four standing
checks (self-update from the previous release; restore a backup into a
clean profile; restart with two passages open and an unsaved note; keyboard
and focus on Windows and Linux), then one observable check per
user-visible fix or feature in this release, each saying what to do and
what counts as yes. List what's covered by automated tests instead, and
the known gaps. Close the previous release's QA issue as superseded, with
a comment linking the new one.
