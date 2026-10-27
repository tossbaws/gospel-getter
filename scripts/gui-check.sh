#!/usr/bin/env bash
# GUI smoke checks against the real app: the release binary or the
# AppImage on Linux, the installed .exe on Windows, driven through its own
# webview by tauri-driver (WebDriver). The checks themselves are the
# tests/gui crate; this script gives them a world to run in and always takes
# it down again.
#
#   [GUI_CHECK_UPDATER_APP=<test build>] \
#       scripts/gui-check.sh <release binary | AppImage | .exe> [out-dir]
#
# The out-dir (default target/gui-check/<timestamp>/) gets results.txt (one
# PASS/FAIL line per check, with the reason), steps.log, screenshots/*.png
# (taken by WebKit or WebView2 through WebDriver), app.log, driver.log
# (tauri-driver with the native driver's own output) and environment.txt.
# Exit code 0 only if every check passed.
#
# Checks 1 to 8, 14 to 16 (the Library), 17 to 19 (search and the
# palette), 20 to 24 (the book and chapter picker), 25 to 29 (Aa, the
# translations, Compare and reading mode), 30 to 37 (the header's
# popovers and panels, the ☰ menu's keys, About's Tab, G and the sidebar,
# and the window crossing 1200 px, also right after a scroll) and 38 to 44
# (the reader: the chapter's end, Compare's grid, the chapter's lists, a
# cross-reference with the mouse's back and forward buttons, Psalm 119, Try
# again and the plain-text fallback) and 46 to 52 (the selection card:
# verses selected with the mouse, Copy and Copy as… read back from the
# clipboard, a bookmark, highlights and a note from the card, a
# cross-reference read in it, the Shift ghost) and 53 to 57 (the keyboard:
# chapters and books turned, verses selected with j and k and Shift, Escape,
# Ctrl+C read back from the clipboard and Ctrl+Shift+C, Alt+← and Alt+→, N;
# the palette's page commands) run on the app given; for 58 (a cold start to
# the saved chapter), 26 and 45 it is closed and started again on the same
# data. The updater's checks (9 to 13) run on
# GUI_CHECK_UPDATER_APP: the GUI checks' test build (cargo build --release
# --features gui-test-updater), against the checks' own local update
# server, with a signing key made for the run; without it they fail as not
# run. Both apps are told the test build's settings file
# (GOSPEL_GETTER_TEST_UPDATER, in the run's own folder); the app as shipped
# ignores it, and checks that it sent the server nothing.
#
# The app as shipped never reaches the real update server: it's started
# with its HTTPS proxy set to a local port nothing answers on (the
# updater's requests honour HTTPS_PROXY and ALL_PROXY, on Linux and
# Windows; NO_PROXY keeps 127.0.0.1 direct), so its launch check fails
# quietly, as offline. Otherwise, once GitHub has a release newer than the
# build, that check's real banner covers the page and other checks' clicks
# land on it. The test build isn't cut off: checks 9 to 13 use their own
# server on 127.0.0.1.
#
# Linux: nothing reaches the desktop of whoever runs this. The app, its
# driver and the checks run with an emptied environment (env -i): no
# WAYLAND_DISPLAY, DISPLAY set to a private Xvfb started here, GDK_BACKEND=x11,
# GTK_USE_PORTAL=0, a private session bus with no service activation (so no
# portal can start on it), and throwaway HOME, XDG_*_HOME and
# XDG_RUNTIME_DIR. xdotool, used only by the checks for the native save
# dialog, is pointed at that Xvfb on every call, after checking that the
# display is the Xvfb started here: its lock file names that process, and
# every socket listening for the display is that process's. The script
# refuses to start the app if it can't confirm the same.
#
# Windows: there is no private display. The app runs on the runner's
# desktop with its usual data folders (Windows resolves them from the
# account, not the environment), which is fine on a throwaway CI runner and
# is why this runs there only. Edge WebDriver runs with --verbose (its log
# goes to driver.log), the app is started through tests/gui's gui-launch so
# its own log reaches app.log, and windows-timeline.log follows the app's
# processes while the checks run.
# Since WebView2 Runtime 150 an elevated app ignores the variable Edge
# WebDriver opens the app's debugging port with, so CI sets the HKLM policy
# for it (see the gui-windows job in dist.yml); this script never changes
# machine policy. If a check fails, the out-dir also gets
# processes.txt, windows.txt, webview2-devtools.txt (what the WebView2
# debugging port answers), devtools-active-port.txt (where the browser's
# DevToolsActivePort file is, and Edge WebDriver's user-data folders) and
# screenshots/desktop.png.
#
# Settings, for CI or a slow machine: GUI_CHECK_BUDGET (seconds for the whole
# run, default 180), GUI_CHECK_STEP_TIMEOUT (seconds per step, default 10),
# GUI_CHECK_SESSION_TIMEOUT (seconds to start the app, default 30),
# GUI_CHECK_RUNNER (a prebuilt gui-check binary instead of building
# tests/gui; gui-launch must be beside it), TAURI_DRIVER and NATIVE_DRIVER
# (paths, if not on PATH).

