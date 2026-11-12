#!/usr/bin/env bash
# Tests which AppImage packaging/install.sh installs, without building
# anything: each case runs a copy of the script in a throwaway copy of the
# files it reads (itself, the icon, src-tauri/tauri.conf.json), with a
# stand-in `cargo` whose `cargo tauri build` writes the AppImages the case
# names into the bundle folder, as the bundler would, and a throwaway HOME.
# The bundle folder can already hold an older build's AppImage, which must
# never be the one installed, and must be left where it was.
#
# Then packaging/uninstall.sh, in a throwaway HOME (never yours) with an
# install and a reading-data folder of its own: it names the reading data
# it would delete, and keeps it unless the answer is "y" (No, an empty
# answer, and the end of input all keep it), finishing normally each time.
#
#   scripts/test-install.sh
#
# Plain bash, no test framework; exit code 0 only if every case passed.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION="$(grep -m1 -oE '"version"[[:space:]]*:[[:space:]]*"[^"]+"' "$REPO_ROOT/src-tauri/tauri.conf.json" |
  sed -E 's/.*"([^"]+)"$/\1/')"
CURRENT="Gospel Getter_${VERSION}_amd64.AppImage"
# Sorts first, and is written first: what `find | head -n1` used to pick.
OLDER="Gospel Getter_2.2.0_amd64.AppImage"

WORK="$(mktemp -d "${TMPDIR:-/tmp}/gg-test-install.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT

failures=0
cases=0
fail() {
  echo "FAIL $CASE: $*" >&2
  failures=$((failures + 1))
}

# A fresh world for one case: $1 names it, the rest are the AppImages that
# `cargo tauri build` writes (none at all with no more arguments).
setup() {
  CASE="$1"
  cases=$((cases + 1))
  shift
  local root="$WORK/$CASE"
  ROOT="$root"
  mkdir -p "$root/repo/packaging" "$root/repo/src-tauri" "$root/home" "$root/bin"
  cp "$REPO_ROOT/packaging/install.sh" "$REPO_ROOT/packaging/icon.png" "$root/repo/packaging/"
  cp "$REPO_ROOT/src-tauri/tauri.conf.json" "$root/repo/src-tauri/"
  BUNDLE="$root/repo/src-tauri/target/release/bundle/appimage"
  mkdir -p "$BUNDLE"
  printf '%s\n' "$@" >"$root/built.txt"

  cat >"$root/bin/cargo" <<'EOF'
#!/usr/bin/env bash
# Stands in for cargo: `cargo tauri --version` answers, and `cargo tauri
# build` (run from src-tauri/) writes each AppImage named in built.txt.
set -euo pipefail
case "$*" in
  "tauri --version") echo "tauri-cli 2.0.0 (stand-in)" ;;
  "tauri build --bundles appimage")
    while IFS= read -r name; do
      [[ -n "$name" ]] || continue
      printf 'built: %s\n' "$name" >"target/release/bundle/appimage/$name"
    done <"$FAKE_BUILT"
    ;;
  *) echo "stand-in cargo: unexpected arguments: $*" >&2; exit 2 ;;
esac
EOF
  # The desktop's caches aren't this test's business.
  printf '#!/bin/sh\nexit 0\n' >"$root/bin/update-desktop-database"
  printf '#!/bin/sh\nexit 0\n' >"$root/bin/gtk-update-icon-cache"
  chmod +x "$root/bin/"*
}

# An older build's AppImage, written before this build starts.
older_build() {
  printf 'older build\n' >"$BUNDLE/$OLDER"
  touch -d '2020-01-01 00:00:00' "$BUNDLE/$OLDER"
}

# Runs the copy of install.sh; its exit code is in STATUS, its stderr in ERR.
run_install() {
  set +e
  env -i PATH="$ROOT/bin:/usr/bin:/bin" HOME="$ROOT/home" TMPDIR="$ROOT" \
    FAKE_BUILT="$ROOT/built.txt" \
    bash "$ROOT/repo/packaging/install.sh" >"$ROOT/out.txt" 2>"$ROOT/err.txt"
  STATUS=$?
  set -e
  ERR="$(cat "$ROOT/err.txt")"
}

installed() {
  echo "$ROOT/home/.local/bin/gospel-getter.AppImage"
}

# The older AppImage is still there, as it was.
older_kept() {
  [[ "$(cat "$BUNDLE/$OLDER" 2>/dev/null)" == "older build" ]] || fail "the older AppImage was changed or removed"
}

# 1. An older AppImage beside the one just built: the one just built.
setup just-built "$CURRENT"
older_build
run_install
if ((STATUS != 0)); then
  fail "exited $STATUS: $ERR"
elif [[ "$(cat "$(installed)")" != "built: $CURRENT" ]]; then
  fail "installed '$(cat "$(installed)")', not $CURRENT"
fi
older_kept

# 2. The build wrote two: ambiguous, so nothing is installed.
setup ambiguous "$CURRENT" "Gospel Getter_${VERSION}_x86_64.AppImage"
older_build
run_install
((STATUS != 0)) || fail "installed one of two AppImages just built"
[[ "$ERR" == *"more than one .AppImage"*"ambiguous"* ]] || fail "unclear error: $ERR"
[[ "$ERR" == *"$CURRENT"* && "$ERR" == *"_x86_64.AppImage"* ]] || fail "the error doesn't name both: $ERR"
[[ "$ERR" != *"$OLDER"* ]] || fail "the error names the older build as just built: $ERR"
[[ ! -e "$(installed)" ]] || fail "something was installed"
older_kept

