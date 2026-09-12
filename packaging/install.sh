#!/usr/bin/env bash
# Installs Gospel Getter as a background service with a real taskbar icon:
# builds the binary, installs a systemd --user service so it's always
# running, and creates a desktop launcher (via `omarchy webapp install` on
# Omarchy, or a plain .desktop file elsewhere).
set -euo pipefail

APP_NAME="Gospel Getter"
BIN_NAME="gospel_getter"
BIN_DIR="$HOME/.local/bin"
DATA_DIR="$HOME/.local/share/gospel-getter"
SYSTEMD_USER_DIR="$HOME/.config/systemd/user"
ICON_DIR="$HOME/.local/share/icons/hicolor/512x512/apps"
DESKTOP_DIR="$HOME/.local/share/applications"
URL="http://127.0.0.1:3002"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

echo "==> Building $APP_NAME (release)"
if ! command -v cargo >/dev/null 2>&1; then
  echo "error: cargo (Rust) not found." >&2
  if command -v omarchy >/dev/null 2>&1; then
    echo "Install it with: omarchy install dev-env rust" >&2
  else
    echo "Install Rust from https://rustup.rs, then re-run this script." >&2
  fi
  exit 1
fi
(cd "$REPO_ROOT" && cargo build --release)

echo "==> Installing binary to $BIN_DIR"
mkdir -p "$BIN_DIR"
install -m 755 "$REPO_ROOT/target/release/$BIN_NAME" "$BIN_DIR/$BIN_NAME"

# WorkingDirectory in the unit must exist before systemd will start it; the
# app creates data/ and the SQLite file itself on first run.
mkdir -p "$DATA_DIR"

echo "==> Installing systemd user service"
mkdir -p "$SYSTEMD_USER_DIR"
cp "$REPO_ROOT/packaging/gospel-getter.service" "$SYSTEMD_USER_DIR/gospel-getter.service"
systemctl --user daemon-reload
systemctl --user enable gospel-getter.service
# `restart`, not `start`: this script doubles as the update path (re-run it
# after pulling in code changes), and `start` is a no-op on an
# already-running service — it would leave the OLD binary running even
# though a new one just got copied over it. `restart` always picks up
# whatever is on disk now.
systemctl --user restart gospel-getter.service

echo "==> Waiting for the server to come up"
ready=false
for _ in $(seq 1 40); do
  if curl -fs -o /dev/null "$URL/"; then
    ready=true
    break
  fi
  sleep 0.5
done
if [[ "$ready" != true ]]; then
  echo "warning: server didn't respond after 20s — check: systemctl --user status gospel-getter" >&2
fi

echo "==> Creating the app launcher"
if command -v omarchy >/dev/null 2>&1; then
  omarchy webapp install "$APP_NAME" "$URL" "$REPO_ROOT/packaging/icon.png"
else
  mkdir -p "$ICON_DIR" "$DESKTOP_DIR"
  install -m 644 "$REPO_ROOT/packaging/icon.png" "$ICON_DIR/gospel-getter.png"

  browser_exec=""
  for candidate in chromium google-chrome-stable google-chrome brave brave-browser; do
    if command -v "$candidate" >/dev/null 2>&1; then
      browser_exec="$candidate --app=$URL --class=GospelGetter"
      break
    fi
  done
  exec_line="${browser_exec:-xdg-open $URL}"

  cat > "$DESKTOP_DIR/gospel-getter.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=$APP_NAME
Comment=Read the Bible, book by book, chapter by chapter
Icon=gospel-getter
Exec=$exec_line
Terminal=false
Categories=Education;Spirituality;
EOF

  update-desktop-database "$DESKTOP_DIR" >/dev/null 2>&1 || true
  gtk-update-icon-cache "$HOME/.local/share/icons/hicolor" >/dev/null 2>&1 || true
fi

echo
echo "$APP_NAME is installed and running at $URL"
echo "Look for it in your application launcher. To remove it: packaging/uninstall.sh"
