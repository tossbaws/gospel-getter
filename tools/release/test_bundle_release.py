"""Tests for bundle_release.py. Run from the repository root:
    python3 -m unittest discover -s tools/release

A run's artifacts are stood in for by folders laid out as dist.yml's bundle
job downloads them, with signatures shaped as the Tauri CLI writes them, by
a key ID the test chooses.
"""

import base64
import contextlib
import hashlib
import io
import json
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

import bundle_release as b
import make_latest_json
import sign_installers

from test_sign_installers import APP_KEY, INSTALLERS, OTHER_KEY, pubkey

CLIS = {
    "gospel-getter-cli-linux-x86_64": "gospel-getter-cli-x86_64-unknown-linux-gnu",
    "gospel-getter-cli-windows-x86_64": "gospel-getter-cli-x86_64-pc-windows-msvc.exe",
}

# The stable names a release uploads, by build artifact.
STABLE = {
    "gospel-getter-linux-appimage": "Gospel-Getter_amd64.AppImage",
    "gospel-getter-linux-deb": "Gospel-Getter_amd64.deb",
    "gospel-getter-windows-nsis-installer": "Gospel-Getter_x64-setup.exe",
    "gospel-getter-windows-msi-installer": "Gospel-Getter_x64_en-US.msi",
}


def signature(name: str, version: str, key: bytes) -> str:
    """A .sig's text, as the Tauri CLI writes one."""
    blob = base64.b64encode(b"Ed" + key + bytes(64)).decode()
    text = (
        "untrusted comment: signature from tauri secret key\n"
        f"{blob}\n"
        f"trusted comment: timestamp:1760000000\tfile:{name}\tversion:{version}\n"
        "FakeGlobalSignature==\n"
    )
    return base64.b64encode(text.encode()).decode()


