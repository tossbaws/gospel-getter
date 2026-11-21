"""Tests for upgrade_check.py. Run from the repository root:
    python3 -m unittest discover -s tools/ci

The CLI answers are gospel-getter-cli 2.33.1's, as it read back a database
upgraded from 2.32.2 (cut to the fields the check reads).
"""

import io
import json
import os
import sqlite3
import tempfile
import unittest
from contextlib import redirect_stdout
from pathlib import Path
from unittest import mock

import upgrade_check as u

SCHEMA_1 = (u.MIGRATE_DIR / "schema_1.sql").read_text(encoding="utf-8")

ANSWERS = {
    "list_bookmarks": {"translation": "kjv", "total": 1, "bookmarks": [
        {"id": 1, "reference": "Romans 8:28", "book": "Romans", "chapter": 8,
         "verse_start": 28, "verse_end": 28}]},
    "list_highlights": {"translation": "kjv", "highlights": [
        {"color": "green", "reference": "John 3:16", "book": "John", "chapter": 3,
         "verse_start": 16, "verse_end": 16}]},
    "list_notes": {"translation": "kjv", "notes": [
        {"id": 1, "source": "reader_note", "reference": "Psalms 23:1–2", "book": "Psalms",
         "chapter": 23, "verse_start": 1, "verse_end": 2, "body": u.NOTE_BODY}]},
    "get_reading_place": {"saved": True, "reference": "Luke 2:10", "translation": "kjv",
                          "book": "Luke", "chapter": 2, "verse": 10},
}


def version_1_database(path):
    """A database made from schema_1.sql, at version 1, with KJV in it, as
    2.32.2 makes one (its verses aren't needed here)."""
    db = sqlite3.connect(path)
    body = "\n".join(l for l in SCHEMA_1.splitlines() if not l.startswith("--"))
    db.executescript(body)
    db.execute("INSERT INTO translations (id, code, name) VALUES (1, 'kjv', 'King James Version')")
    for book in (19, 42, 43, 45):
        db.execute("INSERT INTO books (id, name, testament, chapter_count) VALUES (?, 'x', 'NT', 50)", (book,))
    db.execute("PRAGMA user_version = 1")
    db.commit()
    db.close()


class ThisCheckout(unittest.TestCase):
    def test_reads_the_version_identifier_and_schema_this_checkout_builds(self):
        conf = u.TAURI_CONF.read_text(encoding="utf-8")
        self.assertRegex(u.expected_version(conf), r"^\d+\.\d+\.\d+")
        self.assertEqual(u.identifier(conf), "com.tossbaws.gospel-getter")
        self.assertGreaterEqual(u.schema_version(u.MIGRATE_RS.read_text(encoding="utf-8")), 2)

    def test_schema_2_promises_its_copy(self):
        steps = u.kept_copies(p.read_text(encoding="utf-8") for p in u.MIGRATE_DIR.glob("schema_*.rs"))
        self.assertEqual(steps.get(2), True)
        self.assertNotIn(1, steps)

    def test_a_schema_version_it_cannot_find_stops_the_check(self):
        with self.assertRaises(u.Refused):
            u.schema_version("const SCHEMA_VERSION: i64 = 2;")


class Copies(unittest.TestCase):
    def test_only_the_steps_that_ran_and_keep_a_copy(self):
        steps = {2: True, 3: False, 4: True}
        self.assertEqual(u.promised_copies(1, 4, steps), [2, 4])
        self.assertEqual(u.promised_copies(2, 4, steps), [4])
        self.assertEqual(u.promised_copies(2, 2, steps), [])
        self.assertEqual(u.kept_copies(["Step {\n    to: 3,\n    keep_copy: false,\n"]), {3: False})


