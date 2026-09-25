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
(cd "$SRC_TAURI" && APPIMAGE_EXTRACT_AND_RUN=1 NO_STRIP=1 cargo tauri build --bundles appimage)

APPIMAGE="$(find "$SRC_TAURI/target/release/bundle/appimage" -maxdepth 1 -name '*.AppImage' | head -n1)"
if [[ -z "$APPIMAGE" ]]; then
  echo "error: build finished but no .AppImage was found under target/release/bundle/appimage" >&2
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
