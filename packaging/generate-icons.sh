#!/usr/bin/env bash
# Regenerates every shipped icon from the one canonical source,
# packaging/icon.svg. Rerun this after editing that file, then commit the
# results together; tests/frontend/icons.test.mjs fails if any of them
# drift from it (a stale colour, size or favicon).
#
# Needs: rsvg-convert (librsvg) and the Tauri CLI (`cargo install
# tauri-cli`).
#
# Outputs:
#   packaging/icon.png           512x512, installed by install.sh
#   src-tauri/icons/*.png        the Tauri set listed in tauri.conf.json,
#                                plus 64x64.png and icon.png
#   src-tauri/icons/icon.ico     Windows (16-256 px)
#   src-tauri/icons/icon.icns    macOS
#   ui/favicon.svg, favicon.png  the webview's favicon (64x64 PNG)
#   packaging/icon.svg.sha256    the source's hash, so the test can tell
#                                when icon.svg changed but this didn't run

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SRC="$REPO_ROOT/packaging/icon.svg"
TAURI_ICONS="$REPO_ROOT/src-tauri/icons"

for tool in rsvg-convert cargo; do
    command -v "$tool" >/dev/null || { echo "missing $tool" >&2; exit 1; }
done

rsvg-convert -w 512 -h 512 "$SRC" -o "$REPO_ROOT/packaging/icon.png"

cp "$SRC" "$REPO_ROOT/ui/favicon.svg"
rsvg-convert -w 64 -h 64 "$SRC" -o "$REPO_ROOT/ui/favicon.png"

# `tauri icon` also writes Android, iOS and Windows Store icons; only the
# files this desktop app actually ships are copied back. (It writes
# icon.icns's entries in no fixed order, so a rerun can change that file's
# bytes without changing any image in it.)
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
if ! (cd "$REPO_ROOT/src-tauri" && cargo tauri icon "$SRC" --output "$tmp") >"$tmp/log" 2>&1; then
    cat "$tmp/log" >&2
    exit 1
fi
for f in 32x32.png 64x64.png 128x128.png 128x128@2x.png icon.png icon.ico icon.icns; do
    cp "$tmp/$f" "$TAURI_ICONS/$f"
done

(cd "$REPO_ROOT/packaging" && sha256sum icon.svg > icon.svg.sha256)

echo "Regenerated icons from $SRC"
