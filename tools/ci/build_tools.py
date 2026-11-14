#!/usr/bin/env python3
"""Fetch CI's build tools with the workflow's token, check each against its
pinned SHA-256, and put them where Trunk and the Tauri bundler look, so
neither downloads anything itself.

The tools are release assets on GitHub, which Trunk, its wasm-bindgen and
wasm-opt, and the Tauri bundler's AppImage, NSIS and WiX tools otherwise
download anonymously at build time; on 2026-11-09 those anonymous downloads
failed for hours while authenticated ones still worked. Each tool is a row of
tools/ci/build-tools.txt (which says where each version is set). Three sets:

    trunk    Trunk, wasm-bindgen and wasm-opt, into a bin folder put on PATH
             (Trunk uses a tool on PATH when its version is the one it wants)
    bundler  the tauri-bundler's tools for TAURI_CLI_VERSION, into its tools
             folder (~/.cache/tauri, %LOCALAPPDATA%\\tauri), where it looks
             before downloading
    signer   Tauri's own prebuilt cargo-tauri for TAURI_CLI_VERSION, into the
             same bin folder on PATH, for dist.yml's signing jobs: a pinned
             download in place of compiling tauri-cli in a job with no build
             cache (5-6 minutes, on every release)

.github/actions/build-tools runs it: `plan` names the cache (key and path),
actions/cache restores it, `install` checks every cached file against its
SHA-256, fetches what's missing or doesn't match with `gh release download`
(GH_TOKEN), and lays the tools out. A cache hit therefore needs no network,
and a file that doesn't match its SHA-256, cached or downloaded, is never
used. After the build, `check` fails if Trunk or the bundler fetched or
replaced any tool itself.

Usage (in CI; the paths come from RUNNER_OS, RUNNER_TEMP and the home folder):
    python3 tools/ci/build_tools.py plan trunk
    python3 tools/ci/build_tools.py install trunk
    python3 tools/ci/build_tools.py check trunk [bundler]

Every tool a `bin=` row lays out is run once with --version, which has to
print its pinned version, before anything uses it.
"""

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
import time
import zipfile
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
MANIFEST = ROOT / "tools" / "ci" / "build-tools.txt"
SETS = ("trunk", "bundler", "signer")
# The sets with a folder their user would download into, which `check` watches.
# Nothing downloads a signer: its user is the signing job itself.
WATCHED_SETS = ("trunk", "bundler")
# The signer set's one tool: the prebuilt Tauri CLI, run as `cargo tauri`.
SIGNER = "cargo-tauri"
OSES = ("Linux", "Windows")
INSTALL_KINDS = ("bin", "file", "unzip", "unzip-top")
SHA256 = re.compile(r"[0-9a-f]{64}")
EXACT_VERSION = re.compile(r"\d+\.\d+\.\d+")
# Written by the bundler from copies built into it, not downloaded.
BUNDLER_OWN_FILES = {"linuxdeploy-plugin-gtk.sh", "linuxdeploy-plugin-gstreamer.sh"}
# The bundler zeroes linuxdeploy's AppImage magic bytes in place, every run.
BUNDLER_CHANGES_IN_PLACE = re.compile(r"linuxdeploy-[0-9a-f]+-[a-z0-9_]+\.AppImage")
# The words each tool's --version starts with, for its version.
VERSION_OUTPUT = {
    "trunk": lambda v: ["trunk", v],
    "wasm-bindgen": lambda v: ["wasm-bindgen", v],
    "wasm-opt": lambda v: ["wasm-opt", "version", v.removeprefix("version_")],
    SIGNER: lambda v: ["tauri-cli", v],
}


class Refused(Exception):
    """Something the build can't go on with; the message says what to do."""


@dataclass(frozen=True)
class Tool:
    set: str
    os: str
    name: str
    version: str
    repository: str
    tag: str
    asset: str
    sha256: str
    install: str

    @property
    def kind(self):
        return self.install.split("=", 1)[0]

    @property
    def target(self):
        return self.install.split("=", 1)[1]


