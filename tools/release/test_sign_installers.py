"""Tests for sign_installers.py. Run from the repository root:
    python3 -m unittest discover -s tools/release

The Tauri CLI is stood in for by a small script that writes a signature
shaped as the CLI writes one, by a key ID the test chooses, and records how
it was called (and, when a test asks, changes the installer it signs).
"""

import base64
import contextlib
import hashlib
import io
import json
import os
import shlex
import shutil
import subprocess
import sys
import tempfile
import textwrap
import unittest
from pathlib import Path
from unittest import mock

import sign_installers as s

INSTALLERS = {
    "gospel-getter-linux-appimage": "Gospel Getter_2.6.0_amd64.AppImage",
    "gospel-getter-linux-deb": "Gospel Getter_2.6.0_amd64.deb",
    "gospel-getter-windows-nsis-installer": "Gospel Getter_2.6.0_x64-setup.exe",
    "gospel-getter-windows-msi-installer": "Gospel Getter_2.6.0_x64_en-US.msi",
}

APP_KEY = bytes.fromhex("770FDDD51FD1EE2C")
OTHER_KEY = bytes.fromhex("0102030405060708")

FAKE_CLI = textwrap.dedent(
    """
    import base64, json, os, sys
    from pathlib import Path

    args = sys.argv[1:]
    assert args[:2] == ["signer", "sign"], args
    version = args[args.index("--app-version") + 1]
    signed = os.environ.get("FAKE_SIGNED_VERSION", version)
    key = bytes.fromhex(os.environ["FAKE_KEY_ID"])
    target = Path(args[-1])
    with open(os.environ["FAKE_CALLS"], "a", encoding="utf-8") as calls:
        calls.write(json.dumps(args) + "\\n")
    blob = base64.b64encode(b"Ed" + key + bytes(64)).decode()
    text = (
        "untrusted comment: signature from tauri secret key\\n"
        f"{blob}\\n"
        f"trusted comment: timestamp:1760000000\\tfile:{target.name}\\tversion:{signed}\\n"
        "FakeGlobalSignature==\\n"
    )
    target.with_name(target.name + ".sig").write_text(base64.b64encode(text.encode()).decode())
    if os.environ.get("FAKE_CHANGE_INSTALLER") == target.name:
        with open(target, "ab") as installer:
            installer.write(b" changed")
    """
)


def pubkey(key: bytes) -> str:
    """A base64 minisign public key, as tauri.conf.json holds it."""
    text = (
        f"untrusted comment: minisign public key: {key[::-1].hex().upper()}\n"
        f"{base64.b64encode(b'Ed' + key + bytes(32)).decode()}\n"
    )
    return base64.b64encode(text.encode()).decode()


