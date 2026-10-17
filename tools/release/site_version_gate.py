#!/usr/bin/env python3
"""Decide whether the website may be deployed: only when the version it
shows has been released, so it never advertises a version whose downloads
don't exist yet.

The site's download links are releases/latest/download/<name>, which serve
the latest *published* release. A release-prep merge bumps the site's
"Version X.Y.Z" before that release is published, so deploying it then
would show the new version over the old downloads.
.github/workflows/pages.yml runs this before every deploy.

Usage, from the repository root:
    python3 tools/release/site_version_gate.py --site site/index.html \
        --published v2.16.0 [--published v2.17.0 ...]
    python3 tools/release/site_version_gate.py --site site/index.html --site-version

The first prints `deploy=true` or `deploy=false` (for $GITHUB_OUTPUT), and
the reason on stderr. Each --published is a published release's tag (the
latest release's, and the one that started the deploy, if any); the newest
one counts. The second prints the version the site shows. Either exits 2
if the site's version can't be read.
"""

import argparse
import os
import re
import sys
from pathlib import Path

VERSION = re.compile(r"\bVersion (\d+\.\d+\.\d+)\b")
TAG = re.compile(r"v?(\d+)\.(\d+)\.(\d+)")


def parse(version):
    """'v2.16.0' or '2.16.0' as (2, 16, 0), compared as numbers."""
    match = TAG.fullmatch(version.strip())
    if not match:
        raise ValueError(f"not a version: {version!r}")
    return tuple(int(part) for part in match.groups())


def site_version(html):
    """The one version the page shows ("Version X.Y.Z"). It shows it more
    than once; they must agree (site.test.mjs checks the same)."""
    found = set(VERSION.findall(html))
    if len(found) != 1:
        raise ValueError(f"the site shows {len(found)} versions, not 1: {sorted(found)}")
    return found.pop()


def decide(site, published):
    """(deploy, reason): deploy only if the site's version is at or below
    the newest published release."""
    if not published:
        return False, "no published release to compare with, so the site isn't deployed"
    newest = max(published, key=parse)
    if parse(site) <= parse(newest):
        return True, f"the site shows {site}, and {newest} is published"
    return False, (f"the site shows {site}, but the newest published release is {newest}; "
                   f"it deploys when {site} is published")


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--site", required=True, type=Path, help="the site's index.html")
    parser.add_argument("--published", action="append", default=[],
                        help="a published release's tag; repeat for several")
    parser.add_argument("--site-version", action="store_true",
                        help="print the version the site shows, and stop")
    args = parser.parse_args(argv)
    try:
        site = site_version(args.site.read_text(encoding="utf-8"))
        published = [tag for tag in args.published if tag.strip()]
        for tag in published:
            parse(tag)
    except (OSError, ValueError) as e:
        print(f"site_version_gate: {e}", file=sys.stderr)
        return 2
    if args.site_version:
        print(site)
        return 0
    deploy, reason = decide(site, published)
    print(f"deploy={'true' if deploy else 'false'}")
    print(reason, file=sys.stderr)
    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary:
        with open(summary, "a", encoding="utf-8") as f:
            f.write(f"{'Deploying' if deploy else 'Not deploying'}: {reason}.\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
