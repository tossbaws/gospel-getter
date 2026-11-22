"""Tests for make_previous_json.py. Run from the repository root:
    python3 -m unittest discover -s tools/release
"""

import contextlib
import copy
import io
import json
import tempfile
import unittest
from pathlib import Path

import make_latest_json as latest
import make_previous_json as m
from test_make_latest_json import signature

MIGRATE_RS = """//! The database's schema.
pub const SCHEMA_VERSION: i64 = 2;

pub const OLDEST_READABLE_SCHEMA_VERSION: i64 = 1;
"""


def manifest(version="2.35.0", signed=None):
    base = f"https://github.com/tossbaws/gospel-getter/releases/download/v{version}/"
    return {
        "version": version,
        "notes": "## 2.35.0\n\n- Notes beside the text",
        "pub_date": "2026-11-21T12:00:00Z",
        "platforms": {
            key: {"signature": signature(signed or version, file=name), "url": base + name}
            for key, name in latest.PLATFORMS.items()
        },
    }


class MakePreviousJson(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.out = Path(self.tmp.name) / "bundle" / "previous.json"
        self.asked = []

    def tearDown(self):
        self.tmp.cleanup()

    def run_main(self, served=None, migrate_rs=MIGRATE_RS, for_version="2.36.0", previous="2.35.0"):
        served = manifest() if served is None else served

        def fetch(version, repo):
            self.asked.append(("gh", version, repo))
            return served

        def read(version):
            self.asked.append(("git", version))
            return migrate_rs

        err = io.StringIO()
        with contextlib.redirect_stderr(err), contextlib.redirect_stdout(io.StringIO()):
            code = m.main(["--for", for_version, "--previous", previous, "--out", str(self.out)],
                          fetch_latest=fetch, read_migrate=read)
        return code, err.getvalue()

    def test_is_the_previous_releases_latest_json_with_its_schema(self):
        code, err = self.run_main()
        self.assertEqual(code, 0, err)
        written = json.loads(self.out.read_text())
        self.assertEqual(written, {**manifest(), "schemaVersion": 2})
        # Its own release's files and the tag's migrate.rs, nothing else.
        self.assertEqual(self.asked, [("gh", "2.35.0", "tossbaws/gospel-getter"), ("git", "2.35.0")])
        for entry in written["platforms"].values():
            self.assertIn("/releases/download/v2.35.0/", entry["url"])
            self.assertNotIn("/latest/", entry["url"])

    def test_refuses_a_signature_not_made_for_the_previous_version(self):
        for bad, why in [(signature("2.36.0"), "made for version 2.36.0"),
                         (signature(None), "made for version (none)"),
                         ("", "has no signature"),
                         ("not base64!", "isn't a Tauri updater signature")]:
            with self.subTest(why=why):
                served = manifest()
                served["platforms"]["windows-x86_64-nsis"]["signature"] = bad
                code, err = self.run_main(served)
                self.assertEqual(code, 1)
                self.assertIn("windows-x86_64-nsis", err)
                self.assertIn(why, err)
                self.assertFalse(self.out.exists())

    def test_refuses_a_url_that_isnt_the_previous_releases_own_asset(self):
        for url in [
            "https://github.com/tossbaws/gospel-getter/releases/download/v2.36.0/Gospel-Getter_amd64.deb",
            "https://github.com/tossbaws/gospel-getter/releases/latest/download/Gospel-Getter_amd64.deb",
            "https://example.com/tossbaws/gospel-getter/releases/download/v2.35.0/Gospel-Getter_amd64.deb",
            "https://github.com/tossbaws/gospel-getter/releases/download/v2.35.0/../v2.36.0/x.deb",
            None,
        ]:
            with self.subTest(url=url):
                served = manifest()
                served["platforms"]["linux-x86_64-deb"]["url"] = url
                code, err = self.run_main(served)
                self.assertEqual(code, 1)
                self.assertIn("linux-x86_64-deb's URL", err)
                self.assertFalse(self.out.exists())

    def test_refuses_a_manifest_for_another_version_or_missing_an_installer(self):
        code, err = self.run_main(manifest("2.34.0"))
        self.assertEqual(code, 1)
        self.assertIn("names version '2.34.0', not 2.35.0", err)
        served = manifest()
        del served["platforms"]["windows-x86_64-msi"]
        code, err = self.run_main(served)
        self.assertEqual(code, 1)
        self.assertIn("no entry for windows-x86_64-msi", err)
        code, err = self.run_main({"version": "2.35.0"})
        self.assertEqual(code, 1)
        self.assertIn("no platforms", err)
        self.assertFalse(self.out.exists())

    def test_refuses_when_the_schema_version_isnt_found(self):
        for text in ["", "pub const SCHEMA_VERSION: i64 = two;", MIGRATE_RS + MIGRATE_RS]:
            with self.subTest(text=text[:30]):
                code, err = self.run_main(migrate_rs=text)
                self.assertEqual(code, 1)
                self.assertIn("SCHEMA_VERSION", err)
                self.assertFalse(self.out.exists())

    def test_refuses_a_previous_that_isnt_an_earlier_plain_release(self):
        for for_version, previous in [("2.35.0", "2.35.0"), ("2.35.0", "2.36.0"), ("2.36.0", "v2.35.0"),
                                      ("2.36.0", "2.35.0-rc.1"), ("2.36", "2.35.0")]:
            with self.subTest(previous=previous, for_version=for_version):
                code, err = self.run_main(manifest(previous) if latest.VERSION.match(previous) else None,
                                          for_version=for_version, previous=previous)
                self.assertEqual(code, 1, err)
                self.assertFalse(self.out.exists())

    def test_compares_versions_as_numbers(self):
        served = manifest("2.9.0")
        code, err = self.run_main(served, for_version="2.10.0", previous="2.9.0")
        self.assertEqual(code, 0, err)

    def test_can_read_both_from_files(self):
        folder = Path(self.tmp.name)
        (folder / "latest.json").write_text(json.dumps(manifest()))
        (folder / "migrate.rs").write_text(MIGRATE_RS.replace("= 2;", "= 3;"))
        err = io.StringIO()

        def never(*_):
            raise AssertionError("not asked")

        with contextlib.redirect_stderr(err), contextlib.redirect_stdout(io.StringIO()):
            code = m.main(["--for", "2.36.0", "--previous", "2.35.0", "--out", str(self.out),
                           "--latest-json", str(folder / "latest.json"),
                           "--migrate-rs", str(folder / "migrate.rs")],
                          fetch_latest=never, read_migrate=never)
        self.assertEqual(code, 0, err.getvalue())
        self.assertEqual(json.loads(self.out.read_text())["schemaVersion"], 3)

    def test_the_manifest_is_copied_not_changed(self):
        served = manifest()
        kept = copy.deepcopy(served)
        self.assertEqual(self.run_main(served)[0], 0)
        self.assertEqual(served, kept)

    def test_this_checkouts_schema_is_read_as_a_release_would_be(self):
        root = Path(__file__).resolve().parents[2]
        text = (root / m.MIGRATE_RS).read_text(encoding="utf-8")
        self.assertGreaterEqual(m.schema_version(text, "this checkout"), 2)


if __name__ == "__main__":
    unittest.main()