set -euo pipefail

usage() {
    sed -n '2,33p' "$0" | sed 's/^# \{0,1\}//'
}

if [[ $# -lt 1 || $# -gt 2 || $1 == -h || $1 == --help ]]; then
    usage
    [[ $# -ge 1 && ($1 == -h || $1 == --help) ]] && exit 0
    exit 2
fi

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
app="$(realpath "$1")"
updater_app="${GUI_CHECK_UPDATER_APP:-}"
out="${2:-$repo/target/gui-check/$(date +%Y%m%d-%H%M%S)}"
budget="${GUI_CHECK_BUDGET:-180}"
step_timeout="${GUI_CHECK_STEP_TIMEOUT:-10}"

case "$(uname -s)" in
    Linux) platform=linux ;;
    MINGW* | MSYS* | CYGWIN*) platform=windows ;;
    *)
        echo "gui-check: unsupported platform $(uname -s) (Linux and Windows only)" >&2
        exit 2
        ;;
esac

if [[ ! -f $app ]]; then
    echo "gui-check: no such file: $1" >&2
    exit 2
fi
if [[ -n $updater_app ]]; then
    if [[ ! -f $updater_app ]]; then
        echo "gui-check: no such file (GUI_CHECK_UPDATER_APP): $updater_app" >&2
        exit 2
    fi
    updater_app="$(realpath "$updater_app")"
fi

mkdir -p "$out/screenshots"
out="$(realpath "$out")"
results="$out/results.txt"
# The checks, as results.txt numbers and names them (tests/gui report.rs).
checks=("1 driver" "2 csp" "3 export-notes" "4 screenshots" "5 welcome" "6 note-editor" "7 shortcuts" "8 import-preview"
    "9 update-banner" "10 update-about" "11 update-download" "12 update-link-only" "13 update-offline")
: >"$results"
work="$(mktemp -d "${TMPDIR:-/tmp}/gui-check.XXXXXXXX")"
# The test build's updater settings, which the checks write.
updater_settings="$work/test-updater.json"
started=$SECONDS
# Where the app as shipped sends its HTTPS (and any other) requests: port 9
# on this computer, which nothing serves and only root could bind.
cut_off_proxy="http://127.0.0.1:9"

log() {
    printf '[gui-check %3ds] %s\n' "$((SECONDS - started))" "$*" | tee -a "$out/steps.log" >&2
}

# A failure before the checks could run still leaves a results file that
# says so, one FAIL per check.
setup_failed() {
    log "setup failed: $*"
    local check
    for check in "${checks[@]}"; do
        echo "FAIL $check: not run: setup failed: $*"
    done >"$results"
    exit 1
}

# Every check gets a line in results.txt: those with no result get a FAIL
# saying why.
complete_results() {
    local check
    for check in "${checks[@]}"; do
        grep -qE "^(PASS|FAIL) ${check%% *} " "$results" || echo "FAIL $check: $1" >>"$results"
    done
}

# ---- Cleanup: everything this run started, whatever happens. ----

xvfb_pid=""
bus_pid=""
bus_dir=""
driver_pid=""
timeline_pid=""

# Every process started on Linux carries this in its environment
# (XDG_RUNTIME_DIR is the run's own), however it was spawned: tauri-driver,
# WebKitWebDriver, the app and WebKit's helper processes, the bus, Xvfb.
marked_pids() {
    local environ pid
    for environ in /proc/[0-9]*/environ; do
        pid="${environ#/proc/}"
        pid="${pid%/environ}"
        [[ $pid == "$$" ]] && continue
        if grep -qzxF "XDG_RUNTIME_DIR=$work/runtime" "$environ" 2>/dev/null; then
            echo "$pid"
        fi
    done
}

# Waits up to $1 seconds for the given pids to be gone.
wait_gone() {
    local deadline=$((SECONDS + $1)) pid alive
    shift
    while ((SECONDS <= deadline)); do
        alive=0
        for pid in "$@"; do
            kill -0 "$pid" 2>/dev/null && alive=1
        done
        ((alive)) || return 0
        sleep 0.1
    done
    return 1
}

cleanup() {
    local status=$?
    trap - EXIT INT TERM
    set +e
    if ((status == 130 || status == 143)); then
        complete_results "no result: the run was interrupted"
    fi
    log "cleaning up"
    if [[ $platform == linux ]]; then
        local pids
        mapfile -t pids < <(marked_pids)
        if ((${#pids[@]})); then
            kill -TERM "${pids[@]}" 2>/dev/null
            if ! wait_gone 5 "${pids[@]}"; then
                log "still running after SIGTERM, killing: $(marked_pids | tr '\n' ' ')"
                mapfile -t pids < <(marked_pids)
                ((${#pids[@]})) && kill -KILL "${pids[@]}" 2>/dev/null
                wait_gone 5 "${pids[@]}"
            fi
        fi
        mapfile -t pids < <(marked_pids)
        if ((${#pids[@]})); then
            log "WARNING: processes left running: ${pids[*]}"
        else
            log "no processes left from this run"
        fi
    else
        [[ -n $timeline_pid ]] && kill "$timeline_pid" 2>/dev/null
        [[ -n $driver_pid ]] && kill "$driver_pid" 2>/dev/null
        taskkill //F //IM msedgedriver.exe >/dev/null 2>&1
        taskkill //F //IM gui-launch.exe >/dev/null 2>&1
        taskkill //F //IM "$(basename "$app")" >/dev/null 2>&1
    fi
    rm -rf "$work"
    [[ -n $bus_dir ]] && rm -rf "$bus_dir"
    log "results in $out (exit $status)"
    exit "$status"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

log "app: $app"
log "test build for the updater checks: ${updater_app:-none, so checks 9 to 13 do not run}"
log "out: $out"

# ---- The checks binary. ----

runner="${GUI_CHECK_RUNNER:-}"
if [[ -z $runner ]]; then
    log "building tests/gui"
    cargo build --quiet --locked --manifest-path "$repo/tests/gui/Cargo.toml" \
        || setup_failed "tests/gui did not build"
    runner="$repo/tests/gui/target/debug/gui-check"
    [[ $platform == windows ]] && runner="$runner.exe"
fi
[[ -x $runner ]] || setup_failed "no gui-check binary at $runner"

tauri_driver="${TAURI_DRIVER:-$(command -v tauri-driver || true)}"
[[ -n $tauri_driver ]] || setup_failed "tauri-driver is not installed (cargo install tauri-driver --locked)"

# The run budget starts here, once everything is built.
started=$SECONDS

# A free local port: nothing answers on it.
free_port() {
    local port
    for _ in $(seq 50); do
        port=$((20000 + RANDOM % 40000))
        if ! (exec 3<>"/dev/tcp/127.0.0.1/$port") 2>/dev/null; then
            echo "$port"
            return 0
        fi
    done
    return 1
}

driver_args=()
run_env=()
launch="$app"
launch_updater=""
xvfb_display=""

if [[ $platform == linux ]]; then
    native_driver="${NATIVE_DRIVER:-$(command -v WebKitWebDriver || true)}"
    [[ -n $native_driver ]] || setup_failed "WebKitWebDriver is not installed"
    for tool in Xvfb xdotool dbus-daemon ss curl; do
        command -v "$tool" >/dev/null || setup_failed "$tool is not installed"
    done
    driver_args+=(--native-driver "$native_driver")

    mkdir -p "$work"/{home,data,config,cache,state,runtime,export}
    # The bus socket's own short directory: a socket path must fit in 108
    # bytes, which $work may not.
    bus_dir="$(mktemp -d /tmp/gui-check-bus.XXXXXXXX)"
    chmod 700 "$work/runtime"

    # The emptied environment everything below runs in.
    run_env=(
        env -i
        PATH="$PATH"
        HOME="$work/home"
        USER="${USER:-gui-check}"
        LANG=C.UTF-8
        XDG_DATA_HOME="$work/data"
        XDG_CONFIG_HOME="$work/config"
        XDG_CACHE_HOME="$work/cache"
        XDG_STATE_HOME="$work/state"
        XDG_RUNTIME_DIR="$work/runtime"
        GOSPEL_GETTER_TEST_UPDATER="$updater_settings"
        GDK_BACKEND=x11
        GTK_USE_PORTAL=0
        GSETTINGS_BACKEND=memory
        NO_AT_BRIDGE=1
        NO_COLOR=1
    )

    # A display number nothing uses: no lock file, and no socket for it,
    # on disk or in the abstract namespace. Not Xvfb's own -displayfd
    # search: in that mode Xvfb writes no lock file and goes by whether a
    # socket binds, and it took :0 beside Hyprland's Xwayland, so a client
    # of "Xvfb's" display would have reached the real desktop.
    display_number=""
    listeners="$(ss -xlH 2>/dev/null)" || setup_failed "ss (iproute2) is needed to check X displays"
    for n in $(seq 99 199); do
        [[ -e /tmp/.X$n-lock || -e /tmp/.X11-unix/X$n ]] && continue
        grep -qE "[@ ]/tmp/\.X11-unix/X$n( |$)" <<<"$listeners" && continue
        display_number=$n
        break
    done
    [[ -n $display_number ]] || setup_failed "no free X display between :99 and :199"
    xvfb_display=":$display_number"

    "${run_env[@]}" Xvfb "$xvfb_display" -screen 0 1440x1000x24 -nolisten tcp -noreset \
        >"$out/xvfb.log" 2>&1 &
    xvfb_pid=$!

    # Ready, and safe to use, only once that display is the Xvfb started
    # here: its lock file names this pid, the pid is Xvfb, and every
    # socket listening for the display (on disk and abstract) is that
    # process's. Refuse to go on otherwise.
    xvfb_confirmed() {
        local lock_pid owners
        lock_pid="$({ tr -d ' \n' <"/tmp/.X$display_number-lock"; } 2>/dev/null)" || return 1
        [[ $lock_pid =~ ^[0-9]+$ && $((10#$lock_pid)) == "$xvfb_pid" ]] || return 1
        [[ "$(cat "/proc/$xvfb_pid/comm" 2>/dev/null)" == Xvfb ]] || return 1
        owners="$(ss -xlpH | grep -E "[@ ]/tmp/\.X11-unix/X$display_number( |$)" \
            | grep -oE 'pid=[0-9]+' | sort -u)"
        [[ $owners == "pid=$xvfb_pid" ]]
    }
    deadline=$((SECONDS + 10))
    until xvfb_confirmed; do
        kill -0 "$xvfb_pid" 2>/dev/null || setup_failed "Xvfb exited (see xvfb.log)"
        ((SECONDS < deadline)) || setup_failed "could not confirm within 10s that $xvfb_display is the private Xvfb (pid $xvfb_pid)"
        sleep 0.1
    done
    log "private display $xvfb_display (Xvfb pid $xvfb_pid)"

    # A private session bus that starts nothing on demand: no
    # <servicedir>, so no portal, no dconf, no a11y bus.
    cat >"$work/bus.conf" <<EOF
<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <listen>unix:dir=$bus_dir</listen>
  <auth>EXTERNAL</auth>
  <policy context="default">
    <allow send_destination="*" eavesdrop="true"/>
    <allow eavesdrop="true"/>
    <allow own="*"/>
  </policy>
</busconfig>
EOF
    mkfifo "$work/bus.fifo"
    "${run_env[@]}" dbus-daemon --config-file="$work/bus.conf" --nofork --nopidfile \
        --print-address=3 3>"$work/bus.fifo" >"$out/dbus.log" 2>&1 &
    bus_pid=$!
    bus_address=""
    read -r -t 10 bus_address <"$work/bus.fifo" || true
    [[ $bus_address == "unix:path=$bus_dir/"* ]] \
        || setup_failed "the private session bus did not start within 10s (got '$bus_address')"
    log "private session bus $bus_address (pid $bus_pid)"

    run_env+=(DISPLAY="$xvfb_display" DBUS_SESSION_BUS_ADDRESS="$bus_address")

    # An AppImage is tested as shipped: its own runtime extracts it (no
    # FUSE needed) and its own AppRun starts it.
    if [[ "$(head -c 11 "$app" | tail -c 3 | od -An -tx1 | tr -d ' \n')" == 414902 ]]; then
        log "extracting the AppImage"
        cp "$app" "$work/app.AppImage"
        chmod +x "$work/app.AppImage"
        (cd "$work" && ./app.AppImage --appimage-extract >/dev/null) \
            || setup_failed "the AppImage did not extract"
        launch="$work/squashfs-root/AppRun"
        [[ -x $launch ]] || setup_failed "the AppImage has no AppRun"
    fi

    # tauri-driver passes its environment to WebKitWebDriver, which starts
    # the app; this wrapper cuts the app off from the real update server
    # (see the top) and sends its output to app.log.
    {
        echo '#!/usr/bin/env bash'
        printf 'exec env HTTPS_PROXY=%q HTTP_PROXY=%q ALL_PROXY=%q NO_PROXY=%q %q "$@" >>%q 2>&1\n' \
            "$cut_off_proxy" "$cut_off_proxy" "$cut_off_proxy" "127.0.0.1,localhost,::1" \
            "$launch" "$out/app.log"
    } >"$work/launch-app"
    chmod +x "$work/launch-app"
    launch="$work/launch-app"
    if [[ -n $updater_app ]]; then
        {
            echo '#!/usr/bin/env bash'
            printf 'exec %q "$@" >>%q 2>&1\n' "$updater_app" "$out/app-updater.log"
        } >"$work/launch-updater"
        chmod +x "$work/launch-updater"
        launch_updater="$work/launch-updater"
    fi
    checks_settings="$updater_settings"
    export_dir="$work/export"
else
    native_driver="${NATIVE_DRIVER:-$(command -v msedgedriver.exe || command -v msedgedriver || true)}"
    [[ -n $native_driver ]] || setup_failed "msedgedriver is not installed"
    # tauri-driver passes the native driver only --port and --host; this
    # wrapper adds --verbose. Its log goes to driver.log with tauri-driver's.
    printf '@"%s" --verbose %%*\r\n' "$(cygpath -w "$native_driver")" >"$work/msedgedriver-verbose.cmd"
    driver_args+=(--native-driver "$(cygpath -w "$work/msedgedriver-verbose.cmd")")
    # Edge WebDriver starts gui-launch, which starts the app with its
    # output going to app.log (see tests/gui/src/bin/gui-launch.rs).
    launcher="$(dirname "$runner")/gui-launch.exe"
    [[ -x $launcher ]] || setup_failed "no gui-launch.exe beside $runner"
    launch="$(cygpath -w "$launcher")"
    mkdir -p "$work/export"
    export_dir="$(cygpath -w "$work/export")"
    # gui-launch gives the app as shipped (not the test build) this proxy.
    run_env=(env GUI_LAUNCH_TARGET="$(cygpath -w "$app")" GUI_LAUNCH_LOG="$(cygpath -w "$out/app.log")"
        GUI_LAUNCH_CUT_OFF_PROXY="$cut_off_proxy"
        GOSPEL_GETTER_TEST_UPDATER="$(cygpath -w "$updater_settings")")
    if [[ -n $updater_app ]]; then
        # The same launcher under another name, which starts the test build
        # (gui-launch picks its target by its own name).
        cp "$launcher" "$work/gui-launch-updater.exe"
        launch_updater="$(cygpath -w "$work/gui-launch-updater.exe")"
        run_env+=(GUI_LAUNCH_UPDATER_TARGET="$(cygpath -w "$updater_app")"
            GUI_LAUNCH_UPDATER_LOG="$(cygpath -w "$out/app-updater.log")")
    fi
    checks_settings="$(cygpath -w "$updater_settings")"
fi

port="$(free_port)" || setup_failed "no free port for tauri-driver"
native_port="$(free_port)" || setup_failed "no free port for the native driver"
while [[ $native_port == "$port" ]]; do native_port="$(free_port)"; done

# Which package a file comes from, with its version, where the system can
# say (neither tauri-driver nor WebKitWebDriver reports its own version).
package_of() {
    local package
    if command -v pacman >/dev/null; then
        pacman -Qo "$1" 2>/dev/null | sed 's/.* is owned by //'
    elif command -v dpkg >/dev/null; then
        package="$(dpkg -S "$1" 2>/dev/null | head -1 | cut -d: -f1)"
        [[ -n $package ]] && echo "$package $(dpkg-query -W -f='${Version}' "$package")"
    fi
    return 0
}

if [[ $platform == linux ]]; then
    native_version="$(package_of "$native_driver")"
else
    native_version="$("$native_driver" --version 2>&1 | head -1 || true)"
fi
{
    echo "platform: $platform"
    echo "app: $app"
    echo "launched as: $launch"
    echo "tauri-driver: $tauri_driver"
    echo "native driver: $native_driver (${native_version:-version unknown})"
    echo "display: ${xvfb_display:-desktop (Windows)}"
    echo "the app as shipped: HTTPS_PROXY, HTTP_PROXY and ALL_PROXY $cut_off_proxy (no real update server)"
    echo "budget: ${budget}s, step timeout: ${step_timeout}s, session timeout: ${GUI_CHECK_SESSION_TIMEOUT:-30}s"
    if [[ $platform == windows ]]; then
        echo "TEMP: $(cygpath -w "${TEMP:-}" 2>/dev/null), TMP: $(cygpath -w "${TMP:-}" 2>/dev/null)"
        # WebView2 (Runtime 150 and later) ignores Edge WebDriver's
        # WEBVIEW2_* variables in an elevated app; see dist.yml.
        echo "integrity: $("$(cygpath -u "$SYSTEMROOT")/System32/whoami.exe" //groups 2>/dev/null \
            | grep -o 'Mandatory Label\\[A-Za-z ]*Level' | head -1)"
        echo "WebView2 HKLM policy: $(reg query 'HKLM\Software\Policies\Microsoft\Edge\WebView2\AdditionalBrowserArguments' 2>&1 | tr -s '\r\n ' ' ')"
    fi
} >"$out/environment.txt"

"${run_env[@]}" "$tauri_driver" --port "$port" --native-port "$native_port" "${driver_args[@]}" \
    >"$out/driver.log" 2>&1 &
driver_pid=$!

# Ready when it answers.
deadline=$((SECONDS + 10))
until curl -fsS --max-time 1 "http://127.0.0.1:$port/status" >/dev/null 2>&1; do
    kill -0 "$driver_pid" 2>/dev/null || setup_failed "tauri-driver exited (see driver.log)"
    ((SECONDS < deadline)) || setup_failed "tauri-driver did not answer within 10s (see driver.log)"
    sleep 0.1
done
log "tauri-driver on 127.0.0.1:$port (native driver on $native_port)"

# Windows: what the app's processes and windows were doing, every 2s,
# while the checks run.
windows_timeline() {
    local image
    # Diagnostics only: nothing here may stop the run.
    set +e +o pipefail
    while :; do
        echo "== $(date +%T)"
        for image in gui-launch.exe "$(basename "$app")" msedgewebview2.exe msedgedriver.exe; do
            tasklist //v //fo csv //nh //fi "imagename eq $image" 2>&1 | grep -v '^INFO:'
        done
        sleep 2
    done >>"$out/windows-timeline.log"
}

# Windows, after a failure: the app's processes (with their command lines),
# the windows on the desktop, what any WebView2 debugging port answers,
# and a screenshot of the desktop.
windows_diagnostics() {
    log "collecting Windows diagnostics"
    cat >"$work/diagnostics.ps1" <<'PS1'
param([Parameter(Mandatory)][string]$OutDir)
$ErrorActionPreference = 'Continue'
$names = 'gui-launch|gospel_getter|msedgewebview2|msedgedriver|tauri-driver'
$processes = Get-CimInstance Win32_Process | Where-Object { $_.Name -match $names }
$processes | Select-Object ProcessId, ParentProcessId, Name, CreationDate, CommandLine |
    Format-List | Out-File -Width 4096 (Join-Path $OutDir 'processes.txt')
Get-Process | Where-Object { $_.MainWindowHandle -ne 0 } |
    Select-Object Id, ProcessName, MainWindowTitle, Responding |
    Format-Table -AutoSize | Out-File -Width 4096 (Join-Path $OutDir 'windows.txt')
$webview = @($processes | Where-Object { $_.Name -eq 'msedgewebview2.exe' } | ForEach-Object { $_.ProcessId })
$ports = @(Get-NetTCPConnection -State Listen -ErrorAction SilentlyContinue |
    Where-Object { $webview -contains $_.OwningProcess } | ForEach-Object { $_.LocalPort } | Sort-Object -Unique)
& {
    "WebView2 processes: $($webview -join ', ')"
    "ports they listen on: $($ports -join ', ')"
    foreach ($port in $ports) {
        foreach ($path in '/json/version', '/json/list') {
            "== http://127.0.0.1:$port$path"
            try { (Invoke-WebRequest -UseBasicParsing -TimeoutSec 5 "http://127.0.0.1:$port$path").Content }
            catch { "no answer: $_" }
        }
    }
} | Out-File -Width 4096 (Join-Path $OutDir 'webview2-devtools.txt')
# Where Edge WebDriver waits for the browser's DevToolsActivePort (its
# scoped user-data folders), and where any such file actually is.
& {
    $roots = @((Join-Path $env:SystemRoot 'SystemTemp'), $env:TEMP, $env:LOCALAPPDATA, $env:APPDATA) |
        Where-Object { $_ -and (Test-Path -LiteralPath $_) } | Sort-Object -Unique
    "searched: $($roots -join '; ')"
    Get-ChildItem -LiteralPath $roots -Recurse -Depth 5 -Force -Filter DevToolsActivePort -ErrorAction SilentlyContinue |
        ForEach-Object { "$($_.FullName) [$($_.LastWriteTime.ToString('HH:mm:ss'))]: $((Get-Content -LiteralPath $_.FullName -Raw) -replace "`r?`n", ' | ')" }
    foreach ($root in $roots) {
        Get-ChildItem -LiteralPath $root -Directory -Filter 'scoped_dir*' -Force -ErrorAction SilentlyContinue | ForEach-Object {
            "== $($_.FullName)"
            Get-ChildItem -LiteralPath $_.FullName -Recurse -Depth 1 -Force -ErrorAction SilentlyContinue |
                ForEach-Object { "   $($_.FullName.Substring($root.Length))" }
        }
    }
} | Out-File -Width 4096 (Join-Path $OutDir 'devtools-active-port.txt')
try {
    Add-Type -AssemblyName System.Windows.Forms, System.Drawing
    $screen = [System.Windows.Forms.SystemInformation]::VirtualScreen
    $bitmap = New-Object System.Drawing.Bitmap $screen.Width, $screen.Height
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    $graphics.CopyFromScreen($screen.Left, $screen.Top, 0, 0, $bitmap.Size)
    $bitmap.Save((Join-Path $OutDir 'screenshots\desktop.png'), [System.Drawing.Imaging.ImageFormat]::Png)
} catch { "desktop screenshot failed: $_" | Out-File (Join-Path $OutDir 'webview2-devtools.txt') -Append }
PS1
    powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass \
        -File "$(cygpath -w "$work/diagnostics.ps1")" -OutDir "$(cygpath -w "$out")" \
        >>"$out/steps.log" 2>&1 || log "the Windows diagnostics failed (see steps.log)"
}

if [[ $platform == windows ]]; then
    windows_timeline &
    timeline_pid=$!
fi

remaining=$((budget - (SECONDS - started)))
# A backstop in case the checks hang past their own budget. GNU timeout
# only: Windows has an unrelated timeout.exe.
backstop=()
if timeout --version 2>/dev/null | grep -q GNU; then
    backstop=(timeout --kill-after=5 "$((remaining + 20))")
fi
# In the background, waited for: a signal to this script interrupts `wait`
# at once (not after the checks finish), and cleanup stops the checks with
# everything else.
"${run_env[@]}" \
    GUI_WEBDRIVER_URL="http://127.0.0.1:$port" \
    GUI_APP="$launch" \
    GUI_UPDATER_APP="$launch_updater" \
    GUI_TEST_UPDATER_FILE="$checks_settings" \
    GUI_OUT_DIR="$out" \
    GUI_EXPORT_DIR="$export_dir" \
    GUI_BUDGET_SECS="$remaining" \
    GUI_STEP_TIMEOUT_SECS="$step_timeout" \
    GUI_SESSION_TIMEOUT_SECS="${GUI_CHECK_SESSION_TIMEOUT:-30}" \
    GUI_XVFB_DISPLAY="$xvfb_display" \
    GUI_XVFB_PID="$xvfb_pid" \
    "${backstop[@]}" "$runner" \
    2> >(tee -a "$out/steps.log" >&2) &
status=0
wait "$!" || status=$?
if [[ $platform == windows ]]; then
    kill "$timeline_pid" 2>/dev/null || true
    timeline_pid=""
    ((status == 0)) || windows_diagnostics
fi

# Whatever happened, results.txt names every check.
if ((status == 124 || status == 137)); then
    why="no result: the checks hung and were stopped $((remaining + 20))s in (see steps.log)"
else
    why="no result: the checks stopped early (exit $status, see steps.log)"
fi
complete_results "$why"
grep -q '^FAIL ' "$results" && ((status == 0)) && status=1
log "checks finished (exit $status) in $((SECONDS - started))s"
cat "$results" >&2
exit "$status"