class PreviousRelease(unittest.TestCase):
    SUMS = ("bc89  Gospel-Getter_amd64.AppImage\n"
            "5d5c  Gospel-Getter_x64-setup.exe\n")

    def test_finds_its_asset_in_sha256sums(self):
        self.assertEqual(u.checksum(self.SUMS, "Gospel-Getter_x64-setup.exe"), "5d5c")
        self.assertEqual(u.checksum("ABCD *Gospel-Getter_amd64.AppImage\n", "Gospel-Getter_amd64.AppImage"), "abcd")
        with self.assertRaises(u.Refused):
            u.checksum(self.SUMS, "Gospel-Getter_x64_en-US.msi")

    def fake_gh(self, out, payload, fail=None):
        calls = []

        def gh(*args):
            calls.append(args)
            if fail and args[0] == fail:
                raise u.Refused("HTTP 503")
            if args[0] == "api":
                return "v2.33.1\n"
            (out / u.ASSETS["linux"]).write_bytes(payload)
            digest = u.hashlib.sha256(b"the real one").hexdigest()
            (out / "SHA256SUMS").write_text(f"{digest}  {u.ASSETS['linux']}\n", encoding="utf-8")
            return ""

        return gh, calls

    def test_downloads_the_latest_and_checks_it(self):
        with tempfile.TemporaryDirectory() as temp, mock.patch.dict(os.environ, {"GH_TOKEN": "x"}):
            out = Path(temp)
            gh, calls = self.fake_gh(out, b"the real one")
            with redirect_stdout(io.StringIO()):
                tag, path = u.previous("linux", out, "o/r", gh=gh)
            self.assertEqual((tag, path), ("v2.33.1", out / "Gospel-Getter_amd64.AppImage"))
            self.assertEqual(calls[0], ("api", "repos/o/r/releases/latest", "--jq", ".tag_name"))
            self.assertIn("v2.33.1", calls[1])

    def test_a_file_that_does_not_match_never_passes(self):
        with tempfile.TemporaryDirectory() as temp, mock.patch.dict(os.environ, {"GH_TOKEN": "x"}):
            gh, _ = self.fake_gh(Path(temp), b"something else")
            with self.assertRaisesRegex(u.Refused, "not SHA256SUMS'"):
                u.previous("linux", Path(temp), "o/r", gh=gh)

    def test_a_failed_download_fails_with_its_reason(self):
        with tempfile.TemporaryDirectory() as temp, mock.patch.dict(os.environ, {"GH_TOKEN": "x"}):
            for step, words in (("api", "couldn't find the previous"), ("release", "couldn't download v2.33.1")):
                gh, _ = self.fake_gh(Path(temp), b"the real one", fail=step)
                with self.assertRaisesRegex(u.Refused, words):
                    u.previous("linux", Path(temp), "o/r", gh=gh)

    def test_needs_the_workflow_token(self):
        with mock.patch.dict(os.environ, {}, clear=True), self.assertRaisesRegex(u.Refused, "GH_TOKEN"):
            u.previous("linux", Path("."), "o/r", gh=None)


class Seeding(unittest.TestCase):
    def test_seeds_a_version_1_database_by_its_own_columns(self):
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "gospel_getter.db"
            version_1_database(path)
            seeded = u.seed(path)
            self.assertEqual(seeded["schema"], 1)
            self.assertEqual(seeded["bookmarks"], [(1, 45, 8, 28, 28)])
            self.assertEqual(seeded["highlights"], [(43, 3, 16, "green")])
            self.assertEqual(seeded["notes"], [(1, 19, 23, 1, 2, u.NOTE_BODY)])
            self.assertEqual(seeded["reading_position"], [(1, 42, 2)])
            self.assertEqual(seeded["reading_place"], [(1, 42, 2, 10)])
            self.assertEqual(u.user_version(path), 1)

    def test_a_database_without_the_readers_tables_stops_the_check(self):
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "gospel_getter.db"
            sqlite3.connect(path).close()
            with self.assertRaisesRegex(u.Refused, "lacks"):
                u.seed(path)


class ReadBack(unittest.TestCase):
    def test_every_seeded_item_found(self):
        self.assertEqual({k: ok for k, (ok, _) in u.kept(ANSWERS).items()},
                         dict.fromkeys(u.SEED, True))

    def test_each_lost_item_fails_alone(self):
        for command, key in (("list_bookmarks", "bookmarks"), ("list_highlights", "highlights"),
                             ("list_notes", "notes")):
            answers = json.loads(json.dumps(ANSWERS))
            answers[command][key] = []
            lost = [k for k, (ok, _) in u.kept(answers).items() if not ok]
            self.assertEqual(len(lost), 1, command)
        answers = json.loads(json.dumps(ANSWERS))
        answers["list_notes"]["notes"][0]["body"] = "changed"
        answers["get_reading_place"] = {"saved": False}
        lost = sorted(k for k, (ok, _) in u.kept(answers).items() if not ok)
        self.assertEqual(lost, ["note", "reading place"])


