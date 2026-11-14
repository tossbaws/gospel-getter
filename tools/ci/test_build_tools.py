"""Tests for build_tools.py and the pins in build-tools.txt. Run from the
repository root:
    python3 -m unittest discover -s tools/ci
"""

import contextlib
import hashlib
import io
import os
import re
import shutil
import stat
import tarfile
import tempfile
import unittest
import unittest.mock
import zipfile
from pathlib import Path

import build_tools as b

ROOT = Path(__file__).resolve().parents[2]


def read(path):
    return (ROOT / path).read_text(encoding="utf-8")


def tauri_cli_version():
    """TAURI_CLI_VERSION in dist.yml, where it's set."""
    found = re.findall(r'^  TAURI_CLI_VERSION: "([^"]+)"$', read(".github/workflows/dist.yml"), re.M)
    assert len(found) == 1, found
    return found[0]


def repo_wanted(tool_set, runner_os):
    return b.wanted(
        b.read_manifest(read("tools/ci/build-tools.txt")),
        tool_set,
        runner_os,
        cargo_lock=read("ui-leptos/Cargo.lock"),
        trunk_toml=read("Trunk.toml"),
        tauri_cli=tauri_cli_version(),
    )


def row(**changes):
    fields = dict(
        set="trunk", os="Linux", name="trunk", version="1.0.0", repository="o/r", tag="v1",
        asset="trunk.tar.gz", sha256="0" * 64, install="bin=trunk",
    )
    fields.update(changes)
    return b.Tool(**fields)


def pinned(tool, data):
    """The tool, pinned to `data`'s SHA-256."""
    return b.Tool(**{**vars(tool), "sha256": hashlib.sha256(data).hexdigest()})


def tar_gz(members):
    buffer = io.BytesIO()
    with tarfile.open(fileobj=buffer, mode="w:gz") as tarred:
        for name, data in members.items():
            info = tarfile.TarInfo(name)
            info.size = len(data)
            tarred.addfile(info, io.BytesIO(data))
    return buffer.getvalue()


def zip_of(members):
    buffer = io.BytesIO()
    with zipfile.ZipFile(buffer, "w") as zipped:
        for name, data in members.items():
            zipped.writestr(name, data)
    return buffer.getvalue()


class Downloads:
    """Stands in for gh release download: serves bytes by asset name, and
    records each download."""

    def __init__(self, assets):
        self.assets = assets
        self.asked = []

    def __call__(self, tool, into):
        self.asked.append(tool.asset)
        path = into / tool.asset
        path.write_bytes(self.assets[tool.asset])
        return path