# 3. The build wrote none: the older one isn't installed in its place.
setup none-built
older_build
run_install
((STATUS != 0)) || fail "installed the older AppImage"
[[ "$ERR" == *"wrote no .AppImage"* && "$ERR" == *"$OLDER"* ]] || fail "unclear error: $ERR"
[[ ! -e "$(installed)" ]] || fail "something was installed"
older_kept

# 4. The one just built isn't this source's version.
setup wrong-version "Gospel Getter_0.0.1_amd64.AppImage"
run_install
((STATUS != 0)) || fail "installed an AppImage of another version"
[[ "$ERR" == *"isn't version $VERSION"* ]] || fail "unclear error: $ERR"
[[ ! -e "$(installed)" ]] || fail "something was installed"

# 5. No older AppImage at all: the one just built, as ever.
setup only-one "$CURRENT"
run_install
if ((STATUS != 0)); then
  fail "exited $STATUS: $ERR"
elif [[ "$(cat "$(installed)")" != "built: $CURRENT" ]]; then
  fail "didn't install $CURRENT"
fi

# ---- packaging/uninstall.sh ----

# A fresh HOME for one uninstall case ($1): the app installed as install.sh
# installs it, its reading-data folder, and another app's folder beside it.
setup_uninstall() {
  CASE="$1"
  cases=$((cases + 1))
  ROOT="$WORK/$CASE"
  local home="$ROOT/home"
  mkdir -p "$ROOT/bin" "$home/.local/bin" "$home/.local/share/applications" \
    "$home/.local/share/icons/hicolor/512x512/apps" "$home/.local/share/other-app"
  printf 'appimage\n' >"$home/.local/bin/gospel-getter.AppImage"
  printf '[Desktop Entry]\n' >"$home/.local/share/applications/gospel-getter.desktop"
  printf 'png\n' >"$home/.local/share/icons/hicolor/512x512/apps/gospel-getter.png"
  DATA="$home/.local/share/com.tossbaws.gospel-getter"
  mkdir -p "$DATA/logs"
  printf 'notes, bookmarks, highlights\n' >"$DATA/gospel_getter.db"
  printf '{}\n' >"$DATA/window-state.json"
  printf 'kept\n' >"$home/.local/share/other-app/data"
  printf '#!/bin/sh\nexit 0\n' >"$ROOT/bin/update-desktop-database"
  chmod +x "$ROOT/bin/"*
}

# Runs packaging/uninstall.sh with stdin from $1; its exit code is in
# STATUS, its stdout in OUT.
run_uninstall() {
  set +e
  env -i PATH="$ROOT/bin:/usr/bin:/bin" HOME="$ROOT/home" TMPDIR="$ROOT" \
    bash "$REPO_ROOT/packaging/uninstall.sh" <"$1" >"$ROOT/out.txt" 2>"$ROOT/err.txt"
  STATUS=$?
  set -e
  OUT="$(cat "$ROOT/out.txt")"
}

# The app's own files are gone, another app's are not, and the run ended
# normally, having named the reading data before asking.
uninstalled() {
  ((STATUS == 0)) || fail "exited $STATUS: $(cat "$ROOT/err.txt")"
  [[ ! -e "$ROOT/home/.local/bin/gospel-getter.AppImage" ]] || fail "the AppImage is still there"
  [[ ! -e "$ROOT/home/.local/share/applications/gospel-getter.desktop" ]] || fail "the launcher is still there"
  [[ "$(cat "$ROOT/home/.local/share/other-app/data" 2>/dev/null)" == kept ]] || fail "another app's data was touched"
  local said
  for said in "notes, bookmarks, highlights" "AI findings and reading place" \
    "Export data… → Everything (backup)" "nothing has been removed yet" "[y/N]" "has been uninstalled."; do
    [[ "$OUT" == *"$said"* ]] || fail "doesn't say \"$said\": $OUT"
  done
  [[ "$OUT" != *"Bible database"* ]] || fail "still calls it the Bible database: $OUT"
}

data_kept() {
  [[ "$(cat "$DATA/gospel_getter.db" 2>/dev/null)" == "notes, bookmarks, highlights" ]] || fail "the reading data was deleted"
  [[ -f "$DATA/window-state.json" ]] || fail "part of the data folder was deleted"
  [[ "$OUT" == *"Kept $DATA"* ]] || fail "doesn't say it kept the data: $OUT"
}

# 6. "n": kept.
setup_uninstall uninstall-no
printf 'n\n' >"$ROOT/answer"
run_uninstall "$ROOT/answer"
uninstalled
data_kept

# 7. An empty answer (Enter): No, kept.
setup_uninstall uninstall-empty
printf '\n' >"$ROOT/answer"
run_uninstall "$ROOT/answer"
uninstalled
data_kept

# 8. No answer at all (the end of input, as from /dev/null): No, kept, and
# the script still finishes (under set -e, a bare `read` at EOF stopped it).
setup_uninstall uninstall-eof
run_uninstall /dev/null
uninstalled
data_kept

# 9. "y": the data folder, and only it, is deleted.
setup_uninstall uninstall-yes
printf 'y\n' >"$ROOT/answer"
run_uninstall "$ROOT/answer"
uninstalled
[[ ! -e "$DATA" ]] || fail "the data folder is still there"
[[ "$OUT" == *"Deleted $DATA"* ]] || fail "doesn't say it deleted the data: $OUT"

if ((failures > 0)); then
  echo "$failures check(s) failed" >&2
  exit 1
fi
echo "install.sh and uninstall.sh: all $cases cases passed"