class Printed(unittest.TestCase):
    def test_a_verdict_names_the_item_without_its_verses(self):
        found = {"reference": "John 3:16", "verses": [{"verse": 16, "text": "For God so loved"}]}
        self.assertEqual(u.without_verses(found), {"reference": "John 3:16"})
        self.assertEqual(u.without_verses([found]), [{"reference": "John 3:16"}])


class Started(unittest.TestCase):
    def seen(self, **kw):
        seen = u.Start()
        seen.alive, seen.titles, seen.seconds = True, ["Gospel Getter"], 12
        for k, v in kw.items():
            setattr(seen, k, v)
        return seen

    def test_a_window_open_and_nothing_wrong_passes(self):
        ok, detail = u.started_verdict(self.seen(), 10)
        self.assertTrue(ok, detail)
        # Before 2.33.0, the passage came first.
        self.assertTrue(u.started_verdict(self.seen(titles=["Genesis 1 · KJV — Gospel Getter"]), 10)[0])

    def test_2_33_0s_failure_fails(self):
        # What 2.33.0 did on a PC upgraded from 2.32.x: alive, waiting on
        # its dialog, with the error in its log.
        output = ("2026-11-21T10:00:00Z ERROR gospel_getter_lib: Gospel Getter couldn't start: Failed to "
                  "initialize application state: Failed to run database migrations: schema_1.sql holds a "
                  "statement that isn't a CREATE TABLE or INDEX: it.\n")
        ok, detail = u.started_verdict(self.seen(titles=["Gospel Getter couldn't start"], output=output), 10)
        self.assertFalse(ok)
        self.assertIn("schema_1.sql", detail)
        self.assertIn("dialog is open", detail)

    def test_a_crash_or_no_window_fails(self):
        self.assertFalse(u.started_verdict(self.seen(alive=False, closed="exited with 101"), 10)[0])
        self.assertFalse(u.started_verdict(self.seen(titles=[]), 10)[0])
        self.assertFalse(u.started_verdict(self.seen(log="... Gospel Getter couldn't start: x"), 10)[0])

    def test_a_rotated_log_is_read_whole(self):
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "gospel-getter.log"
            path.write_text("old line that is long\n", encoding="utf-8")
            log = u.LogFile(path)
            with open(path, "a", encoding="utf-8") as f:
                f.write("new\n")
            self.assertEqual(log.since(), "new\n")
            path.write_text("rotated\n", encoding="utf-8")
            self.assertEqual(log.since(), "rotated\n")

    def test_versions_compare_by_their_numbers(self):
        self.assertEqual(u.numeric("2.33.1.0"), "2.33.1")
        self.assertEqual(u.numeric("v2.34.0"), "2.34.0")
        self.assertIsNone(u.numeric(""))
        self.assertEqual(u.log_says("x INFO a: Using database at /d/gospel_getter.db\n", "Using database at "),
                         "/d/gospel_getter.db")


class Refusal(unittest.TestCase):
    def test_refuses_without_the_ci_only_switch(self):
        for value in (None, "1", "yes"):
            env = {} if value is None else {u.SWITCH: value}
            with mock.patch.dict(os.environ, env, clear=True), redirect_stdout(io.StringIO()) as out:
                code = u.main(["run", "--from", "a", "--to", "b", "--work", "w"])
            self.assertEqual(code, 2)
            self.assertIn("refusing to run", out.getvalue())

    def test_refuses_an_existing_data_folder(self):
        with tempfile.TemporaryDirectory() as temp:
            platform = u.Windows.__new__(u.Windows)
            platform.data_dir = Path(temp)
            platform.log_dir = Path(temp) / "missing" / "logs"
            with self.assertRaisesRegex(u.Refused, "already exists"):
                platform.guard()


if __name__ == "__main__":
    unittest.main()
