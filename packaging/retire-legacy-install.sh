#!/usr/bin/env bash
# One-time cleanup for anyone upgrading from the old systemd/browser-wrapper
# install of Gospel Getter (the one that ran a local web server at
# 127.0.0.1:3002 and opened it in a browser) to the new Tauri desktop app.
#
# Stops and removes the old background service, its browser-wrapper
# launcher, its icon, and the old server binary.
#
# Never touches ~/.local/share/gospel-getter/data/gospel_getter.db — the
# new app already copies it into its own app data directory the first time
# it runs (see the README's "Where your data lives" section). This script
# leaves the legacy database file exactly where it was, in case you want
# to keep it as a backup; delete it yourself once you've confirmed the new
# app has everything you expect.
#
# Only run this after you've built and verified the new Tauri app (e.g.
# via packaging/install.sh) — this script doesn't check for that itself.
set -euo pipefail

echo "==> Stopping and removing the old background service"
systemctl --user disable --now gospel-getter.service 2>/dev/null || true
rm -f "$HOME/.config/systemd/user/gospel-getter.service"
systemctl --user daemon-reload 2>/dev/null || true

echo "==> Removing the old browser-wrapper launcher and icon"
if command -v omarchy >/dev/null 2>&1; then
  omarchy webapp remove "Gospel Getter" 2>/dev/null || true
fi
rm -f "$HOME/.local/share/applications/Gospel Getter.desktop"
rm -f "$HOME/.local/share/icons/hicolor/256x256/apps/gospel-getter.png"
update-desktop-database "$HOME/.local/share/applications" >/dev/null 2>&1 || true

echo "==> Removing the old server binary"
rm -f "$HOME/.local/bin/gospel_getter"

echo
echo "The old browser-based install has been removed."
echo "Your Bible data at ~/.local/share/gospel-getter/data/gospel_getter.db was left in place."
echo "Install the new desktop app with: packaging/install.sh"