def read_manifest(text):
    """The rows of build-tools.txt, each checked for shape."""
    tools = []
    for number, line in enumerate(text.splitlines(), 1):
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        fields = line.split()
        if len(fields) != 9:
            raise Refused(f"build-tools.txt line {number}: {len(fields)} columns, not 9: {line}")
        tool = Tool(*fields)
        problems = []
        if tool.set not in SETS:
            problems.append(f"set {tool.set!r} isn't one of {SETS}")
        if tool.os not in OSES:
            problems.append(f"os {tool.os!r} isn't one of {OSES}")
        if not SHA256.fullmatch(tool.sha256):
            problems.append("the SHA-256 isn't 64 lowercase hex digits")
        if "=" not in tool.install or tool.kind not in INSTALL_KINDS or not tool.target:
            problems.append(f"install {tool.install!r} isn't one of {INSTALL_KINDS} with a target")
        elif Path(tool.target).is_absolute() or ".." in Path(tool.target).parts:
            problems.append(f"install target {tool.target!r} leaves its folder")
        if "/" in tool.asset or "\\" in tool.asset:
            problems.append(f"asset {tool.asset!r} isn't a file name")
        if problems:
            raise Refused(f"build-tools.txt line {number}: " + "; ".join(problems))
        tools.append(tool)
    return tools


def trunk_version(tools):
    """Trunk's version: the one every trunk row carries."""
    versions = {t.version for t in tools if t.name == "trunk"}
    if len(versions) != 1:
        raise Refused(f"build-tools.txt should pin one Trunk version, not {sorted(versions)}")
    return versions.pop()


def wasm_bindgen_version(cargo_lock):
    """The wasm-bindgen crate's version in ui-leptos/Cargo.lock: the CLI's has
    to be the same."""
    found = re.findall(r'^name = "wasm-bindgen"\nversion = "([^"]+)"$', cargo_lock, re.M)
    if len(found) != 1:
        raise Refused(f"ui-leptos/Cargo.lock should hold one wasm-bindgen, not {found}")
    return found[0]


def wasm_opt_version(trunk_toml):
    """wasm-opt's version, from Trunk.toml's [tools] table."""
    table = re.search(r"^\[tools\]$(.*?)(?=^\[|\Z)", trunk_toml, re.M | re.S)
    found = table and re.search(r'^wasm_opt\s*=\s*"([^"]+)"', table.group(1), re.M)
    if not found:
        raise Refused('Trunk.toml should pin wasm-opt: [tools] wasm_opt = "version_N"')
    return found.group(1)


def wanted(tools, tool_set, runner_os, *, cargo_lock, trunk_toml, tauri_cli):
    """The rows one set needs on one OS, in the manifest's order."""
    rows = [t for t in tools if t.set == tool_set and t.os == runner_os]
    if tool_set == "trunk":
        versions = {
            "trunk": (trunk_version(tools), "build-tools.txt"),
            "wasm-bindgen": (wasm_bindgen_version(cargo_lock), "ui-leptos/Cargo.lock"),
            "wasm-opt": (wasm_opt_version(trunk_toml), "Trunk.toml"),
        }
        picked = []
        for name, (version, source) in versions.items():
            match = [t for t in rows if t.name == name and t.version == version]
            if len(match) != 1:
                raise Refused(
                    f"{source} wants {name} {version}, and build-tools.txt has {len(match)} "
                    f"rows for it on {runner_os}, not 1: add its release asset and SHA-256"
                )
            picked += match
        return sorted(picked, key=rows.index)
    if not tauri_cli or not EXACT_VERSION.fullmatch(tauri_cli):
        raise Refused(f"TAURI_CLI_VERSION should be an exact version (X.Y.Z), not {tauri_cli!r}")
    picked = [t for t in rows if t.version == tauri_cli]
    if tool_set == "signer":
        if [t.name for t in picked] != [SIGNER] or picked[0].kind != "bin":
            raise Refused(
                f"build-tools.txt should have one signer row for tauri-cli {tauri_cli} on "
                f"{runner_os}, not {len(picked)}: add {SIGNER} {tauri_cli} from tauri-apps/tauri's "
                f"tauri-cli-v{tauri_cli} release (its {SIGNER}-<target> archive), with its SHA-256 "
                f"and install bin={SIGNER}"
            )
        return picked
    if not picked:
        raise Refused(
            f"build-tools.txt has no bundler tools for tauri-cli {tauri_cli} on {runner_os}: add "
            "the files its tauri-bundler downloads (linuxdeploy.rs, nsis/mod.rs, msi/mod.rs), "
            "each with its SHA-256"
        )
    return picked


