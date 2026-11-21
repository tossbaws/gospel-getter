"""Tests for committed_bytes.py. Run from the repository root:
    python3 -m unittest discover -s tools/ci

Each test makes a repository, commits LF files, and checks a second clone
out the way a CI runner might, with the clone's own git config.
"""

import contextlib
import io
import os
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest import mock

import committed_bytes as c

# The 2.33.1 rules (the repository's own .gitattributes, in short).
ATTRIBUTES = b"* text=auto eol=lf\nraw.txt -text\n"
SQL = b"CREATE TABLE books (id INTEGER);\nCREATE INDEX b ON books (id);\n"
RAW_CRLF = b"verse one\r\nverse two\r\n"

# No machine's own git config: only what each test sets.
ENV = {
    **os.environ,
    "GIT_CONFIG_NOSYSTEM": "1",
    "GIT_CONFIG_GLOBAL": os.devnull,
    "GIT_AUTHOR_NAME": "t",
    "GIT_AUTHOR_EMAIL": "t@example.invalid",
    "GIT_COMMITTER_NAME": "t",
    "GIT_COMMITTER_EMAIL": "t@example.invalid",
}


def run(*args: str, cwd: Path) -> str:
    return subprocess.run(
        args, cwd=cwd, env=ENV, check=True, capture_output=True, text=True
    ).stdout


class CommittedBytesTest(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self._tmp.cleanup)
        self.tmp = Path(self._tmp.name)
        self.origin = self.tmp / "origin"
        self.origin.mkdir()
        run("git", "init", "-q", "-b", "main", cwd=self.origin)
        run("git", "config", "core.autocrlf", "false", cwd=self.origin)
        (self.origin / "schema_1.sql").write_bytes(SQL)
        (self.origin / "raw.txt").write_bytes(RAW_CRLF)
        (self.origin / "sub").mkdir()
        (self.origin / "sub" / "a b.txt").write_bytes(b"spaces\n")
        # committed_bytes runs git with the process's environment.
        patch = mock.patch.dict(os.environ, ENV, clear=True)
        patch.start()
        self.addCleanup(patch.stop)

    def commit(self, attributes: bytes | None):
        if attributes is not None:
            (self.origin / ".gitattributes").write_bytes(attributes)
        run("git", "add", "-A", cwd=self.origin)
        run("git", "commit", "-q", "-m", "files", cwd=self.origin)

    def clone(self, *config: str) -> Path:
        """A clone checked out with `config` (`key=value`, as `git -c`)."""
        target = self.tmp / f"clone{len(list(self.tmp.iterdir()))}"
        args = ["git"]
        for setting in config:
            args += ["-c", setting]
        run(*args, "clone", "-q", str(self.origin), str(target), cwd=self.tmp)
        for setting in config:
            key, value = setting.split("=", 1)
            run("git", "config", key, value, cwd=target)
        return target

    def assert_clean(self, repo: Path):
        compared, mismatches = c.check(repo)
        self.assertGreater(compared, 0)
        self.assertEqual(mismatches, [])

    def test_a_plain_checkout_is_the_committed_bytes(self):
        self.commit(ATTRIBUTES)
        self.assert_clean(self.clone())

    def test_autocrlf_is_harmless_under_the_eol_rule(self):
        # The Windows runner's own setting, with .gitattributes' first rule.
        self.commit(ATTRIBUTES)
        repo = self.clone("core.autocrlf=true")
        self.assertEqual((repo / "schema_1.sql").read_bytes(), SQL)
        self.assertEqual((repo / "raw.txt").read_bytes(), RAW_CRLF)
        self.assert_clean(repo)

    def test_a_crlf_checkout_fails_and_names_the_files(self):
        # 2.33.0's Windows build: autocrlf=true and no eol rule.
        self.commit(b"raw.txt -text\n")
        repo = self.clone("core.autocrlf=true")
        self.assertEqual(
            (repo / "schema_1.sql").read_bytes(), SQL.replace(b"\n", b"\r\n")
        )
        _, mismatches = c.check(repo)
        self.assertEqual(
            sorted(m.path for m in mismatches), [".gitattributes", "schema_1.sql", "sub/a b.txt"]
        )
        self.assertTrue(all(m.reason == "differs from the committed bytes" for m in mismatches))

    def test_renormalizing_would_miss_that_checkout(self):
        # Why the check hashes the bytes on disk (see the module's docstring).
        self.commit(b"raw.txt -text\n")
        repo = self.clone("core.autocrlf=true")
        run("git", "add", "--renormalize", ".", cwd=repo)
        run("git", "diff", "--cached", "--exit-code", cwd=repo)
        run("git", "diff", "--exit-code", cwd=repo)
        self.assertTrue(c.check(repo)[1])

    def test_a_smudge_filter_fails(self):
        self.commit(ATTRIBUTES + b"*.sql filter=upper\n")
        repo = self.clone(
            "filter.upper.smudge=tr a-z A-Z",
            "filter.upper.clean=cat",
        )
        _, mismatches = c.check(repo)
        self.assertEqual([m.path for m in mismatches], ["schema_1.sql"])

    def test_ident_fails(self):
        (self.origin / "id.txt").write_bytes(b"$Id$\n")
        self.commit(ATTRIBUTES + b"id.txt ident\n")
        repo = self.clone()
        self.assertNotEqual((repo / "id.txt").read_bytes(), b"$Id$\n")
        _, mismatches = c.check(repo)
        self.assertEqual([m.path for m in mismatches], ["id.txt"])

    def test_an_edited_or_missing_file_fails(self):
        self.commit(ATTRIBUTES)
        repo = self.clone()
        (repo / "schema_1.sql").write_bytes(SQL + b"-- more\n")
        (repo / "sub" / "a b.txt").unlink()
        _, mismatches = c.check(repo)
        self.assertEqual(
            mismatches,
            [
                c.Mismatch("schema_1.sql", "differs from the committed bytes"),
                c.Mismatch("sub/a b.txt", "missing from the working tree"),
            ],
        )

    @unittest.skipIf(os.name == "nt", "a symlink needs privileges on Windows")
    def test_a_symlink_is_compared_by_its_target(self):
        os.symlink("schema_1.sql", self.origin / "link.sql")
        self.commit(ATTRIBUTES)
        self.assert_clean(self.clone())

    def test_untracked_files_are_left_alone(self):
        self.commit(ATTRIBUTES)
        repo = self.clone()
        (repo / "dist").mkdir()
        (repo / "dist" / "index.html").write_bytes(b"built\r\n")
        self.assert_clean(repo)

    def test_blob_id_is_gits(self):
        self.commit(ATTRIBUTES)
        for data in [b"", SQL, RAW_CRLF]:
            want = subprocess.run(
                ["git", "hash-object", "--stdin", "--no-filters"],
                cwd=self.origin,
                env=ENV,
                input=data,
                check=True,
                capture_output=True,
            ).stdout.decode().strip()
            self.assertEqual(c.blob_id(data, "sha1"), want)

    def test_main_reports_and_exits(self):
        self.commit(b"raw.txt -text\n")
        bad = self.clone("core.autocrlf=true")
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            self.assertEqual(c.main(["committed_bytes.py", str(bad)]), 1)
        self.assertIn("::error file=schema_1.sql::schema_1.sql differs", out.getvalue())
        self.assertIn("w/crlf", out.getvalue())

        self.commit(ATTRIBUTES)
        good = self.clone()
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            self.assertEqual(c.main(["committed_bytes.py", str(good)]), 0)
        self.assertIn("are as committed", out.getvalue())


if __name__ == "__main__":
    unittest.main()
