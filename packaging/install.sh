#!/usr/bin/env bash
# Builds Gospel Getter as a Tauri AppImage and installs it as a normal
# desktop app: an executable in ~/.local/bin, an icon, and a .desktop
# launcher. There's no background service and nothing listens on a
# port — the app only runs while its window is open, same as any other
# native desktop app.
set -euo pipefail

APP_NAME="Gospel Getter"
BIN_DIR="$HOME/.local/bin"
ICON_DIR="$HOME/.local/share/icons/hicolor/512x512/apps"
DESKTOP_DIR="$HOME/.local/share/applications"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SRC_TAURI="$REPO_ROOT/src-tauri"
INSTALLED_APPIMAGE="$BIN_DIR/gospel-getter.AppImage"
BUNDLE_DIR="$SRC_TAURI/target/release/bundle/appimage"

echo "==> Building $APP_NAME (Tauri AppImage)"
if ! command -v cargo >/dev/null 2>&1; then
  echo "error: cargo (Rust) not found." >&2
  if command -v omarchy >/dev/null 2>&1; then
    echo "Install it with: omarchy install dev-env rust" >&2
  else
    echo "Install Rust from https://rustup.rs, then re-run this script." >&2
  fi
  exit 1
fi
if ! (cd "$SRC_TAURI" && cargo tauri --version >/dev/null 2>&1); then
  echo "error: the Tauri CLI isn't installed." >&2
  echo "Install it with: cargo install tauri-cli --version \"^2.0.0\" --locked" >&2
  exit 1
fi

# Two workarounds needed on a system with only fuse3 installed (no legacy
# fuse2) and a very new binutils, e.g. current Arch:
#   - linuxdeploy (which the AppImage bundler shells out to) is itself an
#     AppImage and can't FUSE-mount itself to run; extracting and running
#     directly sidesteps that without requiring a fuse2 install.
#   - linuxdeploy's own bundled `strip` doesn't understand the newer
#     `.relr.dyn` ELF section some system libraries are now built with,
#     and aborts on it; NO_STRIP skips stripping instead (the binary is
#     already stripped by cargo/rustc, so this doesn't leave debug info
#     behind, just the still-unstripped system libraries bundled from
#     /usr/lib).
#
# The bundle folder can keep AppImages from older builds, so which one to
# install is decided by identity, not by whichever `find` lists first: the
# one AppImage written after this build started, carrying the version in
# src-tauri/tauri.conf.json. Anything else stops here with what's there, and
# nothing in the folder is deleted.
VERSION="$(grep -m1 -oE '"version"[[:space:]]*:[[:space:]]*"[^"]+"' "$SRC_TAURI/tauri.conf.json" |
  sed -E 's/.*"([^"]+)"$/\1/' || true)"
if [[ ! "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+ ]]; then
  echo "error: couldn't read the app's version from src-tauri/tauri.conf.json" >&2
  exit 1
fi
BUILD_STARTED="$(mktemp)"
trap 'rm -f "$BUILD_STARTED"' EXIT
(cd "$SRC_TAURI" && APPIMAGE_EXTRACT_AND_RUN=1 NO_STRIP=1 cargo tauri build --bundles appimage)

# Prints the AppImages in the bundle folder, one per line: those written
# after the build started with `-newer "$BUILD_STARTED"`, else all of them.
appimages() {
  [[ -d "$BUNDLE_DIR" ]] || return 0
  find "$BUNDLE_DIR" -maxdepth 1 -type f -name '*.AppImage' "$@" | sort
}
mapfile -t BUILT < <(appimages -newer "$BUILD_STARTED")
if ((${#BUILT[@]} == 0)); then
  echo "error: the build finished but wrote no .AppImage to $BUNDLE_DIR" >&2
  mapfile -t OLDER < <(appimages)
  if ((${#OLDER[@]} > 0)); then
    echo "These are from earlier builds, so none was installed:" >&2
    printf '  %s\n' "${OLDER[@]##*/}" >&2
  fi
  exit 1
fi
if ((${#BUILT[@]} > 1)); then
  echo "error: the build wrote more than one .AppImage to $BUNDLE_DIR, so which to install is ambiguous:" >&2
  printf '  %s\n' "${BUILT[@]##*/}" >&2
  echo "Nothing was installed." >&2
  exit 1
fi
APPIMAGE="${BUILT[0]}"
if [[ "$(basename "$APPIMAGE")" != *"_${VERSION}_"*.AppImage ]]; then
  echo "error: the build wrote $(basename "$APPIMAGE"), which isn't version $VERSION (src-tauri/tauri.conf.json), so it wasn't installed" >&2
  exit 1
fi

echo "==> Installing $(basename "$APPIMAGE") to $INSTALLED_APPIMAGE"
mkdir -p "$BIN_DIR"
install -m 755 "$APPIMAGE" "$INSTALLED_APPIMAGE"

echo "==> Installing the icon and app launcher"
mkdir -p "$ICON_DIR" "$DESKTOP_DIR"
install -m 644 "$REPO_ROOT/packaging/icon.png" "$ICON_DIR/gospel-getter.png"

cat > "$DESKTOP_DIR/gospel-getter.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=$APP_NAME
Comment=Read the Bible, book by book, chapter by chapter
Icon=gospel-getter
Exec=$INSTALLED_APPIMAGE
Terminal=false
Categories=Education;Spirituality;
EOF

update-desktop-database "$DESKTOP_DIR" >/dev/null 2>&1 || true
gtk-update-icon-cache "$HOME/.local/share/icons/hicolor" >/dev/null 2>&1 || true

echo
echo "$APP_NAME is installed. Look for it in your application launcher,"
echo "or run it directly: $INSTALLED_APPIMAGE"
echo
echo "It's a normal desktop app — nothing runs in the background, and"
echo "nothing listens on a network port, until you actually open it."
echo
echo "Upgrading from the old browser-based version? Its background service"
echo "and launcher are still in place until you run:"
echo "  packaging/retire-legacy-install.sh"
echo
echo "To remove this app: packaging/uninstall.sh"
