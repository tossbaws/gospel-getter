#!/usr/bin/env python3
"""Which of main's caches a newer one has superseded, for
.github/workflows/cache-cleanup.yml to delete.

Each cache's key names what it caches and then hashes of what it was built
from: rust-cache's `v0-rust-linux-Linux-x64-872dfffb-d449d622` ends with a
hash of the toolchain and environment and one of the lockfiles, and the
build tools' and npm's keys end with a hash of their pins or lockfile. A
new lockfile or pin saves an entry under a new key and leaves the old one,
which main's runs, finding the new key, never restore again. It takes room
in the repository's 10 GB until it goes unused for seven days, or GitHub
evicts something first, the least recently used, which can be one of
main's live entries.

A family is a key with its hash segments taken out: dash-separated
segments of eight or more lowercase hex digits. The trailing hashes are
the ones that change with each lockfile or pin. ci.yml's rust-cache keys
also carry the hash of .github/ci/cargo-config.toml in the middle
(`v0-rust-<64 hex>-build-and-test-...`), and when that file changes, the
old entries are superseded just the same, so that hash goes too. Versions
(2.12.4, version_123) and names (build-and-test, x64) are never all hex.

In each family only the newest entry (by when it was saved) is kept, and
an entry saved at the same time as the newest is never deleted either:
only one strictly older than its family's newest goes.

Usage, from the workflow (the list on stdin, as gh prints it):
    gh cache list --ref refs/heads/main --json id,key,sizeInBytes,createdAt \\
        | python3 tools/ci/superseded_caches.py
prints one line per cache to delete, `<id> <size in bytes> <key>`, and a
summary on stderr.
"""

import json
import re
import sys
from collections import defaultdict
from datetime import datetime

HASH = re.compile(r"[0-9a-f]{8,}")


def family(key):
    """The key without its hash segments."""
    return "-".join(segment for segment in key.split("-") if not HASH.fullmatch(segment))


def saved_at(cache):
    """When the cache was saved, from gh's createdAt (ISO 8601, a Z offset)."""
    return datetime.fromisoformat(cache["createdAt"].replace("Z", "+00:00"))


def superseded(caches):
    """The caches a newer one of the same family has superseded, oldest
    first: never a family's newest, nor one saved with it."""
    families = defaultdict(list)
    for cache in caches:
        families[family(cache["key"])].append(cache)
    old = []
    for members in families.values():
        newest = max(saved_at(cache) for cache in members)
        old += [cache for cache in members if saved_at(cache) < newest]
    return sorted(old, key=lambda cache: (saved_at(cache), cache["key"]))


def main(stdin=sys.stdin, stdout=sys.stdout, stderr=sys.stderr):
    caches = json.load(stdin)
    old = superseded(caches)
    for cache in old:
        print(f"{cache['id']} {cache['sizeInBytes']} {cache['key']}", file=stdout)
    freed = sum(cache["sizeInBytes"] for cache in old)
    total = sum(cache["sizeInBytes"] for cache in caches)
    print(
        f"{len(old)} of {len(caches)} caches superseded, {freed} of {total} bytes",
        file=stderr,
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
