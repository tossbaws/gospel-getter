# Gospel Getter website

The product page for Gospel Getter (`index.html`) and its full feature guide
(`guide.html`): plain HTML and CSS, with no JavaScript, framework, build step,
analytics or cookies. The fonts are self-hosted in `assets/fonts/`.
`.github/workflows/pages.yml` publishes this folder to GitHub Pages, but
only once the version it shows is a published release (see "Downloads and
releases").

The page is served at the root of `https://gospelgetter.chirhocode.com/`, but
**every internal URL is still relative** (`brand.css`, `assets/…`,
`#download`) so the page also works from a folder, as in a local preview.
Never start one with `/`.

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

The site deploys itself when the release is published, so the page
never shows a version before its downloads exist:

- The release-prep merge bumps the version; its push deploys nothing (the
  Website run says it's waiting for the release).
- Publishing the release starts the Website workflow on `main`, which
  waits until `releases/latest/download/` serves the new version, then
  deploys.
- A site-only fix pushed to `main` while its version is released deploys
  at once.

`tools/release/site_version_gate.py` makes the call (its tests run in CI);
**Run workflow** on the Website workflow, on `main`, deploys under the
same rule.

## Where the assets come from

- The icons (favicon, header) are the Chi Rho Code LLC mark; see
  "Brand" below.
- The screenshots are the real app (`ui/index.html`) showing the real
  bundled KJV, BSB and WEB text, in WebKitGTK, the engine the Linux app uses.
  The feature screenshots (hero, search, copy, bookmark and note, compare, and
  `og-image.jpg`) are in Chi Rho, the app's default theme, in a 1280×800
  window, where the book and chapter picker is a sidebar; the theme tiles on
  the guide page show every theme but System, which is Chi Rho or Vellum as
  the computer is set. The README's screenshots (`screenshots/`) come from
  the same script with `--readme`, at their final sizes.
  `tools/site/capture_screenshots.py` loads the page offscreen and answers
  its Tauri calls with the app's real command code, through the frontend
  tests' bridge (`src-tauri/examples/frontend_bridge.rs`) and a throwaway
  database. It then uses the page's own controls, as the frontend tests do.
  Nothing is drawn over or added. `tools/site/build_images.sh` resizes and
  crops the captures into WebP, plus `og-image.jpg` for link previews. The
  scripts live outside `site/` so GitHub Pages doesn't publish them.

  ```bash
  trunk build --release                                                       # the page, into dist/
  cargo build --manifest-path src-tauri/Cargo.toml --example frontend_bridge
  PYTHONDONTWRITEBYTECODE=1 python3 tools/site/capture_screenshots.py --all   # every image, in place
  ```

  `--all` captures the site's scenes into a temporary folder, runs
  `build_images.sh` on them and captures the README's into `screenshots/`.
  To capture some scenes, or look at the raw captures first:

  ```bash
  PYTHONDONTWRITEBYTECODE=1 python3 tools/site/capture_screenshots.py /tmp/gg-raw            # the site's, raw
  PYTHONDONTWRITEBYTECODE=1 python3 tools/site/capture_screenshots.py /tmp/gg-raw reading search compare select
  tools/site/build_images.sh /tmp/gg-raw
  PYTHONDONTWRITEBYTECODE=1 python3 tools/site/capture_screenshots.py --readme screenshots   # the README's, in place
  ```

  `--theme NAME` sets the feature screenshots' theme (default `chi-rho`);
  naming scenes captures only those, and `build_images.sh` leaves any
  theme tile it has no capture for as it is. Every capture marks the
  first-run welcome as seen and sets no highlights, and fails if either
  would be on screen.

  You'll need PyGObject with WebKit2 4.1, a display (no window is shown)
  and ImageMagick 7 with WebP support. Check the alt text in `index.html`
  still describes each image after recapturing.

## Brand: Chi Rho Code LLC

The Chi Rho Code LLC mark is the site's icon: the favicon (and Apple touch
icon) and the header beside "Gospel Getter", as the flat mark. It also
appears, large, in "About the maker" (the detailed mark). The byline under
"Gospel Getter" and the footer are text only. The
site's look follows the mark, a round stained-glass window with
circuit-trace lead lines. (The Gospel Getter app keeps its own icon, the
white Chi-Rho on `#14082c`, in `packaging/`; the website no longer uses it.)

The Chi-Rho, Alpha (Α) and Omega (Ω) are public-domain symbols. The Alpha
and Omega ornaments beside the About heading are text, hidden from
assistive technology.

**Assets in `assets/brand/`**, all derived locally: the WebP marks with
ImageMagick 7, the flat PNGs and `favicon.ico` with
`tools/site/build_brand_icons.py`. The brand source art isn't committed; the
sources below are paths in its `Brand_Attempts/` folder:

| File | Size | Source and derivation |
|---|---|---|
| `chirho-code-mark-240.webp`, `chirho-code-mark-480.webp` | 240 and 480 px (1x/2x), 22 KB and 60 KB | `Brand_Attempts/2026-10-05_brand_chi-rho_C2_circuit-reliquary_polish.png` (2048×2048, the final mark). Cropped to the window (1940×1940 at +54+64), the black corners made transparent with a circular mask (radius 967, blurred 2px), resized and saved as WebP with alpha (quality 86). Used large in About. |
| `chirho-code-flat-small-16.png`, `chirho-code-flat-small-32.png` | 16 and 32 px | `Brand_Attempts/favicon/2026-10-09_c2/2026-10-05_chi-rho_circuit_flat-small_{16,32}.png`. The flat small-size mark (an emboldened glyph, no rim): the 16 and 32 favicons. |
| `chirho-code-flat-48.png`, `chirho-code-flat-64.png` | 48 and 64 px | `Brand_Attempts/favicon/2026-10-09_c2/2026-10-05_chi-rho_circuit_flat_{48,64}.png`. The 48 and 64 favicons, and the header icon (48, 40 px shown). |
| `chirho-code-flat-180.png` | 180 px | `Brand_Attempts/favicon/2026-10-09_c2/2026-10-05_chi-rho_circuit_flat_180.png`. The Apple touch icon, and the header icon's 2x. |
| `favicon.ico` | 16, 32, 48, 64 | `Brand_Attempts/favicon/2026-10-09_c2/favicon.ico` (16 and 32 from the flat-small mark, 48 and 64 from the flat mark). |

The flat set is redrawn from the final mark (C2) by
`tools/site/build_brand_icons.py`, which writes the whole set and, with
`--site`, copies these files here (see
`favicon/2026-10-09_c2/2026-10-09_chi-rho_circuit_flat_c2.json`). It follows
the method of the earlier set drawn from C
(`favicon/2026-10-05_chi-rho_circuit_flat.json`). The detailed mark is for
64px and up; the flat-small one for 16 and 32.

## Look and stylesheets (design of 2026-10-31)

The page is built like the mark's stained-glass window: panes of flat colour
fitted edge to edge, with 8px gaps of "lead" (`#26241b`) between them, as on
the Chi Rho Code company site. Gospel Getter sits on the window's emerald
side.

- `brand.css` is a copy of `site/brand.css` in `tossbaws/chirhocode-site`:
  fonts, colours, the header bar, panes, buttons and the footer frame.
  Change both copies together (see "Updating brand.css").
- `gospel-getter.css` is this site's own layout, the theme demo and the
  guide page.

**Make it yours** is the page's one interactive piece: Psalm 23 (WEB, verses
1–6) in Literata, re-set in the app's themes, text sizes and layouts by radio
buttons and `:has()`, with no script. Its theme colours are copied from
`ui/index.html`, and its verses from `src-tauri/data/web.json`;
`tests/frontend/site.test.mjs` fails if either drifts. When a theme changes
or one is added, update `gospel-getter.css` and the radio buttons.