class TheRepositorysPins(unittest.TestCase):
    """What CI will ask for exists in build-tools.txt, and the other places
    that name the same tools agree with it."""

    def test_every_set_resolves_on_every_runner_ci_uses(self):
        for tool_set, runner_os, names in [
            ("trunk", "Linux", ["trunk", "wasm-bindgen", "wasm-opt"]),
            ("trunk", "Windows", ["trunk", "wasm-bindgen", "wasm-opt"]),
            ("bundler", "Linux", ["apprun", "linuxdeploy", "linuxdeploy-plugin-appimage"]),
            ("bundler", "Windows", ["nsis", "nsis-tauri-utils", "wix"]),
            ("signer", "Linux", ["cargo-tauri"]),
        ]:
            with self.subTest(tool_set=tool_set, runner_os=runner_os):
                self.assertEqual([t.name for t in repo_wanted(tool_set, runner_os)], names)

    def test_versions_come_from_their_one_place(self):
        tools = {t.name: t for t in repo_wanted("trunk", "Linux")}
        lock = re.search(
            r'^name = "wasm-bindgen"\nversion = "([^"]+)"$', read("ui-leptos/Cargo.lock"), re.M
        )
        self.assertEqual(tools["wasm-bindgen"].version, lock.group(1))
        self.assertIn(f'wasm_opt = "{tools["wasm-opt"].version}"', read("Trunk.toml"))
        for tool in repo_wanted("bundler", "Linux") + repo_wanted("bundler", "Windows"):
            self.assertEqual(tool.version, tauri_cli_version())
        [signer] = repo_wanted("signer", "Linux")
        self.assertEqual(signer.version, tauri_cli_version())

    def test_the_signer_is_tauri_apps_own_release_of_that_version(self):
        # The prebuilt binary is trusted because it's tauri-apps' own release
        # asset for exactly TAURI_CLI_VERSION, pinned by its SHA-256.
        [signer] = repo_wanted("signer", "Linux")
        version = tauri_cli_version()
        self.assertEqual(
            (signer.name, signer.repository, signer.tag, signer.asset, signer.install),
            ("cargo-tauri", "tauri-apps/tauri", f"tauri-cli-v{version}",
             "cargo-tauri-x86_64-unknown-linux-gnu.tgz", "bin=cargo-tauri"),
        )
        signers = [t for t in b.read_manifest(read("tools/ci/build-tools.txt")) if t.set == "signer"]
        self.assertEqual(signers, [signer], "one signer, and only for TAURI_CLI_VERSION")

    def test_tauri_cli_is_installed_at_exactly_that_version(self):
        dist = read(".github/workflows/dist.yml")
        self.assertRegex(tauri_cli_version(), r"^\d+\.\d+\.\d+$")
        self.assertIn('cargo install tauri-cli --version "${{ env.TAURI_CLI_VERSION }}"', dist)

    def test_the_workflows_no_longer_download_trunk_anonymously(self):
        for workflow in ["ci.yml", "dist.yml"]:
            text = read(f".github/workflows/{workflow}")
            self.assertFalse("jetli/trunk-action" in text, f"{workflow} uses jetli/trunk-action")
            self.assertFalse("0.21.14" in text, f"{workflow} pins Trunk itself")

    def test_the_pkgbuild_builds_with_the_same_tools(self):
        pkgbuild = read("packaging/PKGBUILD")
        tools = {t.name: t for t in repo_wanted("trunk", "Linux")}

        def variable(name):
            found = re.findall(rf"^{name}=(\S+)$", pkgbuild, re.M)
            self.assertEqual(len(found), 1, name)
            return found[0]

        self.assertEqual(variable("_trunk"), tools["trunk"].version)
        self.assertEqual(variable("_wasm_bindgen"), tools["wasm-bindgen"].version)
        self.assertEqual(f"version_{variable('_binaryen')}", tools["wasm-opt"].version)
        x86_64 = re.search(r"^sha256sums_x86_64=\(([^)]*)\)", pkgbuild, re.M | re.S).group(1)
        self.assertEqual(
            re.findall(r"'([0-9a-f]{64})'", x86_64),
            [tools[name].sha256 for name in ["trunk", "wasm-bindgen", "wasm-opt"]],
        )


