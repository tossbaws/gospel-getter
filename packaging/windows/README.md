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
`src-tauri/target/release/bundle/`. The generated `gospel-getter.ico` in
`src-tauri/icons/` is used automatically; there's nothing Windows-specific
left to hand-maintain here.

**Not verified as part of the Tauri migration** — that work happened on
Linux, and Linux artifact verification (the AppImage) was the hard
requirement. A Windows build should be tried on an actual Windows machine
before it's relied on.
