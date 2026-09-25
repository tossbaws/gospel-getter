#!/usr/bin/env bash
# Reverses install.sh: removes the installed AppImage, launcher, and icon.
# Prompts before touching your reading data.
set -euo pipefail

APP_NAME="Gospel Getter"
APP_DATA_DIR="$HOME/.local/share/com.tossbaws.gospel-getter"

echo "==> Removing the app launcher and icon"
rm -f "$HOME/.local/share/applications/gospel-getter.desktop"
rm -f "$HOME/.local/share/icons/hicolor/512x512/apps/gospel-getter.png"
update-desktop-database "$HOME/.local/share/applications" >/dev/null 2>&1 || true

echo "==> Removing the installed AppImage"
rm -f "$HOME/.local/bin/gospel-getter.AppImage"

if [[ -d "$APP_DATA_DIR" ]]; then
  read -r -p "Also delete your local Bible database at $APP_DATA_DIR? [y/N] " reply
  if [[ "$reply" =~ ^[Yy]$ ]]; then
    rm -rf "$APP_DATA_DIR"
    echo "Data removed."
  else
    echo "Left $APP_DATA_DIR in place."
  fi
fi

echo "$APP_NAME has been uninstalled."