class Manifest(unittest.TestCase):
    def test_a_malformed_row_is_refused_with_its_line(self):
        good = " ".join(vars(row()).values())
        for bad in [
            good.replace("0" * 64, "abc"),
            good.replace("bin=trunk", "bin=../../trunk"),
            good.replace("bin=trunk", "copy=trunk"),
            good.replace("Linux", "macOS"),
            good + " extra",
        ]:
            with self.subTest(bad=bad), self.assertRaisesRegex(b.Refused, "line 2"):
                b.read_manifest(f"# comment\n{bad}\n")

    def test_a_version_without_rows_says_what_to_add(self):
        tools = b.read_manifest(read("tools/ci/build-tools.txt"))
        lock = read("ui-leptos/Cargo.lock").replace(
            'name = "wasm-bindgen"\nversion = "', 'name = "wasm-bindgen"\nversion = "9.'
        )
        with self.assertRaisesRegex(b.Refused, r"Cargo.lock wants wasm-bindgen 9\..*SHA-256"):
            b.wanted(tools, "trunk", "Linux", cargo_lock=lock, trunk_toml=read("Trunk.toml"),
                     tauri_cli="")
        with self.assertRaisesRegex(b.Refused, "no bundler tools for tauri-cli 9.9.9"):
            b.wanted(tools, "bundler", "Linux", cargo_lock="", trunk_toml="", tauri_cli="9.9.9")
        with self.assertRaisesRegex(b.Refused, "exact version"):
            b.wanted(tools, "bundler", "Linux", cargo_lock="", trunk_toml="", tauri_cli="^2.0.0")
        with self.assertRaisesRegex(
            b.Refused, r"one signer row for tauri-cli 9\.9\.9 on Linux, not 0: add cargo-tauri "
            r"9\.9\.9 from tauri-apps/tauri's tauri-cli-v9\.9\.9 release.*SHA-256",
        ):
            b.wanted(tools, "signer", "Linux", cargo_lock="", trunk_toml="", tauri_cli="9.9.9")
        with self.assertRaisesRegex(b.Refused, "exact version"):
            b.wanted(tools, "signer", "Linux", cargo_lock="", trunk_toml="", tauri_cli="")
        with self.assertRaisesRegex(b.Refused, "on Windows, not 0"):
            b.wanted(tools, "signer", "Windows", cargo_lock="", trunk_toml="",
                     tauri_cli=tauri_cli_version())

    def test_the_signer_set_is_its_one_cargo_tauri_on_path(self):
        good = row(set="signer", name="cargo-tauri", version="2.0.0", install="bin=cargo-tauri")
        for bad in [
            [good, b.Tool(**{**vars(good), "asset": "other.tgz"})],
            [b.Tool(**{**vars(good), "name": "tauri"})],
            [b.Tool(**{**vars(good), "install": "file=cargo-tauri"})],
        ]:
            with self.subTest(bad=bad), self.assertRaisesRegex(b.Refused, "one signer row"):
                b.wanted(bad, "signer", "Linux", cargo_lock="", trunk_toml="", tauri_cli="2.0.0")
        self.assertEqual(
            b.wanted([good], "signer", "Linux", cargo_lock="", trunk_toml="", tauri_cli="2.0.0"),
            [good],
        )
        self.assertIn("tauri-cli-2.0.0", b.cache_key("signer", "Linux", [good]))
        self.assertTrue(b.cache_key("signer", "Linux", [good]).startswith("build-tools-signer-"))

    def test_wasm_opt_must_be_pinned_in_trunk_toml(self):
        with self.assertRaisesRegex(b.Refused, r"\[tools\] wasm_opt"):
            b.wasm_opt_version('[build]\nwasm_opt = "version_1"\n')
        self.assertEqual(b.wasm_opt_version('[tools]\nwasm_opt = "version_9"\n'), "version_9")

    def test_the_cache_key_changes_with_any_version_or_checksum(self):
        rows = repo_wanted("trunk", "Linux")
        key = b.cache_key("trunk", "Linux", rows)
        self.assertIn("trunk-0.21.14", key)
        for changed in [
            [b.Tool(**{**vars(rows[0]), "version": "0.21.15"})] + rows[1:],
            [b.Tool(**{**vars(rows[0]), "sha256": "1" * 64})] + rows[1:],
        ]:
            self.assertNotEqual(b.cache_key("trunk", "Linux", changed), key)
        self.assertNotEqual(b.cache_key("trunk", "Windows", rows), key)


