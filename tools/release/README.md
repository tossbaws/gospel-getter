# Release tools

## `make_latest_json.py`: the update manifest

The app checks
`https://github.com/tossbaws/gospel-getter/releases/latest/download/latest.json`
for updates (`plugins.updater` in `src-tauri/tauri.conf.json`), so every
release must have a `latest.json` asset. Without one, release installers
can't see the release as an update.

1. Take the release's files from the verified Distribution Artifacts run
   on the release commit. That's the run dispatched after the merge (`gh
   workflow run dist.yml --ref main`): the merge's own push run builds
   nothing. They're in one artifact, `gospel-getter-release-bundle`, which
   the run's last job, "Assemble the release files", uploads after "Sign
   the installers (release)" has signed them (see `sign_installers.py` and
   `bundle_release.py` below): take it once that job has succeeded, never
   from an attempt that failed. No other run makes one. It holds the four
   installers under their stable names with their `.sig` updater
   signatures, the two CLI binaries, and `SHA256SUMS` over the six:

   | Platform key | Asset | Signature |
   |---|---|---|
   | `linux-x86_64-appimage` | `Gospel-Getter_amd64.AppImage` | `Gospel-Getter_amd64.AppImage.sig` |
   | `linux-x86_64-deb` | `Gospel-Getter_amd64.deb` | `Gospel-Getter_amd64.deb.sig` |
   | `windows-x86_64-nsis` | `Gospel-Getter_x64-setup.exe` | `Gospel-Getter_x64-setup.exe.sig` |
   | `windows-x86_64-msi` | `Gospel-Getter_x64_en-US.msi` | `Gospel-Getter_x64_en-US.msi.sig` |

   and `gospel-getter-cli-x86_64-unknown-linux-gnu` and
   `gospel-getter-cli-x86_64-pc-windows-msvc.exe`, which have no `.sig`
   and aren't in `latest.json`: the updater never fetches them, since each
   installer carries its own copy.

   The run keeps its artifacts 14 days (`retention-days` in `dist.yml`);
   a release published later needs a new dispatched run.

   Check every file against `SHA256SUMS` once more, here, before anything
   else (the bundle job checked it too):

   ```sh
   gh run download RUN_ID -n gospel-getter-release-bundle -D bundle
   (cd bundle && sha256sum -c SHA256SUMS)
   ```

2. Write the manifest from the bundle's files, into the bundle (`--out`
   defaults to `bundle/latest.json`). It refuses if anything is missing or a
   signature was made for another version:

   ```sh
   python3 tools/release/make_latest_json.py --version X.Y.Z \
       --notes-file NOTES.md --assets bundle
   ```

3. Upload everything in `bundle/` (the installers, their `.sig` files, the
   two CLI binaries, `SHA256SUMS` and `latest.json`) to the `vX.Y.Z`
   release. The manifest's URLs point at that tag
   (`releases/download/vX.Y.Z/...`), so it names exactly the four
   installers it was made for.

### By hand, if there's no bundle

If the bundle job didn't run or can't be re-run, assemble the same files
from the run's other artifacts. `bundle_release.py` does exactly this, and
can be run locally on the downloads instead (the layout is in its
docstring).

1. Take the four signed installers from the four `-signed` artifacts,
   which "Sign the installers (release)" uploads once both platforms' GUI
   checks have passed. Each holds the installer, its `.sig` and
   `INSTALLER-SHA256SUMS`, the installer's SHA-256 as it was built:

   | Build artifact (unsigned, as built) | Signed artifact (take this one) |
   |---|---|
   | `gospel-getter-linux-appimage` | `gospel-getter-linux-appimage-signed` |
   | `gospel-getter-linux-deb` | `gospel-getter-linux-deb-signed` |
   | `gospel-getter-windows-nsis-installer` | `gospel-getter-windows-nsis-installer-signed` |
   | `gospel-getter-windows-msi-installer` | `gospel-getter-windows-msi-installer-signed` |

   Check each installer against its recorded SHA-256 before anything else:

   ```sh
   for a in gospel-getter-linux-appimage gospel-getter-linux-deb \
            gospel-getter-windows-nsis-installer gospel-getter-windows-msi-installer; do
       gh run download RUN_ID -n "$a-signed" -D "signed/$a"
       (cd "signed/$a" && sha256sum -c INSTALLER-SHA256SUMS)
   done
   ```

   The build artifacts stay in the run as they were built, without a
   `.sig`, so compare each signed installer with the one the GUI checks
   ran on (`gh run download RUN_ID -n gospel-getter-linux-deb`, and so
   on): the same SHA-256.

2. Rename both files of each pair to the stable names in the table under
   step 1 above.

3. Take the two standalone CLI binaries from the same run, from its
   `gospel-getter-cli-linux-x86_64` and `gospel-getter-cli-windows-x86_64`
   artifacts, and keep their names.

4. Write `SHA256SUMS`, covering the four installers and the two CLI
   binaries, by these names. Each installer's hash is the one in its
   `INSTALLER-SHA256SUMS`: renaming changes no byte.

5. Write the manifest and upload, as in steps 2 and 3 above.

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
the four build artifacts by their exact names, never a `-signed` one. This
script writes each installer's SHA-256 beside it (`INSTALLER-SHA256SUMS`),
signs each one as `cargo tauri build` would have (`cargo tauri signer sign
--app-version`, the version in `src-tauri/tauri.conf.json`), checks each
signature is for that version and by the key in `tauri.conf.json`, checks
each installer still has the SHA-256 it recorded (signing only adds the
`.sig`), and makes the update manifest from them in a scratch folder as a
dry run. It refuses an installer that was signed or hashed before, and a
`-signed` artifact. The job then uploads each installer, with its `.sig`
and `INSTALLER-SHA256SUMS`, as the build artifact's name with `-signed`
after it (the table above). The build's own artifacts are never replaced.

If the `sign` job fails, re-run it (**Re-run failed jobs**): no rebuild,
and no GUI checks again. The re-run downloads the build's artifacts as they
were built, signs them afresh and uploads all four `-signed` artifacts
again, replacing any that the failed attempt uploaded (`overwrite: true`,
on the `-signed` names only). Take the release's installers only once an
attempt of the job has succeeded.

A run dispatched on another branch, and a pull request from a `release/`
branch or with the `ci:full` label, rehearse this (the `sign-rehearsal`
job) with a throwaway key made for the run, then the bundle (below) on its
files. Everything the rehearsal uploads has a `rehearsal-` name
(`rehearsal-gospel-getter-linux-deb-signed`, `rehearsal-release-bundle`
and so on), which nothing a release takes has, and its signatures are by
a key the app doesn't trust. To try a change to the signing on a pull
request, add the label (`gh pr edit <number> --add-label ci:full`) rather
than dispatching a second run of the branch by hand: its own pull_request
run then rehearses (and runs the Windows GUI checks), and so does every
push to it after. The label never reaches the `sign` job.

```sh
cargo tauri signer generate --ci --write-keys rehearsal.key
python3 tools/release/sign_installers.py DIR \
    --private-key-path rehearsal.key --pubkey-file rehearsal.key.pub
