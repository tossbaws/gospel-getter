# Release tools

## `make_latest_json.py`: the update manifest

The app checks
`https://github.com/tossbaws/gospel-getter/releases/latest/download/latest.json`
for updates (`plugins.updater` in `src-tauri/tauri.conf.json`), so every
release must have a `latest.json` asset. Without one, release installers
can't see the release as an update.

1. Take the four installers from the verified Distribution Artifacts run on
   the release commit. That's the run dispatched after the merge
   (`gh workflow run dist.yml --ref main`): the merge's own push run builds
   nothing. Each artifact also holds the installer's `.sig`
   updater signature, which the run's last job, "Sign the installers
   (release)", puts there once both platforms' GUI checks have passed (see
   `sign_installers.py` below): take them after it has finished. No other
   run is signed. Rename both files of each pair to the stable names:

   | Platform key | Asset | Signature |
   |---|---|---|
   | `linux-x86_64-appimage` | `Gospel-Getter_amd64.AppImage` | `Gospel-Getter_amd64.AppImage.sig` |
   | `linux-x86_64-deb` | `Gospel-Getter_amd64.deb` | `Gospel-Getter_amd64.deb.sig` |
   | `windows-x86_64-nsis` | `Gospel-Getter_x64-setup.exe` | `Gospel-Getter_x64-setup.exe.sig` |
   | `windows-x86_64-msi` | `Gospel-Getter_x64_en-US.msi` | `Gospel-Getter_x64_en-US.msi.sig` |

2. Write the manifest. It refuses if anything is missing or a signature
   was made for another version:

   ```sh
   python3 tools/release/make_latest_json.py --version X.Y.Z \
       --notes-file NOTES.md --assets DIR
   ```

3. Take the two standalone CLI binaries from the same run, from its
   `gospel-getter-cli-linux-x86_64` and `gospel-getter-cli-windows-x86_64`
   artifacts, and keep their names:
   `gospel-getter-cli-x86_64-unknown-linux-gnu` and
   `gospel-getter-cli-x86_64-pc-windows-msvc.exe`. They have no `.sig` and
   aren't in `latest.json`: the updater never fetches them, since each
   installer carries its own copy.

4. Write `SHA256SUMS`, covering the four installers and the two CLI
   binaries, by these names.

5. Upload the installers, their `.sig` files, the two CLI binaries,
   `latest.json` and `SHA256SUMS` to the `vX.Y.Z` release. The manifest's
   URLs point at that tag (`releases/download/vX.Y.Z/...`), so it names
   exactly the four installers it was made for.

Each installer has its own platform key, which the updater plugin looks up
for the bundle the running copy was installed from. A `.deb` install is only
ever offered the `.deb`, an MSI install the MSI, and so on. The app also
requires the version inside each signature to match the manifest's
(`requireSignedVersion`), which stops an old, validly signed build from
being passed off as a new one.

## `sign_installers.py`: the updater signatures

No job that builds an installer has the updater's signing key: every
build is unsigned (`cargo tauri build --no-sign`). On a run dispatched on
`main`, dist.yml's `sign` job, in the `release` environment (which deploys
from `main` only) and the only job that names the key's secrets, downloads
the four installers, and this script signs each one as `cargo tauri build`
would have (`cargo tauri signer sign --app-version`, the version in
`src-tauri/tauri.conf.json`), checks each signature is for that version
and by the key in `tauri.conf.json`, and makes the update manifest from
them in a scratch folder as a dry run. The job then puts each installer's
artifact back with its `.sig` beside it.

A run dispatched on another branch, and a pull request from a `release/`
branch, rehearse this (the `sign-rehearsal` job) with a throwaway key made
for the run, uploading nothing:

```sh
cargo tauri signer generate --ci --write-keys rehearsal.key
python3 tools/release/sign_installers.py DIR \
    --private-key-path rehearsal.key --pubkey-file rehearsal.key.pub
```

The release's key and its password are repository secrets
(`TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`). Moving
them into the `release` environment, so that only its jobs can read them,
needs their values, so only the key's owner can do it; the workflow already
names them nowhere else.

## `site_version_gate.py`: when the website deploys

`.github/workflows/pages.yml` deploys `site/` only when the version the
site shows (`Version X.Y.Z` in `site/index.html`) is at or below the newest
published release, so a release-prep merge doesn't put a new version on
the site over the old downloads. Publishing the release starts the deploy
(see `site/README.md`):

```sh
python3 tools/release/site_version_gate.py --site site/index.html --published v2.16.0
```

prints `deploy=true` or `deploy=false`, with the reason on stderr.
A pre-release's tag (`v2.19.0-beta.1`) is ignored, and publishing a
pre-release starts no deploy: the site never shows or links a pre-release.

Tests: `python3 -m unittest discover -s tools/release` (run in CI).