class Fetch(unittest.TestCase):
    def setUp(self):
        self.dir = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.dir)
        self.data = b"the real tool"
        self.tool = pinned(row(), self.data)

    def test_a_cache_hit_downloads_nothing(self):
        (self.dir / self.tool.asset).write_bytes(self.data)
        downloads = Downloads({})
        self.assertEqual(b.fetch(self.tool, self.dir, downloads), "from the cache")
        self.assertEqual(downloads.asked, [])

    def test_a_miss_is_downloaded_and_kept(self):
        downloads = Downloads({self.tool.asset: self.data})
        self.assertEqual(b.fetch(self.tool, self.dir, downloads), "downloaded with the workflow token")
        self.assertEqual((self.dir / self.tool.asset).read_bytes(), self.data)

    def test_a_cached_file_that_doesnt_match_is_never_used(self):
        (self.dir / self.tool.asset).write_bytes(b"poisoned")
        downloads = Downloads({self.tool.asset: self.data})
        with contextlib.redirect_stdout(io.StringIO()):
            b.fetch(self.tool, self.dir, downloads)
        self.assertEqual(downloads.asked, [self.tool.asset])
        self.assertEqual((self.dir / self.tool.asset).read_bytes(), self.data)

    def test_a_download_that_doesnt_match_is_refused_and_not_kept(self):
        downloads = Downloads({self.tool.asset: b"something else"})
        with self.assertRaisesRegex(b.Refused, "isn't used"):
            b.fetch(self.tool, self.dir, downloads)
        self.assertEqual(sorted(self.dir.iterdir()), [])

    def test_without_a_token_nothing_is_downloaded(self):
        with unittest.mock.patch.dict(os.environ, {"GH_TOKEN": ""}):
            with self.assertRaisesRegex(b.Refused, "GH_TOKEN"):
                b.gh_download(self.tool, self.dir)


class LayOut(unittest.TestCase):
    def setUp(self):
        self.dir = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.dir)
        self.bin = self.dir / "bin"
        self.tools = self.dir / "tauri"

    def archive(self, name, data):
        path = self.dir / name
        path.write_bytes(data)
        return path

    def test_a_member_of_a_tar_gz_or_a_zip_goes_into_bin(self):
        tgz = self.archive("w.tar.gz", tar_gz({"w-1/wasm-bindgen": b"tar", "w-1/other": b"x"}))
        out = b.lay_out(row(install="bin=w-1/wasm-bindgen"), tgz, self.bin, self.tools)
        self.assertEqual((out, out.read_bytes()), (self.bin / "wasm-bindgen", b"tar"))
        self.assertTrue(out.stat().st_mode & stat.S_IXUSR)
        zipped = self.archive("t.zip", zip_of({"trunk.exe": b"zip"}))
        out = b.lay_out(row(install="bin=trunk.exe"), zipped, self.bin, self.tools)
        self.assertEqual(out.read_bytes(), b"zip")
        with self.assertRaisesRegex(b.Refused, "has no nothing"):
            b.lay_out(row(install="bin=nothing"), zipped, self.bin, self.tools)

    def test_the_bundlers_files_go_where_it_looks(self):
        nsis = self.archive("nsis-3.13.zip", zip_of({"nsis-3.13/NSIS.exe": b"n", "nsis-3.13/Include/x.nsh": b"i"}))
        b.lay_out(row(install="unzip-top=NSIS"), nsis, self.bin, self.tools)
        dll = self.archive("nsis_tauri_utils.dll", b"dll")
        b.lay_out(row(install="file=NSIS/Plugins/x86-unicode/additional/nsis_tauri_utils.dll"), dll,
                  self.bin, self.tools)
        wix = self.archive("wix.zip", zip_of({"candle.exe": b"c", "x86/burn.exe": b"b"}))
        b.lay_out(row(install="unzip=WixTools314"), wix, self.bin, self.tools)
        self.assertEqual(
            sorted(b.snapshot(self.tools)),
            ["NSIS/Include/x.nsh", "NSIS/NSIS.exe", "NSIS/Plugins/x86-unicode/additional/nsis_tauri_utils.dll",
             "WixTools314/candle.exe", "WixTools314/x86/burn.exe"],
        )

    def test_a_zip_reaching_outside_its_folder_is_refused(self):
        bad = self.archive("bad.zip", zip_of({"../escape": b"x"}))
        with self.assertRaisesRegex(b.Refused, "outside"):
            b.lay_out(row(install="unzip=WixTools314"), bad, self.bin, self.tools)
        self.assertFalse((self.tools / "escape").exists())