```

The release's key and its password (`TAURI_SIGNING_PRIVATE_KEY`,
`TAURI_SIGNING_PRIVATE_KEY_PASSWORD`) are secrets of the `release`
environment only, since 2026-11-11: the repository has no secrets, so only
a job in that environment, deploying from `main`, can read them. The
`sign` job is the only one; the workflow names them nowhere else.

## `bundle_release.py`: the release's files

dist.yml's `bundle` job ("Assemble the release files") runs after the
`sign` job, on the same condition, with no environment, no secret and
`contents: read`. It downloads the four `-signed` artifacts, the four
build artifacts they were signed from, and the two CLI artifacts, each by
its exact name, and this script checks them and assembles the release's
files: each installer must have the SHA-256 its `INSTALLER-SHA256SUMS`
recorded and be byte for byte its build artifact, and its `.sig` must be
for the app's version and by the app's key. It refuses, writing nothing,
on a missing or unexpected file or any mismatch. The job uploads the
result, under the stable names with `SHA256SUMS`, as
`gospel-getter-release-bundle`, which the publish takes (step 1 above).

After the signing rehearsal the same job runs on the rehearsal's
artifacts, against its throwaway public key, and uploads
`rehearsal-release-bundle`, so every `ci:full` pull request tries the
bundle before a release does.

```sh
python3 tools/release/bundle_release.py DIR OUT [--pubkey-file KEY.pub]
```

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