**Keep what matters** shows the selection card cropped from
`select-1600.webp` (640×820 at +660+66). Pointing at a note lights its part
of the card with a `clip-path` whose percentages are of the whole capture.
Re-measure them if that screenshot is recaptured.

**Fonts** (all SIL Open Font License 1.1, licence texts beside them):

| File | Face | Used for |
|---|---|---|
| `source-serif-4-opsz.woff2` | Source Serif 4, variable | Headings and large statements |
| `instrument-sans-wght.woff2` | Instrument Sans, variable | Body text, buttons, navigation |
| `literata-regular.woff2` | Literata 400 | The Psalm 23 sample, as in the app (copied from `ui/fonts/`) |

**Palette** (`brand.css`, `:root`), sampled from the mark:

| Token | Hex | Used for |
|---|---|---|
| `--lead` | `#26241b` | Page background, showing between the panes |
| `--black`, `--night` | `#0b0c0a`, `#121410` | Most panes |
| `--emerald-deep`, `--emerald` | `#0c5734`, `#0e704f` | Gospel Getter's panes |
| `--teal-deep` | `#0b5c5f` | "Made for your computer", Linux download |
| `--gold`, `--gold-glow` | `#f1cb75`, `#fcec9c` | The primary button, chosen controls, the open question, list markers, focus |
| `--ink`, `--ink-soft` | `#efeeea`, `#c9c6ba` | Text and secondary text |
| `--ink-dark`, `--ink-dark-soft` | `#0b0c0a`, `#1d2a24` | Text on gold |

Every text colour meets WCAG AA (4.5:1) on every background it's used on;
the lowest pair is `--ink` on `--emerald`, 5.24:1. Recheck this when
changing a colour.

### Updating brand.css

Everything after `brand.css`'s header comment is the same, byte for byte, as
`site/brand.css` in `tossbaws/chirhocode-site`. The header names the commit it
was copied from, and pins the sha256 of every line after the comment.
`tests/frontend/site.test.mjs` fails if the file no longer matches that pin,
so an edit made only here can't merge.

To change it, make the change in chirhocode-site first and push it. Then,
from this repository's root, with chirhocode-site checked out at
`../chirhocode-site`:

```bash
CHIRHO=../chirhocode-site
C=$(git -C "$CHIRHO" rev-parse HEAD)   # or the branch or commit to copy
# This copy's header, then chirhocode-site's file after its own header.
{
  awk '{ print } /\*\// { exit }' site/brand.css
  git -C "$CHIRHO" show "$C:site/brand.css" | awk 'body; /\*\// { body = 1 }'
} > site/brand.css.new && mv site/brand.css.new site/brand.css
# The new pin, written into the header with the commit.
H=$(awk 'body; /\*\// { body = 1 }' site/brand.css | sha256sum | cut -d ' ' -f 1)
sed -i \
  -e "s/^\(   Source: .*, commit \)[0-9a-f]\{40\}\$/\1$C/" \
  -e "s/^\(   sha256 of the lines after this comment: \)[0-9a-f]\{64\}\$/\1$H/" \
  site/brand.css
git diff site/brand.css                # only the change you meant, and the two header lines
(cd tests/frontend && node --test site.test.mjs)
```

## When the site moves to its own domain

- Add a `CNAME` file here with the domain, and set the custom domain in the
  repository's Pages settings. Don't add it before DNS is set up: it breaks
  the github.io address.
- Change the absolute `og:image` URL in `index.html`. Open Graph needs a
  full URL, so it's the one non-relative link to this site.
