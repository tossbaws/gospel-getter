#!/usr/bin/env python3
"""Assemble a release's files from a Distribution Artifacts run, checked.

Usage, from the repository root (Python 3 standard library only):
    python3 tools/release/bundle_release.py DIR OUT [--pubkey-file KEY.pub]

DIR holds the run's artifacts, each downloaded into a folder of its name:

    DIR/signed/<build artifact>/   its -signed artifact: the installer, its
                                   .sig and INSTALLER-SHA256SUMS
    DIR/built/<build artifact>/    the build artifact itself, as built
    DIR/cli/<CLI artifact>/        gospel-getter-cli-linux-x86_64 and
                                   gospel-getter-cli-windows-x86_64

for the four build artifacts (gospel-getter-linux-appimage and so on,
tools/release/README.md). Every installer is checked against the SHA-256
its INSTALLER-SHA256SUMS recorded before it was signed, and against the
build artifact the GUI checks ran on (the same bytes), and its .sig must be
for the app's version (src-tauri/tauri.conf.json) and by the key the app
trusts (its updater pubkey, or --pubkey-file for the signing rehearsal's
throwaway key). Then OUT gets the four installers under their stable names
(make_latest_json.PLATFORMS) with their .sig files, the two CLI binaries
under their own names, and SHA256SUMS over the six (as `sha256sum` writes
it, and checks it with -c): everything a release uploads but latest.json,
which needs the release notes and stays a step of the publish.

It refuses, writing nothing, if any file is missing or unexpected, a
checksum doesn't match, or a signed installer isn't its build's. dist.yml's
bundle job runs it after the sign job (or the signing rehearsal); it names
no secret, and reads only what those jobs uploaded.
"""

import argparse
import shutil
import sys
from pathlib import Path

import make_latest_json
import sign_installers as signing

# Each build artifact, by the platform key of the installer it holds.
INSTALLER_ARTIFACTS = {
    "gospel-getter-linux-appimage": "linux-x86_64-appimage",
    "gospel-getter-linux-deb": "linux-x86_64-deb",
    "gospel-getter-windows-nsis-installer": "windows-x86_64-nsis",
    "gospel-getter-windows-msi-installer": "windows-x86_64-msi",
}

# Each CLI artifact, and the one file in it, uploaded under that name.
CLI_ARTIFACTS = {
    "gospel-getter-cli-linux-x86_64": "gospel-getter-cli-x86_64-unknown-linux-gnu",
    "gospel-getter-cli-windows-x86_64": "gospel-getter-cli-x86_64-pc-windows-msvc.exe",
}

SHA256SUMS = "SHA256SUMS"

# The installer's file name ends in this, by platform key.
SUFFIXES = {key: suffix for suffix, key in signing.KINDS.items()}


class Refused(Exception):
    """Why nothing was written; every reason found, one per line."""


def files_in(folder: Path) -> dict[str, Path]:
    """Everything directly in `folder`, by name: a nested folder counts
    too, so it's reported as unexpected rather than skipped."""
    return {path.name: path for path in sorted(folder.iterdir())}


def check_installer(root: Path, artifact: str, key: str, version: str, pubkey: str, problems: list[str]):
    """The signed installer of one build artifact, if everything about it
    holds; else None, with what doesn't in `problems`."""
    signed_dir, built_dir = root / "signed" / artifact, root / "built" / artifact
    found = []
    for folder in (signed_dir, built_dir):
        if not folder.is_dir():
            problems.append(f"{artifact}: no {folder}")
            found.append(None)
        else:
            found.append(files_in(folder))
    signed, built = found
    if signed is None or built is None:
        return None
    installers = [name for name in signed if name.endswith(SUFFIXES[key])]
    if len(installers) != 1:
        problems.append(f"{artifact}: {len(installers)} {key} installers in {signed_dir}, not 1")
        return None
    installer = signed[installers[0]]
    expected = {installer.name, f"{installer.name}.sig", signing.SUMS}
    missing, extra = sorted(expected - signed.keys()), sorted(signed.keys() - expected)
    if missing:
        problems.append(f"{artifact}: missing from {signed_dir}: {', '.join(missing)}")
    if extra:
        problems.append(f"{artifact}: unexpected in {signed_dir}: {', '.join(extra)}")
    if missing or extra or not all(path.is_file() for path in signed.values()):
        return None
    reasons = []
    digest = signing.sha256(installer)
    sums = (signed_dir / signing.SUMS).read_text(encoding="utf-8").splitlines()
    if sums != [f"{digest}  {installer.name}"]:
        reasons.append(f"its {signing.SUMS} says {sums!r}, not its SHA-256 {digest}")
    if sorted(built) != [installer.name] or not built[installer.name].is_file():
        reasons.append(f"its build artifact holds {sorted(built)}, not {installer.name} alone")
    elif signing.sha256(built[installer.name]) != digest:
        reasons.append(
            f"it isn't the installer as built: SHA-256 {digest} signed, "
            f"{signing.sha256(built[installer.name])} built"
        )
    try:
        signing.check_signature(installer, version, pubkey)
    except signing.SigningError as e:
        reasons.append(str(e))
    if reasons:
        problems.extend(f"{artifact}: {installer.name}: {reason}" for reason in reasons)
        return None
    return installer, digest


