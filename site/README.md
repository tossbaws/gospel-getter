# Gospel Getter website

The product page for Gospel Getter: plain HTML and CSS, with no JavaScript,
framework, build step, web fonts, analytics or cookies.
`.github/workflows/pages.yml` publishes this folder to GitHub Pages on every
push to `main` that changes it.

The page is served first at `https://tossbaws.github.io/gospel-getter/` and
later at the root of a domain, so **every internal URL is relative**
(`styles.css`, `assets/…`, `#download`). Never start one with `/`.

## Preview locally

```bash
python3 -m http.server --directory site 8000
# then open http://localhost:8000/
```

Opening `site/index.html` straight from disk works too.

## Downloads and releases

The download links never change. They use GitHub's
`https://github.com/tossbaws/gospel-getter/releases/latest/download/<name>`
links, which always serve the newest release's file of that name, and
"What's new" / "Release notes" link to `releases/latest`. That works
because, from v2.4.0 on, every release's assets have the same names:

| File | What it is |
|---|---|
| `Gospel-Getter_x64-setup.exe` | Windows installer (NSIS, recommended) |
| `Gospel-Getter_x64_en-US.msi` | Windows MSI package |
| `Gospel-Getter_amd64.AppImage` | Linux AppImage |
| `Gospel-Getter_amd64.deb` | Debian/Ubuntu package |
| `SHA256SUMS` | Checksums of the four files above, by these names |

The Distribution Artifacts workflow builds versioned file names; they're
renamed to these when the release is published, without changing the
bytes. Releases before v2.4.0 keep their versioned names.

**On each release, only the displayed version changes:** replace
`Version X.Y.Z` in `index.html` with the new version, and run `npm test` in
`tests/frontend/`. `tests/frontend/site.test.mjs` enforces it: it reads the
version from `src-tauri/tauri.conf.json` and fails until every version the
page shows matches. It also fails if any of the five stable links is
missing, or if any download or release link names a version. CI runs it on
every pull request.

Publish the release (with the stable asset names) when you merge the
version bump: until a release has those names, the links 404.

## Where the assets come from

- `assets/icon.svg` and `assets/favicon.svg` are copies of
  `packaging/icon.svg`, and `assets/favicon.png` is a copy of
  `ui/favicon.png`. When the icon changes, copy them again.
- The screenshots are the real app (`ui/index.html`) showing the real
  bundled KJV and WEB text, in WebKitGTK, the engine the Linux app uses.
  `tools/site/capture_screenshots.py` loads the page offscreen and answers
  its Tauri calls with the app's real command code, through the frontend
  tests' bridge (`src-tauri/examples/frontend_bridge.rs`) and a throwaway
  database. It then uses the page's own controls, as the frontend tests do.
  Nothing is drawn over or added. `tools/site/build_images.sh` resizes and
  crops the captures into WebP, plus `og-image.jpg` for link previews. The
  scripts live outside `site/` so GitHub Pages doesn't publish them.

  ```bash
  cargo build --manifest-path src-tauri/Cargo.toml --example frontend_bridge
  python3 tools/site/capture_screenshots.py /tmp/gg-raw
  tools/site/build_images.sh /tmp/gg-raw
  ```

  You'll need PyGObject with WebKit2 4.1, a display (no window is shown)
  and ImageMagick 7 with WebP support. Check the alt text in `index.html`
  still describes each image after recapturing.

## When the site moves to its own domain

- Add a `CNAME` file here with the domain, and set the custom domain in the
  repository's Pages settings. Don't add it before DNS is set up: it breaks
  the github.io address.
- Change the absolute `og:image` URL in `index.html`. Open Graph needs a
  full URL, so it's the one non-relative link to this site.
