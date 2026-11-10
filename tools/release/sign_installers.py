#!/usr/bin/env python3
"""Sign a Distribution Artifacts run's four installers for the updater.

Usage, from the repository root (Python 3 standard library only, and the
Tauri CLI as `cargo tauri`):
    python3 tools/release/sign_installers.py DIR \\
        [--private-key-path KEY --pubkey-file KEY.pub]

DIR holds the four installers the dist.yml jobs built, each in the folder
of the artifact it was downloaded into (download-artifact's layout):

    gospel-getter-linux-appimage/Gospel Getter_X.Y.Z_amd64.AppImage
    gospel-getter-linux-deb/Gospel Getter_X.Y.Z_amd64.deb
    gospel-getter-windows-nsis-installer/Gospel Getter_X.Y.Z_x64-setup.exe
    gospel-getter-windows-msi-installer/Gospel Getter_X.Y.Z_x64_en-US.msi

Each gets the .sig beside it that `cargo tauri build` writes when it signs
(`cargo tauri signer sign --app-version`, the same signature, bound to the
version in src-tauri/tauri.conf.json), and each signature is checked: for
that version, and by the key the app trusts (the updater pubkey in
tauri.conf.json, or --pubkey-file). Then, for a release version, the
update manifest is made from them in a scratch folder
(make_latest_json.build_manifest), so a run that signs is one a release
can take; nothing is written there but the .sig files.

The key is the Tauri CLI's to read: TAURI_SIGNING_PRIVATE_KEY and
TAURI_SIGNING_PRIVATE_KEY_PASSWORD in the environment (dist.yml's sign
job, in the release environment, on a run dispatched on main), or a key
file (--private-key-path, the signing rehearsal's throwaway key, made
without a password). This script never reads the key itself.
"""

import argparse
import base64
import binascii
import json
import os
import shlex
import subprocess
import sys
import tempfile
from pathlib import Path

import make_latest_json

REPO_ROOT = Path(__file__).resolve().parents[2]
CONF = REPO_ROOT / "src-tauri" / "tauri.conf.json"

# Each installer, by the end of its file name, and its platform key in the
# update manifest (make_latest_json.PLATFORMS).
KINDS = {
    ".AppImage": "linux-x86_64-appimage",
    ".deb": "linux-x86_64-deb",
    "-setup.exe": "windows-x86_64-nsis",
    ".msi": "windows-x86_64-msi",
}


class SigningError(Exception):
    """Why the installers weren't signed, or a signature isn't right."""


def find_installers(root: Path) -> dict[str, Path]:
    """The four installers under `root`, by platform key: exactly one each."""
    found: dict[str, list[Path]] = {key: [] for key in KINDS.values()}
    for path in sorted(root.rglob("*")):
        if not path.is_file():
            continue
        for suffix, key in KINDS.items():
            if path.name.endswith(suffix):
                found[key].append(path)
    problems = []
    for key, paths in found.items():
        if not paths:
            problems.append(f"no {key} installer")
        elif len(paths) > 1:
            problems.append(f"{len(paths)} {key} installers: {', '.join(p.name for p in paths)}")
    if problems:
        raise SigningError(f"under {root}: {'; '.join(problems)}")
    return {key: paths[0] for key, paths in found.items()}


def key_id(text: str, what: str) -> bytes:
    """The 8-byte key ID of a base64 minisign public key or Tauri signature.

    Both are base64 of a minisign file: an untrusted comment line, then a
    base64 line that starts with the algorithm (2 bytes) and the key ID.
    """
    try:
        lines = base64.b64decode(text.strip(), validate=True).decode("utf-8").splitlines()
        blob = base64.b64decode(lines[1], validate=True)
    except (binascii.Error, UnicodeDecodeError, IndexError):
        raise SigningError(f"{what} isn't a minisign key or signature") from None
    if len(blob) < 10:
        raise SigningError(f"{what} is too short to hold a key ID")
    return blob[2:10]


def shown(key: bytes) -> str:
    """A key ID as minisign shows it (`minisign public key: 2CEED11FD5DD0F77`):
    the little-endian number, in hex."""
    return key[::-1].hex().upper()


def app_version(conf: Path) -> str:
    """The app's version, which `cargo tauri build` binds its signatures to."""
    version = json.loads(conf.read_text(encoding="utf-8")).get("version")
    if not isinstance(version, str) or not version:
        raise SigningError(f"{conf} has no version")
    return version


