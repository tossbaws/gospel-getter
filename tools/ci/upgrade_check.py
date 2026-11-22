#!/usr/bin/env python3
"""Upgrade an install of one release to another, the way a reader does, and
check the upgraded app starts and keeps the reader's data.

Every release build runs this from the previous release to itself, because a
build that passes every check on a fresh profile can still fail to start on
an upgraded one (2.33.0 couldn't start on any Windows PC upgraded from
2.32.x).

    previous  the release GitHub marks Latest: its installer for this
              platform, downloaded with `gh release download` (GH_TOKEN) and
              checked against that release's SHA256SUMS
    run       install --from, start it once (it makes its database), close
              it, seed a bookmark, a highlight, a note and a reading place
              into that database, install --to over it, start that, and
              assert, one line each:
                5a  it started: alive after the start-up window, no
                    "couldn't start" in its log, no start-failed dialog,
                    its main window open
                5b  it's --expect-version (the version the running program
                    and its install report)
                5c  the database is at --expect-schema (PRAGMA user_version),
                    with each <db>.before-schema-N copy a step that ran
                    promises
                5d  every seeded item is still there, read back with the
                    --to install's own gospel-getter-cli (read-only)
    back      the other way, as About's Go back does: install --from (this
              build) on no data, start it once, seed the same items,
              install --to (the previous release) over it, start that, and
              assert 5a, 5b (it's --expect-version, the previous release's),
              5c (the database is still at this build's schema: an older
              app keeps a newer schema as it is) and 5d (read back with the
              previous release's own gospel-getter-cli). On Windows this is
              the older NSIS installer run with /S over the newer, exactly
              what Go back runs. The data folders an earlier `run` left are
              deleted first.

The installers are any two: Windows takes NSIS setup .exes (installed with
/S, the second over the first), Linux AppImages (each extracted and started
with its AppRun, on the same data folder). Defaults come from this checkout:
the version from src-tauri/tauri.conf.json, the schema from the migrate
module. Usage, from dist.yml's upgrade jobs:
    python3 tools/ci/upgrade_check.py previous --platform linux --out DIR
    python3 tools/ci/upgrade_check.py run --from OLD --to NEW --work DIR
    python3 tools/ci/upgrade_check.py back --from NEW --to OLD --work DIR2 \
        --expect-version OLD_VERSION

It installs over the account's own app and writes into the app's data
folder, so it refuses to run unless GOSPEL_GETTER_UPGRADE_CHECK=throwaway-runner
is set (dist.yml sets it), and unless that data folder doesn't exist yet. On
Linux the apps run with HOME and XDG_* in --work, on a private Xvfb this
starts, so nothing reaches a desktop; Windows resolves the data folders from
the account, so it runs on a throwaway runner only.
"""

import argparse
import hashlib
import json
import os
import re
import shutil
import signal
import sqlite3
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
TAURI_CONF = ROOT / "src-tauri" / "tauri.conf.json"
MIGRATE_RS = ROOT / "src-tauri" / "core" / "src" / "db" / "migrate.rs"
MIGRATE_DIR = ROOT / "src-tauri" / "core" / "src" / "db" / "migrate"
REPOSITORY = "tossbaws/gospel-getter"

SWITCH = "GOSPEL_GETTER_UPGRADE_CHECK"
SWITCH_VALUE = "throwaway-runner"

# The release assets `previous` downloads, by platform.
ASSETS = {"windows": "Gospel-Getter_x64-setup.exe", "linux": "Gospel-Getter_amd64.AppImage"}

MAIN_TITLE = "Gospel Getter"
FAILED_TITLE = "Gospel Getter couldn't start"
FAILED_TEXT = "couldn't start"
DATABASE_FILE = "gospel_getter.db"
LOG_FILE = "gospel-getter.log"

# What's seeded, by the columns schemas 1 and 2 share (schema_1.sql, and
# schema_2.rs's rebuilds, which keep every column): nothing either refuses.
NOTE_BODY = "Seeded by tools/ci/upgrade_check.py before the upgrade."
SEED = {
    "bookmark": {"book_id": 45, "chapter": 8, "verse_start": 28, "verse_end": 28},
    "highlight": {"book_id": 43, "chapter": 3, "verse": 16, "color": "green"},
    "note": {"book_id": 19, "chapter": 23, "verse_start": 1, "verse_end": 2, "body": NOTE_BODY},
    "reading place": {"translation": "kjv", "book_id": 42, "chapter": 2, "verse": 10},
}
# How gospel-getter-cli reads each seeded item back: its command, the list
# in its answer (None: the answer is the item), and the item's fields.
READ_BACK = {
    "bookmark": ("list_bookmarks", "bookmarks", {"reference": "Romans 8:28"}),
    "highlight": ("list_highlights", "highlights", {"reference": "John 3:16", "color": "green"}),
    "note": ("list_notes", "notes", {"reference": "Psalms 23:1\u20132", "body": NOTE_BODY}),
    "reading place": ("get_reading_place", None, {"reference": "Luke 2:10", "translation": "kjv"}),
}

