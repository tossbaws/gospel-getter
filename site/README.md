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

- The icons (favicon, header, hero) are the Chi Rho Code LLC mark; see
  "Brand" below.
- The screenshots are the real app (`ui/index.html`) showing the real
  bundled KJV and WEB text, in WebKitGTK, the engine the Linux app uses.
  The feature screenshots (hero, search, copy and bookmark, compare, and
  `og-image.jpg`) are in the Matrix theme; the "Make it yours" strip shows
  all six themes.
  `tools/site/capture_screenshots.py` loads the page offscreen and answers
  its Tauri calls with the app's real command code, through the frontend
  tests' bridge (`src-tauri/examples/frontend_bridge.rs`) and a throwaway
  database. It then uses the page's own controls, as the frontend tests do.
  Nothing is drawn over or added. `tools/site/build_images.sh` resizes and
  crops the captures into WebP, plus `og-image.jpg` for link previews. The
  scripts live outside `site/` so GitHub Pages doesn't publish them.

  ```bash
  cargo build --manifest-path src-tauri/Cargo.toml --example frontend_bridge
  python3 tools/site/capture_screenshots.py /tmp/gg-raw            # everything
  python3 tools/site/capture_screenshots.py /tmp/gg-raw reading search compare select
  tools/site/build_images.sh /tmp/gg-raw
  ```

  `--theme NAME` sets the feature screenshots' theme (default `matrix`);
  naming scenes captures only those, and `build_images.sh` leaves any
  theme tile it has no capture for as it is. Every capture marks the
  first-run welcome as seen and sets no highlights, and fails if either
  would be on screen.

  You'll need PyGObject with WebKit2 4.1, a display (no window is shown)
  and ImageMagick 7 with WebP support. Check the alt text in `index.html`
  still describes each image after recapturing.

## Brand: Chi Rho Code LLC

The Chi Rho Code LLC mark is the site's icon: the favicon (and Apple touch
icon), the header beside "Gospel Getter", and the small icon above the
headline, all as the flat mark. It also appears in the footer (flat) and,
large, in "About the maker" (the detailed mark). The byline under "Gospel
Getter" is text only, since the header's icon is already the mark. The
site's look follows the mark, a round stained-glass window with
circuit-trace lead lines. (The Gospel Getter app keeps its own icon, the
white Chi-Rho on `#14082c`, in `packaging/`; the website no longer uses it.)

The Chi-Rho, Alpha (Α) and Omega (Ω) are public-domain symbols. The Alpha
and Omega ornaments in About and the footer are text, hidden from
assistive technology.

**Assets in `assets/brand/`**, all derived locally with ImageMagick 7 (the
2048px original isn't committed):

| File | Size | Source and derivation |
|---|---|---|
| `chirho-code-mark-240.webp`, `chirho-code-mark-480.webp` | 240 and 480 px (1x/2x), 22 KB and 60 KB | `/home/m4sk/Generated_Art/Brand_Attempts/2026-10-05_brand_chi-rho_C2_circuit-reliquary_polish.png` (2048×2048, the final mark). Cropped to the window (1940×1940 at +54+64), the black corners made transparent with a circular mask (radius 967, blurred 2px), resized and saved as WebP with alpha (quality 86). Used large in About. |
| `chirho-code-flat-small-16.png`, `chirho-code-flat-small-32.png` | 16 and 32 px | `/home/m4sk/Generated_Art/Brand_Attempts/favicon/2026-10-05_chi-rho_circuit_flat-small_{16,32}.png`, re-saved without metadata. The flat small-size mark (an emboldened glyph, no rim): the 16 and 32 favicons, the hero icon (32, 28 px shown) and the footer (32). |
| `chirho-code-flat-48.png`, `chirho-code-flat-64.png` | 48 and 64 px | `/home/m4sk/Generated_Art/Brand_Attempts/favicon/2026-10-05_chi-rho_circuit_flat_{48,64}.png`, re-saved without metadata. The 48 and 64 favicons, the header icon (48, 40 px shown), and the 2x of the hero icon and the footer. |
| `chirho-code-flat-180.png` | 180 px | `/home/m4sk/Generated_Art/Brand_Attempts/favicon/2026-10-05_chi-rho_circuit_flat_180.png`, re-saved without metadata. The Apple touch icon, and the header icon's 2x. |
| `favicon.ico` | 16, 32, 48, 64 | `/home/m4sk/Generated_Art/Brand_Attempts/favicon/favicon.ico`, copied unchanged (16 and 32 from the flat-small mark, 48 and 64 from the flat mark). |

The flat set was drawn from an earlier version of the same mark (see
`favicon/2026-10-05_chi-rho_circuit_flat.json` beside it). The detailed
mark is for 64px and up; the flat-small one for 16 and 32.

**Palette** (`styles.css`, `:root`):

| Token | Hex | Used for |
|---|---|---|
| `--bg` | `#0b0d0c` | Page background (the mark's near-black) |
| `--bg-raised` | `#101613` | Alternate sections, header |
| `--pane` | `#131c18` | Glass panes (download panels, columns, notes), with a faint emerald-to-teal wash |
| `--footer-bg` | `#070908` | Footer |
| `--text` | `#ede7d8` | Body text |
| `--text-muted` | `#bdb6a5` | Secondary text (lede, notes, captions' context) |
| `--gold` | `#f2c14e` | Primary buttons, eyebrow, focus rings, lead-line nodes, list markers, caution edge |
| `--gold-pale` | `#f8e59f` | Headings, footer and caution links, the primary button's highlight |
| `--on-gold` | `#1a140e` | Text on gold buttons |
| `--emerald` | `#16785a` | Glass washes, the screenshots' outer frame |
| `--teal` | `#42b8a5` | Secondary button edges, middle lead-line nodes, Alpha and Omega |
| `--link` | `#86dccd` | Links and secondary button text |
| `--caution-bg` | `#2a210b` | The SmartScreen warning pane |

Every text colour meets WCAG AA (4.5:1) on every background it's used on,
the gradient washes included; the lowest pair is `--text-muted` on a pane's
emerald wash, 7.44:1. Recheck this when changing a colour.

## When the site moves to its own domain

- Add a `CNAME` file here with the domain, and set the custom domain in the
  repository's Pages settings. Don't add it before DNS is set up: it breaks
  the github.io address.
- Change the absolute `og:image` URL in `index.html`. Open Graph needs a
  full URL, so it's the one non-relative link to this site.
