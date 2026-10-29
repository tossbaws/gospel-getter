#!/usr/bin/env bash
# Builds WebKitGTK (the webkit2gtk-4.1 API: GTK 3, libsoup 3) for the Linux
# AppImage and installs it into a staging directory, for webkitgtk.yml.
#
#   .github/webkitgtk/build.sh <verified tarball> <work dir> <stage dir>
#
# CONFIGURE_ONLY=1 stops after CMake has configured the tree (a quick check
# of the dependencies and options), JOBS sets ninja's parallelism.
#
# The prefix is /usr, installed with DESTDIR into <stage dir>, not a prefix
# of its own: Tauri's AppImage bundler copies WebKit's helper processes
# (WebKitWebProcess, WebKitNetworkProcess, the injected bundle) only from
# /usr/lib/x86_64-linux-gnu/webkit2gtk-4.1, and its linuxdeploy GTK plugin
# makes the bundled libwebkit* find them by rewriting "/usr" to "././" in the
# library. dist.yml lays the staged tree over the AppImage job's /usr. The
# libexec directory is Debian's (lib/x86_64-linux-gnu), so the helpers land
# where the bundler looks.
#
# Every option below that differs from WebKitGTK's own default is a public
# option, and why is beside it. This file is part of the cache key.
set -euo pipefail

tarball="$1"
work="$2"
stage="$3"
jobs="${JOBS:-$(nproc)}"

src="$work/src"
build="$work/build"
rm -rf "$src" "$build" "$stage"
mkdir -p "$src" "$build" "$stage"
tar -xJf "$tarball" -C "$src" --strip-components=1

started=$SECONDS
options=(
    -G Ninja
    -DPORT=GTK
    -DCMAKE_BUILD_TYPE=Release
    -DCMAKE_INSTALL_PREFIX=/usr
    -DCMAKE_INSTALL_LIBDIR=lib/x86_64-linux-gnu
    -DCMAKE_INSTALL_LIBEXECDIR=lib/x86_64-linux-gnu
    # GCC 12.3 from jammy-updates: WebKit needs GCC 12.2 or newer (C++23),
    # and jammy's default GCC is 11. Its libstdc++ is jammy's own
    # libstdc++6, so nothing newer than jammy is needed at run time.
    -DCMAKE_C_COMPILER=gcc-12
    -DCMAKE_CXX_COMPILER=g++-12
    -DCMAKE_C_COMPILER_LAUNCHER=ccache
    -DCMAKE_CXX_COMPILER_LAUNCHER=ccache
    # The 4.1 API (GTK 3, libsoup 3), which Tauri 2 links.
    -DUSE_GTK4=OFF
    # Not needed to build or run the app: Tauri's bindings are generated
    # Rust, not GObject introspection.
    -DENABLE_DOCUMENTATION=OFF
    -DENABLE_INTROSPECTION=OFF
    -DENABLE_MINIBROWSER=OFF
    # Media: the app plays no audio or video, speaks nothing and reads no
    # gamepad, so no GStreamer (USE_GSTREAMER off turns off video, Web
    # Audio, WebCodecs and speech synthesis with it), Flite or libmanette.
    -DUSE_GSTREAMER=OFF
    -DENABLE_VIDEO=OFF
    -DENABLE_WEB_AUDIO=OFF
    -DENABLE_SPEECH_SYNTHESIS=OFF
    -DENABLE_GAMEPAD=OFF
    # Not in jammy (libjxl), or not used (AVIF images, HTTP-auth passwords
    # in libsecret), or not packaged (libbacktrace, for crash backtraces).
    -DUSE_JPEGXL=OFF
    -DUSE_AVIF=OFF
    -DUSE_LIBSECRET=OFF
    -DUSE_LIBBACKTRACE=OFF
    # Kept at their defaults, named so the choice is visible: the JIT,
    # the Bubblewrap sandbox (bwrap and xdg-dbus-proxy from jammy), spell
    # checking (the note editor uses it) and WebKitWebDriver (not bundled;
    # it's what drives this WebKit in the GUI checks).
    -DENABLE_JIT=ON
    -DENABLE_BUBBLEWRAP_SANDBOX=ON
    -DENABLE_SPELLCHECK=ON
    -DENABLE_WEBDRIVER=ON
)
cmake -S "$src" -B "$build" "${options[@]}"
configured=$SECONDS
echo "configure: $((configured - started))s"

if [[ ${CONFIGURE_ONLY:-0} == 1 ]]; then
    exit 0
fi

ninja -C "$build" -j "$jobs"
built=$SECONDS
echo "build: $((built - configured))s"

DESTDIR="$stage" cmake --install "$build" --strip
echo "install: $((SECONDS - built))s"