ANSI = re.compile(r"\x1b\[[0-9;]*[A-Za-z]")


def is_main(title):
    """The app's main window: "Gospel Getter", or, before 2.33.0, the
    passage then " — Gospel Getter"."""
    return title == MAIN_TITLE or title.endswith(f" \u2014 {MAIN_TITLE}")


def main_open(titles):
    return any(is_main(t) for t in titles)


class Refused(Exception):
    """The check couldn't be set up; nothing was asserted."""


def say(line):
    print(f"[upgrade-check] {line}", flush=True)


# --- what this checkout expects -------------------------------------------


def expected_version(conf_text):
    return json.loads(conf_text)["version"]


def identifier(conf_text):
    return json.loads(conf_text)["identifier"]


def schema_version(migrate_rs):
    found = re.search(r"pub const SCHEMA_VERSION: i64 = (\d+);", migrate_rs)
    if not found:
        raise Refused("migrate.rs has no `pub const SCHEMA_VERSION: i64 = N;`")
    return int(found.group(1))


def kept_copies(step_sources):
    """{to: keep_copy} for each step's `Step { to: N, keep_copy: B, ... }`."""
    steps = {}
    for text in step_sources:
        found = re.search(r"Step\s*\{\s*to:\s*(\d+),\s*keep_copy:\s*(true|false),", text)
        if found:
            steps[int(found.group(1))] = found.group(2) == "true"
    return steps


def promised_copies(old, new, steps):
    """The `.before-schema-N` copies the steps from `old` to `new` promise."""
    return [to for to in range(old + 1, new + 1) if steps.get(to)]


# --- the previous release ---------------------------------------------------


def checksum(sums_text, name):
    for line in sums_text.splitlines():
        parts = line.split()
        if len(parts) == 2 and parts[1].lstrip("*") == name:
            return parts[0].lower()
    raise Refused(f"SHA256SUMS lists no {name}")