class Changes(unittest.TestCase):
    before = {"AppRun-x86_64": [1, 1], "linuxdeploy-07333c6-x86_64.AppImage": [2, 2]}

    def test_a_new_or_replaced_tool_is_named(self):
        after = {**self.before, "AppRun-x86_64": [1, 9], "NSIS/new.dll": [1, 1]}
        self.assertEqual(b.changes(self.before, after, "bundler"), ["AppRun-x86_64", "NSIS/new.dll"])
        self.assertEqual(b.changes({}, {"wasm-bindgen-0.2.129/wasm-bindgen": [1, 1]}, "trunk"),
                         ["wasm-bindgen-0.2.129/wasm-bindgen"])

    def test_what_the_bundler_writes_or_changes_itself_is_not(self):
        after = {**self.before, "linuxdeploy-07333c6-x86_64.AppImage": [2, 9],
                 "linuxdeploy-plugin-gtk.sh": [3, 3], "linuxdeploy-plugin-gstreamer.sh": [4, 4]}
        self.assertEqual(b.changes(self.before, after, "bundler"), [])
        # A linuxdeploy it fetched itself is still named.
        self.assertEqual(b.changes({}, {"linuxdeploy-abc1234-x86_64.AppImage": [1, 1]}, "bundler"),
                         ["linuxdeploy-abc1234-x86_64.AppImage"])


