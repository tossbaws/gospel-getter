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

## On each release

The download links go straight to one release's files, because GitHub's
`releases/latest/download/` links need file names without the version in
them. After bumping the version:

1. In `index.html`, replace the old version everywhere it appears (the
   links, the file names and the version text) with the new one.
2. Update the file sizes next to the download buttons, from the release's
   assets.
3. Run `npm test` in `tests/frontend/`.

`tests/frontend/site.test.mjs` reads the version from
`src-tauri/tauri.conf.json` and fails until every download link and every
version shown on the page match it, and all five files (the `.exe`, `.msi`,
`.AppImage`, `.deb` and `SHA256SUMS`) are linked. CI runs it on every pull
request, so a version bump can't merge with a stale site.

Publish the release before merging the site update, or the links will
404 until you do.

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
