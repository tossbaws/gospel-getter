#!/usr/bin/env python3
"""Move the release's version strings to a new version: the edits a
release-prep PR makes, apart from the CHANGELOG entry and any prose
(RELEASING.md, "The bump").

Usage, from the repository root:
    python3 tools/release/bump.py X.Y.Z --check   # print the edits, change nothing
    python3 tools/release/bump.py X.Y.Z           # make them

The current version is src-tauri/Cargo.toml's. Every other place has to
show the same one first, and the new version has to be a plain X.Y.Z above
it, or nothing is written. Each edit is anchored to the one line it
changes and must match exactly once, so a version mentioned anywhere else
(the CHANGELOG's history, the README's "from 2.36.0 on", test fixtures) is
never touched, and wording that moved stops the bump instead of being
skipped. Cargo.lock's two entries are edited as `cargo update --workspace
--offline` writes them; core stays 0.0.0. The PKGBUILD's pkgrel goes back
to 1, and its checksums are left alone.
"""

import argparse
import re
import sys
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
PLAIN = re.compile(r"(\d+)\.(\d+)\.(\d+)")
ANY = re.compile(r"(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.-]+))?")


class Refused(Exception):
    pass


@dataclass(frozen=True)
class Edit:
    path: str
    line: int
    old: str
    new: str


def parse(version):
    """(major, minor, patch, pre-release or None)."""
    match = ANY.fullmatch(version)
    if not match:
        raise Refused(f"not a version: {version!r}")
    major, minor, patch, pre = match.groups()
    return int(major), int(minor), int(patch), pre


def newer(new, current):
    """Whether `new` (plain) comes after `current`: a pre-release sorts
    before its own release."""
    n, c = parse(new), parse(current)
    if n[:3] != c[:3]:
        return n[:3] > c[:3]
    return c[3] is not None


def pkgver(version):
    """PKGBUILD's pkgver: no '-', and a pre-release's dots dropped
    (2.19.0-beta.1 is 2.19.0beta1), as packaging_config.rs checks."""
    major, minor, patch, pre = parse(version)
    return f"{major}.{minor}.{patch}{(pre or '').replace('.', '')}"


def read(path):
    """As bytes, so a file's line endings are written back as they were."""
    return path.read_bytes().decode("utf-8")


def current_version(root):
    text = read(root / "src-tauri/Cargo.toml")
    match = re.search(r'^\[package\]\n(?:[^\[\n].*\n|\n)*?version = "([^"]+)"$', text, re.M)
    if not match:
        raise Refused('src-tauri/Cargo.toml has no version = "..." in [package]')
    return match.group(1)


def rules(current, new):
    """(path, pattern, replacement) for each edit. Each pattern is one
    whole line (re.M), so it can only match where a bump edits."""
    c = re.escape(current)
    package = r'^(\[package\]\n(?:[^\[\n].*\n|\n)*?version = ")' + c + r'(")$'
    return [
        ("src-tauri/Cargo.toml", package, rf"\g<1>{new}\g<2>"),
        ("src-tauri/cli/Cargo.toml", package, rf"\g<1>{new}\g<2>"),
        ("src-tauri/tauri.conf.json", r'^(  "version": ")' + c + r'(",)$', rf"\g<1>{new}\g<2>"),
        (
            "src-tauri/Cargo.lock",
            r'^(name = "gospel_getter"\nversion = ")' + c + r'(")$',
            rf"\g<1>{new}\g<2>",
        ),
        (
            "src-tauri/Cargo.lock",
            r'^(name = "gospel-getter-cli"\nversion = ")' + c + r'(")$',
            rf"\g<1>{new}\g<2>",
        ),
        ("packaging/PKGBUILD", r"^(_pkgver=)" + c + r"$", rf"\g<1>{new}"),
        ("packaging/PKGBUILD", r"^(pkgver=)" + re.escape(pkgver(current)) + r"$", rf"\g<1>{new}"),
        ("packaging/PKGBUILD", r"^(pkgrel=)\d+$", r"\g<1>1"),
        (
            "site/index.html",
            r'^(\s*<p class="small hero-note">Version )' + c + r"(, for 64-bit computers\.</p>)$",
            rf"\g<1>{new}\g<2>",
        ),
        (
            "site/index.html",
            r'^(\s*<p class="release-line"><strong>Version )' + c + r"(\.</strong> .*)$",
            rf"\g<1>{new}\g<2>",
        ),
        # The update path: "through" names the last version that updates
        # itself to the new one, which is the current one.
        (
            "README.md",
            r"^(\*\*Coming from 2\.6\.0 through )[0-9A-Za-z.-]+(\?\*\* Each updates itself to )"
            + c
            + r"(: it)$",
            rf"\g<1>{current}\g<2>{new}\g<3>",
        ),
        (
            "README.md",
            r"^(update themselves\. The 2\.19\.0 pre-release updates itself to )" + c + r"( the)$",
            rf"\g<1>{new}\g<2>",
        ),
        ("README.md", r"^(Install )" + c + r"( by hand once, from the)$", rf"\g<1>{new}\g<2>"),
    ]


def plan(root, new):
    """The edits, and each file's new text. Refuses, before anything is
    written, a version that isn't a plain one above the current one, or an
    edit that doesn't match exactly once."""
    if not PLAIN.fullmatch(new):
        raise Refused(f"the new version must be X.Y.Z, not {new!r}")
    current = current_version(root)
    if not newer(new, current):
        raise Refused(f"{new} isn't greater than the current version, {current}")
    texts, edits = {}, []
    for path, pattern, replacement in rules(current, new):
        if path not in texts:
            texts[path] = read(root / path)
        text = texts[path]
        found = list(re.finditer(pattern, text, re.M))
        if len(found) != 1:
            raise Refused(
                f"{path}: expected one line matching {pattern!r}, found {len(found)} "
                f"(is every version string at {current}?)"
            )
        match = found[0]
        old = match.group(0)
        changed = match.expand(replacement)
        if old != changed:
            # The match can start on an earlier line (the [package] header,
            # the lock entry's name); report the line that changes.
            first = next(i for i, (a, b) in enumerate(zip(old.split("\n"), changed.split("\n"))) if a != b)
            line = text.count("\n", 0, match.start()) + 1 + first
            edits.append(Edit(path, line, old.split("\n")[first], changed.split("\n")[first]))
            texts[path] = text[: match.start()] + changed + text[match.end() :]
    order = list(dict.fromkeys(path for path, _, _ in rules(current, new)))
    edits.sort(key=lambda edit: (order.index(edit.path), edit.line))
    return current, edits, texts


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("version", help="the new version, X.Y.Z")
    parser.add_argument("--check", action="store_true", help="print the edits and change nothing")
    parser.add_argument("--root", type=Path, default=ROOT, help=argparse.SUPPRESS)
    args = parser.parse_args(argv)
    try:
        current, edits, texts = plan(args.root, args.version)
    except (Refused, OSError) as error:
        print(f"bump.py: {error}", file=sys.stderr)
        return 2
    verb = "would change" if args.check else "changed"
    print(f"{current} -> {args.version}: {verb} {len(edits)} lines")
    for edit in edits:
        print(f"{edit.path}:{edit.line}\n  - {edit.old.strip()}\n  + {edit.new.strip()}")
    if not args.check:
        for path in sorted({edit.path for edit in edits}):
            (args.root / path).write_bytes(texts[path].encode("utf-8"))
    return 0


if __name__ == "__main__":
    sys.exit(main())