def check_cli(root: Path, artifact: str, name: str, problems: list[str]):
    """The one CLI binary of a CLI artifact, if it's there alone."""
    folder = root / "cli" / artifact
    if not folder.is_dir():
        problems.append(f"{artifact}: no {folder}")
        return None
    found = files_in(folder)
    if sorted(found) != [name] or not found[name].is_file():
        problems.append(f"{artifact}: {folder} holds {sorted(found)}, not {name} alone")
        return None
    return found[name], signing.sha256(found[name])


def bundle(root: Path, out: Path, version: str, pubkey: str) -> list[str]:
    """Check everything under `root`, then write the release's files in
    `out`; returns what was checked and written, a line each."""
    if out.exists() and (not out.is_dir() or any(out.iterdir())):
        raise Refused(f"{out} isn't an empty folder: the bundle is made afresh")
    problems: list[str] = []
    installers = {
        key: check_installer(root, artifact, key, version, pubkey, problems)
        for artifact, key in INSTALLER_ARTIFACTS.items()
    }
    clis = {name: check_cli(root, artifact, name, problems) for artifact, name in CLI_ARTIFACTS.items()}
    if problems:
        raise Refused("\n".join(problems))

    out.mkdir(parents=True, exist_ok=True)
    report, sums = [], []
    for key, (installer, digest) in installers.items():
        name = make_latest_json.PLATFORMS[key]
        shutil.copyfile(installer, out / name)
        shutil.copyfile(installer.with_name(f"{installer.name}.sig"), out / f"{name}.sig")
        sums.append((name, digest))
        report.append(
            f"{name} ({installer.name}): SHA-256 {digest}, as recorded and as built; "
            f"{name}.sig for {version}"
        )
    for name, (binary, digest) in clis.items():
        shutil.copyfile(binary, out / name)
        sums.append((name, digest))
        report.append(f"{name}: SHA-256 {digest}")
    # Copied, not trusted: each file in OUT is hashed again.
    for name, digest in sums:
        if signing.sha256(out / name) != digest:
            shutil.rmtree(out)
            raise Refused(f"{name} changed while it was copied into {out}")
    (out / SHA256SUMS).write_text("".join(f"{d}  {n}\n" for n, d in sums), encoding="utf-8")
    report.append(f"{SHA256SUMS}: {len(sums)} files")
    if make_latest_json.VERSION.match(version):
        try:
            manifest = make_latest_json.build_manifest(version, "Bundle check.", out, "2026-01-01T00:00:00Z")
        except make_latest_json.ManifestError as e:
            shutil.rmtree(out)
            raise Refused(f"the update manifest couldn't be made from the bundle: {e}") from None
        report.append(f"the update manifest can be made from it: {', '.join(manifest['platforms'])}")
    else:
        report.append(f"{version} is a pre-release, which has no update manifest; not tried")
    return report


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("artifacts", type=Path, help="the downloaded artifacts (signed/, built/, cli/)")
    parser.add_argument("out", type=Path, help="an empty or new folder for the release's files")
    parser.add_argument("--pubkey-file", type=Path, help="the key to check against (else the app's)")
    parser.add_argument("--conf", type=Path, default=signing.CONF, help=argparse.SUPPRESS)
    args = parser.parse_args(argv)
    try:
        version = signing.app_version(args.conf)
        if args.pubkey_file:
            pubkey = args.pubkey_file.read_text(encoding="utf-8").strip()
        else:
            pubkey = signing.app_pubkey(args.conf)
        report = bundle(args.artifacts, args.out, version, pubkey)
    except (Refused, signing.SigningError, OSError) as e:
        for line in str(e).splitlines():
            print(f"bundle: error: {line}", file=sys.stderr)
        print("bundle: REFUSED, nothing written", file=sys.stderr)
        return 1
    for line in report:
        print(f"bundle: {line}")
    key = signing.shown(signing.key_id(pubkey, "the public key"))
    print(f"bundle: OK, {len(list(args.out.iterdir()))} files in {args.out}, signed by key {key}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