class BundleRelease(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        root = Path(self.tmp.name)
        self.dir = root / "bundle-in"
        self.out = root / "release-bundle"
        self.conf = root / "tauri.conf.json"
        self.write_conf("2.6.0")
        for artifact, name in INSTALLERS.items():
            data = f"installer {name}".encode()
            built = self.dir / "built" / artifact
            built.mkdir(parents=True)
            (built / name).write_bytes(data)
            signed = self.dir / "signed" / artifact
            signed.mkdir(parents=True)
            (signed / name).write_bytes(data)
            (signed / f"{name}.sig").write_text(signature(name, "2.6.0", APP_KEY))
            (signed / sign_installers.SUMS).write_text(f"{hashlib.sha256(data).hexdigest()}  {name}\n")
        for artifact, name in CLIS.items():
            (self.dir / "cli" / artifact).mkdir(parents=True)
            (self.dir / "cli" / artifact / name).write_bytes(f"cli {name}".encode())

    def write_conf(self, version):
        self.conf.write_text(
            json.dumps({"version": version, "plugins": {"updater": {"pubkey": pubkey(APP_KEY)}}})
        )

    def run_main(self, *extra):
        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stderr(err), contextlib.redirect_stdout(out):
            code = b.main([str(self.dir), str(self.out), "--conf", str(self.conf), *extra])
        return code, out.getvalue(), err.getvalue()

    def assert_refused(self, *reasons):
        code, out, err = self.run_main()
        self.assertEqual(code, 1, out)
        for reason in reasons:
            self.assertIn(reason, err)
        self.assertIn("bundle: REFUSED, nothing written", err)
        self.assertFalse(self.out.exists() and any(self.out.iterdir()), "something was written")
        return err

    def signed(self, artifact):
        return self.dir / "signed" / artifact / INSTALLERS[artifact]

    def test_a_good_run_is_bundled_under_the_stable_names(self):
        code, out, err = self.run_main()
        self.assertEqual(code, 0, err)
        expected = sorted(
            [*STABLE.values(), *(f"{n}.sig" for n in STABLE.values()), *CLIS.values(), "SHA256SUMS"]
        )
        self.assertEqual(sorted(p.name for p in self.out.iterdir()), expected)
        for artifact, name in STABLE.items():
            self.assertEqual((self.out / name).read_bytes(), self.signed(artifact).read_bytes())
            self.assertEqual(
                (self.out / f"{name}.sig").read_text(),
                self.signed(artifact).with_name(f"{INSTALLERS[artifact]}.sig").read_text(),
            )
        self.assertIn("bundle: OK, 11 files", out)
        self.assertIn("signed by key 2CEED11FD5DD0F77", out)
        self.assertIn("the update manifest can be made from it", out)
        self.assertEqual(err, "")

    def test_sha256sums_covers_the_installers_and_clis_and_sha256sum_checks_it(self):
        self.assertEqual(self.run_main()[0], 0)
        lines = (self.out / "SHA256SUMS").read_text().splitlines()
        self.assertEqual([line.split("  ", 1)[1] for line in lines], [*STABLE.values(), *CLIS.values()])
        for line in lines:
            digest, name = line.split("  ", 1)
            self.assertEqual(hashlib.sha256((self.out / name).read_bytes()).hexdigest(), digest)
        if shutil.which("sha256sum"):
            checked = subprocess.run(["sha256sum", "-c", "SHA256SUMS"], cwd=self.out, capture_output=True)
            self.assertEqual(checked.returncode, 0, checked.stdout)

    def test_the_bundle_makes_the_update_manifest_the_publish_writes(self):
        self.assertEqual(self.run_main()[0], 0)
        manifest = make_latest_json.build_manifest("2.6.0", "Notes.", self.out, "2026-01-01T00:00:00Z")
        self.assertEqual(sorted(manifest["platforms"]), sorted(make_latest_json.PLATFORMS))

    def test_refuses_a_missing_sig(self):
        artifact = "gospel-getter-linux-deb"
        self.signed(artifact).with_name(f"{INSTALLERS[artifact]}.sig").unlink()
        self.assert_refused(f"{artifact}: missing from", f"{INSTALLERS[artifact]}.sig")

    def test_refuses_a_missing_artifact(self):
        shutil.rmtree(self.dir / "cli" / "gospel-getter-cli-windows-x86_64")
        shutil.rmtree(self.dir / "built" / "gospel-getter-windows-msi-installer")
        self.assert_refused(
            "gospel-getter-cli-windows-x86_64: no ",
            "gospel-getter-windows-msi-installer: no ",
        )

    def test_refuses_an_installer_that_isnt_its_recorded_sha256(self):
        artifact = "gospel-getter-windows-nsis-installer"
        sums = self.signed(artifact).with_name(sign_installers.SUMS)
        sums.write_text(f"{'0' * 64}  {INSTALLERS[artifact]}\n")
        self.assert_refused(f"{artifact}: {INSTALLERS[artifact]}: its INSTALLER-SHA256SUMS says")

    def test_refuses_a_signed_installer_that_isnt_the_one_built(self):
        artifact = "gospel-getter-linux-appimage"
        # Changed after it was built, and recorded as changed: the build
        # artifact the GUI checks ran on still tells.
        installer = self.signed(artifact)
        installer.write_bytes(b"something else")
        installer.with_name(sign_installers.SUMS).write_text(
            f"{hashlib.sha256(b'something else').hexdigest()}  {installer.name}\n"
        )
        self.assert_refused(f"{artifact}: {installer.name}: it isn't the installer as built")

    def test_refuses_a_build_artifact_holding_another_file(self):
        artifact = "gospel-getter-linux-deb"
        built = self.dir / "built" / artifact
        (built / INSTALLERS[artifact]).rename(built / "Gospel Getter_2.5.0_amd64.deb")
        self.assert_refused(f"{artifact}: {INSTALLERS[artifact]}: its build artifact holds")

    def test_refuses_a_signature_for_another_version_or_by_another_key(self):
        appimage, msi = "gospel-getter-linux-appimage", "gospel-getter-windows-msi-installer"
        for artifact, sig in [
            (appimage, signature(INSTALLERS[appimage], "2.5.0", APP_KEY)),
            (msi, signature(INSTALLERS[msi], "2.6.0", OTHER_KEY)),
        ]:
            self.signed(artifact).with_name(f"{INSTALLERS[artifact]}.sig").write_text(sig)
        self.assert_refused("was signed for version 2.5.0, not 2.6.0", "not the trusted key 2CEED11FD5DD0F77")

    def test_the_rehearsal_checks_against_its_own_public_key(self):
        for artifact, name in INSTALLERS.items():
            self.signed(artifact).with_name(f"{name}.sig").write_text(signature(name, "2.6.0", OTHER_KEY))
        self.assert_refused("not the trusted key 2CEED11FD5DD0F77")
        key = Path(self.tmp.name) / "rehearsal.key.pub"
        key.write_text(pubkey(OTHER_KEY))
        code, out, err = self.run_main("--pubkey-file", str(key))
        self.assertEqual(code, 0, err)
        self.assertIn("signed by key 0807060504030201", out)

    def test_refuses_anything_unexpected_beside_an_installer(self):
        artifact = "gospel-getter-windows-msi-installer"
        folder = self.dir / "signed" / artifact
        (folder / "notes.txt").write_text("?")
        (folder / "nested").mkdir()
        self.assert_refused(f"{artifact}: unexpected in", "nested, notes.txt")

    def test_refuses_two_installers_of_a_kind(self):
        artifact = "gospel-getter-linux-appimage"
        shutil.copyfile(self.signed(artifact), self.signed(artifact).with_name("Other_amd64.AppImage"))
        self.assert_refused(f"{artifact}: 2 linux-x86_64-appimage installers")

    def test_refuses_a_cli_artifact_holding_more(self):
        folder = self.dir / "cli" / "gospel-getter-cli-linux-x86_64"
        (folder / "extra").write_bytes(b"x")
        self.assert_refused("gospel-getter-cli-linux-x86_64: ", "not gospel-getter-cli-x86_64-unknown-linux-gnu alone")

    def test_every_problem_is_named_at_once(self):
        self.signed("gospel-getter-linux-deb").with_name(f"{INSTALLERS['gospel-getter-linux-deb']}.sig").unlink()
        shutil.rmtree(self.dir / "cli" / "gospel-getter-cli-linux-x86_64")
        err = self.assert_refused()
        self.assertEqual(err.count("bundle: error: "), 2, err)

    def test_the_bundle_is_made_afresh(self):
        self.out.mkdir()
        (self.out / "old").write_text("left over")
        code, _, err = self.run_main()
        self.assertEqual(code, 1)
        self.assertIn("isn't an empty folder", err)
        self.assertEqual(sorted(p.name for p in self.out.iterdir()), ["old"])

    def test_a_pre_release_is_bundled_but_has_no_manifest_to_try(self):
        self.write_conf("2.7.0-beta.1")
        for artifact, name in INSTALLERS.items():
            self.signed(artifact).with_name(f"{name}.sig").write_text(signature(name, "2.7.0-beta.1", APP_KEY))
        code, out, err = self.run_main()
        self.assertEqual(code, 0, err)
        self.assertIn("is a pre-release, which has no update manifest", out)

    def test_the_artifacts_are_the_ones_dist_yml_builds(self):
        self.assertEqual(sorted(b.INSTALLER_ARTIFACTS), sorted(INSTALLERS))
        self.assertEqual(b.CLI_ARTIFACTS, CLIS)
        self.assertEqual(sorted(b.INSTALLER_ARTIFACTS.values()), sorted(make_latest_json.PLATFORMS))
        dist = (Path(__file__).resolve().parents[2] / ".github" / "workflows" / "dist.yml").read_text()
        for artifact in [*b.INSTALLER_ARTIFACTS, *b.CLI_ARTIFACTS]:
            self.assertIn(f"          name: {artifact}\n", dist)
        for artifact, name in b.CLI_ARTIFACTS.items():
            self.assertIn(f"          path: src-tauri/binaries/{name}\n", dist)


if __name__ == "__main__":
    unittest.main()