class SignInstallers(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        root = Path(self.tmp.name)
        self.dir = root / "installers"
        for artifact, name in INSTALLERS.items():
            (self.dir / artifact).mkdir(parents=True)
            (self.dir / artifact / name).write_bytes(f"installer {name}".encode())
        # The CLI beside the installers is no installer.
        (self.dir / "gospel-getter-cli-x86_64-pc-windows-msvc.exe").write_bytes(b"cli")
        self.conf = root / "tauri.conf.json"
        self.write_conf("2.6.0")
        self.cli = root / "fake_cli.py"
        self.cli.write_text(FAKE_CLI)
        self.calls = root / "calls.jsonl"
        env = {
            "FAKE_KEY_ID": APP_KEY.hex(),
            "FAKE_CALLS": str(self.calls),
            "TAURI_SIGNING_PRIVATE_KEY": "the job's secret",
        }
        patcher = mock.patch.dict(os.environ, env)
        patcher.start()
        self.addCleanup(patcher.stop)
        os.environ.pop("FAKE_SIGNED_VERSION", None)
        os.environ.pop("FAKE_CHANGE_INSTALLER", None)

    def tearDown(self):
        self.tmp.cleanup()

    def write_conf(self, version):
        self.conf.write_text(
            json.dumps({"version": version, "plugins": {"updater": {"pubkey": pubkey(APP_KEY)}}})
        )

    def run_main(self, *extra):
        out, err = io.StringIO(), io.StringIO()
        cli = f"{shlex.quote(sys.executable)} {shlex.quote(str(self.cli))}"
        with contextlib.redirect_stderr(err), contextlib.redirect_stdout(out):
            code = s.main([str(self.dir), "--conf", str(self.conf), "--cli", cli, *extra])
        return code, out.getvalue(), err.getvalue()

    def sigs(self):
        return sorted(p.name for p in self.dir.rglob("*.sig"))

    def calls_made(self):
        if not self.calls.exists():
            return []
        return [json.loads(line) for line in self.calls.read_text().splitlines()]

    def test_signs_each_installer_beside_itself_for_the_apps_version(self):
        code, out, err = self.run_main()
        self.assertEqual(code, 0, err)
        for artifact, name in INSTALLERS.items():
            self.assertTrue((self.dir / artifact / f"{name}.sig").is_file(), name)
        self.assertEqual(len(self.sigs()), 4, "the CLI binary isn't signed")
        for call in self.calls_made():
            self.assertEqual(call[:4], ["signer", "sign", "--app-version", "2.6.0"])
            # The key comes from the CLI's environment, never the command line.
            self.assertNotIn("--private-key", call)
            self.assertNotIn("--private-key-path", call)
        self.assertIn("By key 2CEED11FD5DD0F77", out)
        self.assertIn("the update manifest can be made from them", out)

    def test_the_installers_folder_may_be_relative(self):
        # As dist.yml runs it: `sign_installers.py installers`, from the
        # folder above, so every installer's path is relative.
        here = os.getcwd()
        os.chdir(self.dir.parent)
        self.addCleanup(os.chdir, here)
        self.dir = Path("installers")
        code, out, err = self.run_main()
        self.assertEqual(code, 0, err)
        self.assertIn("the update manifest can be made from them", out)

    def test_a_key_file_is_passed_with_its_empty_password(self):
        key = Path(self.tmp.name) / "rehearsal.key"
        key.write_text("throwaway")
        os.environ.pop("TAURI_SIGNING_PRIVATE_KEY")
        code, _, err = self.run_main("--private-key-path", str(key))
        self.assertEqual(code, 0, err)
        for call in self.calls_made():
            at = call.index("--private-key-path")
            self.assertEqual(call[at : at + 4], ["--private-key-path", str(key), "--password", ""])

    def test_the_rehearsal_checks_against_its_own_public_key(self):
        os.environ["FAKE_KEY_ID"] = OTHER_KEY.hex()
        pub = Path(self.tmp.name) / "rehearsal.key.pub"
        pub.write_text(pubkey(OTHER_KEY) + "\n")
        code, out, err = self.run_main("--pubkey-file", str(pub))
        self.assertEqual(code, 0, err)
        self.assertIn("By key 0807060504030201", out)

    def test_refuses_a_signature_by_another_key(self):
        os.environ["FAKE_KEY_ID"] = OTHER_KEY.hex()
        code, _, err = self.run_main()
        self.assertEqual(code, 1)
        self.assertIn("was made by key 0807060504030201, not the trusted key 2CEED11FD5DD0F77", err)

    def test_refuses_a_signature_for_another_version(self):
        os.environ["FAKE_SIGNED_VERSION"] = "2.5.0"
        code, _, err = self.run_main()
        self.assertEqual(code, 1)
        self.assertIn("was signed for version 2.5.0, not 2.6.0", err)

    def test_signs_nothing_unless_all_four_installers_are_there(self):
        (self.dir / "gospel-getter-windows-msi-installer" / INSTALLERS["gospel-getter-windows-msi-installer"]).unlink()
        code, _, err = self.run_main()
        self.assertEqual(code, 1)
        self.assertIn("no windows-x86_64-msi installer", err)
        self.assertEqual(self.calls_made(), [])
        self.assertEqual(self.sigs(), [])

    def test_signs_nothing_when_an_installer_is_ambiguous(self):
        (self.dir / "gospel-getter-linux-deb" / "Gospel Getter_2.5.0_amd64.deb").write_bytes(b"old")
        code, _, err = self.run_main()
        self.assertEqual(code, 1)
        self.assertIn("2 linux-x86_64-deb installers", err)
        self.assertEqual(self.calls_made(), [])

    def test_signs_nothing_twice(self):
        name = INSTALLERS["gospel-getter-linux-deb"]
        (self.dir / "gospel-getter-linux-deb" / f"{name}.sig").write_text("earlier")
        code, _, err = self.run_main()
        self.assertEqual(code, 1)
        self.assertIn("were signed before", err)
        self.assertEqual(self.calls_made(), [])

    def sums(self, root=None):
        """Each artifact's INSTALLER-SHA256SUMS under `root`, by artifact."""
        root = root or self.dir
        return {
            artifact: (root / artifact / s.SUMS).read_text(encoding="utf-8")
            for artifact in INSTALLERS
            if (root / artifact / s.SUMS).is_file()
        }

    def test_records_each_installers_sha256_beside_it(self):
        code, out, err = self.run_main()
        self.assertEqual(code, 0, err)
        sums = self.sums()
        for artifact, name in INSTALLERS.items():
            digest = hashlib.sha256(f"installer {name}".encode()).hexdigest()
            # As `sha256sum` writes it: the hash, two spaces, the file's name.
            self.assertEqual(sums[artifact], f"{digest}  {name}\n")
            self.assertIn(f"SHA-256 {digest}, the same before and after", out)

    def test_the_record_is_what_sha256sum_checks(self):
        sha256sum = shutil.which("sha256sum")
        if sha256sum is None:
            self.skipTest("no sha256sum here")
        code, _, err = self.run_main()
        self.assertEqual(code, 0, err)
        for artifact in INSTALLERS:
            checked = subprocess.run(
                [sha256sum, "-c", s.SUMS], cwd=self.dir / artifact, capture_output=True, text=True
            )
            self.assertEqual(checked.returncode, 0, checked.stdout + checked.stderr)

    def test_refuses_an_installer_that_signing_changed(self):
        name = INSTALLERS["gospel-getter-windows-nsis-installer"]
        os.environ["FAKE_CHANGE_INSTALLER"] = name
        before = hashlib.sha256(f"installer {name}".encode()).hexdigest()
        code, out, err = self.run_main()
        self.assertEqual(code, 1)
        self.assertIn(f"{name} changed while it was signed: SHA-256 {before} before, ", err)
        self.assertNotIn("Signed ", out)

    def test_a_hash_that_isnt_the_one_recorded_is_refused(self):
        installers = s.find_installers(self.dir)
        recorded = s.record_hashes(installers)
        s.check_hashes(installers, recorded)
        key = "linux-x86_64-deb"
        with self.assertRaisesRegex(s.SigningError, "changed while it was signed"):
            s.check_hashes(installers, {**recorded, key: "0" * 64})
        # And the file that goes up with it must say the same.
        installers[key].with_name(s.SUMS).write_text(f"{'0' * 64}  {installers[key].name}\n")
        with self.assertRaisesRegex(s.SigningError, "INSTALLER-SHA256SUMS beside .* says 0+, not the"):
            s.check_hashes(installers, recorded)

    def test_a_failed_sign_job_runs_again_on_the_build_artifacts(self):
        # The build's artifacts, which the sign job never replaces: each
        # attempt downloads them afresh.
        build = Path(self.tmp.name) / "build-artifacts"
        shutil.copytree(self.dir, build)
        first = Path(self.tmp.name) / "attempt-1"
        shutil.copytree(build, first)
        self.dir = first
        code, _, err = self.run_main()
        self.assertEqual(code, 0, err)
        recorded = self.sums()
        # What signed once is refused (as when the sign job replaced the
        # build's artifacts with signed ones, and a re-run took those).
        code, _, err = self.run_main()
        self.assertEqual(code, 1)
        self.assertIn("were signed before", err)
        # The re-run's own download: the build's artifacts, as built.
        second = Path(self.tmp.name) / "attempt-2"
        shutil.copytree(build, second)
        self.dir = second
        code, _, err = self.run_main()
        self.assertEqual(code, 0, err)
        self.assertEqual(self.sums(), recorded, "the same installers, the same hashes")
        for artifact, name in INSTALLERS.items():
            self.assertEqual(
                sorted(p.name for p in (build / artifact).iterdir()), [name], "the build's artifacts are untouched"
            )

    def test_signs_nothing_when_a_signed_artifact_was_downloaded(self):
        # A pattern such as gospel-getter-linux-* would also take the sign
        # job's own gospel-getter-linux-appimage-signed.
        signed = self.dir / "gospel-getter-linux-appimage-signed"
        signed.mkdir()
        name = INSTALLERS["gospel-getter-linux-appimage"]
        (signed / name).write_bytes(f"installer {name}".encode())
        (signed / f"{name}.sig").write_text("earlier")
        code, _, err = self.run_main()
        self.assertEqual(code, 1)
        self.assertIn("gospel-getter-linux-appimage-signed under", err)
        self.assertIn("a signed artifact, not the build's own", err)
        self.assertEqual(self.calls_made(), [])
        self.assertEqual(self.sums(), {})

    def test_signs_nothing_when_an_installer_was_hashed_before(self):
        artifact = "gospel-getter-windows-msi-installer"
        (self.dir / artifact / s.SUMS).write_text(f"{'0' * 64}  {INSTALLERS[artifact]}\n")
        code, _, err = self.run_main()
        self.assertEqual(code, 1)
        self.assertIn("INSTALLER-SHA256SUMS is already beside", err)
        self.assertEqual(self.calls_made(), [])

    def test_needs_a_key(self):
        os.environ.pop("TAURI_SIGNING_PRIVATE_KEY")
        code, _, err = self.run_main()
        self.assertEqual(code, 1)
        self.assertIn("no signing key", err)
        self.assertEqual(self.calls_made(), [])

    def test_a_pre_release_is_signed_but_has_no_manifest_to_make(self):
        self.write_conf("2.19.0-beta.1")
        code, out, err = self.run_main()
        self.assertEqual(code, 0, err)
        self.assertEqual(len(self.sigs()), 4)
        self.assertIn("2.19.0-beta.1 is a pre-release, which has no update manifest", out)

    def test_the_apps_key_is_the_one_in_tauri_conf(self):
        conf = json.loads((s.REPO_ROOT / "src-tauri" / "tauri.conf.json").read_text())
        self.assertEqual(s.app_pubkey(s.CONF), conf["plugins"]["updater"]["pubkey"])
        self.assertEqual(s.shown(s.key_id(s.app_pubkey(s.CONF), "it")), "2CEED11FD5DD0F77")
        self.assertEqual(s.app_version(s.CONF), conf["version"])

    def test_every_installer_kind_is_a_manifest_platform(self):
        import make_latest_json

        self.assertEqual(set(s.KINDS.values()), set(make_latest_json.PLATFORMS))


if __name__ == "__main__":
    unittest.main()