def cache_key(tool_set, runner_os, rows):
    """Exact versions, and every row's SHA-256: a changed pin is a new cache."""
    if tool_set == "trunk":
        names = "-".join(f"{t.name}-{t.version}" for t in rows)
    else:
        names = f"tauri-cli-{rows[0].version}"
    digest = hashlib.sha256("\n".join(" ".join(vars(t).values()) for t in rows).encode())
    return f"build-tools-{tool_set}-{runner_os}-{names}-{digest.hexdigest()[:16]}"


def sha256_of(path):
    digest = hashlib.sha256()
    with open(path, "rb") as file:
        for block in iter(lambda: file.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def fetch(tool, cache_dir, download):
    """The tool's asset in cache_dir, matching its SHA-256: the cached one if
    it matches, else a fresh download. Returns how it came."""
    path = cache_dir / tool.asset
    if path.is_file():
        if sha256_of(path) == tool.sha256:
            return "from the cache"
        print(f"::warning::the cached {tool.asset} doesn't match its SHA-256; downloading it again")
        path.unlink()
    cache_dir.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=cache_dir) as temp:
        got = download(tool, Path(temp))
        actual = sha256_of(got)
        if actual != tool.sha256:
            raise Refused(
                f"{tool.repository} {tool.tag} {tool.asset} has SHA-256 {actual}, not the pinned "
                f"{tool.sha256}: it isn't used. If upstream rebuilt it, check the new one and "
                "update build-tools.txt."
            )
        os.replace(got, path)
    return "downloaded with the workflow token"


def gh_download(tool, into, attempts=3):
    """gh release download, authenticated by GH_TOKEN; a few tries."""
    if not os.environ.get("GH_TOKEN"):
        raise Refused("GH_TOKEN isn't set: the tools are downloaded with the workflow's token")
    command = ["gh", "release", "download", tool.tag, "--repo", tool.repository,
               "--pattern", tool.asset, "--dir", str(into)]
    for attempt in range(1, attempts + 1):
        result = subprocess.run(command, capture_output=True, text=True)
        got = into / tool.asset
        if result.returncode == 0 and got.is_file():
            return got
        print(f"{' '.join(command)}: try {attempt} failed: {result.stderr.strip()}", file=sys.stderr)
        if attempt < attempts:
            time.sleep(10 * attempt)
    raise Refused(f"couldn't download {tool.asset} from {tool.repository} {tool.tag}")


def archive_member(archive, member):
    """One member's bytes, from a .zip or a .tar.gz."""
    try:
        if archive.name.endswith(".zip"):
            with zipfile.ZipFile(archive) as zipped:
                return zipped.read(member)
        with tarfile.open(archive, "r:gz") as tarred:
            extracted = tarred.extractfile(member)
            if extracted is None:
                raise Refused(f"{member} in {archive.name} isn't a file")
            return extracted.read()
    except KeyError:
        raise Refused(f"{archive.name} has no {member}") from None