def sha256(path):
    digest = hashlib.sha256()
    with open(path, "rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def gh(*args):
    done = subprocess.run(["gh", *args], capture_output=True, text=True)
    if done.returncode != 0:
        raise Refused(f"gh {' '.join(args)} failed ({done.returncode}): {done.stderr.strip()}")
    return done.stdout


def previous(platform, out, repo, gh=gh):
    """Download the Latest release's installer for `platform` into `out` and
    check it; return (tag, path). Any failure stops the job: a check that
    couldn't get the previous release never passes."""
    if not os.environ.get("GH_TOKEN"):
        raise Refused("GH_TOKEN isn't set: the previous release is downloaded with the workflow's token")
    asset = ASSETS[platform]
    try:
        tag = gh("api", f"repos/{repo}/releases/latest", "--jq", ".tag_name").strip()
    except Refused as e:
        raise Refused(f"couldn't find the previous (Latest) release: {e}") from e
    if not tag:
        raise Refused("GitHub named no Latest release to upgrade from")
    out.mkdir(parents=True, exist_ok=True)
    for name in (asset, "SHA256SUMS"):
        (out / name).unlink(missing_ok=True)
    try:
        gh("release", "download", tag, "-R", repo, "-D", str(out), "-p", asset, "-p", "SHA256SUMS")
    except Refused as e:
        raise Refused(f"couldn't download {tag}'s {asset}: {e}") from e
    want = checksum((out / "SHA256SUMS").read_text(encoding="utf-8"), asset)
    got = sha256(out / asset)
    if got != want:
        raise Refused(f"{tag}'s {asset} has SHA-256 {got}, not SHA256SUMS' {want}")
    say(f"previous release: {tag}, {asset}, SHA-256 {got} (matches its SHA256SUMS)")
    return tag, out / asset


# --- logs, the database and the CLI ------------------------------------------


def log_says(text, prefix):
    """The rest of the first line of `text` after `prefix`."""
    for line in text.splitlines():
        at = line.find(prefix)
        if at >= 0:
            return line[at + len(prefix) :].strip()
    return None


def start_failure(text):
    """The first line saying the app couldn't start, cut short, or None."""
    for line in text.splitlines():
        if FAILED_TEXT in line:
            return line.strip()[:300]
    return None


class LogFile:
    """The part of the app's log file written since `mark`."""

    def __init__(self, path):
        self.path = path
        self.offset = path.stat().st_size if path.exists() else 0

    def since(self):
        if not self.path.exists():
            return ""
        data = self.path.read_bytes()
        # A smaller file was rotated (log_file.rs): it's all new.
        data = data[self.offset :] if len(data) >= self.offset else data
        return data.decode("utf-8", "replace")


def open_ro(path):
    return sqlite3.connect(f"{Path(path).as_uri()}?mode=ro", uri=True)


def user_version(path):
    with open_ro(path) as db:
        return db.execute("PRAGMA user_version").fetchone()[0]


def seed(path):
    """Write SEED into the database `from` made, by its own columns, and
    return what was written, as rows read back."""
    db = sqlite3.connect(path)
    try:
        version = db.execute("PRAGMA user_version").fetchone()[0]
        tables = {r[0] for r in db.execute("SELECT name FROM sqlite_master WHERE type = 'table'")}
        need = {"bookmarks", "highlights", "notes", "reading_position", "reading_place", "translations"}
        if not need <= tables:
            raise Refused(f"the previous release's database lacks {sorted(need - tables)}")
        place = SEED["reading place"]
        found = db.execute("SELECT id FROM translations WHERE code = ?", (place["translation"],)).fetchone()
        if not found:
            raise Refused(f"the previous release's database has no {place['translation']} translation")
        translation_id = found[0]
        with db:
            b = SEED["bookmark"]
            db.execute(
                "INSERT INTO bookmarks (book_id, chapter, verse_start, verse_end) VALUES (?, ?, ?, ?)",
                (b["book_id"], b["chapter"], b["verse_start"], b["verse_end"]),
            )
            h = SEED["highlight"]
            db.execute(
                "INSERT INTO highlights (book_id, chapter, verse, color) VALUES (?, ?, ?, ?)",
                (h["book_id"], h["chapter"], h["verse"], h["color"]),
            )
            n = SEED["note"]
            db.execute(
                "INSERT INTO notes (book_id, chapter, verse_start, verse_end, body) VALUES (?, ?, ?, ?, ?)",
                (n["book_id"], n["chapter"], n["verse_start"], n["verse_end"], n["body"]),
            )
            db.execute(
                "INSERT OR REPLACE INTO reading_position (id, translation_id, book_id, chapter) VALUES (1, ?, ?, ?)",
                (translation_id, place["book_id"], place["chapter"]),
            )
            db.execute(
                "INSERT OR REPLACE INTO reading_place (id, translation_id, book_id, chapter, verse, offset_px) "
                "VALUES (1, ?, ?, ?, ?, NULL)",
                (translation_id, place["book_id"], place["chapter"], place["verse"]),
            )
        seeded = {
            "schema": version,
            "bookmarks": db.execute("SELECT id, book_id, chapter, verse_start, verse_end FROM bookmarks").fetchall(),
            "highlights": db.execute("SELECT book_id, chapter, verse, color FROM highlights").fetchall(),
            "notes": db.execute("SELECT id, book_id, chapter, verse_start, verse_end, body FROM notes").fetchall(),
            "reading_position": db.execute("SELECT translation_id, book_id, chapter FROM reading_position").fetchall(),
            "reading_place": db.execute("SELECT translation_id, book_id, chapter, verse FROM reading_place").fetchall(),
        }
    finally:
        db.close()
    return seeded


def rows_in(answer, key):
    """The objects under `key` in an answer, or the answer itself."""
    if key is None:
        return [answer] if isinstance(answer, dict) else []
    rows = answer.get(key) if isinstance(answer, dict) else None
    return [r for r in rows if isinstance(r, dict)] if isinstance(rows, list) else []


def kept(answers):
    """{item: (kept?, what the CLI said)} for each item of SEED, from the
    CLI's answers (READ_BACK)."""
    result = {}
    for item, (command, key, want) in READ_BACK.items():
        rows = rows_in(answers.get(command), key)
        match = [r for r in rows if all(r.get(k) == v for k, v in want.items())]
        result[item] = (bool(match), match[0] if match else rows)
    return result


def without_verses(found):
    """What the CLI said, without the verses' text, which the log would
    otherwise cut off mid-verse (read-back.json keeps it whole)."""
    if isinstance(found, list):
        return [without_verses(f) for f in found]
    if isinstance(found, dict):
        return {k: v for k, v in found.items() if k != "verses"}
    return found


def cli_answers(cli, database, env):
    answers = {}
    for command, _, _ in READ_BACK.values():
        done = subprocess.run([str(cli), "--db", str(database), command], capture_output=True,
                              encoding="utf-8", env=env)
        if done.returncode != 0:
            raise Refused(f"{cli.name} {command} failed ({done.returncode}): {done.stderr.strip()}")
        answers[command] = json.loads(done.stdout)
    return answers


def cli_version(cli, env):
    done = subprocess.run([str(cli), "--version"], capture_output=True, encoding="utf-8", env=env)
    return done.stdout.strip().removeprefix("gospel-getter-cli ").strip()


def numeric(version):
    """2.33.1 of 2.33.1, 2.33.1.0 or v2.33.1."""
    found = re.match(r"v?(\d+)\.(\d+)\.(\d+)", version or "")
    return ".".join(found.groups()) if found else None


# --- the platforms -----------------------------------------------------------


class Install:
    def __init__(self, program, cli, reported):
        self.program = program
        self.cli = cli
        # What the install itself says it is (the uninstall entry), if anything.
        self.reported = reported


class Linux:
    """AppImages, each extracted into its own folder and started with its
    AppRun, with HOME and XDG_* in the work folder, on a private Xvfb."""

    name = "linux"

    def __init__(self, work, ident):
        self.work = work
        self.ident = ident
        for tool in ("Xvfb", "xdotool"):
            if not shutil.which(tool):
                raise Refused(f"{tool} is not installed")
        home = work / "home"
        self.env = {
            "PATH": os.environ.get("PATH", "/usr/bin:/bin"),
            "HOME": str(home),
            "USER": os.environ.get("USER", "upgrade-check"),
            "LANG": "C.UTF-8",
            "XDG_DATA_HOME": str(home / "data"),
            "XDG_CONFIG_HOME": str(home / "config"),
            "XDG_CACHE_HOME": str(home / "cache"),
            "XDG_STATE_HOME": str(home / "state"),
            "XDG_RUNTIME_DIR": str(home / "runtime"),
            "GDK_BACKEND": "x11",
            "GTK_USE_PORTAL": "0",
            "GSETTINGS_BACKEND": "memory",
            "NO_AT_BRIDGE": "1",
            "NO_COLOR": "1",
        }
        for key in ("XDG_DATA_HOME", "XDG_CONFIG_HOME", "XDG_CACHE_HOME", "XDG_STATE_HOME", "XDG_RUNTIME_DIR"):
            Path(self.env[key]).mkdir(parents=True, exist_ok=True)
        os.chmod(self.env["XDG_RUNTIME_DIR"], 0o700)
        self.data_dir = Path(self.env["XDG_DATA_HOME"]) / ident
        self.log_dir = self.data_dir / "logs"
        self.xvfb = None
        self.start_xvfb()

    def start_xvfb(self):
        # A number with no lock file and no socket, never Xvfb's own
        # -displayfd search, which can take a display a desktop's Xwayland
        # serves (scripts/gui-check.sh's reason).
        number = next(
            (n for n in range(99, 200)
             if not Path(f"/tmp/.X{n}-lock").exists() and not Path(f"/tmp/.X11-unix/X{n}").exists()),
            None,
        )
        if number is None:
            raise Refused("no free X display between :99 and :199")
        log = open(self.work / "xvfb.log", "wb")
        self.xvfb = subprocess.Popen(
            ["Xvfb", f":{number}", "-screen", "0", "1440x1000x24", "-nolisten", "tcp", "-noreset"],
            env=self.env, stdout=log, stderr=subprocess.STDOUT,
        )
        deadline = time.monotonic() + 10
        while True:
            try:
                lock = int(Path(f"/tmp/.X{number}-lock").read_text().strip())
            except (OSError, ValueError):
                lock = None
            if lock == self.xvfb.pid and Path(f"/proc/{lock}/comm").read_text().strip() == "Xvfb":
                break
            if self.xvfb.poll() is not None or time.monotonic() > deadline:
                raise Refused(f"couldn't confirm :{number} is the private Xvfb (see xvfb.log)")
            time.sleep(0.1)
        self.env["DISPLAY"] = f":{number}"
        say(f"private display :{number} (Xvfb pid {self.xvfb.pid})")

    def guard(self):
        if self.data_dir.exists():
            raise Refused(f"{self.data_dir} already exists: this check starts from no data")

    def clear(self):
        """The app's data folder, which is in the work folder, gone."""
        return clear_folders([self.data_dir])

    def install(self, installer, slot):
        folder = self.work / slot
        if folder.exists():
            shutil.rmtree(folder)
        folder.mkdir(parents=True)
        image = folder / "app.AppImage"
        shutil.copyfile(installer, image)
        image.chmod(0o755)
        done = subprocess.run([str(image), "--appimage-extract"], cwd=folder, env=self.env,
                              stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, text=True)
        root = folder / "squashfs-root"
        if done.returncode != 0 or not (root / "AppRun").exists():
            raise Refused(f"{installer} didn't extract: {done.stderr.strip()}")
        cli = root / "usr" / "bin" / "gospel-getter-cli"
        programs = [p for p in (root / "usr" / "bin").iterdir() if p.name != cli.name]
        if len(programs) != 1:
            raise Refused(f"which program in {root / 'usr' / 'bin'} is the app is ambiguous: {programs}")
        return Install(root / "AppRun", cli, None), programs[0]

    def launch(self, install, out):
        return subprocess.Popen([str(install.program)], env=self.env, stdout=out, stderr=subprocess.STDOUT,
                                start_new_session=True)

    def windows(self, proc):
        done = subprocess.run(["xdotool", "search", "--onlyvisible", "--name", "."],
                              capture_output=True, text=True, env=self.env)
        titles = []
        for window in done.stdout.split():
            name = subprocess.run(["xdotool", "getwindowname", window], capture_output=True, text=True,
                                  env=self.env)
            titles.append(name.stdout.strip())
        return titles

    def running_program(self, proc, binary):
        """The app's own process: the one whose program is `binary`."""
        want = binary.resolve()
        for entry in Path("/proc").iterdir():
            if not entry.name.isdigit():
                continue
            try:
                if Path(os.readlink(entry / "exe")).resolve() == want:
                    return want
            except OSError:
                continue
        return None

    def close(self, proc):
        # No window manager on the Xvfb to ask the window to close, so the
        # app gets SIGTERM; every write it made committed (SQLite, WAL).
        try:
            os.killpg(proc.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
        try:
            proc.wait(timeout=20)
        except subprocess.TimeoutExpired:
            os.killpg(proc.pid, signal.SIGKILL)
            proc.wait()
        return "SIGTERM"

    def program_version(self, binary):
        return None

    def finish(self):
        if self.xvfb and self.xvfb.poll() is None:
            self.xvfb.terminate()
            self.xvfb.wait()


class Windows:
    """NSIS installers, each installed silently, the second over the first,
    with the account's own data folders."""

    name = "windows"

    def __init__(self, work, ident):
        self.work = work
        self.ident = ident
        self.env = dict(os.environ)
        self.data_dir = Path(os.environ["APPDATA"]) / ident
        self.log_dir = Path(os.environ["LOCALAPPDATA"]) / ident / "logs"

    def guard(self):
        for folder in (self.data_dir, self.log_dir.parent):
            if folder.exists():
                raise Refused(f"{folder} already exists: this check starts from no data")

    def clear(self):
        """The account's app data folders (the database, the logs, the
        WebView2 profile), which an earlier run left, gone: only behind the
        throwaway-runner switch."""
        return clear_folders([self.data_dir, self.log_dir.parent])

    def entry(self):
        import winreg

        roots = [(winreg.HKEY_CURRENT_USER, r"Software\Microsoft\Windows\CurrentVersion\Uninstall"),
                 (winreg.HKEY_LOCAL_MACHINE, r"Software\Microsoft\Windows\CurrentVersion\Uninstall")]
        for hive, path in roots:
            try:
                key = winreg.OpenKey(hive, path)
            except OSError:
                continue
            for i in range(winreg.QueryInfoKey(key)[0]):
                try:
                    sub = winreg.OpenKey(key, winreg.EnumKey(key, i))
                    name = winreg.QueryValueEx(sub, "DisplayName")[0]
                except OSError:
                    continue
                if "Gospel Getter" in name:
                    def value(v):
                        try:
                            return str(winreg.QueryValueEx(sub, v)[0])
                        except OSError:
                            return ""
                    if value("WindowsInstaller") == "1":
                        continue
                    return {"name": name, "version": value("DisplayVersion"),
                            "location": value("InstallLocation").strip('"')}
        return None

    def install(self, installer, slot):
        done = subprocess.run([str(installer), "/S"])
        if done.returncode != 0:
            raise Refused(f"{installer} /S exited with {done.returncode}")
        time.sleep(5)
        entry = self.entry()
        if not entry or not entry["location"]:
            raise Refused(f"Gospel Getter isn't registered after {installer} /S")
        location = Path(entry["location"])
        say(f"installed {Path(installer).name} ({slot}): {entry['name']} {entry['version']} in {location}")
        program = location / "gospel_getter.exe"
        if not program.exists():
            found = sorted(p.name for p in location.glob("*.exe"))
            raise Refused(f"no gospel_getter.exe in {location} (found {found})")
        return Install(program, location / "gospel-getter-cli.exe", entry["version"]), program

    def launch(self, install, out):
        return subprocess.Popen([str(install.program)], env=self.env, stdout=out, stderr=subprocess.STDOUT)

    def _windows(self, pid):
        import ctypes
        from ctypes import wintypes

        user32 = ctypes.windll.user32
        found = []

        @ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
        def each(hwnd, _):
            owner = wintypes.DWORD()
            user32.GetWindowThreadProcessId(hwnd, ctypes.byref(owner))
            if owner.value == pid and user32.IsWindowVisible(hwnd):
                text = ctypes.create_unicode_buffer(512)
                user32.GetWindowTextW(hwnd, text, 512)
                found.append((hwnd, text.value))
            return True

        user32.EnumWindows(each, 0)
        return found

    def windows(self, proc):
        return [title for _, title in self._windows(proc.pid)]

    def running_program(self, proc, binary):
        return binary if proc.poll() is None else None

    def close(self, proc):
        import ctypes

        # Only the window a reader would close: the main window, or the
        # start-failed dialog. Its other windows (the single-instance
        # plugin's, WebView2's host) aren't the reader's to close.
        wm_close = 0x0010
        asked = [title for hwnd, title in self._windows(proc.pid)
                 if (is_main(title) or title == FAILED_TITLE)
                 and ctypes.windll.user32.PostMessageW(hwnd, wm_close, 0, 0)]
        try:
            proc.wait(timeout=30)
            return f"WM_CLOSE to {asked}"
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait()
            return f"killed: it didn't close within 30s of WM_CLOSE to {asked}"

    def program_version(self, binary):
        done = subprocess.run(
            ["powershell", "-NoProfile", "-Command", f"(Get-Item -LiteralPath '{binary}').VersionInfo.ProductVersion"],
            capture_output=True, text=True,
        )
        return done.stdout.strip() or None

    def finish(self):
        pass


def clear_folders(folders):
    """Remove each of `folders` that exists; the ones removed."""
    removed = []
    for folder in folders:
        if folder.exists():
            shutil.rmtree(folder)
            removed.append(str(folder))
    return removed


# --- one start ---------------------------------------------------------------


class Start:
    """What one start of an install showed."""

    def __init__(self):
        self.alive = False
        self.titles = []
        self.output = ""
        self.log = ""
        self.program = None
        self.seconds = 0.0
        self.closed = ""


def start(platform, install, binary, label, seconds, settle, until=None):
    """Start `install`, watch it for up to `seconds` until its main window
    is open (and `until` is in its output, if given), then `settle` more, and
    close it."""
    out_path = platform.work / f"{label}.out.log"
    log = LogFile(platform.log_dir / LOG_FILE)
    seen = Start()
    began = time.monotonic()
    with open(out_path, "wb") as out:
        proc = platform.launch(install, out)
        try:
            deadline = began + seconds
            while time.monotonic() < deadline and proc.poll() is None:
                time.sleep(1)
                seen.titles = platform.windows(proc)
                text = ANSI.sub("", out_path.read_text(encoding="utf-8", errors="replace"))
                if FAILED_TITLE in seen.titles or start_failure(text):
                    break
                if main_open(seen.titles) and (until is None or until in text):
                    break
            settle_until = time.monotonic() + settle
            while time.monotonic() < settle_until and proc.poll() is None:
                time.sleep(1)
            seen.seconds = time.monotonic() - began
            seen.alive = proc.poll() is None
            if seen.alive:
                seen.titles = platform.windows(proc)
                seen.program = platform.running_program(proc, binary)
        finally:
            seen.closed = platform.close(proc) if proc.poll() is None else f"exited with {proc.returncode}"
    seen.output = ANSI.sub("", out_path.read_text(encoding="utf-8", errors="replace"))
    seen.log = log.since()
    return seen


# --- the check ---------------------------------------------------------------


class Verdicts:
    def __init__(self):
        self.failed = []

    def line(self, step, what, ok, detail):
        say(f"{step} {what}: {'PASS' if ok else 'FAIL'}: {detail}")
        if not ok:
            self.failed.append(step)
            if os.environ.get("GITHUB_ACTIONS") == "true":
                print(f"::error title=Upgrade check {step}::{what}: {detail}", flush=True)


def started_verdict(seen, seconds_after):
    """(ok, detail) for 5a."""
    failure = start_failure(seen.output) or start_failure(seen.log)
    problems = []
    if not seen.alive:
        problems.append(f"the process exited ({seen.closed}) within {seen.seconds:.0f}s")
    if failure:
        problems.append(f"its log says: {failure}")
    if FAILED_TITLE in seen.titles:
        problems.append(f"the \"{FAILED_TITLE}\" dialog is open, waiting")
    if seen.alive and not main_open(seen.titles):
        problems.append(f"no \"{MAIN_TITLE}\" window opened (windows: {seen.titles})")
    if problems:
        return False, "; ".join(problems)
    return True, (f"alive {seen.seconds:.0f}s after launch, {seconds_after}s after its window opened; "
                  f"no \"{FAILED_TEXT}\" in its log, windows {seen.titles}")


def run(args, platform_class=None, back=False):
    """`run` (the previous release, then this build over it) or, with
    `back`, `back` (this build, then the previous release over it)."""
    if os.environ.get(SWITCH) != SWITCH_VALUE:
        raise Refused(
            f"refusing to run: this installs over the app and writes its data folder, so it runs only on a "
            f"throwaway CI runner, with {SWITCH}={SWITCH_VALUE}"
        )
    conf = TAURI_CONF.read_text(encoding="utf-8")
    if back and not args.expect_version:
        raise Refused("back needs --expect-version: the version of the release it goes back to")
    want_version = args.expect_version or expected_version(conf)
    want_schema = args.expect_schema or schema_version(MIGRATE_RS.read_text(encoding="utf-8"))
    steps = kept_copies(p.read_text(encoding="utf-8") for p in sorted(MIGRATE_DIR.glob("schema_*.rs")))
    work = Path(args.work).resolve()
    work.mkdir(parents=True, exist_ok=True)
    if platform_class is None:
        platform_class = Windows if sys.platform == "win32" else Linux
    platform = platform_class(work, identifier(conf))
    (first_name, second_name) = ("this build", "the previous release") if back else ("the previous release", "this build")
    say(f"from {args.from_installer} to {args.to_installer} ({'going back' if back else 'upgrading'}); "
        f"expecting version {want_version}, schema {want_schema}")
    verdicts = Verdicts()
    try:
        if back:
            removed = platform.clear()
            if removed:
                say(f"removed the data an earlier check left: {removed}")
        platform.guard()

        # 1-2: the first install, as a reader has it.
        old, old_binary = platform.install(Path(args.from_installer), "from")
        first = start(platform, old, old_binary, "from", args.startup_seconds, 2, until="Built search index")
        database = Path(log_says(first.output, "Using database at ") or platform.data_dir / DATABASE_FILE)
        ok, detail = started_verdict(first, 2)
        say(f"{first_name} started once: {'yes' if ok else 'NO'}: {detail}; closed: {first.closed}")
        if not ok:
            raise Refused(f"{first_name} didn't start, so there's nothing to go on from")
        if not database.exists():
            raise Refused(f"{first_name} made no database at {database}")

        # 3: the reader's data, in its own schema.
        seeded = seed(database)
        old_schema = seeded["schema"]
        (work / "seeded.json").write_text(json.dumps(seeded, indent=2), encoding="utf-8")
        say(f"seeded {database} at schema {old_schema}:")
        for key in ("bookmarks", "highlights", "notes", "reading_position", "reading_place"):
            say(f"  {key}: {seeded[key]}")

        # 4: the second install, over it.
        new, new_binary = platform.install(Path(args.to_installer), "to")
        seen = start(platform, new, new_binary, "to", args.startup_seconds, args.settle_seconds)
        say(f"{second_name} started; closed: {seen.closed}")

        # 5a
        ok, detail = started_verdict(seen, args.settle_seconds)
        verdicts.line("5a", "it started", ok, detail)

        # 5b
        cli = cli_version(new.cli, platform.env) if new.cli.exists() else None
        reports = {
            "running program": platform.program_version(new_binary) if seen.program else None,
            "install": new.reported,
            "its gospel-getter-cli": cli,
        }
        reported = {k: v for k, v in reports.items() if v}
        wrong = {k: v for k, v in reported.items() if numeric(v) != want_version}
        ok = bool(seen.program) and bool(reported) and not wrong
        running = f"the running program is {seen.program}" if seen.program else f"{second_name}'s program wasn't running"
        verdicts.line("5b", f"it's {want_version}", ok, f"{running}; reported: {reported}")

        # 5c
        now = user_version(database)
        ok, detail = schema_verdict(database, old_schema, want_schema, now, [] if back else promised_copies(old_schema, want_schema, steps))
        if back:
            detail += "; an older app leaves a newer schema as it is"
        verdicts.line("5c", f"the database is at schema {want_schema}", ok, detail)

        # 5d
        answers = cli_answers(new.cli, database, platform.env)
        (work / "read-back.json").write_text(json.dumps(answers, indent=2), encoding="utf-8")
        for item, (ok, found) in kept(answers).items():
            verdicts.line("5d", f"the seeded {item} is kept", ok, json.dumps(without_verses(found))[:400])
    finally:
        platform.finish()

    if verdicts.failed:
        say(f"FAILED: {', '.join(verdicts.failed)}")
        return 1
    say("PASSED: every assertion")
    return 0


def schema_verdict(database, old_schema, want_schema, now, promised):
    """(ok, detail) for 5c: the database at `want_schema`, with each
    `.before-schema-N` copy in `promised` beside it."""
    copies = {to: Path(f"{database}.before-schema-{to}") for to in promised}
    missing = [str(p) for p in copies.values() if not p.exists()]
    copy_versions = {str(p): user_version(p) for p in copies.values() if p.exists()}
    ok = now == want_schema and not missing
    detail = f"PRAGMA user_version {now} (was {old_schema}); "
    if promised:
        detail += f"copies promised: {[str(p) for p in copies.values()]}, at {copy_versions}"
        if missing:
            detail += f", missing: {missing}"
    else:
        detail += "no step ran, so no before-schema copy is promised"
    return ok, detail


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = parser.add_subparsers(dest="command", required=True)
    p = sub.add_parser("previous", help="download and check the Latest release's installer")
    p.add_argument("--platform", choices=sorted(ASSETS), required=True)
    p.add_argument("--out", required=True)
    p.add_argument("--repo", default=os.environ.get("GITHUB_REPOSITORY", REPOSITORY))
    for name, help in (("run", "upgrade --from to --to and check it"),
                       ("back", "go back from --from (this build) to --to (the previous release) and check it")):
        r = sub.add_parser(name, help=help)
        r.add_argument("--from", dest="from_installer", required=True)
        r.add_argument("--to", dest="to_installer", required=True)
        r.add_argument("--work", required=True)
        r.add_argument("--expect-version")
        r.add_argument("--expect-schema", type=int)
        r.add_argument("--startup-seconds", type=int, default=90)
        r.add_argument("--settle-seconds", type=int, default=10)
    args = parser.parse_args(argv)
    # The CLI's answers and the windows' titles are UTF-8 (an en dash, a
    # middle dot), whatever the console's code page.
    for stream in (sys.stdout, sys.stderr):
        if hasattr(stream, "reconfigure"):
            stream.reconfigure(encoding="utf-8", errors="replace")
    try:
        if args.command == "previous":
            tag, path = previous(args.platform, Path(args.out), args.repo)
            if os.environ.get("GITHUB_OUTPUT"):
                with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as out:
                    out.write(f"tag={tag}\ninstaller={path}\n")
            print(path)
            return 0
        return run(args, back=args.command == "back")
    except Refused as e:
        say(f"ERROR: {e}")
        if os.environ.get("GITHUB_ACTIONS") == "true":
            print(f"::error title=Upgrade check::{e}", flush=True)
        return 2


if __name__ == "__main__":
    sys.exit(main())
