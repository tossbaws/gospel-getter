#!/usr/bin/env bash
# Tests which AppImage packaging/install.sh installs, without building
# anything: each case runs a copy of the script in a throwaway copy of the
# files it reads (itself, the icon, src-tauri/tauri.conf.json), with a
# stand-in `cargo` whose `cargo tauri build` writes the AppImages the case
# names into the bundle folder, as the bundler would, and a throwaway HOME.
# The bundle folder can already hold an older build's AppImage, which must
# never be the one installed, and must be left where it was.
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
fail() {
  echo "FAIL $CASE: $*" >&2
  failures=$((failures + 1))
}

# A fresh world for one case: $1 names it, the rest are the AppImages that
# `cargo tauri build` writes (none at all with no more arguments).
setup() {
  CASE="$1"
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

if ((failures > 0)); then
  echo "$failures check(s) failed" >&2
  exit 1
fi
echo "install.sh: all 5 cases passed"
