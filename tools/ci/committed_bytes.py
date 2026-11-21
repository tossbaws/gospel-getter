#!/usr/bin/env python3
"""Fail if a checkout's working tree isn't byte for byte the commit's.

2.33.0's Windows installers embedded a CRLF schema_1.sql: dist.yml's Windows
jobs checked out with core.autocrlf=true while ci.yml's test-windows had
opted out, so every test passed on bytes other than the ones shipped.
.gitattributes' first rule now gives every checkout LF; this proves it, in
every job that builds or tests the app, before anything reads a file.

Each tracked file of HEAD is hashed as git hashes a blob, from the bytes on
disk with no filter, and compared with the commit's object id. That catches
anything a checkout can do to a file: an end-of-line conversion, a smudge
filter, `ident`, working-tree-encoding, or a missing file. Two weaker checks
were rejected:
  - `git ls-files --eol` reports line endings only, so it misses every
    other filter, and it needs the `-text` files excepted by hand.
  - `git add --renormalize .` then `git diff --exit-code` passes on the very
    checkout that shipped 2.33.0: renormalizing runs the clean filter,
    which turns the CRLF back into the LF the index holds, so nothing
    differs.

A symlink is compared by its target (on Windows, where git writes it as a
file holding the target, by that file); a submodule is left to its own
checkout. Paths are reported as git prints them.

Usage (in CI: .github/actions/committed-bytes):
    python3 tools/ci/committed_bytes.py [REPOSITORY]
"""

import hashlib
import os
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path

BLOB = "blob"
SYMLINK_MODE = "120000"
# GitHub shows a step's first ten error annotations; every file is listed
# after them anyway.
MAX_ANNOTATED = 10
# Paths per `git ls-files` call, well inside Windows' command line limit.
PATHS_PER_CALL = 100


@dataclass(frozen=True)
class Entry:
    mode: str
    kind: str
    oid: str
    path: bytes


@dataclass(frozen=True)
class Mismatch:
    path: str
    reason: str


def git(repo: Path, *args: str) -> bytes:
    # Paths are passed as they are, never as patterns.
    return subprocess.run(
        ["git", "-C", str(repo), "--literal-pathspecs", *args],
        check=True,
        capture_output=True,
    ).stdout


def committed_entries(repo: Path) -> list[Entry]:
    """Every entry of HEAD's tree, at every depth."""
    entries = []
    for record in git(repo, "ls-tree", "-r", "-z", "--full-tree", "HEAD").split(b"\0"):
        if not record:
            continue
        meta, path = record.split(b"\t", 1)
        mode, kind, oid = meta.decode("ascii").split(" ")
        entries.append(Entry(mode, kind, oid, path))
    return entries


def blob_id(data: bytes, algorithm: str) -> str:
    """The object id git gives a blob holding exactly `data`."""
    digest = hashlib.new(algorithm)
    digest.update(b"blob %d\0" % len(data))
    digest.update(data)
    return digest.hexdigest()


def on_disk(repo: Path, entry: Entry) -> bytes | None:
    path = repo / os.fsdecode(entry.path)
    if entry.mode == SYMLINK_MODE and path.is_symlink():
        return os.fsencode(os.readlink(path))
    try:
        return path.read_bytes()
    except (FileNotFoundError, NotADirectoryError):
        return None


def check(repo: Path) -> tuple[int, list[Mismatch]]:
    """How many files were compared, and those that differ."""
    algorithm = git(repo, "rev-parse", "--show-object-format").decode().strip()
    compared = 0
    mismatches = []
    for entry in committed_entries(repo):
        if entry.kind != BLOB:
            continue
        compared += 1
        shown = entry.path.decode("utf-8", "backslashreplace")
        data = on_disk(repo, entry)
        if data is None:
            mismatches.append(Mismatch(shown, "missing from the working tree"))
        elif blob_id(data, algorithm) != entry.oid:
            mismatches.append(Mismatch(shown, "differs from the committed bytes"))
    return compared, mismatches


def eol_report(repo: Path, paths: list[str]) -> str:
    """`git ls-files --eol` for these paths, which says whether a line
    ending conversion is why. For information only."""
    report = []
    for start in range(0, len(paths), PATHS_PER_CALL):
        chunk = paths[start : start + PATHS_PER_CALL]
        try:
            out = git(repo, "ls-files", "--eol", "--", *chunk)
        except subprocess.CalledProcessError as error:
            return f"(git ls-files --eol failed: {error})\n"
        report.append(out.decode("utf-8", "backslashreplace"))
    return "".join(report)


def main(argv: list[str]) -> int:
    repo = Path(argv[1] if len(argv) > 1 else ".").resolve()
    compared, mismatches = check(repo)
    if compared == 0:
        print(f"::error::{repo}: HEAD has no files to compare")
        return 1
    if not mismatches:
        print(f"committed bytes: all {compared} files of HEAD are as committed")
        return 0
    for mismatch in mismatches[:MAX_ANNOTATED]:
        print(f"::error file={mismatch.path}::{mismatch.path} {mismatch.reason}")
    print("Every file that differs:")
    for mismatch in mismatches:
        print(f"  {mismatch.path}: {mismatch.reason}")
    print("git ls-files --eol for them (i/ is the commit, w/ the checkout):")
    print(eol_report(repo, [m.path for m in mismatches]), end="")
    print(
        f"::error::{len(mismatches)} of {compared} files aren't the committed "
        "bytes: the checkout converted them (see tools/ci/committed_bytes.py)"
    )
    return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
