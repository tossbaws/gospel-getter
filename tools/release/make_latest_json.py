#!/usr/bin/env python3
"""Write latest.json, the update manifest the app reads, for one release.

Usage, from the repository root (Python 3 standard library only):
    python3 tools/release/make_latest_json.py --version 2.6.0 \\
        --notes-file NOTES.md --assets DIR [--out DIR/latest.json]

DIR holds the release's renamed installers and, beside each, the .sig file
the Tauri CLI wrote for it in dist.yml (renamed the same way):

    Gospel-Getter_amd64.AppImage        Gospel-Getter_amd64.AppImage.sig
    Gospel-Getter_amd64.deb             Gospel-Getter_amd64.deb.sig
    Gospel-Getter_x64-setup.exe         Gospel-Getter_x64-setup.exe.sig
    Gospel-Getter_x64_en-US.msi         Gospel-Getter_x64_en-US.msi.sig

Each installer gets its own platform key, `{os}-{arch}-{bundle}`, which
tauri-plugin-updater looks up first for the bundle the running copy was
installed from, so a .deb install is only ever offered the .deb, an MSI
install the MSI, and so on. There's deliberately no bare `linux-x86_64` or
`windows-x86_64` key: the plugin would fall back to it for any bundle it
doesn't recognise. URLs point at the versioned tag, not /latest/, so a
manifest always names exactly the files it was made for.

It refuses to write anything if an installer or its signature is missing,
or if a signature wasn't made for this version (the CLI records the
version in each signature's trusted comment, and the app checks it).
"""

import argparse
import base64
import binascii
import datetime
import json
import os
import re
import sys
import tempfile
import urllib.parse
from pathlib import Path

REPO = "tossbaws/gospel-getter"

# Platform key -> the release asset's stable name.
PLATFORMS = {
    "linux-x86_64-appimage": "Gospel-Getter_amd64.AppImage",
    "linux-x86_64-deb": "Gospel-Getter_amd64.deb",
    "windows-x86_64-nsis": "Gospel-Getter_x64-setup.exe",
    "windows-x86_64-msi": "Gospel-Getter_x64_en-US.msi",
}

VERSION = re.compile(r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$")


class ManifestError(Exception):
    """Why no manifest was written."""


def signed_version(signature: str) -> str | None:
    """The version in a Tauri updater signature's trusted comment, if any.

    A .sig file is base64 of a minisign signature: an untrusted comment
    line, the signature, a `trusted comment: ...` line of tab-separated
    `key:value` fields, and the global signature.
    """
    try:
        text = base64.b64decode(signature, validate=True).decode("utf-8")
    except (binascii.Error, UnicodeDecodeError):
        raise ManifestError("isn't a Tauri updater signature (not base64 text)") from None
    lines = text.splitlines()
    if len(lines) < 4 or not lines[0].startswith("untrusted comment:"):
        raise ManifestError("isn't a Tauri updater signature")
    for line in lines:
        if line.startswith("trusted comment: "):
            fields = dict(
                field.split(":", 1) for field in line[len("trusted comment: "):].split("\t") if ":" in field
            )
            return fields.get("version")
    raise ManifestError("has no trusted comment")


def build_manifest(version: str, notes: str, assets: Path, pub_date: str, repo: str = REPO) -> dict:
    if not VERSION.match(version):
        raise ManifestError(f"{version!r} isn't a release version like 2.6.0 (no v, no pre-release)")
    missing = []
    for name in PLATFORMS.values():
        for path in (assets / name, assets / f"{name}.sig"):
            if not path.is_file():
                missing.append(path.name)
    if missing:
        raise ManifestError(f"missing from {assets}: {', '.join(missing)}")

    platforms = {}
    for key, name in PLATFORMS.items():
        signature = (assets / f"{name}.sig").read_text(encoding="utf-8").strip()
        if not signature:
            raise ManifestError(f"{name}.sig is empty")
        try:
            signed = signed_version(signature)
        except ManifestError as e:
            raise ManifestError(f"{name}.sig {e}") from None
        if signed != version:
            raise ManifestError(
                f"{name}.sig was signed for version {signed or '(none)'}, not {version}"
            )
        url = f"https://github.com/{repo}/releases/download/v{version}/{urllib.parse.quote(name)}"
        platforms[key] = {"signature": signature, "url": url}

    return {"version": version, "notes": notes, "pub_date": pub_date, "platforms": platforms}


def write_json(manifest: dict, out: Path) -> None:
    """Write `out` all at once, so a failure never leaves half a file."""
    out.parent.mkdir(parents=True, exist_ok=True)
    fd, tmp = tempfile.mkstemp(dir=out.parent, prefix=".latest.json.")
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as f:
            json.dump(manifest, f, indent=2, ensure_ascii=False)
            f.write("\n")
        os.replace(tmp, out)
    except BaseException:
        os.unlink(tmp)
        raise


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--version", required=True, help="the release version, e.g. 2.6.0")
    notes = parser.add_mutually_exclusive_group(required=True)
    notes.add_argument("--notes", help="the release notes")
    notes.add_argument("--notes-file", type=Path, help="a file with the release notes")
    parser.add_argument("--assets", type=Path, required=True, help="the renamed release assets and .sig files")
    parser.add_argument("--out", type=Path, help="where to write it (default: ASSETS/latest.json)")
    parser.add_argument("--pub-date", help="RFC 3339 publication time (default: now, UTC)")
    args = parser.parse_args(argv)

    text = args.notes if args.notes is not None else args.notes_file.read_text(encoding="utf-8")
    pub_date = args.pub_date or datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    try:
        datetime.datetime.fromisoformat(pub_date.replace("Z", "+00:00"))
    except ValueError:
        print(f"error: {pub_date!r} isn't an RFC 3339 date", file=sys.stderr)
        return 1
    try:
        manifest = build_manifest(args.version, text.strip(), args.assets, pub_date)
    except ManifestError as e:
        print(f"error: {e}; no manifest written", file=sys.stderr)
        return 1
    out = args.out or args.assets / "latest.json"
    write_json(manifest, out)
    print(f"Wrote {out} for {args.version}: {', '.join(manifest['platforms'])}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
