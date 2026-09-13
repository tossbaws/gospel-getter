#!/usr/bin/env bash
# Builds GospelGetterSetup.exe: cross-compiles the release binary for
# Windows, then compiles the NSIS installer script around it.
#
# Requires (on the build machine, e.g. via `pacman`/`yay` on Arch):
#   rustup target add x86_64-pc-windows-gnu
#   mingw-w64-gcc (cross C toolchain — sqlx's bundled SQLite needs a C compiler)
#   nsis (provides `makensis`)
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
WIN_DIR="$REPO_ROOT/packaging/windows"

echo "==> Cross-compiling gospel_getter.exe (release, x86_64-pc-windows-gnu)"
(cd "$REPO_ROOT" && cargo build --release --target x86_64-pc-windows-gnu)

echo "==> Compiling installer with NSIS"
(cd "$WIN_DIR" && makensis installer.nsi)

echo
echo "Built: $WIN_DIR/GospelGetterSetup.exe"