def app_pubkey(conf: Path) -> str:
    """The updater public key the app trusts."""
    config = json.loads(conf.read_text(encoding="utf-8"))
    pubkey = config.get("plugins", {}).get("updater", {}).get("pubkey")
    if not isinstance(pubkey, str) or not pubkey:
        raise SigningError(f"{conf} has no plugins.updater.pubkey")
    return pubkey


def check_signature(installer: Path, version: str, pubkey: str) -> None:
    """The .sig beside `installer` is for `version`, by `pubkey`'s key."""
    sig = installer.with_name(installer.name + ".sig")
    if not sig.is_file() or not sig.read_text(encoding="utf-8").strip():
        raise SigningError(f"no signature was written for {installer.name}")
    signature = sig.read_text(encoding="utf-8").strip()
    try:
        signed = make_latest_json.signed_version(signature)
    except make_latest_json.ManifestError as e:
        raise SigningError(f"{sig.name} {e}") from None
    if signed != version:
        raise SigningError(f"{sig.name} was signed for version {signed or '(none)'}, not {version}")
    made_by, trusted = key_id(signature, sig.name), key_id(pubkey, "the public key")
    if made_by != trusted:
        raise SigningError(
            f"{sig.name} was made by key {shown(made_by)}, not the trusted key {shown(trusted)}"
        )


def rehearse_manifest(installers: dict[str, Path], version: str) -> str:
    """Make the update manifest from the signed installers, in a scratch
    folder, as a release would (tools/release/README.md). A pre-release has
    no manifest, so it has nothing to rehearse."""
    if not make_latest_json.VERSION.match(version):
        return f"{version} is a pre-release, which has no update manifest; not made"
    with tempfile.TemporaryDirectory() as scratch:
        assets = Path(scratch)
        for key, installer in installers.items():
            name = make_latest_json.PLATFORMS[key]
            (assets / name).symlink_to(installer)
            (assets / f"{name}.sig").write_bytes(installer.with_name(installer.name + ".sig").read_bytes())
        try:
            manifest = make_latest_json.build_manifest(version, "Rehearsal.", assets, "2026-01-01T00:00:00Z")
        except make_latest_json.ManifestError as e:
            raise SigningError(f"the update manifest couldn't be made from them: {e}") from None
    return f"the update manifest can be made from them: {', '.join(manifest['platforms'])}"


def sign(installers: dict[str, Path], version: str, cli: list[str], key_path: Path | None) -> None:
    """Have the Tauri CLI sign each installer, as `cargo tauri build` would."""
    # A key file is the rehearsal's, made without a password: said so, or
    # the CLI would ask for one.
    key_args = ["--private-key-path", str(key_path), "--password", ""] if key_path else []
    for installer in installers.values():
        command = [*cli, "signer", "sign", "--app-version", version, *key_args, str(installer)]
        if subprocess.run(command, stdin=subprocess.DEVNULL, check=False).returncode != 0:
            raise SigningError(f"the Tauri CLI couldn't sign {installer.name}")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("installers", type=Path, help="the downloaded installer artifacts")
    parser.add_argument("--private-key-path", type=Path, help="a key file (else the CLI's environment)")
    parser.add_argument("--pubkey-file", type=Path, help="the key to check against (else the app's)")
    parser.add_argument("--conf", type=Path, default=CONF, help=argparse.SUPPRESS)
    parser.add_argument("--cli", default="cargo tauri", help=argparse.SUPPRESS)
    args = parser.parse_args(argv)

    try:
        if args.private_key_path is None and not os.environ.get("TAURI_SIGNING_PRIVATE_KEY"):
            raise SigningError(
                "no signing key: TAURI_SIGNING_PRIVATE_KEY isn't set (the sign job's secret) "
                "and no --private-key-path was given"
            )
        version = app_version(args.conf)
        if args.pubkey_file:
            pubkey = args.pubkey_file.read_text(encoding="utf-8").strip()
        else:
            pubkey = app_pubkey(args.conf)
        installers = find_installers(args.installers)
        for installer in installers.values():
            stale = installer.with_name(installer.name + ".sig")
            if stale.exists():
                raise SigningError(f"{stale.name} is already there: these installers were signed before")
        sign(installers, version, shlex.split(args.cli), args.private_key_path)
        for installer in installers.values():
            check_signature(installer, version, pubkey)
        rehearsal = rehearse_manifest(installers, version)
    except SigningError as e:
        print(f"error: {e}", file=sys.stderr)
        return 1
    for installer in installers.values():
        print(f"Signed {installer.name} for {version} ({installer.name}.sig)")
    print(f"By key {shown(key_id(pubkey, 'the public key'))}; {rehearsal}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
