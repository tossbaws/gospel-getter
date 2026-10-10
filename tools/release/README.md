# Release tools

## `make_latest_json.py`: the update manifest

The app checks
`https://github.com/tossbaws/gospel-getter/releases/latest/download/latest.json`
for updates (`plugins.updater` in `src-tauri/tauri.conf.json`), so every
release must have a `latest.json` asset. Without one, release installers
can't see the release as an update.

1. Take the four installers from the verified Distribution Artifacts run on
   the release commit. Each artifact also holds the installer's `.sig`
   updater signature. Rename both files of each pair to the stable names:

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

3. Upload the installers, their `.sig` files, `latest.json` and
   `SHA256SUMS` to the `vX.Y.Z` release. The manifest's URLs point at that
   tag (`releases/download/vX.Y.Z/...`), so it names exactly these files.

Each installer has its own platform key, which the updater plugin looks up
for the bundle the running copy was installed from. A `.deb` install is only
ever offered the `.deb`, an MSI install the MSI, and so on. The app also
requires the version inside each signature to match the manifest's
(`requireSignedVersion`), which stops an old, validly signed build from
being passed off as a new one.

Tests: `python3 -m unittest discover -s tools/release` (run in CI).
