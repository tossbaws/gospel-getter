# Windows packaging

The hand-rolled NSIS installer that used to live here (`installer.nsi`,
`build.sh`, `open-gospel-getter.vbs`) is gone — it existed only to run the
old browser-wrapper server and open it in the default browser, and that
whole approach no longer applies now that Gospel Getter is a native Tauri
desktop app.

Tauri's own bundler produces the Windows installer directly from
`src-tauri/tauri.conf.json` (`"bundle": {"targets": "all", ...}`), the
same config used for the Linux AppImage/deb builds. On a Windows machine
(or a proper cross-compilation setup — this hasn't been attempted from
Linux), producing an installer is just:

```bash
cd src-tauri
cargo tauri build
```

which yields both an NSIS `.exe` and an MSI under
`src-tauri/target/release/bundle/`. The generated `icon.ico` in
`src-tauri/icons/` (made from `packaging/icon.svg` by
`packaging/generate-icons.sh`) is used automatically; there's nothing
Windows-specific left to hand-maintain here.

The released installers also carry `gospel-getter-cli.exe` beside the app. To
build that too, build it first and name it for its target, as the `windows` job
in `.github/workflows/dist.yml` does, and add
`--config tauri.sidecar.conf.json`.

**What CI checks on Windows.** The `windows` job in
`.github/workflows/dist.yml` builds both installers on a Windows runner for
every pull request and push to `main` (except a release-prep merge's push,
whose installers come from the run dispatched on it), and installs and
removes each silently: the setup.exe (`/S`) and the MSI (`msiexec /i` and
`/x`). It checks each one registers itself, puts `gospel-getter-cli.exe`
beside the app (which it runs), and leaves nothing registered once removed;
a build without the MSI fails. The `test-windows` job in
`.github/workflows/ci.yml` runs the Rust tests on Windows. Before a release
(a run dispatched by hand, or a pull request from a `release/` branch), the
`gui-windows` job installs the setup.exe and drives the installed app
through its own window with the GUI checks (`scripts/gui-check.sh`). What
CI doesn't cover is a person's desktop: SmartScreen's warning for an
installer that isn't code-signed, and Windows editions other than the
runner's Windows Server.
