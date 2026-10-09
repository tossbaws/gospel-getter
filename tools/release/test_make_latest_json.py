"""Tests for make_latest_json.py. Run from the repository root:
    python3 -m unittest discover -s tools/release
"""

import base64
import contextlib
import io
import json
import tempfile
import unittest
from pathlib import Path

import make_latest_json as m

NAMES = [
    "Gospel-Getter_amd64.AppImage",
    "Gospel-Getter_amd64.deb",
    "Gospel-Getter_x64-setup.exe",
    "Gospel-Getter_x64_en-US.msi",
]


def signature(version="2.6.0", file="Gospel Getter_2.6.0_amd64.AppImage"):
    """A .sig file's contents, shaped as the Tauri CLI writes them."""
    comment = f"timestamp:1760000000\tfile:{file}"
    if version is not None:
        comment += f"\tversion:{version}"
    text = (
        "untrusted comment: signature from tauri secret key\n"
        "RUR3D93VH9HuLFakeSignatureBytes==\n"
        f"trusted comment: {comment}\n"
        "FakeGlobalSignature==\n"
    )
    return base64.b64encode(text.encode()).decode()


class MakeLatestJson(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.dir = Path(self.tmp.name)
        for name in NAMES:
            (self.dir / name).write_bytes(b"installer")
            (self.dir / f"{name}.sig").write_text(signature() + "\n")

    def tearDown(self):
        self.tmp.cleanup()

    def run_main(self, *extra):
        err = io.StringIO()
        with contextlib.redirect_stderr(err), contextlib.redirect_stdout(io.StringIO()):
            code = m.main(["--version", "2.6.0", "--notes", " Notes \n", "--assets", str(self.dir),
                           "--pub-date", "2026-10-20T12:00:00Z", *extra])
        return code, err.getvalue()

    def test_writes_one_entry_per_installer_at_the_versioned_tag(self):
        code, err = self.run_main()
        self.assertEqual(code, 0, err)
        manifest = json.loads((self.dir / "latest.json").read_text())
        self.assertEqual(manifest["version"], "2.6.0")
        self.assertEqual(manifest["notes"], "Notes")
        self.assertEqual(manifest["pub_date"], "2026-10-20T12:00:00Z")
        base = "https://github.com/tossbaws/gospel-getter/releases/download/v2.6.0/"
        self.assertEqual(
            {k: v["url"] for k, v in manifest["platforms"].items()},
            {
                "linux-x86_64-appimage": base + "Gospel-Getter_amd64.AppImage",
                "linux-x86_64-deb": base + "Gospel-Getter_amd64.deb",
                "windows-x86_64-nsis": base + "Gospel-Getter_x64-setup.exe",
                "windows-x86_64-msi": base + "Gospel-Getter_x64_en-US.msi",
            },
        )
        # No bare `linux-x86_64`/`windows-x86_64`: the plugin would hand
        # it to any bundle it doesn't recognise.
        self.assertNotIn("linux-x86_64", manifest["platforms"])
        self.assertNotIn("windows-x86_64", manifest["platforms"])
        for entry in manifest["platforms"].values():
            self.assertNotIn("/latest/", entry["url"])
            self.assertEqual(entry["signature"], signature(), "the .sig file's contents")

    def test_each_signature_is_its_own_files(self):
        for name in NAMES:
            (self.dir / f"{name}.sig").write_text(signature(file=name))
        self.assertEqual(self.run_main()[0], 0)
        platforms = json.loads((self.dir / "latest.json").read_text())["platforms"]
        for key, name in m.PLATFORMS.items():
            self.assertEqual(platforms[key]["signature"], signature(file=name))

    def test_refuses_when_an_installer_or_signature_is_missing(self):
        for missing in ["Gospel-Getter_amd64.deb", "Gospel-Getter_x64_en-US.msi.sig"]:
            with self.subTest(missing=missing):
                (self.dir / missing).unlink()
                code, err = self.run_main()
                self.assertEqual(code, 1)
                self.assertIn(missing, err)
                self.assertIn("no manifest written", err)
                self.assertFalse((self.dir / "latest.json").exists())
                self.setUp()

    def test_lists_everything_missing_at_once(self):
        (self.dir / "Gospel-Getter_amd64.AppImage.sig").unlink()
        (self.dir / "Gospel-Getter_x64-setup.exe").unlink()
        code, err = self.run_main()
        self.assertEqual(code, 1)
        self.assertIn("Gospel-Getter_amd64.AppImage.sig", err)
        self.assertIn("Gospel-Getter_x64-setup.exe", err)

    def test_refuses_a_signature_for_another_version_or_none(self):
        for bad, why in [(signature("2.5.0"), "signed for version 2.5.0"),
                         (signature(None), "signed for version (none)"),
                         ("", "is empty"),
                         ("not base64!", "isn't a Tauri updater signature"),
                         (base64.b64encode(b"hello").decode(), "isn't a Tauri updater signature")]:
            with self.subTest(why=why):
                (self.dir / "Gospel-Getter_amd64.deb.sig").write_text(bad)
                code, err = self.run_main()
                self.assertEqual(code, 1)
                self.assertIn("Gospel-Getter_amd64.deb.sig", err)
                self.assertIn(why, err)
                self.assertFalse((self.dir / "latest.json").exists())

    def test_refuses_a_version_that_isnt_a_plain_release(self):
        for version in ["v2.6.0", "2.6", "2.6.0-beta.1", "02.6.0", ""]:
            with self.subTest(version=version):
                with self.assertRaises(m.ManifestError):
                    m.build_manifest(version, "", self.dir, "2026-10-20T12:00:00Z")

    def test_notes_can_come_from_a_file_and_out_can_be_elsewhere(self):
        notes = self.dir / "NOTES.md"
        notes.write_text("## 2.6.0\n\n- Updates from inside the app\n")
        out = self.dir / "dist" / "latest.json"
        err = io.StringIO()
        with contextlib.redirect_stderr(err), contextlib.redirect_stdout(io.StringIO()):
            code = m.main(["--version", "2.6.0", "--notes-file", str(notes), "--assets", str(self.dir),
                           "--out", str(out), "--pub-date", "2026-10-20T12:00:00Z"])
        self.assertEqual(code, 0, err.getvalue())
        self.assertEqual(json.loads(out.read_text())["notes"], "## 2.6.0\n\n- Updates from inside the app")

    def test_refuses_a_bad_date(self):
        code, err = self.run_main("--pub-date", "next tuesday")
        self.assertEqual(code, 1)
        self.assertIn("RFC 3339", err)

    def test_a_failed_write_leaves_no_partial_file(self):
        out = self.dir / "latest.json"
        with self.assertRaises(TypeError):
            m.write_json({"bad": object()}, out)
        self.assertFalse(out.exists())
        self.assertEqual([p.name for p in self.dir.iterdir() if p.name.startswith(".latest")], [])


if __name__ == "__main__":
    unittest.main()
