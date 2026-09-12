#!/usr/bin/env bash
# Reverses install.sh: stops and removes the service, the launcher/icon, and
# the installed binary. Prompts before touching your reading data.
set -euo pipefail

APP_NAME="Gospel Getter"

echo "==> Stopping and removing the service"
systemctl --user disable --now gospel-getter.service 2>/dev/null || true
rm -f "$HOME/.config/systemd/user/gospel-getter.service"
systemctl --user daemon-reload 2>/dev/null || true

echo "==> Removing the launcher/icon"
if command -v omarchy >/dev/null 2>&1; then
  omarchy webapp remove "$APP_NAME" 2>/dev/null || true
else
  rm -f "$HOME/.local/share/applications/gospel-getter.desktop"
  rm -f "$HOME/.local/share/icons/hicolor/512x512/apps/gospel-getter.png"
  update-desktop-database "$HOME/.local/share/applications" >/dev/null 2>&1 || true
fi

echo "==> Removing the installed binary"
rm -f "$HOME/.local/bin/gospel_getter"

if [[ -d "$HOME/.local/share/gospel-getter" ]]; then
  read -r -p "Also delete your local Bible database at ~/.local/share/gospel-getter? [y/N] " reply
  if [[ "$reply" =~ ^[Yy]$ ]]; then
    rm -rf "$HOME/.local/share/gospel-getter"
    echo "Data removed."
  else
    echo "Left ~/.local/share/gospel-getter in place."
  fi
fi

echo "$APP_NAME has been uninstalled."