class InstallAndCheck(unittest.TestCase):
    """The whole of a Linux job's use, with stand-in tools and downloads."""

    def setUp(self):
        self.dir = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.dir)
        for name in ["output", "path", "env"]:
            (self.dir / name).touch()
        self.env = {
            "RUNNER_OS": "Linux", "RUNNER_TEMP": str(self.dir / "temp"),
            "XDG_CACHE_HOME": str(self.dir / "cache"), "GITHUB_OUTPUT": str(self.dir / "output"),
            "GITHUB_PATH": str(self.dir / "path"), "GITHUB_ENV": str(self.dir / "env"),
        }
        self.runner = b.Runner(self.env)

    def script(self, words):
        return f"#!/bin/sh\necho '{words}'\n".encode()

    def trunk_rows(self):
        archives = {
            "trunk.tar.gz": tar_gz({"trunk": self.script("trunk 0.21.14")}),
            "wb.tar.gz": tar_gz({"wb/wasm-bindgen": self.script("wasm-bindgen 0.2.129")}),
            "bin.tar.gz": tar_gz({"b/bin/wasm-opt": self.script("wasm-opt version 123 (version_123)")}),
        }
        rows = [
            pinned(row(asset="trunk.tar.gz", version="0.21.14"), archives["trunk.tar.gz"]),
            pinned(row(name="wasm-bindgen", asset="wb.tar.gz", version="0.2.129",
                       install="bin=wb/wasm-bindgen"), archives["wb.tar.gz"]),
            pinned(row(name="wasm-opt", asset="bin.tar.gz", version="version_123",
                       install="bin=b/bin/wasm-opt"), archives["bin.tar.gz"]),
        ]
        return rows, archives

    def quietly(self, call, *args):
        with contextlib.redirect_stdout(io.StringIO()) as out:
            call(*args)
        return out.getvalue()

    def test_a_first_run_downloads_and_a_second_uses_only_the_cache(self):
        rows, archives = self.trunk_rows()
        first = Downloads(archives)
        out = self.quietly(b.install, "trunk", self.runner, rows, first)
        self.assertEqual(first.asked, ["trunk.tar.gz", "wb.tar.gz", "bin.tar.gz"])
        self.assertIn("0 from the cache, 3 downloaded with the workflow token", out)
        self.assertEqual((self.dir / "path").read_text(), f"{self.runner.bin}\n")
        self.assertIn("TRUNK_SKIP_VERSION_CHECK=true", (self.dir / "env").read_text())

        second = Downloads({})
        out = self.quietly(b.install, "trunk", self.runner, rows, second)
        self.assertEqual(second.asked, [])
        self.assertIn("3 from the cache, 0 downloaded", out)
        self.assertIn("trunk: the build fetched no tools itself",
                      self.quietly(b.check, ["trunk"], self.runner))

    def test_a_tool_of_the_wrong_version_is_refused(self):
        rows, archives = self.trunk_rows()
        archives["trunk.tar.gz"] = tar_gz({"trunk": self.script("trunk 0.22.0")})
        rows[0] = pinned(rows[0], archives["trunk.tar.gz"])
        with self.assertRaisesRegex(b.Refused, "not trunk 0.21.14"):
            self.quietly(b.install, "trunk", self.runner, rows, Downloads(archives))

    def test_check_fails_when_trunk_downloaded_a_tool_itself(self):
        rows, archives = self.trunk_rows()
        self.quietly(b.install, "trunk", self.runner, rows, Downloads(archives))
        fetched = self.runner.trunk_cache / "wasm-opt-version_123" / "bin" / "wasm-opt"
        fetched.parent.mkdir(parents=True)
        fetched.write_bytes(b"downloaded")
        with contextlib.redirect_stdout(io.StringIO()) as out:
            with self.assertRaisesRegex(b.Refused, "downloaded tools of its own"):
                b.check(["trunk"], self.runner)
        self.assertIn("wasm-opt-version_123/bin/wasm-opt", out.getvalue())

    def signer_rows(self, printed="tauri-cli 2.12.4"):
        archive = tar_gz({"cargo-tauri": self.script(printed), "README.md": b"readme"})
        rows = [pinned(row(set="signer", name="cargo-tauri", version="2.12.4",
                           asset="cargo-tauri.tgz", install="bin=cargo-tauri"), archive)]
        return rows, {"cargo-tauri.tgz": archive}

    def test_the_signer_goes_on_path_after_its_version_is_checked(self):
        rows, archives = self.signer_rows()
        out = self.quietly(b.install, "signer", self.runner, rows, Downloads(archives))
        self.assertIn("tauri-cli 2.12.4", out)
        self.assertEqual((self.runner.bin / "cargo-tauri").read_bytes(), self.script("tauri-cli 2.12.4"))
        self.assertEqual((self.dir / "path").read_text(), f"{self.runner.bin}\n")
        self.assertEqual((self.dir / "env").read_text(), "")
        out = self.quietly(b.install, "signer", self.runner, rows, Downloads({}))
        self.assertIn("1 from the cache, 0 downloaded", out)
        # Nothing downloads a signer itself: there's no folder to watch.
        with self.assertRaisesRegex(b.Refused, "nothing downloads signer tools itself"):
            b.check(["signer"], self.runner)

    def test_a_signer_of_the_wrong_version_is_refused_before_it_is_on_path(self):
        rows, archives = self.signer_rows(printed="tauri-cli 2.12.5")
        with self.assertRaisesRegex(b.Refused, "not tauri-cli 2.12.4"):
            self.quietly(b.install, "signer", self.runner, rows, Downloads(archives))
        self.assertEqual((self.dir / "path").read_text(), "")

    def test_plan_names_the_cache(self):
        rows, _ = self.trunk_rows()
        self.quietly(b.plan, "trunk", self.runner, rows)
        output = (self.dir / "output").read_text().splitlines()
        self.assertEqual(output[0], f"key={b.cache_key('trunk', 'Linux', rows)}")
        self.assertEqual(output[1], f"path={self.dir / 'temp' / 'build-tools-cache' / 'trunk'}")


if __name__ == "__main__":
    unittest.main()
