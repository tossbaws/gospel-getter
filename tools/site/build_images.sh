#!/usr/bin/env bash
# Turns the raw 2x captures from capture_screenshots.py into the site's
# images in site/assets/: resized and cropped only, never painted on.
# Needs ImageMagick 7 (`magick`) with WebP support.
#
# Usage, from the repository root:
#     tools/site/build_images.sh RAW_DIR
set -euo pipefail

raw=${1:?usage: build_images.sh RAW_DIR}
out=$(dirname "$0")/../../site/assets
mkdir -p "$out"

webp() { magick "$1" "${@:3}" -strip -quality 82 -define webp:method=6 "$2"; }

# Full-window screenshots (1280x800 CSS px), at 1600 and 800 px wide.
for name in compare select; do
    webp "$raw/$name.png" "$out/$name-1600.webp" -resize 1600x
    webp "$raw/$name.png" "$out/$name-800.webp" -resize 800x
done

# Search: the search panel, with a little of the page behind it.
webp "$raw/search.png" "$out/search-1440.webp" -crop 1440x1300+560+0 +repage
webp "$raw/search.png" "$out/search-720.webp" -crop 1440x1300+560+0 +repage -resize 720x

# Hero: the reading view cropped to John 3, with the edges of the faded
# neighbouring chapters (John 2 and John 4) on either side, right of the
# picker's sidebar and below the top bar.
webp "$raw/reading.png" "$out/hero-1400.webp" -crop 1760x1350+686+130 +repage -resize 1400x
webp "$raw/reading.png" "$out/hero-700.webp" -crop 1760x1350+686+130 +repage -resize 700x

# Themes: the centre column's heading and first verses of Psalm 23, below
# the app's top bar, between the neighbouring chapters (the whole column,
# at its Normal width, with none of theirs). A theme that wasn't
# captured this time keeps its existing tile.
for theme in chi-rho vellum vaporwave classic-dark classic-light matrix beast-slayer hot-pink; do
    [ -f "$raw/theme-$theme.png" ] || continue
    webp "$raw/theme-$theme.png" "$out/theme-$theme.webp" -crop 1020x616+1056+137 +repage -resize 720x
done

# Open Graph / social preview: 1200x630 from the reading view, John 3
# centred.
magick "$raw/reading.png" -resize 1600x -crop 1200x630+379+68 +repage -strip -quality 85 "$out/og-image.jpg"
