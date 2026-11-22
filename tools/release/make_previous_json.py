#!/usr/bin/env python3
"""Write previous.json, which About's Go back reads, for one release.

Usage, from the repository root (Python 3 standard library, gh and git):
    python3 tools/release/make_previous_json.py --for 2.36.0 --previous 2.35.0 \\
        [--out bundle/previous.json]

Each release carries a previous.json naming the release kept before it
(GitHub keeps Latest and one previous release; the ones before are deleted,
their tags kept). The app reads its own release's copy
(releases/download/v<its version>/previous.json, updater::previous_manifest)
and offers "Go back to <previous>" from it.

It's that previous release's own latest.json, as published (downloaded with
`gh release download v<previous> -p latest.json`), unchanged, plus
`schemaVersion`: the database schema the previous release was built for
(`SCHEMA_VERSION` in `git show v<previous>:src-tauri/core/src/db/migrate.rs`),
which the app compares with its database's before it offers to go back
(migrate::older_app_can_use).

It refuses, writing nothing, unless the previous release is a plain release
lower than --for, its latest.json names that version, every installer has an
entry whose signature was made for that version and whose URL is one of that
release's own assets (releases/download/v<previous>/), and the schema version
is found. The app checks the signatures again (same key, same version) before
it installs anything.

--latest-json FILE and --migrate-rs FILE read those from files instead of gh
and git (offline, or to check a draft).
"""

import argparse
import json
import re
import subprocess
import sys
import tempfile
from pathlib import Path

import make_latest_json as latest

SCHEMA = re.compile(r"^pub const SCHEMA_VERSION: i64 = (\d+);$", re.MULTILINE)
MIGRATE_RS = "src-tauri/core/src/db/migrate.rs"


class PreviousError(Exception):
    """Why no previous.json was written."""


def parts(version: str) -> tuple[int, int, int]:
    found = latest.VERSION.match(version)
    if not found:
        raise PreviousError(f"{version!r} isn't a release version like 2.35.0 (no v, no pre-release)")
    return tuple(int(n) for n in found.groups())


def schema_version(migrate_rs: str, previous: str) -> int:
    found = SCHEMA.findall(migrate_rs)
    if len(found) != 1:
        raise PreviousError(f"v{previous}'s {MIGRATE_RS} has no single `pub const SCHEMA_VERSION: i64 = N;`")
    return int(found[0])


def build_previous(for_version: str, previous: str, manifest: dict, migrate_rs: str,
                   repo: str = latest.REPO) -> dict:
    if parts(previous) >= parts(for_version):
        raise PreviousError(f"{previous} isn't a release before {for_version}")
    if not isinstance(manifest, dict) or manifest.get("version") != previous:
        found = manifest.get("version") if isinstance(manifest, dict) else None
        raise PreviousError(f"v{previous}'s latest.json names version {found!r}, not {previous}")
    platforms = manifest.get("platforms")
    if not isinstance(platforms, dict):
        raise PreviousError(f"v{previous}'s latest.json has no platforms")
    missing = sorted(set(latest.PLATFORMS) - set(platforms))
    if missing:
        raise PreviousError(f"v{previous}'s latest.json has no entry for {', '.join(missing)}")
    prefix = f"https://github.com/{repo}/releases/download/v{previous}/"
    for key, entry in sorted(platforms.items()):
        if not isinstance(entry, dict):
            raise PreviousError(f"{key} isn't an entry")
        url = entry.get("url")
        if not isinstance(url, str) or not url.startswith(prefix) or "/" in url[len(prefix):]:
            raise PreviousError(f"{key}'s URL {url!r} isn't one of v{previous}'s own assets ({prefix}...)")
        signature = entry.get("signature")
        if not isinstance(signature, str) or not signature:
            raise PreviousError(f"{key} has no signature")
        try:
            signed = latest.signed_version(signature)
        except latest.ManifestError as e:
            raise PreviousError(f"{key}'s signature {e}") from None
        if signed != previous:
            raise PreviousError(f"{key}'s signature was made for version {signed or '(none)'}, not {previous}")
    return {**manifest, "schemaVersion": schema_version(migrate_rs, previous)}


def gh_latest_json(previous: str, repo: str) -> dict:
    with tempfile.TemporaryDirectory() as tmp:
        done = subprocess.run(
            ["gh", "release", "download", f"v{previous}", "-R", repo, "-p", "latest.json", "-D", tmp],
            capture_output=True, text=True,
        )
        if done.returncode != 0:
            raise PreviousError(f"couldn't download v{previous}'s latest.json: {done.stderr.strip()}")
        return json.loads((Path(tmp) / "latest.json").read_text(encoding="utf-8"))


def git_migrate_rs(previous: str) -> str:
    done = subprocess.run(["git", "show", f"v{previous}:{MIGRATE_RS}"], capture_output=True, text=True)
    if done.returncode != 0:
        raise PreviousError(f"couldn't read {MIGRATE_RS} at v{previous}: {done.stderr.strip()}")
    return done.stdout


def main(argv: list[str] | None = None, fetch_latest=gh_latest_json, read_migrate=git_migrate_rs) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--for", dest="for_version", required=True, help="the release it's for, e.g. 2.36.0")
    parser.add_argument("--previous", required=True, help="the release kept before it, e.g. 2.35.0")
    parser.add_argument("--out", type=Path, default=Path("bundle/previous.json"))
    parser.add_argument("--repo", default=latest.REPO)
    parser.add_argument("--latest-json", type=Path, help="the previous release's latest.json, not downloaded")
    parser.add_argument("--migrate-rs", type=Path, help="the previous release's migrate.rs, not from git")
    args = parser.parse_args(argv)
    try:
        parts(args.for_version)
        parts(args.previous)
        if args.latest_json:
            manifest = json.loads(args.latest_json.read_text(encoding="utf-8"))
        else:
            manifest = fetch_latest(args.previous, args.repo)
        migrate_rs = (args.migrate_rs.read_text(encoding="utf-8") if args.migrate_rs
                      else read_migrate(args.previous))
        previous = build_previous(args.for_version, args.previous, manifest, migrate_rs, args.repo)
    except (PreviousError, json.JSONDecodeError, OSError) as e:
        print(f"error: {e}; no previous.json written", file=sys.stderr)
        return 1
    latest.write_json(previous, args.out)
    print(f"Wrote {args.out} for {args.for_version}: back to {args.previous} "
          f"(schema {previous['schemaVersion']}), {', '.join(sorted(previous['platforms']))}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