def unzip(archive, into):
    """Every member, refusing any that would land outside `into`."""
    into = into.resolve()
    with zipfile.ZipFile(archive) as zipped:
        for name in zipped.namelist():
            if not (into / name).resolve().is_relative_to(into):
                raise Refused(f"{archive.name} has a member outside its folder: {name}")
        zipped.extractall(into)


def lay_out(tool, archive, bin_dir, tools_dir):
    """Put one tool where its user looks."""
    if tool.kind == "bin":
        bin_dir.mkdir(parents=True, exist_ok=True)
        out = bin_dir / Path(tool.target).name
        out.write_bytes(archive_member(archive, tool.target))
        out.chmod(0o755)
        return out
    tools_dir.mkdir(parents=True, exist_ok=True)
    out = tools_dir / tool.target
    if tool.kind == "file":
        out.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(archive, out)
        out.chmod(0o755)
        return out
    if out.exists():
        shutil.rmtree(out)
    if tool.kind == "unzip":
        out.mkdir(parents=True)
        unzip(archive, out)
        return out
    # unzip-top: the zip's one top-level folder becomes `out`.
    with tempfile.TemporaryDirectory(dir=tools_dir) as temp:
        unzip(archive, Path(temp))
        top = list(Path(temp).iterdir())
        if len(top) != 1 or not top[0].is_dir():
            raise Refused(f"{archive.name} should hold one top-level folder, not {top}")
        out.parent.mkdir(parents=True, exist_ok=True)
        os.replace(top[0], out)
    return out


def snapshot(folder):
    """Every file under folder: its size and modification time."""
    if not folder.is_dir():
        return {}
    return {
        path.relative_to(folder).as_posix(): [path.stat().st_size, path.stat().st_mtime_ns]
        for path in sorted(folder.rglob("*"))
        if path.is_file()
    }


def changes(before, after, tool_set):
    """Files a build added or replaced in a tools folder, minus what the
    bundler writes or changes itself."""
    changed = []
    for name, state in after.items():
        if before.get(name) == state:
            continue
        base = name.rsplit("/", 1)[-1]
        if tool_set == "bundler" and (
            base in BUNDLER_OWN_FILES
            or (name in before and BUNDLER_CHANGES_IN_PLACE.fullmatch(base))
        ):
            continue
        changed.append(name)
    return changed


class Runner:
    """Where things go on this runner."""

    def __init__(self, env=os.environ):
        self.env = env
        self.os = env.get("RUNNER_OS", "")
        if self.os not in OSES:
            raise Refused(f"RUNNER_OS should be one of {OSES}, not {self.os!r}")
        temp = Path(env.get("RUNNER_TEMP") or tempfile.gettempdir())
        self.state = temp / "build-tools"
        self.bin = self.state / "bin"
        if self.os == "Windows":
            local = Path(env["LOCALAPPDATA"])
            self.tauri = local / "tauri"
            self.trunk_cache = local / "trunkrs" / "trunk" / "cache"
        else:
            cache = env.get("XDG_CACHE_HOME") or str(Path.home() / ".cache")
            self.tauri = Path(cache) / "tauri"
            self.trunk_cache = Path(cache) / "trunk"
        self.cache_root = temp / "build-tools-cache"

    def cache_dir(self, tool_set):
        return self.cache_root / tool_set

    def watched(self, tool_set):
        """The folder a set's user would download into."""
        return self.trunk_cache if tool_set == "trunk" else self.tauri

    def snapshot_file(self, tool_set):
        return self.state / f"{tool_set}.files.json"

    def output(self, name, value):
        with open(self.env["GITHUB_OUTPUT"], "a", encoding="utf-8") as out:
            out.write(f"{name}={value}\n")

    def append(self, variable, line):
        if self.env.get(variable):
            with open(self.env[variable], "a", encoding="utf-8") as out:
                out.write(f"{line}\n")


