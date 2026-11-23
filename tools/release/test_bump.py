"""Tests for bump.py. Run from the repository root:
    python3 -m unittest discover -s tools/release
"""

import contextlib
import io
import shutil
import tempfile
import unittest
from pathlib import Path

import bump as b

ROOT = Path(__file__).resolve().parents[2]

# What a bump edits, as the files have it, plus a version literal beside
# each that a bump must leave alone.
TREE = {
    "src-tauri/Cargo.toml": '[package]\nname = "gospel_getter"\nversion = "2.36.0"\n\n'
    '[dependencies]\nfoo = { version = "2.36.0" }\n',
    "src-tauri/cli/Cargo.toml": '[package]\nname = "gospel-getter-cli"\n# A comment.\n'
    'version = "2.36.0"\n\n[dev-dependencies]\nbar = "2.36.0"\n',
    "src-tauri/tauri.conf.json": '{\n  "productName": "Gospel Getter",\n  "version": "2.36.0",\n'
    '  "x": { "version": "2.36.0" }\n}\n',
    "src-tauri/Cargo.lock": '[[package]]\nname = "gospel-getter-cli"\nversion = "2.36.0"\n\n'
    '[[package]]\nname = "gospel_getter"\nversion = "2.36.0"\n\n'
    '[[package]]\nname = "gospel_getter_core"\nversion = "0.0.0"\n\n'
    '[[package]]\nname = "other"\nversion = "2.36.0"\n',
    "packaging/PKGBUILD": "# Bump _pkgver=2.36.0 by hand? No.\n_pkgver=2.36.0\npkgver=2.36.0\npkgrel=3\n"
    "sha256sums=('SKIP')\n",
    "site/index.html": '                <p class="small hero-note">Version 2.36.0, for 64-bit computers.</p>\n'
    '                <p class="release-line"><strong>Version 2.36.0.</strong> Free, for 64-bit '
    "Windows and Linux. <a href=\"x\">What’s new</a></p>\n",
    "README.md": "From 2.36.0 on, About offers Go back.\n"
    "**Coming from 2.6.0 through 2.35.0?** Each updates itself to 2.36.0: it\n"
    "update themselves. The 2.19.0 pre-release updates itself to 2.36.0 the\n"
    "Install 2.36.0 by hand once, from the\n"
    "Coming from 2.33.0 on Windows? Install 2.33.1 or newer.\n",
    "CHANGELOG.md": "## [2.36.0] - 2026-11-22\n",
}


class Tree:
    def __enter__(self):
        self.dir = Path(tempfile.mkdtemp(prefix="bump-test-"))
        for path, text in TREE.items():
            (self.dir / path).parent.mkdir(parents=True, exist_ok=True)
            (self.dir / path).write_bytes(text.encode("utf-8"))
        return self.dir

    def __exit__(self, *_):
        shutil.rmtree(self.dir)


def run(*args):
    out, err = io.StringIO(), io.StringIO()
    with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
        code = b.main(list(args))
    return code, out.getvalue(), err.getvalue()


def files(root):
    return {path: (root / path).read_bytes().decode("utf-8") for path in TREE}


class Versions(unittest.TestCase):
    def test_only_a_greater_version_is_newer(self):
        self.assertTrue(b.newer("2.36.1", "2.36.0"))
        self.assertTrue(b.newer("2.37.0", "2.36.9"))
        self.assertTrue(b.newer("2.10.0", "2.9.0"))
        self.assertTrue(b.newer("2.19.0", "2.19.0-beta.1"))
        self.assertFalse(b.newer("2.36.0", "2.36.0"))
        self.assertFalse(b.newer("2.35.9", "2.36.0"))
        self.assertFalse(b.newer("2.18.9", "2.19.0-beta.1"))

    def test_pkgver_drops_a_pre_releases_dash_and_dots(self):
        self.assertEqual(b.pkgver("2.19.0-beta.1"), "2.19.0beta1")
        self.assertEqual(b.pkgver("2.36.0"), "2.36.0")


