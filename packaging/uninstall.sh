#!/usr/bin/env bash
# Reverses install.sh: removes the installed AppImage, launcher, and icon.
# Asks first whether to delete your reading data too, before anything is
# removed, so you can still stop and export it from the app. Only "y"
# deletes it: No, an empty answer and no answer at all (end of input) all
# keep it.
set -euo pipefail

APP_NAME="Gospel Getter"
APP_DATA_DIR="$HOME/.local/share/com.tossbaws.gospel-getter"

delete_data=no
if [[ -d "$APP_DATA_DIR" ]]; then
  echo "==> Your reading data"
  echo "$APP_DATA_DIR holds your notes, bookmarks, highlights,"
  echo "AI findings and reading place, not only the Bible text. Deleting it"
  echo "deletes all of them, for good."
  echo "To keep a copy, stop here (Ctrl+C: nothing has been removed yet), open"
  echo "$APP_NAME, choose ☰ → Export data… → Everything (backup), then run this again."
  printf '%s' "Delete your notes, bookmarks, highlights, AI findings and reading place (the whole folder)? [y/N] "
  reply=""
  if ! read -r reply; then
    # End of input: no answer, so No.
    reply=""
    echo
  fi
  if [[ "$reply" =~ ^[Yy]$ ]]; then
    delete_data=yes
  fi
fi

echo "==> Removing the app launcher and icon"
rm -f "$HOME/.local/share/applications/gospel-getter.desktop"
rm -f "$HOME/.local/share/icons/hicolor/512x512/apps/gospel-getter.png"
update-desktop-database "$HOME/.local/share/applications" >/dev/null 2>&1 || true

echo "==> Removing the installed AppImage"
rm -f "$HOME/.local/bin/gospel-getter.AppImage"

if [[ -d "$APP_DATA_DIR" ]]; then
  if [[ $delete_data == yes ]]; then
    rm -rf "$APP_DATA_DIR"
    echo "Deleted $APP_DATA_DIR and your reading data in it."
  else
    echo "Kept $APP_DATA_DIR: your reading data is still there."
  fi
fi

echo "$APP_NAME has been uninstalled."