def rows_for(tool_set, runner, env=os.environ):
    return wanted(
        read_manifest(MANIFEST.read_text(encoding="utf-8")),
        tool_set,
        runner.os,
        cargo_lock=(ROOT / "ui-leptos" / "Cargo.lock").read_text(encoding="utf-8"),
        trunk_toml=(ROOT / "Trunk.toml").read_text(encoding="utf-8"),
        tauri_cli=env.get("TAURI_CLI_VERSION", ""),
    )


def plan(tool_set, runner, rows):
    key = cache_key(tool_set, runner.os, rows)
    for tool in rows:
        print(f"{tool.name} {tool.version}: {tool.repository} {tool.tag} {tool.asset}")
    print(f"cache key: {key}")
    runner.output("key", key)
    runner.output("path", str(runner.cache_dir(tool_set)))


def install(tool_set, runner, rows, download=gh_download):
    cache_dir = runner.cache_dir(tool_set)
    came = {"from the cache": 0, "downloaded with the workflow token": 0}
    for tool in rows:
        how = fetch(tool, cache_dir, download)
        came[how] += 1
        where = lay_out(tool, cache_dir / tool.asset, runner.bin, runner.tauri)
        print(f"{tool.name} {tool.version}: {how}, SHA-256 matches; at {where}")
    runner.state.mkdir(parents=True, exist_ok=True)
    if tool_set in ("trunk", "signer"):
        for tool in rows:
            program = runner.bin / Path(tool.target).name
            printed = subprocess.run([str(program), "--version"], capture_output=True, text=True)
            expected = VERSION_OUTPUT[tool.name](tool.version)
            if printed.returncode != 0 or printed.stdout.split()[: len(expected)] != expected:
                raise Refused(
                    f"{program} --version printed {printed.stdout!r}, not {' '.join(expected)}"
                )
            print(printed.stdout.strip())
        runner.append("GITHUB_PATH", str(runner.bin))
    if tool_set == "trunk":
        # Trunk asks crates.io for a newer Trunk otherwise.
        runner.append("GITHUB_ENV", "TRUNK_SKIP_VERSION_CHECK=true")
    if tool_set in WATCHED_SETS:
        runner.snapshot_file(tool_set).write_text(
            json.dumps(snapshot(runner.watched(tool_set))), encoding="utf-8"
        )
    summary = ", ".join(f"{count} {how}" for how, count in came.items())
    print(f"::notice title=Build tools ({tool_set}, {runner.os})::{summary}")


def check(tool_sets, runner):
    failed = False
    for tool_set in tool_sets:
        if tool_set not in WATCHED_SETS:
            raise Refused(f"nothing downloads {tool_set} tools itself, so there's nothing to check")
        try:
            before = json.loads(runner.snapshot_file(tool_set).read_text(encoding="utf-8"))
        except FileNotFoundError:
            raise Refused(f"`install {tool_set}` didn't run in this job") from None
        changed = changes(before, snapshot(runner.watched(tool_set)), tool_set)
        if changed:
            failed = True
            print(
                f"::error::the build fetched or replaced {tool_set} tools itself, in "
                f"{runner.watched(tool_set)}: {', '.join(changed)}. Pin them in "
                "tools/ci/build-tools.txt."
            )
        else:
            print(f"{tool_set}: the build fetched no tools itself")
    if failed:
        raise Refused("the build downloaded tools of its own")


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("command", choices=["plan", "install", "check"])
    parser.add_argument("sets", nargs="+", choices=SETS)
    args = parser.parse_args(argv)
    try:
        runner = Runner()
        if args.command == "check":
            check(args.sets, runner)
        else:
            if len(args.sets) != 1:
                parser.error(f"{args.command} takes one set")
            tool_set = args.sets[0]
            rows = rows_for(tool_set, runner, runner.env)
            (plan if args.command == "plan" else install)(tool_set, runner, rows)
    except Refused as refused:
        print(f"::error::{refused}")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