class Bump(unittest.TestCase):
    def test_it_edits_exactly_the_version_strings(self):
        with Tree() as root:
            code, out, _ = run("2.36.1", "--root", str(root))
            self.assertEqual(code, 0)
            self.assertIn("2.36.0 -> 2.36.1: changed 13 lines", out)
            after = files(root)
        expected = {path: text for path, text in TREE.items()}
        expected["src-tauri/Cargo.toml"] = expected["src-tauri/Cargo.toml"].replace(
            'version = "2.36.0"\n\n', 'version = "2.36.1"\n\n', 1
        )
        expected["src-tauri/cli/Cargo.toml"] = expected["src-tauri/cli/Cargo.toml"].replace(
            'version = "2.36.0"\n\n', 'version = "2.36.1"\n\n', 1
        )
        expected["src-tauri/tauri.conf.json"] = expected["src-tauri/tauri.conf.json"].replace(
            '"version": "2.36.0",', '"version": "2.36.1",'
        )
        for name in ("gospel-getter-cli", "gospel_getter"):
            expected["src-tauri/Cargo.lock"] = expected["src-tauri/Cargo.lock"].replace(
                f'name = "{name}"\nversion = "2.36.0"', f'name = "{name}"\nversion = "2.36.1"'
            )
        expected["packaging/PKGBUILD"] = (
            "# Bump _pkgver=2.36.0 by hand? No.\n_pkgver=2.36.1\npkgver=2.36.1\npkgrel=1\n"
            "sha256sums=('SKIP')\n"
        )
        expected["site/index.html"] = expected["site/index.html"].replace("Version 2.36.0", "Version 2.36.1")
        expected["README.md"] = (
            "From 2.36.0 on, About offers Go back.\n"
            "**Coming from 2.6.0 through 2.36.0?** Each updates itself to 2.36.1: it\n"
            "update themselves. The 2.19.0 pre-release updates itself to 2.36.1 the\n"
            "Install 2.36.1 by hand once, from the\n"
            "Coming from 2.33.0 on Windows? Install 2.33.1 or newer.\n"
        )
        self.assertEqual(after, expected)

    def test_check_prints_the_edits_and_changes_nothing(self):
        with Tree() as root:
            code, out, _ = run("2.37.0", "--check", "--root", str(root))
            self.assertEqual(code, 0)
            self.assertEqual(files(root), TREE)
        self.assertIn("2.36.0 -> 2.37.0: would change 13 lines", out)
        self.assertIn("README.md:2\n  - **Coming from 2.6.0 through 2.35.0?**", out)
        self.assertIn("src-tauri/Cargo.lock:3\n", out)

    def test_a_version_not_above_the_current_one_is_refused(self):
        for version in ("2.36.0", "2.35.9", "1.0.0"):
            with Tree() as root:
                code, _, err = run(version, "--root", str(root))
                self.assertEqual(code, 2, version)
                self.assertIn("isn't greater than the current version, 2.36.0", err)
                self.assertEqual(files(root), TREE)

    def test_anything_but_a_plain_version_is_refused(self):
        for version in ("2.37", "v2.37.0", "2.37.0-beta.1", "2.37.0 ", "latest"):
            with Tree() as root:
                code, _, err = run(version, "--root", str(root))
                self.assertEqual(code, 2, version)
                self.assertIn("must be X.Y.Z", err)
                self.assertEqual(files(root), TREE)

    def test_a_file_out_of_step_stops_the_bump_and_writes_nothing(self):
        for path, old, new in [
            ("src-tauri/tauri.conf.json", '"version": "2.36.0",', '"version": "2.35.0",'),
            ("site/index.html", "Version 2.36.0,", "Version 2.35.0,"),
            ("README.md", "Install 2.36.0 by hand", "Install it by hand"),
        ]:
            with Tree() as root:
                (root / path).write_bytes(TREE[path].replace(old, new).encode("utf-8"))
                code, _, err = run("2.36.1", "--root", str(root))
                self.assertEqual(code, 2, path)
                self.assertIn(f"{path}: expected one line matching", err)
                changed = files(root)
                changed[path] = TREE[path]
                self.assertEqual(changed, TREE)

    def test_a_line_found_twice_stops_the_bump(self):
        with Tree() as root:
            doubled = TREE["README.md"] + "Install 2.36.0 by hand once, from the\n"
            (root / "README.md").write_bytes(doubled.encode("utf-8"))
            code, _, err = run("2.36.1", "--root", str(root))
            self.assertEqual(code, 2)
            self.assertIn("found 2", err)

    def test_line_endings_are_kept(self):
        with Tree() as root:
            crlf = TREE["site/index.html"].replace("\n", "\r\n")
            (root / "site/index.html").write_bytes(crlf.encode("utf-8"))
            # A CRLF line doesn't end where `$` looks, so it's refused, not
            # rewritten with LF endings.
            code, _, _ = run("2.36.1", "--root", str(root))
            self.assertEqual(code, 2)
            self.assertEqual((root / "site/index.html").read_bytes(), crlf.encode("utf-8"))


class TheRealTree(unittest.TestCase):
    def test_every_edit_a_bump_makes_is_found(self):
        # The README's or site's wording moving away from what bump.py looks
        # for fails here, not at the next release.
        current = b.current_version(ROOT)
        major, minor, patch, _ = b.parse(current)
        _, edits, _ = b.plan(ROOT, f"{major}.{minor}.{patch + 1}")
        self.assertEqual(
            sorted({edit.path for edit in edits}),
            sorted(
                [
                    "README.md",
                    "packaging/PKGBUILD",
                    "site/index.html",
                    "src-tauri/Cargo.lock",
                    "src-tauri/Cargo.toml",
                    "src-tauri/cli/Cargo.toml",
                    "src-tauri/tauri.conf.json",
                ]
            ),
        )


if __name__ == "__main__":
    unittest.main()
