//! Regression checks on the packaging configuration Tauri and the AUR
//! `PKGBUILD` actually read — `tauri.conf.json` and `PKGBUILD` are data,
//! not code, so a drift between them and the crate (a version bump that
//! misses one of them, an icon path that no longer exists, a placeholder
//! left in place) fails silently at bundle/build time rather than at
//! `cargo check` time. These pin that down.

use serde_json::Value;
use yaml_rust2::{Yaml, YamlLoader};

const TAURI_CONF: &str = include_str!("../tauri.conf.json");

const CARGO_TOML: &str = include_str!("../Cargo.toml");

const PKGBUILD: &str = include_str!("../../packaging/PKGBUILD");

const LIB_RS: &str = include_str!("../src/lib.rs");

const CAPABILITY: &str = include_str!("../capabilities/default.json");

const TRUNK_TOML: &str = include_str!("../../Trunk.toml");

const INSTALL_SH: &str = include_str!("../../packaging/install.sh");

const WINDOWS_PACKAGING: &str = include_str!("../../packaging/windows/README.md");

const NSIS_HOOKS: &str = include_str!("../windows-installer-hooks.nsh");

const SIDECAR_CONF: &str = include_str!("../tauri.sidecar.conf.json");

const GITIGNORE: &str = include_str!("../.gitignore");

const CARGO_LOCK: &str = include_str!("../Cargo.lock");

const CLI_CARGO_TOML: &str = include_str!("../cli/Cargo.toml");

const CLI_MAIN_RS: &str = include_str!("../cli/src/main.rs");

const CORE_CARGO_TOML: &str = include_str!("../core/Cargo.toml");

const CORE_CONNECT_RS: &str = include_str!("../core/src/db/connect.rs");

const APP_MAIN_RS: &str = include_str!("../src/main.rs");

/// Crude but sufficient: pull `version = "X.Y.Z"` out of Cargo.toml's
/// `[package]` table without pulling in a TOML parser as a dev-only dep.
fn cargo_package_version() -> String {
    for line in CARGO_TOML.lines() {
        if let Some(rest) = line.trim().strip_prefix("version = \"")
            && let Some(end) = rest.find('"')
        {
            return rest[..end].to_string();
        }
    }
    panic!("Cargo.toml has no `version = \"...\"` line in [package]");
}

#[test]
fn tauri_conf_is_valid_json_with_expected_identity() {
    let conf: Value = serde_json::from_str(TAURI_CONF).expect("tauri.conf.json must be valid JSON");

    assert_eq!(
        conf["identifier"], "com.tossbaws.gospel-getter",
        "the app identifier drives the Tauri app data directory (see \
         db/legacy.rs, which hardcodes the pre-migration legacy path \
         alongside it) and the installers' registry/bundle identity — it \
         must not drift silently"
    );
    assert_eq!(
        conf["version"],
        cargo_package_version(),
        "tauri.conf.json's version must track Cargo.toml's package \
         version, or the built installers report the wrong version"
    );
    assert_eq!(conf["bundle"]["active"], true);
}

#[test]
fn tauri_conf_targets_all_platform_installers() {
    let conf: Value = serde_json::from_str(TAURI_CONF).unwrap();
    // "all" is what actually produces AppImage+deb on Linux and nsis+msi
    // on Windows in one shot; the dist CI workflow and packaging/install.sh
    // both assume `target/release/bundle/{appimage,deb,nsis,msi}` exist
    // after a build — narrowing this without updating those too would
    // silently break them.
    assert_eq!(
        conf["bundle"]["targets"], "all",
        "bundle.targets must stay \"all\" so both AppImage/deb (Linux) and \
         nsis/msi (Windows) get built without per-platform config drift"
    );
}

/// The copyright holder is Chi Rho Code LLC, but the publisher and the
/// identifier keep the old name: each names something on users' machines.
#[test]
fn tauri_conf_publisher_and_identifier_stay_put_under_the_new_copyright() {
    let conf: Value = serde_json::from_str(TAURI_CONF).unwrap();
    assert_eq!(
        conf["bundle"]["publisher"], "tossbaws",
        "the NSIS install folder lives under HKCU\\Software\\<publisher>; \
         changing it orphans existing installs, so it needs a migration \
         (see 2026-11-01 decision)"
    );
    assert_eq!(
        conf["identifier"], "com.tossbaws.gospel-getter",
        "the identifier names the data folder; changing it loses every \
         user's data"
    );
    let copyright = conf["bundle"]["copyright"]
        .as_str()
        .expect("bundle.copyright should be a string");
    assert!(
        copyright.contains("Chi Rho Code LLC"),
        "bundle.copyright ({copyright:?}) should name Chi Rho Code LLC, \
         the copyright holder in LICENSE"
    );
}

#[test]
fn tauri_conf_icon_paths_all_exist_on_disk() {
    let conf: Value = serde_json::from_str(TAURI_CONF).unwrap();
    let icons = conf["bundle"]["icon"]
        .as_array()
        .expect("bundle.icon should be an array of icon paths");
    assert!(
        !icons.is_empty(),
        "at least one icon path must be configured"
    );
    for icon in icons {
        let path = icon.as_str().expect("each icon entry should be a string");
        let full = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(path);
        assert!(
            full.is_file(),
            "bundle.icon references `{path}`, but no such file exists at \
             {full:?} — a missing icon fails the bundle step, not `cargo \
             build`, so this would otherwise only surface in CI"
        );
    }
}

#[test]
fn pkgbuild_version_tracks_cargo_toml() {
    let cargo_ver = cargo_package_version();
    let version = semver::Version::parse(&cargo_ver).expect("Cargo.toml's version is SemVer");
    // `_pkgver` is the version as the tag has it. pkgver can't hold a '-',
    // so a pre-release drops it and the pre-release's dots (2.19.0-beta.1
    // is 2.19.0beta1), which vercmp sorts before the release itself.
    let pkgver = format!(
        "{}.{}.{}{}",
        version.major,
        version.minor,
        version.patch,
        version.pre.as_str().replace('.', "")
    );
    // Whole lines, since `pkgver=` is also the end of `_pkgver=`.
    for expected in [format!("_pkgver={cargo_ver}"), format!("pkgver={pkgver}")] {
        assert!(
            PKGBUILD.lines().any(|line| line == expected),
            "PKGBUILD's versions should track Cargo.toml's ({cargo_ver}); \
             expected a `{expected}` line"
        );
    }
}

/// The setup.exe tells Explorer to refresh its icon cache once it has
/// installed (see the hook file's header). Tauri runs the hook only if
/// tauri.conf.json names it, and only the macro of that exact name.
#[test]
fn the_nsis_installer_refreshes_the_shell_icon_cache() {
    let conf: Value = serde_json::from_str(TAURI_CONF).unwrap();
    assert_eq!(
        conf["bundle"]["windows"]["nsis"]["installerHooks"], "windows-installer-hooks.nsh",
        "bundle.windows.nsis.installerHooks must name the hook file"
    );
    let body = NSIS_HOOKS
        .split("!macro NSIS_HOOK_POSTINSTALL")
        .nth(1)
        .and_then(|rest| rest.split("!macroend").next())
        .expect("the hook file defines NSIS_HOOK_POSTINSTALL");
    let call = body
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with("System::Call"))
        .expect("NSIS_HOOK_POSTINSTALL calls a Win32 function");
    assert_eq!(
        call, "System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0x1000, p 0, p 0)'",
        "SHChangeNotify with SHCNE_ASSOCCHANGED (0x08000000), flushed (SHCNF_FLUSH, 0x1000)"
    );
}

/// About's Go back runs the previous release's installer over this one
/// (`updater::install_previous_version`), and dist.yml's upgrade jobs run
/// the previous NSIS installer with /S over this build's. Both installers
/// refuse an older version without `allowDowngrades` (the NSIS installer
/// aborts; the MSI's MajorUpgrade gets no AllowDowngrades): Tauri's default
/// is true, and this keeps it said, so a change of default can't take Go
/// back away.
#[test]
fn the_windows_installers_allow_going_back_to_an_older_version() {
    let conf: Value = serde_json::from_str(TAURI_CONF).unwrap();
    assert_eq!(conf["bundle"]["windows"]["allowDowngrades"], true);
}

/// Windows Installer only takes a version of numbers, so the MSI target
/// refuses a pre-release such as `2.19.0-beta.1` unless
/// `bundle.windows.wix.version` gives the MSI its own. That version has to
/// sort below the release itself, so the release's MSI installs over the
/// pre-release's as an upgrade (Windows Installer compares only the first
/// three fields). A release has no such version: its MSI is numbered as
/// the app is.
#[test]
fn only_a_pre_release_gives_the_msi_its_own_version() {
    let conf: Value = serde_json::from_str(TAURI_CONF).unwrap();
    let app = semver::Version::parse(&cargo_package_version()).unwrap();
    let msi = conf["bundle"]["windows"]["wix"]["version"].as_str();
    if app.pre.is_empty() {
        assert_eq!(
            msi, None,
            "a release's MSI is numbered as the app is: remove bundle.windows.wix.version"
        );
        return;
    }
    let msi = msi.expect("a pre-release needs bundle.windows.wix.version for the MSI");
    let fields: Vec<u64> = msi
        .split('.')
        .map(|field| field.parse().expect("the MSI version is numbers only"))
        .collect();
    assert_eq!(
        fields.len(),
        3,
        "major.minor.patch: the fourth field doesn't order MSIs"
    );
    assert!(
        fields[0] <= 255 && fields[1] <= 255 && fields[2] <= 65535,
        "Windows Installer's limits are 255.255.65535, not {msi}"
    );
    assert!(
        (fields[0], fields[1], fields[2]) < (app.major, app.minor, app.patch),
        "the MSI version {msi} must sort below the release {}.{}.{}",
        app.major,
        app.minor,
        app.patch
    );
}

#[test]
fn pkgbuild_points_at_the_real_public_repo() {
    assert!(
        PKGBUILD.contains("https://github.com/tossbaws/gospel-getter"),
        "PKGBUILD's url= and source= should point at the real, public repo"
    );
    assert!(
        !PKGBUILD.contains("<TODO"),
        "PKGBUILD should no longer contain a placeholder repo URL"
    );
}

#[test]
fn pkgbuild_declares_the_tauri_runtime_dependencies() {
    for dep in ["webkit2gtk-4.1", "gtk3"] {
        assert!(
            PKGBUILD.contains(dep),
            "PKGBUILD's depends=() should list `{dep}` — Tauri's webview \
             needs it at runtime, not just to build"
        );
    }
}

#[test]
fn pkgbuild_installs_desktop_file_icon_and_license() {
    assert!(
        PKGBUILD.contains("usr/share/applications/gospel-getter.desktop"),
        "PKGBUILD should install a .desktop launcher"
    );
    assert!(
        PKGBUILD.contains("usr/share/icons/hicolor") && PKGBUILD.contains("apps/gospel-getter.png"),
        "PKGBUILD should install an icon into the hicolor theme"
    );
    assert!(
        PKGBUILD.contains("usr/share/licenses/$pkgname/LICENSE"),
        "PKGBUILD should install the LICENSE file, per Arch packaging \
         convention"
    );
}

/// The window opens at 1280×900 and can't be made smaller than 640×480:
/// the smallest size the layout is designed and checked for (with the book
/// and chapter picker as a popover, at Extra large text).
#[test]
fn tauri_conf_window_is_at_least_640_by_480() {
    let conf: Value = serde_json::from_str(TAURI_CONF).expect("tauri.conf.json must be valid JSON");
    let windows = conf["app"]["windows"]
        .as_array()
        .expect("app.windows should be an array");
    assert_eq!(windows.len(), 1, "one window");
    let window = &windows[0];
    assert_eq!(window["width"], 1280);
    assert_eq!(window["height"], 900);
    assert_eq!(window["minWidth"], 640);
    assert_eq!(window["minHeight"], 480);
    assert_eq!(window["resizable"], true);
}

/// The webview's own page zoom is off: Ctrl+=, Ctrl+- and Ctrl+0 are the
/// app's text-size keys (and Ctrl+wheel does nothing). With
/// `zoomHotkeysEnabled` false, WebView2's `IsZoomControlEnabled` is off on
/// Windows, and Tauri injects no zoom polyfill into WebKitGTK on Linux,
/// which has no zoom keys of its own.
#[test]
fn tauri_conf_turns_off_the_webview_page_zoom() {
    let conf: Value = serde_json::from_str(TAURI_CONF).expect("tauri.conf.json must be valid JSON");
    assert_eq!(conf["app"]["windows"][0]["zoomHotkeysEnabled"], false);
}

/// The window comes back where it was: the official window state plugin is
/// a dependency and is registered at startup, and the window is created by
/// the app (`create: false`), hidden (`visible: false`), so it's shown only
/// once its size and place are restored (see `window.rs`). The plugin is
/// used only from Rust, so the webview gets none of its permissions, nor
/// any window permission.
#[test]
fn the_window_state_plugin_restores_a_hidden_window_before_showing_it() {
    assert!(
        CARGO_TOML
            .lines()
            .any(|l| l.starts_with("tauri-plugin-window-state = \"~2.")),
        "tauri-plugin-window-state should be a desktop dependency"
    );
    assert!(LIB_RS.contains(".plugin(window::state_plugin(&app_data_dir)?)"));
    assert!(LIB_RS.contains("window::create_main_window(app)?;"));

    let conf: Value = serde_json::from_str(TAURI_CONF).unwrap();
    let window = &conf["app"]["windows"][0];
    assert_eq!(window["label"], "main");
    assert_eq!(window["create"], false);
    assert_eq!(window["visible"], false);
    // A new install still opens at 1280x900, with the native title bar.
    assert_eq!(
        (window["width"].as_u64(), window["height"].as_u64()),
        (Some(1280), Some(900))
    );
    assert!(window.get("decorations").is_none());

    let capability: Value = serde_json::from_str(CAPABILITY).unwrap();
    assert_eq!(capability["windows"], serde_json::json!(["main"]));
    assert_eq!(capability["permissions"], serde_json::json!([]));
}

/// The window's title is "Gospel Getter" all session (D24, see
/// `window.rs`): the main window is configured with it, and no code sets
/// the page's title or the window's after. A file dialog's title is a
/// literal (`.set_title("Export notes")`), so that's the only `set_title`.
#[test]
fn the_window_title_is_the_apps_name_and_nothing_changes_it() {
    let conf: Value = serde_json::from_str(TAURI_CONF).unwrap();
    assert_eq!(conf["app"]["windows"][0]["label"], "main");
    assert_eq!(conf["app"]["windows"][0]["title"], "Gospel Getter");

    fn sources(dir: &std::path::Path, found: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap_or_else(|e| panic!("{dir:?}: {e}")) {
            let path = entry.expect("a source entry").path();
            if path.is_dir() {
                sources(&path, found);
            } else if path.extension().is_some_and(|e| e == "rs") {
                found.push(path);
            }
        }
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut found = vec![root.join("ui/index.html"), root.join("ui/leptos-loader.js")];
    sources(&root.join("ui-leptos/src"), &mut found);
    sources(&root.join("src-tauri/src"), &mut found);
    let mut dialogs = 0;
    for path in &found {
        let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{path:?}: {e}"));
        assert!(
            !text.contains("document.title"),
            "{path:?} sets the page's title"
        );
        assert!(
            !text.contains("title_changed"),
            "{path:?} follows the page's title"
        );
        for (at, _) in text.match_indices("set_title(") {
            assert!(
                text[at + "set_title(".len()..].starts_with('"'),
                "{path:?} sets a title that isn't a dialog's literal"
            );
            dialogs += 1;
        }
    }
    assert!(dialogs > 0, "the file dialogs' titles weren't found");
}

/// One app at a time: the official single-instance plugin is a desktop
/// dependency, built for the Tauri 2 the app uses, and the first plugin
/// registered (as its documentation requires), so a second launch hands
/// over and exits before any other plugin or the app's own setup (the
/// database) starts. It has no commands, so the webview gets no permission
/// for it. That a second launch really exits is GUI check 73's.
#[test]
fn the_single_instance_plugin_is_registered_first() {
    let desktop = CARGO_TOML
        .split(
            "[target.'cfg(not(any(target_os = \"android\", target_os = \"ios\")))'.dependencies]",
        )
        .nth(1)
        .and_then(|rest| rest.split("\n[").next())
        .expect("Cargo.toml has a desktop dependencies table");
    assert!(
        desktop
            .lines()
            .any(|l| l.starts_with("tauri-plugin-single-instance = \"~2.")),
        "tauri-plugin-single-instance should be a desktop dependency"
    );
    assert!(
        CARGO_LOCK.contains("name = \"tauri-plugin-single-instance\"\nversion = \"2."),
        "Cargo.lock should resolve a 2.x single-instance plugin"
    );
    assert!(
        CARGO_LOCK.contains("name = \"tauri\"\nversion = \"2."),
        "the plugin is for Tauri 2"
    );

    let first = LIB_RS.find(".plugin(").expect("lib.rs registers plugins");
    assert!(
        LIB_RS[first..].starts_with(".plugin(window::single_instance_plugin())"),
        "the single-instance plugin should be registered before any other"
    );

    let capability: Value = serde_json::from_str(CAPABILITY).unwrap();
    assert_eq!(capability["permissions"], serde_json::json!([]));
}

/// The webview's Content Security Policy, as `tauri.conf.json` sets it.
/// Tauri adds hashes for the page's inline `<script>`s and `<style>` at
/// build time (asset CSP modification stays on), so the page's own code
/// runs and nothing else inline does. The page's Rust pieces (ui-leptos,
/// WebAssembly) are the app's own files too: `'self'` covers the glue
/// script and the fetch of the module, and `'wasm-unsafe-eval'` lets it be
/// compiled, which `'self'` alone does not. A wrong policy fails silently
/// in the built app, so its key directives are pinned here.
fn csp_directives() -> Vec<(String, Vec<String>)> {
    let conf: Value = serde_json::from_str(TAURI_CONF).expect("tauri.conf.json must be valid JSON");
    let security = &conf["app"]["security"];
    assert!(
        security
            .get("dangerousDisableAssetCspModification")
            .is_none(),
        "Tauri must keep hashing the inline script and style into the CSP"
    );
    assert!(
        security.get("assetProtocol").is_none(),
        "the asset protocol is off, so img-src needs no asset: source"
    );
    let csp = security["csp"]
        .as_str()
        .expect("app.security.csp is a policy string, not null");
    csp.split(';')
        .map(str::trim)
        .filter(|d| !d.is_empty())
        .map(|d| {
            let mut parts = d.split_whitespace().map(str::to_string);
            let name = parts.next().expect("a directive name");
            (name, parts.collect())
        })
        .collect()
}

#[test]
fn the_csp_allows_only_the_apps_own_code_and_the_ipc() {
    let directives = csp_directives();
    let get = |name: &str| -> Vec<&str> {
        let found: Vec<_> = directives.iter().filter(|(n, _)| n == name).collect();
        assert_eq!(found.len(), 1, "exactly one {name} directive");
        found[0].1.iter().map(String::as_str).collect()
    };
    for (name, sources) in &directives {
        assert!(
            !sources
                .iter()
                .any(|s| s == "'unsafe-eval'" || s == "'unsafe-hashes'" || s == "*"),
            "{name} must not allow eval, unsafe hashes or everything: {sources:?}"
        );
    }
    assert_eq!(get("default-src"), ["'self'"]);
    assert_eq!(
        get("script-src"),
        ["'self'", "'wasm-unsafe-eval'"],
        "no 'unsafe-inline' or remote scripts; WebAssembly compiles, and \
         nothing else is evaluated"
    );
    assert_eq!(
        get("style-src"),
        ["'self'"],
        "no 'unsafe-inline' styles either"
    );
    assert_eq!(get("img-src"), ["'self'"]);
    assert_eq!(get("font-src"), ["'self'"]);
    assert_eq!(
        get("connect-src"),
        ["'self'", "ipc:", "http://ipc.localhost"],
        "the app's own files (the WebAssembly module is fetched) and the \
         IPC origins (Linux and macOS, Windows): the updater, dialogs and \
         links all go through Rust"
    );
    for name in ["object-src", "base-uri", "form-action", "frame-ancestors"] {
        assert_eq!(get(name), ["'none'"], "{name}");
    }
    assert_eq!(directives.len(), 10, "{directives:?}");
}

/// The release overlay (`tauri.updater.conf.json`) merges into the config
/// for distribution builds; it must not loosen or drop the policy.
#[test]
fn the_release_overlay_leaves_the_csp_alone() {
    let overlay: Value = serde_json::from_str(include_str!("../tauri.updater.conf.json"))
        .expect("tauri.updater.conf.json must be valid JSON");
    assert!(overlay["app"]["security"].is_null(), "{overlay}");
}

/// The GUI checks' test build of the app (feature `gui-test-updater`: its
/// updater reads a test settings file, accepts a plain-HTTP server and
/// never installs; see `src/updater/test_hook.rs`).
const TEST_FEATURE: &str = "gui-test-updater";

/// The app's own package, which dist.yml's app builds name.
const APP_PACKAGE: &str = "gospel_getter";

/// What builds the test app in dist.yml, and the artifacts it goes into.
const TEST_BUILD_STEP: &str = "Build the GUI-check test app";

const TEST_ARTIFACT: &str = "gui-check-test-app-";

/// The jobs that build the test app, beside the linux and windows jobs.
const LINUX_TEST_APP_JOB: &str = "gui-test-app-linux";

const WINDOWS_TEST_APP_JOB: &str = "gui-test-app-windows";

// ---- The workflows, read as YAML
//
// The workflow tests read `.github` as trees (yaml-rust2, which keeps a
// mapping's keys in order, so jobs compare in the order a workflow lists
// them): their rules are about jobs, steps and keys, so quoting, indenting
// and comments change nothing they check.

/// The `.github` folder the workflow tests read: this repository's, or the
/// copy `PACKAGING_CONFIG_GITHUB` names (tests only, to run them against an
/// edited copy of the workflows).
fn github_dir() -> std::path::PathBuf {
    std::env::var_os("PACKAGING_CONFIG_GITHUB").map_or_else(
        || std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.github"),
        std::path::PathBuf::from,
    )
}

/// A workflow, or one of the repository's own actions: its name under
/// `.github` (`workflows/dist.yml`), its text and its tree.
struct GithubFile {
    name: String,
    text: String,
    yaml: Yaml,
}

/// The repository's workflows and its own actions: every one in their
/// folders, so a new one is checked too.
fn github_files() -> Vec<GithubFile> {
    let github = github_dir();
    let mut paths = Vec::new();
    let workflows = std::fs::read_dir(github.join("workflows")).expect(".github/workflows");
    for entry in workflows {
        let path = entry.expect("a workflow").path();
        if path
            .extension()
            .is_some_and(|ext| ext == "yml" || ext == "yaml")
        {
            paths.push((format!("workflows/{}", file_name(&path)), path));
        }
    }
    let actions = std::fs::read_dir(github.join("actions")).expect(".github/actions");
    for entry in actions {
        let dir = entry.expect("an action").path();
        for name in ["action.yml", "action.yaml"] {
            let path = dir.join(name);
            if path.is_file() {
                paths.push((format!("actions/{}/{name}", file_name(&dir)), path));
            }
        }
    }
    paths.sort();
    paths
        .into_iter()
        .map(|(name, path)| {
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            let yaml = parse(&text);
            GithubFile { name, text, yaml }
        })
        .collect()
}

/// One workflow, by its file name (`dist.yml`).
fn workflow(file: &str) -> GithubFile {
    let name = format!("workflows/{file}");
    github_files()
        .into_iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("no .github/{name}"))
}

/// A YAML document's tree.
fn parse(text: &str) -> Yaml {
    YamlLoader::load_from_str(text)
        .unwrap_or_else(|e| panic!("not YAML: {e}\n{text}"))
        .into_iter()
        .next()
        .unwrap_or(Yaml::Null)
}

fn file_name(path: &std::path::Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// A scalar as text: a string as it is, a number or a boolean as YAML
/// reads it (`30`, `false`). `None` for a mapping, a sequence or nothing.
fn scalar(value: &Yaml) -> Option<String> {
    match value {
        Yaml::String(s) | Yaml::Real(s) => Some(s.clone()),
        Yaml::Integer(i) => Some(i.to_string()),
        Yaml::Boolean(b) => Some(b.to_string()),
        _ => None,
    }
}

/// An `if:` as GitHub reads it: `${{ … }}` around all of it changes nothing.
fn condition(value: &Yaml) -> Option<String> {
    let text = scalar(value)?;
    let trimmed = text.trim();
    Some(
        trimmed
            .strip_prefix("${{")
            .and_then(|rest| rest.strip_suffix("}}"))
            .filter(|inner| !inner.contains("${{") && !inner.contains("}}"))
            .map_or(trimmed, str::trim)
            .to_owned(),
    )
}

/// What a value lists (`needs:`, `types:`): a sequence's items, or a lone
/// scalar as one.
fn list(value: &Yaml) -> Vec<String> {
    match value {
        Yaml::Array(items) => items.iter().filter_map(scalar).collect(),
        other => scalar(other).into_iter().collect(),
    }
}

/// A mapping's entries, as (key, value).
fn entries(value: &Yaml) -> Vec<(String, &Yaml)> {
    value
        .as_hash()
        .map(|hash| {
            hash.iter()
                .filter_map(|(k, v)| scalar(k).map(|k| (k, v)))
                .collect()
        })
        .unwrap_or_default()
}

/// A mapping's scalar entries as `key: value`, sorted (a `permissions:`
/// block, a `with:`), or a lone scalar (`permissions: read-all`) as itself.
fn grants(value: &Yaml) -> Vec<String> {
    if let Some(one) = scalar(value) {
        return vec![one];
    }
    let mut grants: Vec<String> = entries(value)
        .into_iter()
        .filter_map(|(k, v)| scalar(v).map(|v| format!("{k}: {v}")))
        .collect();
    grants.sort();
    grants
}

/// Every key and scalar in a tree, whole, in order.
fn scalars(value: &Yaml) -> Vec<String> {
    fn walk(value: &Yaml, out: &mut Vec<String>) {
        match value {
            Yaml::Hash(hash) => {
                for (k, v) in hash {
                    walk(k, out);
                    walk(v, out);
                }
            }
            Yaml::Array(items) => items.iter().for_each(|item| walk(item, out)),
            other => out.extend(scalar(other)),
        }
    }
    let mut out = Vec::new();
    walk(value, &mut out);
    out
}

/// [`scalars`], a line each (a `run:` as its lines).
fn lines(value: &Yaml) -> Vec<String> {
    scalars(value)
        .iter()
        .flat_map(|s| s.lines().map(str::to_owned).collect::<Vec<_>>())
        .collect()
}

/// [`lines`] without the shell's comments (a `run:`'s `# …` lines): what
/// runs or is set.
fn code(value: &Yaml) -> Vec<String> {
    lines(value)
        .into_iter()
        .filter(|l| !l.trim_start().starts_with('#'))
        .collect()
}

/// The value of every mapping entry named `key` in a tree, at any depth.
fn every<'a>(value: &'a Yaml, key: &str) -> Vec<&'a Yaml> {
    fn walk<'a>(value: &'a Yaml, key: &str, out: &mut Vec<&'a Yaml>) {
        match value {
            Yaml::Hash(hash) => {
                for (k, v) in hash {
                    if k.as_str() == Some(key) {
                        out.push(v);
                    }
                    walk(v, key, out);
                }
            }
            Yaml::Array(items) => items.iter().for_each(|item| walk(item, key, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(value, key, &mut out);
    out
}

/// A tree on one line (`{uses: …, with: {…}}`), to compare and to show.
fn flow(value: &Yaml) -> String {
    match value {
        Yaml::Hash(hash) => format!(
            "{{{}}}",
            hash.iter()
                .map(|(k, v)| format!("{}: {}", flow(k), flow(v)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Yaml::Array(items) => format!(
            "[{}]",
            items.iter().map(flow).collect::<Vec<_>>().join(", ")
        ),
        other => scalar(other).unwrap_or_else(|| "~".to_owned()),
    }
}

impl GithubFile {
    /// The workflow's jobs, in order.
    fn jobs(&self) -> Vec<Job<'_>> {
        self.yaml["jobs"]
            .as_hash()
            .map(|jobs| {
                jobs.iter()
                    .filter_map(|(id, yaml)| {
                        Some(Job {
                            id: id.as_str()?,
                            yaml,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn job(&self, id: &str) -> Job<'_> {
        self.jobs()
            .into_iter()
            .find(|job| job.id == id)
            .unwrap_or_else(|| panic!("{}: no job {id}", self.name))
    }

    /// Every job's steps, in order.
    fn steps(&self) -> Vec<Step<'_>> {
        self.jobs()
            .into_iter()
            .flat_map(|job| job.steps())
            .collect()
    }

    /// The workflow's own `env:` value for `key` (not a job's or a step's).
    fn env(&self, key: &str) -> Option<String> {
        scalar(&self.yaml["env"][key])
    }
}

/// A workflow's job, by its id.
#[derive(Debug, Clone, Copy)]
struct Job<'a> {
    id: &'a str,
    yaml: &'a Yaml,
}

impl<'a> Job<'a> {
    fn get(&self, key: &str) -> Option<String> {
        scalar(&self.yaml[key])
    }

    fn has(&self, key: &str) -> bool {
        !self.yaml[key].is_badvalue()
    }

    fn condition(&self) -> Option<String> {
        condition(&self.yaml["if"])
    }

    fn needs(&self) -> Vec<String> {
        list(&self.yaml["needs"])
    }

    fn env(&self, key: &str) -> Option<String> {
        scalar(&self.yaml["env"][key])
    }

    /// Its environment's name (`environment: release`, or a mapping's
    /// `name:`).
    fn environment(&self) -> Option<String> {
        self.get("environment")
            .or_else(|| scalar(&self.yaml["environment"]["name"]))
    }

    fn steps(&self) -> Vec<Step<'a>> {
        let job = self.id;
        self.yaml["steps"]
            .as_vec()
            .map(|steps| steps.iter().map(|yaml| Step { job, yaml }).collect())
            .unwrap_or_default()
    }

    /// Everything it says, a line each ([`lines`]).
    fn text(&self) -> String {
        lines(self.yaml).join("\n")
    }

    /// Its own keys and values, without its steps, a line each.
    fn own_lines(&self) -> Vec<String> {
        entries(self.yaml)
            .into_iter()
            .filter(|(k, _)| k != "steps")
            .flat_map(|(k, v)| std::iter::once(k).chain(lines(v)))
            .collect()
    }
}

/// One step of a job.
#[derive(Debug, Clone, Copy)]
struct Step<'a> {
    job: &'a str,
    yaml: &'a Yaml,
}

impl Step<'_> {
    fn name(&self) -> String {
        self.get("name").unwrap_or_default()
    }

    fn get(&self, key: &str) -> Option<String> {
        scalar(&self.yaml[key])
    }

    fn has(&self, key: &str) -> bool {
        !self.yaml[key].is_badvalue()
    }

    /// The action it uses, without the `@` and the ref after it.
    fn action(&self) -> Option<String> {
        self.get("uses")
            .map(|uses| uses.split('@').next().unwrap_or_default().to_owned())
    }

    fn uploads(&self) -> bool {
        self.action().as_deref() == Some("actions/upload-artifact")
    }

    fn downloads(&self) -> bool {
        self.action().as_deref() == Some("actions/download-artifact")
    }

    fn run(&self) -> String {
        self.get("run").unwrap_or_default()
    }

    /// Its `run:`'s lines, without the shell's comments.
    fn commands(&self) -> Vec<String> {
        code(&self.yaml["run"])
    }

    fn with(&self, key: &str) -> Option<String> {
        scalar(&self.yaml["with"][key])
    }

    fn env(&self, key: &str) -> Option<String> {
        scalar(&self.yaml["env"][key])
    }

    fn condition(&self) -> Option<String> {
        condition(&self.yaml["if"])
    }

    /// Everything it says, a line each ([`lines`]).
    fn text(&self) -> String {
        lines(self.yaml).join("\n")
    }

    /// What it runs or sets ([`code`]).
    fn code(&self) -> String {
        code(self.yaml).join("\n")
    }
}

/// A job's one step using `action`.
fn only_step<'a>(job: &Job<'a>, action: &str) -> Step<'a> {
    let found: Vec<Step<'a>> = job
        .steps()
        .into_iter()
        .filter(|s| s.action().as_deref() == Some(action))
        .collect();
    let [step] = found.as_slice() else {
        panic!("job {}: one {action}: {found:#?}", job.id);
    };
    *step
}

/// The test build can never be a release, or an installer, or anything a
/// packager builds: the feature is off by default; the AUR recipe, the
/// install script and the Windows packaging notes never name it (or turn
/// on every feature); and in dist.yml the installers' builds take no
/// features, while the test app is built only in its own steps, into its
/// own artifacts, which the release procedure never takes (it takes the
/// four installers by name), by a job of its own on each platform that
/// builds and uploads nothing else.
#[test]
fn no_release_or_packaging_build_enables_the_gui_test_updater() {
    // The crate: declared, and not a default.
    let features: Vec<&str> = CARGO_TOML
        .lines()
        .skip_while(|l| l.trim() != "[features]")
        .skip(1)
        .take_while(|l| !l.trim_start().starts_with('['))
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect();
    assert!(
        features.contains(&"gui-test-updater = []"),
        "the feature enables nothing else: {features:?}"
    );
    for line in &features {
        if line.starts_with("default") {
            assert!(!line.contains(TEST_FEATURE), "a default feature: {line}");
        }
    }

    // Packaging: never.
    for (file, text) in [
        ("packaging/PKGBUILD", PKGBUILD),
        ("packaging/install.sh", INSTALL_SH),
        ("packaging/windows/README.md", WINDOWS_PACKAGING),
    ] {
        assert!(!text.contains(TEST_FEATURE), "{file} names {TEST_FEATURE}");
        assert!(
            !text.contains("--all-features"),
            "{file} turns on every feature"
        );
    }

    // dist.yml.
    let dist = workflow("dist.yml");
    let steps = dist.steps();
    let builds_installers = |step: &Step| {
        step.commands()
            .iter()
            .any(|l| l.contains("cargo tauri build"))
    };
    assert!(
        steps.iter().filter(|s| builds_installers(s)).count() >= 2,
        "the installer builds weren't found: {steps:#?}"
    );
    let installer_artifact = |step: &Step| {
        step.with("name")
            .is_some_and(|n| n.starts_with("gospel-getter-"))
    };
    for step in &steps {
        let text = step.text();
        if builds_installers(step) {
            assert!(
                !text.contains("--features")
                    && !text.contains("--all-features")
                    && !text.contains(TEST_FEATURE),
                "an installer build takes features: {step:#?}"
            );
        }
        if text.contains(TEST_FEATURE) && step.name() != TEST_BUILD_STEP {
            // Only the GUI jobs, reading the test app, may name it otherwise.
            assert!(
                !text.contains("cargo ") || text.contains("scripts/gui-check.sh"),
                "{TEST_FEATURE} built outside \"{TEST_BUILD_STEP}\": {step:#?}"
            );
        }
        if step.uploads() && text.contains("gui-check-test-app") {
            assert!(
                step.with("name")
                    .is_some_and(|n| n.starts_with(TEST_ARTIFACT)),
                "the test app goes into its own artifact: {step:#?}"
            );
        }
        if step.uploads() && installer_artifact(step) {
            assert!(
                !text.contains("gui-check-test-app"),
                "an installer artifact takes the test app: {step:#?}"
            );
        }
    }
    let builds: Vec<&Step> = steps
        .iter()
        .filter(|s| s.name() == TEST_BUILD_STEP)
        .collect();
    assert_eq!(
        builds.iter().map(|s| s.job).collect::<Vec<_>>(),
        [LINUX_TEST_APP_JOB, WINDOWS_TEST_APP_JOB],
        "the test app is built by its own Linux and Windows jobs, once each"
    );
    for (build, own_job, artifact) in [
        ("linux", LINUX_TEST_APP_JOB, "gui-check-test-app-linux"),
        (
            "windows",
            WINDOWS_TEST_APP_JOB,
            "gui-check-test-app-windows",
        ),
    ] {
        // The installer job builds the installers and nothing with features.
        for step in dist.job(build).steps() {
            let text = step.text();
            assert!(
                !text.contains(TEST_FEATURE)
                    && !text.contains(TEST_ARTIFACT)
                    && !text.contains("--features"),
                "the {build} job builds or uploads the test app again: {step:#?}"
            );
        }
        // Its own job: no installer, no release artifact, only the test app's.
        let own = dist.job(own_job).steps();
        assert!(
            !own.iter().any(|s| s.text().contains("cargo tauri")),
            "{own_job} builds an installer: {own:#?}"
        );
        let uploads: Vec<Option<String>> = own
            .iter()
            .filter(|s| s.uploads())
            .map(|s| s.with("name"))
            .collect();
        assert_eq!(
            uploads,
            [Some(artifact.to_owned())],
            "{own_job} uploads something other than the test app"
        );
    }
    for build in builds {
        assert!(
            build.run().contains(&format!(
                "cargo build --release --locked -p {APP_PACKAGE} --features {TEST_FEATURE}"
            )),
            "{build:#?}"
        );
        let job = dist.job(build.job).steps();
        let at = job
            .iter()
            .position(|s| s.name() == TEST_BUILD_STEP)
            .expect("the build step is in its job");
        for (i, step) in job.iter().enumerate() {
            if step.uploads() && installer_artifact(step) {
                assert!(
                    i < at,
                    "job {}: an installer is uploaded after the test app is built: {step:#?}",
                    build.job
                );
            }
        }
    }
}

/// The Linux test app is built by a job of its own beside the linux job, so
/// gui-linux waits for the longer of the two builds rather than for both in
/// turn, and waits for both: the AppImage from one and the test app from
/// the other, from the same commit. That job runs on the linux job's
/// Ubuntu (gui-linux runs the binary on that WebKitGTK), under a name no
/// required check has, and adds no Rust cache: it restores the linux job's
/// and never saves, which hits only while both jobs name the same
/// `shared-key` (the linux job's id, which keeps that job's key as it was),
/// install the same Rust and cache the same workspaces (see the job's cache
/// step in dist.yml for rust-cache's key).
#[test]
fn the_linux_test_app_is_built_beside_the_installers_from_their_cache() {
    test_app_is_built_beside(LINUX_TEST_APP_JOB, "linux", "gui-linux", "ubuntu-22.04");
}

/// The same for Windows: gui-test-app-windows beside the windows job, on
/// its runner, from its cache, which gui-windows (and sign after it) waits
/// for with the installers.
#[test]
fn the_windows_test_app_is_built_beside_the_installers_from_their_cache() {
    test_app_is_built_beside(
        WINDOWS_TEST_APP_JOB,
        "windows",
        "gui-windows",
        "windows-latest",
    );
}

/// A test-app job (`own`) beside its installer job (`build`), as the tests
/// above say, for the GUI job that runs both.
fn test_app_is_built_beside(own_job: &str, build: &str, gui: &str, runs_on: &str) {
    let dist = workflow("dist.yml");
    assert_eq!(
        dist.job(gui).needs(),
        [build, own_job],
        "{gui} should wait for the installers and the test app"
    );

    let (own, installers) = (dist.job(own_job), dist.job(build));
    assert_eq!(own.get("runs-on").as_deref(), Some(runs_on), "{own:#?}");
    assert_eq!(own.get("runs-on"), installers.get("runs-on"));
    assert!(own.needs().is_empty(), "it runs beside the {build} job");
    let name = own
        .get("name")
        .unwrap_or_else(|| panic!("no name: {own:#?}"));
    assert!(!name.contains("${{"), "{name}");
    for required in [
        "Linux (AppImage + deb)",
        "Windows (NSIS installer)",
        "GUI checks (Linux)",
    ] {
        assert!(
            !name.contains(required),
            "{own_job}'s name could pass for {required}: {name}"
        );
    }

    let rust = "dtolnay/rust-toolchain";
    assert_eq!(
        flow(only_step(&own, rust).yaml),
        flow(only_step(&installers, rust).yaml),
        "another Rust install misses the {build} job's cache"
    );
    let cache = "Swatinem/rust-cache";
    let (own_cache, build_cache) = (only_step(&own, cache), only_step(&installers, cache));
    assert_eq!(own_cache.with("shared-key").as_deref(), Some(build));
    assert_eq!(
        build_cache.with("shared-key").as_deref(),
        Some(build),
        "the {build} job's key would change"
    );
    assert_eq!(
        own_cache.with("save-if").as_deref(),
        Some("false"),
        "{own_job} saves a cache entry of its own"
    );
    assert_eq!(
        build_cache.with("save-if").as_deref(),
        Some("${{ github.ref == 'refs/heads/main' }}")
    );
    for cache in [own_cache, build_cache] {
        assert_eq!(cache.with("key"), None, "{cache:#?}");
    }
    // `workspaces:`, a line each.
    let workspaces = |step: &Step| -> Vec<String> {
        step.with("workspaces")
            .unwrap_or_default()
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_owned)
            .collect()
    };
    assert_eq!(workspaces(&build_cache), ["src-tauri", "ui-leptos"]);
    assert_eq!(
        workspaces(&own_cache),
        workspaces(&build_cache),
        "other workspaces miss the {build} job's cache"
    );
}

/// What a run for a pull request label other than `ci:full` skips: the
/// builds (and with them every job after them).
const LABEL_GATE: &str = "github.event.action != 'labeled' || github.event.label.name == 'ci:full'";

/// A release's installers come from the run dispatched on its release commit
/// after the merge (`gh workflow run dist.yml`), which joined the same
/// concurrency group as the merge's push run and cancelled it: a red X on
/// every release commit. So the push of a release-prep merge builds nothing,
/// and a dispatched run never cancels the run in progress (it is a group of
/// its own, too: `a_push_never_cancels_a_dispatched_run`). Pull requests (a
/// release/ branch's too) and other pushes build as before; so does a
/// dispatched run. The only other run that skips the builds is one for a
/// pull request label other than `ci:full` ([`LABEL_GATE`]).
#[test]
fn a_release_merge_never_cancels_its_own_build() {
    let dist = workflow("dist.yml");
    for job in ["linux", LINUX_TEST_APP_JOB, "windows"] {
        let condition = dist
            .job(job)
            .condition()
            .unwrap_or_else(|| panic!("job {job} has no `if:`"));
        assert_eq!(
            condition,
            format!(
                "(github.event_name != 'push' || !startsWith(github.event.head_commit.message, \
                 'chore(release):')) && ({LABEL_GATE})"
            ),
            "job {job} builds on a release-prep merge's push, or skips more than that and \
             another label"
        );
    }

    let cancel = scalar(&dist.yaml["concurrency"]["cancel-in-progress"])
        .expect("dist.yml's concurrency has a cancel-in-progress");
    assert_ne!(
        cancel, "true",
        "a dispatched run would cancel the run in progress on its commit"
    );
    assert!(
        cancel.contains("github.event_name != 'workflow_dispatch'"),
        "only a dispatched run should leave the run in progress be: {cancel}"
    );
}

/// The program every installer carries beside the app (`gospel-getter-cli`,
/// A3), as a sidecar named in an overlay. `tauri.conf.json` names none:
/// tauri-build wants a sidecar's file there on every build it's named for, and
/// `cargo build` and `cargo test` come before the file does.
const SIDECAR: &str = "binaries/gospel-getter-cli";

const SIDECAR_STEP: &str = "Build the CLI sidecar";

#[test]
fn the_sidecar_is_named_in_its_overlay_alone() {
    let overlay: Value = serde_json::from_str(SIDECAR_CONF).unwrap();
    assert_eq!(
        overlay["bundle"]["externalBin"],
        serde_json::json!([SIDECAR])
    );
    assert_eq!(
        overlay["bundle"].as_object().map(|b| b.len()),
        Some(1),
        "it changes nothing else: {overlay}"
    );
    let conf: Value = serde_json::from_str(TAURI_CONF).unwrap();
    assert!(conf["bundle"].get("externalBin").is_none());
    let updater: Value = serde_json::from_str(include_str!("../tauri.updater.conf.json")).unwrap();
    assert!(updater["bundle"].get("externalBin").is_none());
    assert!(
        GITIGNORE.lines().any(|l| l == "/binaries/"),
        "the built sidecar stays out of the repository"
    );
}

/// Every installer build takes the overlay, after its job has built the
/// sidecar: on its own, without features, from the lock file.
#[test]
fn every_installer_build_carries_the_sidecar() {
    let dist = workflow("dist.yml");
    for job in ["linux", "windows"] {
        let steps = dist.job(job).steps();
        let built_at = steps
            .iter()
            .position(|s| s.name() == SIDECAR_STEP)
            .unwrap_or_else(|| panic!("job {job} has no \"{SIDECAR_STEP}\" step"));
        let build = steps[built_at].run();
        assert!(
            build.contains("cargo build --release --locked -p gospel-getter-cli"),
            "{build}"
        );
        assert!(!build.contains("--features"), "{build}");
        assert!(build.contains("binaries/gospel-getter-cli-"), "{build}");
        let mut installer_builds = 0;
        for (at, step) in steps.iter().enumerate() {
            for line in step
                .commands()
                .iter()
                .filter(|l| l.trim_start().starts_with("cargo tauri build"))
            {
                installer_builds += 1;
                assert!(
                    line.contains("--config tauri.sidecar.conf.json"),
                    "an installer without the sidecar: {line}"
                );
                assert!(at > built_at, "built before the sidecar: {line}");
            }
        }
        assert!(installer_builds > 0, "job {job} builds no installers");
    }
}

/// The app's four builds in dist.yml (both installers and both test apps)
/// build the app's package alone, after the sidecar step built the CLI: all
/// four, so the test app is the same build variant as the installers (see
/// dist.yml's header).
#[test]
fn every_app_build_builds_only_the_app() {
    let dist = workflow("dist.yml");
    let mut installers = Vec::new();
    let mut test_apps = Vec::new();
    for step in dist.steps() {
        for line in step.commands() {
            let line = line.trim_start().to_owned();
            if line.starts_with("cargo tauri build") {
                installers.push((step.job, line));
            } else if step.name() == TEST_BUILD_STEP && line.starts_with("cargo build") {
                test_apps.push((step.job, line));
            }
        }
    }
    assert_eq!(
        installers.iter().map(|(job, _)| *job).collect::<Vec<_>>(),
        ["linux", "windows"],
        "{installers:#?}"
    );
    assert_eq!(
        test_apps.iter().map(|(job, _)| *job).collect::<Vec<_>>(),
        [LINUX_TEST_APP_JOB, WINDOWS_TEST_APP_JOB],
        "{test_apps:#?}"
    );
    let package = format!(" -p {APP_PACKAGE}");
    for (job, line) in installers {
        assert!(
            line.ends_with(&format!(" --{package}")),
            "job {job}: an installer build that doesn't name the app's package: {line}"
        );
    }
    for (job, line) in test_apps {
        assert!(
            line.contains(&format!("{package} ")),
            "job {job}: a test-app build that doesn't name the app's package: {line}"
        );
    }
}

/// AppImage users can't reach a sidecar (F8), so the program goes up on its
/// own too, from the job that built it.
#[test]
fn the_cli_is_also_uploaded_on_its_own() {
    let dist = workflow("dist.yml");
    for (job, artifact, file) in [
        (
            "linux",
            "gospel-getter-cli-linux-x86_64",
            "src-tauri/binaries/gospel-getter-cli-x86_64-unknown-linux-gnu",
        ),
        (
            "windows",
            "gospel-getter-cli-windows-x86_64",
            "src-tauri/binaries/gospel-getter-cli-x86_64-pc-windows-msvc.exe",
        ),
    ] {
        let uploads: Vec<Step> = dist
            .job(job)
            .steps()
            .into_iter()
            .filter(|s| s.with("name").as_deref() == Some(artifact))
            .collect();
        assert_eq!(uploads.len(), 1, "job {job} uploads {artifact} once");
        let upload = &uploads[0];
        assert!(upload.uploads(), "{upload:#?}");
        assert_eq!(upload.with("path").as_deref(), Some(file), "{upload:#?}");
        assert_eq!(
            upload.with("if-no-files-found").as_deref(),
            Some("error"),
            "{upload:#?}"
        );
    }
}

/// Each kind of installer is checked for it, once installed.
#[test]
fn the_smoke_tests_check_the_sidecar_is_there() {
    let dist = workflow("dist.yml");
    let smoke = |job: &str, name: &str| -> String {
        dist.job(job)
            .steps()
            .into_iter()
            .find(|s| s.name().contains(name))
            .unwrap_or_else(|| panic!("no {name} step in {job}"))
            .run()
    };
    assert!(smoke("linux", "AppImage extracts").contains("usr/bin/gospel-getter-cli --version"));
    let deb = smoke("linux", ".deb installs");
    assert!(deb.contains("'/usr/bin/gospel-getter-cli'"), "{deb}");
    assert!(deb.contains("gospel-getter-cli --version"), "{deb}");
    let nsis = smoke("windows", "silent install");
    assert!(nsis.contains("gospel-getter-cli.exe"), "{nsis}");
    assert!(nsis.contains("--version"), "{nsis}");
}

/// The app never runs its sidecar, so it gains no plugin or permission to, and
/// the page none at all.
#[test]
fn the_app_gets_no_way_to_run_the_sidecar() {
    assert!(!CARGO_TOML.contains("tauri-plugin-shell"));
    assert!(!LIB_RS.contains("tauri_plugin_shell"));
    let capability: Value = serde_json::from_str(CAPABILITY).unwrap();
    assert_eq!(capability["permissions"], serde_json::json!([]));
}

#[test]
fn the_cli_is_the_apps_version_and_finds_the_apps_data() {
    let version = cargo_package_version();
    assert!(
        CLI_CARGO_TOML
            .lines()
            .any(|l| l == format!("version = \"{version}\"")),
        "cli/Cargo.toml's version should track Cargo.toml's ({version})"
    );
    let conf: Value = serde_json::from_str(TAURI_CONF).unwrap();
    let identifier = conf["identifier"].as_str().unwrap();
    assert!(
        CORE_CONNECT_RS.contains(&format!(
            "pub const APP_IDENTIFIER: &str = \"{identifier}\";"
        )),
        "the CLI reads the app's data folder by its identifier, {identifier}"
    );
}

/// On Windows the app is a GUI program and the CLI a console one, so it prints
/// in a terminal; the app stays as it was.
#[test]
fn only_the_app_is_a_windows_gui_program() {
    assert!(APP_MAIN_RS.contains("windows_subsystem = \"windows\""));
    assert!(!CLI_MAIN_RS.contains("#![cfg_attr(") && !CLI_MAIN_RS.contains("#![windows_subsystem"));
}

/// Packages named in Cargo.lock, with what each depends on.
fn lock_packages() -> std::collections::BTreeMap<String, Vec<String>> {
    let mut packages: std::collections::BTreeMap<String, Vec<String>> = Default::default();
    let mut name = String::new();
    let mut in_dependencies = false;
    for line in CARGO_LOCK.lines() {
        if let Some(found) = line
            .strip_prefix("name = \"")
            .and_then(|r| r.strip_suffix('"'))
        {
            name = found.to_string();
            packages.entry(name.clone()).or_default();
        } else if line == "dependencies = [" {
            in_dependencies = true;
        } else if line == "]" {
            in_dependencies = false;
        } else if in_dependencies {
            // `"name",` or `"name version",` where two versions are in the lock.
            let dependency = line.trim().trim_matches([',', '"']);
            let dependency = dependency.split(' ').next().unwrap_or(dependency);
            packages
                .entry(name.clone())
                .or_default()
                .push(dependency.to_string());
        }
    }
    packages
}

/// `gospel-getter-cli` links neither GTK nor WebKit (A3), and no HTTP or other
/// network code (F4): nothing in its dependency tree, dev and build
/// dependencies included, can bring one in.
#[test]
fn the_cli_depends_on_no_webview_and_no_network() {
    let packages = lock_packages();
    let mut seen = std::collections::BTreeSet::new();
    let mut waiting = vec!["gospel-getter-cli".to_string()];
    while let Some(name) = waiting.pop() {
        if !seen.insert(name.clone()) {
            continue;
        }
        let dependencies = packages
            .get(&name)
            .unwrap_or_else(|| panic!("{name} isn't in Cargo.lock"));
        waiting.extend(dependencies.iter().cloned());
    }
    assert!(seen.contains("rmcp") && seen.contains("gospel_getter_core"));
    for package in &seen {
        let banned = package.starts_with("tauri")
            || package.starts_with("webkit2gtk")
            || package.starts_with("gtk")
            || package.starts_with("gdk")
            || [
                "wry", "tao", "glib", "hyper", "reqwest", "axum", "tower", "rustls",
            ]
            .contains(&package.as_str());
        assert!(!banned, "gospel-getter-cli depends on {package}");
    }
    for manifest in [CLI_CARGO_TOML, CORE_CARGO_TOML] {
        assert!(!manifest.contains("tauri"), "{manifest}");
    }
    let rmcp = CLI_CARGO_TOML
        .lines()
        .find(|l| l.starts_with("rmcp = "))
        .expect("rmcp is a dependency");
    assert!(
        rmcp.contains("default-features = false")
            && rmcp.contains("\"server\"")
            && rmcp.contains("\"transport-io\"")
            && !rmcp.contains("http"),
        "{rmcp}"
    );
}

/// The translations' text is embedded once. As a `const` it's copied into each
/// crate instantiating what uses it, and the app's binary grows by all of it.
#[test]
fn the_translations_text_is_a_static() {
    let seed = include_str!("../core/src/db/seed.rs");
    assert!(seed.contains("\nstatic TEXTS: "), "see `seed::TEXTS`");
}

/// The AUR recipe's own build makes the CLI too; it should install it.
#[test]
fn pkgbuild_installs_the_cli() {
    assert!(PKGBUILD.contains("usr/bin/gospel-getter-cli"));
}

/// The page (dist/) isn't in the source archive, and plain cargo doesn't run
/// Tauri's beforeBuildCommand, so the AUR recipe builds it with Trunk before
/// the app, for WebAssembly, with Trunk's tools from its own sources and
/// nothing downloaded during build() (GG-20). tools/ci checks those tools are
/// CI's.
#[test]
fn pkgbuild_builds_the_page_before_the_app() {
    let build: String = PKGBUILD
        .lines()
        .skip_while(|l| *l != "build() {")
        .take_while(|l| *l != "}")
        .collect::<Vec<_>>()
        .join("\n");
    let page = build
        .find("trunk build --release --offline --frozen")
        .unwrap_or_else(|| panic!("build() doesn't build the page offline: {build}"));
    let app = build
        .find("cargo build --release --frozen")
        .unwrap_or_else(|| panic!("build() doesn't build the app offline: {build}"));
    assert!(page < app, "the app is built before its page: {build}");
    assert!(
        PKGBUILD
            .lines()
            .any(|l| l.starts_with("makedepends=(") && l.contains("'rust-wasm'")),
        "the recipe needs the wasm32 target (rust-wasm)"
    );
    for releases in [
        "https://github.com/trunk-rs/trunk/releases/download/",
        "https://github.com/wasm-bindgen/wasm-bindgen/releases/download/",
        "https://github.com/WebAssembly/binaryen/releases/download/",
    ] {
        assert_eq!(
            PKGBUILD.matches(releases).count(),
            2,
            "{releases} should be a source for x86_64 and for aarch64"
        );
    }
}

/// gui-windows launches the installed app by its name, the app crate's
/// binary, which gui-test-app-windows also builds as the test app, never the
/// first program its folder lists: the CLI sidecar sits beside it (GG-19).
#[test]
fn the_windows_gui_checks_launch_the_app_by_its_name() {
    let package = CARGO_TOML
        .lines()
        .find_map(|l| l.strip_prefix("name = \""))
        .and_then(|rest| rest.strip_suffix('"'))
        .expect("Cargo.toml's [package] name");
    let exe = format!("{package}.exe");
    let dist = workflow("dist.yml");
    let gui = dist.job("gui-windows");
    assert_eq!(
        gui.env("GUI_EXE").as_deref(),
        Some(exe.as_str()),
        "gui-windows should name the app's program, {exe}"
    );
    let install = gui
        .steps()
        .into_iter()
        .find(|s| s.name().starts_with("Install the app"))
        .expect("gui-windows installs the app");
    let run = install.run();
    assert!(run.contains("$env:GUI_EXE"), "{install:#?}");
    assert!(
        !run.lines()
            .skip_while(|l| !l.contains("$location"))
            .any(|l| l.contains("Select-Object -First 1")),
        "the app's program is picked by the folder's order: {install:#?}"
    );
    for name in ["gospel-getter-cli.exe", "uninstall.exe"] {
        assert!(run.contains(name), "{name} isn't known: {install:#?}");
    }
    let test_app = dist
        .job(WINDOWS_TEST_APP_JOB)
        .steps()
        .into_iter()
        .find(|s| s.name() == TEST_BUILD_STEP)
        .expect("gui-test-app-windows builds the test app");
    assert!(
        test_app.run().contains(&format!("target/release/{exe}")),
        "{test_app:#?}"
    );
}

/// GitHub expressions treat '' as false, so `a && '' || b` always gives b:
/// the bundle job's FROM did, and failed release run 43641319812 with it.
#[test]
fn no_workflow_expression_has_an_empty_and_branch() {
    for (expression, empty) in [
        ("a && '' || b", true),
        ("a && \"\" || b", true),
        ("a != 'x' && 'y' || ''", false),
        ("a && '''s' || b", false),
        ("a == 'x && ''' || b", false),
    ] {
        assert_eq!(has_empty_and_branch(expression), empty, "{expression}");
    }
    let mut seen = Vec::new();
    for file in github_files() {
        for expression in workflow_expressions(&file.yaml) {
            seen.push((file.name.clone(), expression));
        }
    }
    assert!(
        seen.iter()
            .any(|(_, e)| e.contains("&& 'gospel-getter-release-bundle' ||")),
        "the scan reads the bundle job's env: {seen:#?}"
    );
    let empty: Vec<_> = seen
        .iter()
        .filter(|(_, e)| has_empty_and_branch(e))
        .collect();
    assert!(
        empty.is_empty(),
        "put the empty branch after ||, not after &&: {empty:#?}"
    );
}

/// Every expression in a workflow or action: each `${{ … }}` in a key or a
/// value, and each bare `if:`, which GitHub reads as one.
fn workflow_expressions(yaml: &Yaml) -> Vec<String> {
    let mut expressions = Vec::new();
    for text in scalars(yaml) {
        let mut rest = text.as_str();
        while let Some(start) = rest.find("${{") {
            let after = &rest[start + 3..];
            let Some(end) = after.find("}}") else { break };
            expressions.push(after[..end].trim().to_owned());
            rest = &after[end + 2..];
        }
    }
    for value in every(yaml, "if").into_iter().filter_map(scalar) {
        if !value.contains("${{") {
            expressions.push(value.trim().to_owned());
        }
    }
    expressions
}

/// Whether an `&&` outside a string literal is followed by an empty one
/// (`''` that isn't the start of an escaped quote, or `""`).
fn has_empty_and_branch(expression: &str) -> bool {
    let bytes = expression.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            // Past a literal, in which '' is an escaped quote.
            b'\'' => {
                i += 1;
                while i < bytes.len() {
                    if bytes[i] == b'\'' {
                        if bytes.get(i + 1) == Some(&b'\'') {
                            i += 2;
                            continue;
                        }
                        break;
                    }
                    i += 1;
                }
                i += 1;
            }
            b'&' if bytes.get(i + 1) == Some(&b'&') => {
                match expression[i + 2..].trim_start().as_bytes() {
                    [b'\'', b'\'', b'\'', ..] => {}
                    [b'\'', b'\'', ..] | [b'"', b'"', ..] => return true,
                    _ => {}
                }
                i += 2;
            }
            _ => i += 1,
        }
    }
    false
}

/// Every workflow's token reads only, unless a job needs more (F-08). Each
/// workflow declares its own top-level `permissions:`, since without one
/// the token gets the repository's default, which can be write access to
/// everything. No workflow or job asks for `write-all`, and each grant of
/// write access is one listed here with what it's for; `contents: write`
/// is none of them.
#[test]
fn every_workflow_declares_its_token_and_writes_only_where_listed() {
    // (workflow, job, grant); "" is the workflow's own block.
    const WRITES: [(&str, &str, &str); 5] = [
        // Deletes a closed pull request's caches and main's superseded ones.
        ("workflows/cache-cleanup.yml", "", "actions: write"),
        // Deletes the Distribution run that ci:full superseded.
        ("workflows/dist.yml", "supersede", "actions: write"),
        // Starts the deploy on main when a release is published.
        ("workflows/pages.yml", "from-release", "actions: write"),
        // GitHub Pages' own deploy (actions/deploy-pages).
        ("workflows/pages.yml", "deploy", "id-token: write"),
        ("workflows/pages.yml", "deploy", "pages: write"),
    ];
    let mut wrong = Vec::new();
    let mut writes = Vec::new();
    for file in github_files() {
        if !file.name.starts_with("workflows/") {
            continue;
        }
        let own = &file.yaml["permissions"];
        if own.is_badvalue() {
            wrong.push(format!("{} declares no top-level permissions:", file.name));
            continue;
        }
        let mut blocks = vec![(String::new(), grants(own))];
        for job in file.jobs() {
            if job.has("permissions") {
                blocks.push((job.id.to_owned(), grants(&job.yaml["permissions"])));
            }
        }
        for (job, grants) in blocks {
            for grant in grants {
                if grant.contains("write-all") {
                    wrong.push(format!("{} {job:?}: {grant}", file.name));
                } else if grant.ends_with(": write") {
                    writes.push((file.name.clone(), job.clone(), grant));
                }
            }
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
    let mut listed: Vec<(String, String, String)> = WRITES
        .iter()
        .map(|(f, j, g)| ((*f).to_owned(), (*j).to_owned(), (*g).to_owned()))
        .collect();
    listed.sort();
    writes.sort();
    assert_eq!(
        writes, listed,
        "a write grant not listed here, or a listed one gone"
    );
}

/// Every action a workflow or the repository's own action uses is pinned to
/// a commit, not a tag or branch that can be moved under it (D-01).
/// Dependabot keeps the pins up to date (`.github/dependabot.yml`), with the
/// release each is in as a comment after it, for the reader. The
/// repository's own actions (`./…`) are the commit's own.
#[test]
fn every_action_is_pinned_to_a_commit() {
    let files = github_files();
    assert!(
        files.iter().any(|f| f.name == "workflows/dist.yml")
            && files.iter().any(|f| f.name.starts_with("actions/")),
        "{:?}",
        files.iter().map(|f| &f.name).collect::<Vec<_>>()
    );
    let mut pinned = 0;
    for file in &files {
        for uses in every(&file.yaml, "uses").into_iter().filter_map(scalar) {
            if uses.starts_with("./") {
                continue;
            }
            let place = format!("{}: uses: {uses}", file.name);
            let (_, commit) = uses
                .split_once('@')
                .unwrap_or_else(|| panic!("{place}: not pinned"));
            assert!(
                commit.len() == 40
                    && commit
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
                "{place}: pinned to `{commit}`, not a full commit SHA"
            );
            pinned += 1;
        }
    }
    assert!(pinned > 0, "no actions found");
}

/// Every job in every workflow has a time limit (`timeout-minutes`), so a
/// runner that hangs fails in minutes instead of holding on for GitHub's
/// six hours (M6: a dispatch once sat 28 minutes in `apt-get`). Every
/// `apt-get update` and `apt-get install` retries a failed download and
/// gives up on a mirror that stops answering, in a step with a time limit
/// of its own, so a hung mirror fails that step, not the whole job's time.
/// Only that each is there: the values are the workflows' to choose.
#[test]
fn every_job_has_a_time_limit_and_apt_retries() {
    let workflows: Vec<GithubFile> = github_files()
        .into_iter()
        .filter(|f| f.name.starts_with("workflows/"))
        .collect();
    assert!(
        workflows.iter().any(|f| f.name == "workflows/dist.yml"),
        "{:?}",
        workflows.iter().map(|f| &f.name).collect::<Vec<_>>()
    );
    let mut apt = 0;
    for file in &workflows {
        let jobs = file.jobs();
        assert!(!jobs.is_empty(), "{} has no jobs", file.name);
        for job in &jobs {
            assert!(
                job.has("timeout-minutes"),
                "{}: job {} has no timeout-minutes",
                file.name,
                job.id
            );
        }
        for step in file.steps() {
            let installs: Vec<String> = step
                .commands()
                .into_iter()
                .filter(|l| l.contains("apt-get update") || l.contains("apt-get install"))
                .collect();
            if installs.is_empty() {
                continue;
            }
            for line in &installs {
                assert!(
                    line.contains(" -o Acquire::Retries=")
                        && line.contains(" -o Acquire::http::Timeout="),
                    "{}: job {}: apt-get without its retries: {line}",
                    file.name,
                    step.job
                );
                apt += 1;
            }
            assert!(
                step.has("timeout-minutes"),
                "{}: job {}: the step running apt-get has no timeout-minutes of its own: {step:#?}",
                file.name,
                step.job
            );
        }
    }
    assert!(apt > 0, "no apt-get found");
}

/// Both GUI jobs run the suite once more when it fails
/// (`GUI_CHECK_RETRY`, scripts/gui-check.sh), which marks a check that
/// passes only the second time FLAKY, and keeps both attempts' results in
/// the uploaded artifact (attempt 1's under the run's out-dir). Nothing in
/// CI builds the checks with their `force-fail` feature, or names the
/// variable it reads: a check is only ever failed by hand, to try the retry.
/// Nor does CI repeat a check (`GUI_CHECK_REPEAT`, for chasing a flake by
/// hand).
#[test]
fn the_gui_checks_retry_once_in_ci_and_never_force_a_failure() {
    let dist = workflow("dist.yml");
    for job in ["gui-linux", "gui-windows"] {
        let gui = dist.job(job);
        assert_eq!(
            gui.env("GUI_CHECK_RETRY").as_deref(),
            Some("1"),
            "job {job} doesn't retry the suite"
        );
        let steps = gui.steps();
        let run = steps
            .iter()
            .find(|s| s.run().contains("scripts/gui-check.sh"))
            .unwrap_or_else(|| panic!("job {job} doesn't run the checks"));
        let out = run
            .commands()
            .iter()
            .find_map(|l| {
                l.split_once("scripts/gui-check.sh ")
                    .map(|(_, rest)| rest.to_owned())
            })
            .expect("the checks' arguments");
        assert!(out.contains("/gui-check\""), "job {job}: {out}");
        let upload = steps
            .iter()
            .find(|s| s.uploads())
            .unwrap_or_else(|| panic!("job {job} uploads nothing"));
        assert_eq!(
            upload.with("path").as_deref(),
            Some("${{ runner.temp }}/gui-check"),
            "job {job} uploads the whole out-dir, attempt-1/ too"
        );
        assert_eq!(
            upload.condition().as_deref(),
            Some("always()"),
            "{upload:#?}"
        );
    }
    for file in github_files() {
        for name in ["force-fail", "GUI_CHECK_FORCE_FAIL", "GUI_CHECK_REPEAT"] {
            assert!(!file.text.contains(name), "{} names {name}", file.name);
        }
    }
}

/// Only main saves the Rust caches: every Swatinem/rust-cache step in every
/// workflow saves only on main (`save-if`, which may also ask more, as the
/// frontend shards do) or never (`false`, as the gui-test-app jobs, which
/// restore their installer jobs'), so pull requests and other branches restore
/// main's and don't save copies of it, which pushed main's own out of the
/// repository's 10 GB. cache-cleanup.yml deletes a pull request's caches when
/// it closes, and main's superseded ones after main's CI and Distribution
/// runs (or by hand), with a token that can manage caches and read the
/// repository, nothing else: the job for main's checks out tools/ci alone, for
/// the script that says which, without keeping the token, and the pull
/// request's job checks out nothing.
#[test]
fn only_main_saves_the_rust_caches_and_a_closed_pull_requests_go() {
    const ON_MAIN: &str = "github.ref == 'refs/heads/main'";
    let mut caches = 0;
    for file in github_files() {
        for step in file.steps() {
            if step.action().as_deref() != Some("Swatinem/rust-cache") {
                continue;
            }
            caches += 1;
            let save = step.with("save-if").unwrap_or_else(|| {
                panic!("{}: job {}: no save-if: {step:#?}", file.name, step.job)
            });
            if save == "false" {
                continue;
            }
            assert!(
                save.starts_with("${{ ") && save.ends_with(" }}") && save.contains(ON_MAIN),
                "{}: job {}: saves off main: {save}",
                file.name,
                step.job
            );
            assert!(
                !save.contains("||"),
                "{}: job {}: {save}",
                file.name,
                step.job
            );
        }
    }
    assert!(caches > 0, "no rust-cache steps found");

    let cleanup = workflow("cache-cleanup.yml");
    assert_eq!(
        flow(&cleanup.yaml["on"]),
        flow(&parse(
            "pull_request:
  types: [closed]
workflow_run:
  workflows: [CI, Distribution Artifacts]
  types: [completed]
  branches: [main]
workflow_dispatch:
  inputs:
    dry-run:
      description: Only list the caches it would delete
      type: boolean
      default: false
"
        )),
        "cache-cleanup.yml's triggers"
    );
    assert_eq!(
        grants(&cleanup.yaml["permissions"]),
        ["actions: write", "contents: read"]
    );
    let text = lines(&cleanup.yaml).join("\n");
    assert!(
        text.contains("refs/pull/${{ github.event.pull_request.number }}/merge")
            && text.contains("gh cache delete"),
        "{text}"
    );
    let jobs = cleanup.jobs();
    assert_eq!(
        jobs.iter().map(|job| job.id).collect::<Vec<_>>(),
        ["delete-caches", "delete-superseded"]
    );
    for job in &jobs {
        let id = job.id;
        assert!(!job.has("permissions"), "{id} asks for more");
        let steps = job.steps();
        let checkout = steps
            .iter()
            .find(|s| s.action().as_deref() == Some("actions/checkout"));
        if id == "delete-caches" {
            assert_eq!(
                job.condition().as_deref(),
                Some("github.event_name == 'pull_request'"),
                "{job:#?}"
            );
            assert!(checkout.is_none(), "{id} checks out the repository");
        } else {
            let checkout = checkout.unwrap_or_else(|| panic!("{id} checks out nothing"));
            assert_eq!(
                grants(&checkout.yaml["with"]),
                ["persist-credentials: false", "sparse-checkout: tools/ci"],
                "{id} checks out more than tools/ci, or keeps the token"
            );
            let body = job.text();
            assert!(
                body.contains("--ref refs/heads/main ")
                    && body.contains("python3 tools/ci/superseded_caches.py")
                    && !body.contains("refs/pull/"),
                "{id} acts on more than main's caches, or decides itself: {body}"
            );
            let run = steps.iter().map(Step::run).collect::<Vec<_>>().join("\n");
            let (dry, delete) = (
                run.find("if [ \"$DRY_RUN\" = true ]; then")
                    .expect("a dry run"),
                run.find("gh cache delete").expect("a delete"),
            );
            assert!(
                run.find("cat \"$RUNNER_TEMP/superseded.txt\"")
                    .is_some_and(|printed| printed < dry && dry < delete),
                "{id} deletes before it prints what it deletes: {run}"
            );
        }
    }
}

/// Release builds use one Rust, set in one place (`RUST_TOOLCHAIN` in
/// dist.yml), an exact release: not whatever is stable on the day.
#[test]
fn release_builds_use_one_pinned_rust() {
    let dist = workflow("dist.yml");
    let pinned = dist
        .env("RUST_TOOLCHAIN")
        .expect("dist.yml sets RUST_TOOLCHAIN");
    let parts: Vec<&str> = pinned.split('.').collect();
    assert!(
        parts.len() == 3 && parts.iter().all(|p| p.parse::<u32>().is_ok()),
        "RUST_TOOLCHAIN should be an exact release, X.Y.Z: {pinned}"
    );
    // That every install is this one is every_workflow_builds_with_the_release_rust's.
    assert!(
        dist.steps()
            .iter()
            .any(|s| s.action().as_deref() == Some("dtolnay/rust-toolchain"))
    );
}

/// CI tests the compiler the installers are built with (C-1): ci.yml's
/// `RUST_TOOLCHAIN` is dist.yml's, and every Rust a workflow installs is
/// that one, as `${{ env.RUST_TOOLCHAIN }}` from the workflow's own `env:`
/// (no job or step sets it again), never `stable` or a version of its own.
/// The one exception is the weekly audit, on the stable of the day: cargo
/// there only installs cargo-audit and runs it, so it builds none of the
/// app. Nothing picks a Rust another way (rustup, `cargo +…`,
/// `RUSTUP_TOOLCHAIN`, another action, or a rust-toolchain file, which
/// rustup would follow over the one installed), and no Rust cache drops the
/// rustc version and the `RUST*` variables from its key, so a bump starts
/// fresh caches.
#[test]
fn every_workflow_builds_with_the_release_rust() {
    const PIN: &str = "${{ env.RUST_TOOLCHAIN }}";
    const AUDIT: &str = "workflows/audit.yml";
    let release = workflow("dist.yml")
        .env("RUST_TOOLCHAIN")
        .expect("dist.yml sets RUST_TOOLCHAIN");
    assert_eq!(
        workflow("ci.yml").env("RUST_TOOLCHAIN").as_ref(),
        Some(&release),
        "ci.yml's RUST_TOOLCHAIN isn't dist.yml's"
    );

    let mut pinned = 0;
    let mut audit = None;
    for file in github_files() {
        for line in code(&file.yaml) {
            for other in [
                "rustup toolchain install",
                "rustup install",
                "rustup default",
                "rustup update",
                "rustup override",
                "RUSTUP_TOOLCHAIN",
                "cargo +",
                "actions-rs/toolchain",
                "actions-rust-lang/setup-rust-toolchain",
            ] {
                assert!(
                    !line.contains(other),
                    "{}: picks a Rust of its own: {}",
                    file.name,
                    line.trim()
                );
            }
            assert!(
                !line.contains("add-rust-environment-hash-key"),
                "{}: a Rust cache's key may leave out the toolchain: {}",
                file.name,
                line.trim()
            );
        }
        assert!(
            every(&file.yaml, "RUST_TOOLCHAIN").len() <= 1,
            "{}: a job or step sets RUST_TOOLCHAIN again",
            file.name
        );

        let installs: Vec<Step> = file
            .steps()
            .into_iter()
            .filter(|s| s.action().as_deref() == Some("dtolnay/rust-toolchain"))
            .collect();
        assert_eq!(
            installs.len(),
            every(&file.yaml, "uses")
                .into_iter()
                .filter_map(scalar)
                .filter(|u| u.starts_with("dtolnay/rust-toolchain@"))
                .count(),
            "{}: a Rust installed outside a job's steps",
            file.name
        );
        for install in &installs {
            let toolchain = install.with("toolchain");
            if file.name == AUDIT {
                assert_eq!(toolchain.as_deref(), Some("stable"), "{install:#?}");
                continue;
            }
            assert_eq!(
                toolchain.as_deref(),
                Some(PIN),
                "{}: job {} installs another Rust: {install:#?}",
                file.name,
                install.job
            );
            assert_eq!(
                file.env("RUST_TOOLCHAIN").as_ref(),
                Some(&release),
                "{}: job {} installs a Rust its workflow doesn't pin",
                file.name,
                install.job
            );
            pinned += 1;
        }
        if file.name == AUDIT {
            audit = Some(file);
        }
    }
    assert!(pinned > 0, "no Rust installs found");

    let audit = audit.unwrap_or_else(|| panic!("{AUDIT}"));
    let cargo: Vec<String> = audit
        .steps()
        .iter()
        .flat_map(Step::commands)
        .map(|l| l.trim().to_owned())
        .filter(|l| l.contains("cargo "))
        .collect();
    assert!(!cargo.is_empty(), "{AUDIT} runs no cargo");
    for line in cargo {
        assert!(
            line == "cargo install cargo-audit --locked" || line.starts_with("cargo audit "),
            "{AUDIT} builds with the stable Rust, so only cargo-audit: {line}"
        );
    }

    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    for dir in [
        ".",
        "src-tauri",
        "ui-leptos",
        "tests",
        "tests/gui",
        "tests/frontend",
    ] {
        for name in ["rust-toolchain", "rust-toolchain.toml"] {
            let path = root.join(dir).join(name);
            assert!(
                !path.exists(),
                "{}: rustup would build with its Rust in CI, not RUST_TOOLCHAIN",
                path.display()
            );
        }
    }
}

/// Every crate's `rust-version` is the pinned Rust's major.minor
/// (`RUST_TOOLCHAIN` in ci.yml and dist.yml): the only Rust CI and the
/// releases build with, so the only one tested. Edition 2024's resolver
/// picks dependency versions that fit `rust-version`, so one left behind
/// (1.94, while every job built with 1.99) quietly held back `cargo update`.
/// The app's workspace members (read from its `[workspace]`) each set one;
/// the page's crate and the GUI checks' set none, or the same.
#[test]
fn every_crates_rust_version_is_the_pinned_rusts() {
    let mut pinned: Option<String> = None;
    for file in ["ci.yml", "dist.yml"] {
        let toolchain = workflow(file)
            .env("RUST_TOOLCHAIN")
            .unwrap_or_else(|| panic!("{file} sets no RUST_TOOLCHAIN"));
        let major_minor = toolchain
            .rsplit_once('.')
            .map(|(major_minor, _)| major_minor.to_owned())
            .unwrap_or_else(|| panic!("{file}'s RUST_TOOLCHAIN isn't X.Y.Z: {toolchain}"));
        assert_eq!(
            *pinned.get_or_insert_with(|| major_minor.clone()),
            major_minor,
            "ci.yml's and dist.yml's RUST_TOOLCHAIN differ"
        );
    }
    let pinned = pinned.expect("a pinned Rust");
    let pinned = pinned.as_str();

    let rust_version = |manifest: &str| {
        manifest.lines().find_map(|l| {
            l.strip_prefix("rust-version = \"")
                .and_then(|rest| rest.strip_suffix('"'))
                .map(str::to_owned)
        })
    };
    let members = CARGO_TOML
        .lines()
        .find_map(|l| l.strip_prefix("members = ["))
        .and_then(|rest| rest.strip_suffix(']'))
        .expect("Cargo.toml's [workspace] lists its members on one line");
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut crates = vec![("src-tauri/Cargo.toml".to_owned(), CARGO_TOML.to_owned())];
    for member in members.split(',').map(|m| m.trim().trim_matches('"')) {
        let path = root.join(member).join("Cargo.toml");
        let manifest =
            std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        crates.push((format!("src-tauri/{member}/Cargo.toml"), manifest));
    }
    assert!(crates.len() >= 3, "the app, core and cli: {members}");
    for (file, manifest) in &crates {
        assert_eq!(
            rust_version(manifest).as_deref(),
            Some(pinned),
            "{file}'s rust-version isn't the pinned Rust's {pinned}"
        );
    }
    for dir in ["../ui-leptos", "../tests/gui"] {
        let path = root.join(dir).join("Cargo.toml");
        let manifest =
            std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        if let Some(version) = rust_version(&manifest) {
            assert_eq!(version, pinned, "{}'s rust-version", path.display());
        }
    }
}

/// The updater's signing key (D-01). Its secrets are named by one job
/// alone, dist.yml's sign job: in the release environment (which deploys
/// from main only), on a run dispatched on main. Nothing hands every secret
/// on (`secrets: inherit`, `toJSON(secrets)`), and no workflow-wide `env`
/// carries them into every job. The jobs that build never see the key:
/// they're the same on every run, unsigned.
#[test]
fn only_the_release_job_has_the_signing_key() {
    const KEY: &str = "TAURI_SIGNING_PRIVATE_KEY";
    let mut holders = Vec::new();
    for file in github_files() {
        let text = lines(&file.yaml).join("\n");
        for everything in ["toJSON(secrets)", "toJson(secrets)"] {
            assert!(!text.contains(everything), "{}: {everything}", file.name);
        }
        assert!(
            !every(&file.yaml, "secrets")
                .into_iter()
                .any(|s| scalar(s).as_deref() == Some("inherit")),
            "{}: secrets: inherit",
            file.name
        );
        let outside_jobs: Vec<String> = entries(&file.yaml)
            .into_iter()
            .filter(|(k, _)| k != "jobs")
            .flat_map(|(k, v)| std::iter::once(k).chain(lines(v)))
            .collect();
        assert!(
            !outside_jobs.iter().any(|l| l.contains(KEY)),
            "{} names {KEY} outside its jobs",
            file.name
        );
        if file.name.starts_with("actions/") {
            assert!(!text.contains(KEY), "{} names {KEY}", file.name);
            continue;
        }
        for job in file.jobs() {
            if !job.text().contains(KEY) {
                continue;
            }
            assert_eq!(
                job.environment().as_deref(),
                Some("release"),
                "{}: job {} has the signing key outside the release environment",
                file.name,
                job.id
            );
            assert_eq!(
                job.condition().as_deref(),
                Some("github.event_name == 'workflow_dispatch' && github.ref == 'refs/heads/main'"),
                "{}: job {} has the signing key on more than a run dispatched on main",
                file.name,
                job.id
            );
            holders.push(format!("{} {}", file.name, job.id));
        }
    }
    assert_eq!(holders, ["workflows/dist.yml sign"]);
}

/// The installers' artifacts: the build jobs upload them, and the sign job
/// signs each and uploads it as an artifact of its own, the name with
/// [`SIGNED`] after it, never replacing the build's.
const INSTALLER_ARTIFACTS: [(&str, &str); 4] = [
    ("linux", "gospel-getter-linux-appimage"),
    ("linux", "gospel-getter-linux-deb"),
    ("windows", "gospel-getter-windows-nsis-installer"),
    ("windows", "gospel-getter-windows-msi-installer"),
];

/// What the sign job puts after a build artifact's name for the one it
/// uploads, signed (tools/release/sign_installers.py's `SIGNED_SUFFIX`).
const SIGNED: &str = "-signed";

/// What every artifact of the signing rehearsal's starts with, so none can
/// pass for a release's.
const REHEARSAL: &str = "rehearsal-";

/// The CLI artifacts the build jobs upload, which a release attaches.
const CLI_ARTIFACTS: [&str; 2] = [
    "gospel-getter-cli-linux-x86_64",
    "gospel-getter-cli-windows-x86_64",
];

/// How a release's installers are signed (D-01, see the top of dist.yml).
/// Every build is unsigned (`--no-sign`), with the updater overlay, as a
/// release is built. On a run dispatched on main, the sign job signs those
/// very files once both platforms' GUI checks have passed on them, by
/// tools/release/sign_installers.py (which records each installer's
/// SHA-256, checks each signature and that the installer didn't change,
/// and makes the update manifest as a dry run), and uploads each installer
/// with its .sig as `<build artifact>-signed`, the name a release takes it
/// by (N-S-01: never the build artifact's own name, so a failed sign job
/// can be re-run on the build's artifacts as built). On a run dispatched
/// on another branch, or a release/ branch's pull request, the rehearsal
/// does the same with a throwaway key, and uploads its signed files and the
/// throwaway public key only under [`REHEARSAL`] names, for the bundle job's
/// rehearsal (`the_bundle_job_assembles_the_release_files_without_the_key`):
/// never a name a release takes, and never the private key.
#[test]
fn only_a_release_run_signs_and_it_signs_what_was_checked() {
    let dist = workflow("dist.yml");
    let job = |name: &str| dist.job(name).steps();
    let builds: Vec<String> = ["linux", "windows"]
        .into_iter()
        .flat_map(job)
        .flat_map(|s| s.commands())
        .map(|l| l.trim_start().to_owned())
        .filter(|l| l.starts_with("cargo tauri build"))
        .collect();
    assert!(
        builds.len() >= 2,
        "both jobs' installer builds: {builds:#?}"
    );
    for build in &builds {
        // What follows `--` goes to cargo, not to the Tauri CLI.
        let tauri_args = build
            .split_once(" -- ")
            .map_or(build.as_str(), |(tauri, _)| tauri);
        assert!(
            tauri_args.contains("--config tauri.updater.conf.json")
                && tauri_args.ends_with(" --no-sign"),
            "an installer build that isn't the release's, unsigned: {build}"
        );
    }

    assert_eq!(
        dist.job("sign").needs(),
        [
            "linux",
            "windows",
            "gui-linux",
            "gui-windows",
            "upgrade-linux",
            "upgrade-windows"
        ],
        "the sign job should wait for the builds, both GUI checks and both upgrade checks"
    );
    let sign = job("sign");
    let signs: Vec<&Step> = sign
        .iter()
        .filter(|s| s.run().contains("tools/release/sign_installers.py"))
        .collect();
    assert_eq!(signs.len(), 1, "{sign:#?}");
    assert_eq!(
        signs[0].env("TAURI_SIGNING_PRIVATE_KEY").as_deref(),
        Some("${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}"),
        "{signs:#?}"
    );
    let uploads: Vec<&Step> = sign.iter().filter(|s| s.uploads()).collect();
    for (built_by, artifact) in INSTALLER_ARTIFACTS {
        assert!(
            job(built_by)
                .iter()
                .any(|s| s.uploads() && s.with("name").as_deref() == Some(artifact)),
            "job {built_by} doesn't upload {artifact}"
        );
        // Never under the build artifact's own name: it stays as built.
        assert!(
            !uploads
                .iter()
                .any(|s| s.with("name").as_deref() == Some(artifact)),
            "the sign job replaces the build's {artifact}: {uploads:#?}"
        );
        let signed = format!("{artifact}{SIGNED}");
        let up: Vec<&&Step> = uploads
            .iter()
            .filter(|s| s.with("name").as_deref() == Some(signed.as_str()))
            .collect();
        assert_eq!(up.len(), 1, "the sign job uploads {signed} once");
        assert!(
            up[0].with("path") == Some(format!("installers/{artifact}/"))
                && up[0].with("overwrite").as_deref() == Some("true")
                && up[0].with("if-no-files-found").as_deref() == Some("error"),
            "{up:#?}"
        );
    }
    // Uploaded to nowhere else: the release's artifacts are the four above,
    // each a -signed name.
    assert_eq!(uploads.len(), INSTALLER_ARTIFACTS.len(), "{uploads:#?}");
    for upload in &uploads {
        let name = upload.with("name").unwrap_or_default();
        assert!(
            name.ends_with(SIGNED)
                && INSTALLER_ARTIFACTS
                    .iter()
                    .any(|(_, artifact)| name.strip_suffix(SIGNED) == Some(artifact)),
            "the sign job uploads {name:?}, not a -signed installer artifact: {upload:#?}"
        );
    }
    // overwrite: on the -signed names only, anywhere in dist.yml.
    for step in dist.steps() {
        if step.with("overwrite").as_deref() == Some("true") {
            let name = step.with("name").unwrap_or_default();
            assert!(
                step.job == "sign" && name.ends_with(SIGNED),
                "job {} overwrites {name:?}: {step:#?}",
                step.job
            );
        }
    }

    let rehearsal_job = dist.job("sign-rehearsal");
    let condition = rehearsal_job.condition().expect("the rehearsal has an if:");
    assert!(
        condition.contains(
            "github.event_name == 'workflow_dispatch' && github.ref != 'refs/heads/main'"
        ) && condition.contains("startsWith(github.head_ref, 'release/')"),
        "{condition}"
    );
    assert!(!rehearsal_job.has("environment"));
    let rehearsal = rehearsal_job.steps();
    assert!(
        rehearsal.iter().any(|s| {
            let run = s.run();
            run.contains("python3 tools/release/sign_installers.py installers \\")
                && run.contains("--private-key-path \"$RUNNER_TEMP/rehearsal.key\"")
                && run.contains("--pubkey-file \"$RUNNER_TEMP/rehearsal.key.pub\"")
        }),
        "{rehearsal:#?}"
    );
    // The rehearsal's signatures are worthless (a throwaway key the app
    // doesn't trust), so nothing it uploads can pass for a release's: each
    // of its four signed artifacts is a release's -signed name under
    // REHEARSAL, plus its public key, and never the private key.
    let rehearsal_uploads: Vec<&Step> = rehearsal.iter().filter(|s| s.uploads()).collect();
    let mut uploaded: Vec<(String, String)> = rehearsal_uploads
        .iter()
        .map(|s| {
            (
                s.with("name").unwrap_or_default(),
                s.with("path").unwrap_or_default(),
            )
        })
        .collect();
    uploaded.sort();
    let mut expected: Vec<(String, String)> = INSTALLER_ARTIFACTS
        .iter()
        .map(|(_, a)| {
            (
                format!("{REHEARSAL}{a}{SIGNED}"),
                format!("installers/{a}/"),
            )
        })
        .chain([(
            format!("{REHEARSAL}signing-pubkey"),
            "${{ runner.temp }}/rehearsal.key.pub".to_owned(),
        )])
        .collect();
    expected.sort();
    assert_eq!(
        uploaded, expected,
        "the rehearsal uploads only under rehearsal- names"
    );
    for upload in &rehearsal_uploads {
        assert!(
            upload.with("overwrite").is_none()
                && !lines(upload.yaml)
                    .iter()
                    .any(|l| l.trim_end().ends_with("rehearsal.key"))
                && upload.with("retention-days").as_deref() == Some("7"),
            "{upload:#?}"
        );
    }
    // Both take the build's four artifacts by their exact names, each into
    // its own folder, and nothing else: never a -signed one (a pattern such
    // as gospel-getter-linux-* would take gospel-getter-linux-appimage-signed
    // too, which a re-run of the sign job would then sign again).
    for name in ["sign", "sign-rehearsal"] {
        let steps = job(name);
        let downloads: Vec<&Step> = steps.iter().filter(|s| s.downloads()).collect();
        for download in &downloads {
            assert!(
                download.with("pattern").is_none(),
                "job {name} downloads by a pattern, which can take a -signed artifact: {download:#?}"
            );
            let artifact = download.with("name").unwrap_or_default();
            assert!(
                !artifact.is_empty() && !artifact.ends_with(SIGNED),
                "job {name} downloads {artifact:?}: {download:#?}"
            );
        }
        let mut names: Vec<String> = downloads.iter().filter_map(|s| s.with("name")).collect();
        names.sort_unstable();
        let mut expected: Vec<&str> = INSTALLER_ARTIFACTS.iter().map(|(_, a)| *a).collect();
        expected.sort_unstable();
        assert_eq!(
            names, expected,
            "job {name} downloads the four build artifacts, once each"
        );
        for (_, artifact) in INSTALLER_ARTIFACTS {
            let into = downloads
                .iter()
                .find(|s| s.with("name").as_deref() == Some(artifact))
                .and_then(|s| s.with("path"));
            assert_eq!(
                into,
                Some(format!("installers/{artifact}")),
                "job {name}: {artifact} goes into its own folder"
            );
        }
    }
    // Both get their signer the same way (CI-02): tauri-apps' own prebuilt
    // cargo-tauri at TAURI_CLI_VERSION, from the build-tools action's
    // `signer` set, which checks it against its pinned SHA-256 on every run
    // (a cache hit too), and check that `cargo tauri` is that binary at that
    // version before it signs anything. Nothing else is installed or built:
    // no `cargo install` of any kind, no Rust, no Rust cache, no other set.
    let signer = signer_version();
    for name in ["sign", "sign-rehearsal"] {
        let steps = job(name);
        let tools: Vec<usize> = (0..steps.len())
            .filter(|&i| steps[i].get("uses").as_deref() == Some("./.github/actions/build-tools"))
            .collect();
        assert_eq!(
            tools.len(),
            1,
            "job {name} gets one set of build tools: {steps:#?}"
        );
        assert_eq!(
            steps[tools[0]].with("set").as_deref(),
            Some("signer"),
            "job {name}: {:#?}",
            steps[tools[0]]
        );
        for step in &steps {
            let code = step.code();
            for banned in [
                "cargo install",
                "rust-cache",
                "dtolnay/rust-toolchain@",
                "cargo build",
            ] {
                assert!(
                    !code.contains(banned),
                    "job {name} runs {banned}: {step:#?}"
                );
            }
        }
        let runs = |i: usize| steps[i].commands().join("\n");
        let check = (0..steps.len())
            .find(|&i| runs(i).contains("version=\"$(cargo tauri --version)\""))
            .unwrap_or_else(|| panic!("job {name} doesn't print `cargo tauri --version`"));
        let checked = runs(check);
        assert!(
            checked.contains("[ \"$version\" != \"tauri-cli $TAURI_CLI_VERSION\" ]")
                && checked.contains("want=\"$RUNNER_TEMP/build-tools/bin/cargo-tauri\"")
                && checked.contains("[ \"$found\" != \"$want\" ]")
                && checked.matches("exit 1").count() == 2,
            "job {name} doesn't fail unless `cargo tauri` is the pinned signer: {:#?}",
            steps[check]
        );
        let first_use = (0..steps.len())
            .find(|&i| {
                i != check
                    && (runs(i).contains("cargo tauri") || runs(i).contains("sign_installers.py"))
            })
            .unwrap_or_else(|| panic!("job {name} never signs"));
        assert!(
            tools[0] < check && check < first_use,
            "job {name}: the signer is got, then checked, then used"
        );
    }
    assert_eq!(
        Some(signer),
        dist.env("TAURI_CLI_VERSION"),
        "the signer row's version is TAURI_CLI_VERSION"
    );
}

/// tools/ci/build-tools.txt, the build tools' pins (columns as its header).
const BUILD_TOOLS: &str = include_str!("../../tools/ci/build-tools.txt");

/// The version of the signer set's rows, one per OS, checked to be
/// tauri-apps' own release of the prebuilt Tauri CLI for that version (the
/// Linux one for both signing jobs, which run there, and each for its
/// platform's installer job), pinned by a SHA-256 and laid out as
/// `cargo-tauri` on PATH.
fn signer_version() -> String {
    let rows: Vec<Vec<&str>> = BUILD_TOOLS
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| l.split_whitespace().collect())
        .filter(|fields: &Vec<&str>| fields.first() == Some(&"signer"))
        .collect();
    assert_eq!(
        rows.len(),
        2,
        "build-tools.txt has a signer row for Linux and one for Windows: {rows:#?}"
    );
    let version = rows[0][3];
    for (row, (os, asset, install)) in rows.iter().zip([
        (
            "Linux",
            "cargo-tauri-x86_64-unknown-linux-gnu.tgz",
            "bin=cargo-tauri",
        ),
        (
            "Windows",
            "cargo-tauri-x86_64-pc-windows-msvc.zip",
            "bin=cargo-tauri.exe",
        ),
    ]) {
        assert_eq!(row.len(), 9, "{row:?}");
        assert_eq!(
            (row[1], row[2], row[3], row[4], row[6], row[8]),
            (
                os,
                "cargo-tauri",
                version,
                "tauri-apps/tauri",
                asset,
                install
            ),
            "{row:?}"
        );
        assert_eq!(row[5], format!("tauri-cli-v{version}"), "{row:?}");
        assert!(
            row[7].len() == 64
                && row[7]
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "the signer isn't pinned by a SHA-256: {row:?}"
        );
    }
    version.to_owned()
}

/// The installer jobs build with the prebuilt Tauri CLI the signing jobs
/// sign with (the `signer` set, at TAURI_CLI_VERSION), never tauri-cli
/// compiled by `cargo install`, which took about five minutes on a cold
/// cache. Each checks `cargo tauri` is that binary at that version, as the
/// signing jobs do, before its installers are built; the Rust cache may
/// have restored a cargo-installed one, which cargo would run first, so
/// the check removes it.
#[test]
fn the_installer_jobs_build_with_the_pinned_prebuilt_tauri_cli() {
    let signer = signer_version();
    let dist = workflow("dist.yml");
    assert_eq!(dist.env("TAURI_CLI_VERSION"), Some(signer));
    for job in ["linux", "windows"] {
        let own = dist.job(job).steps();
        assert!(
            !own.iter().any(|s| s.text().contains("cargo install")),
            "job {job} compiles a tool: {own:#?}"
        );
        let got = own
            .iter()
            .position(|s| {
                s.get("uses").as_deref() == Some("./.github/actions/build-tools")
                    && s.with("set").as_deref() == Some("signer")
            })
            .unwrap_or_else(|| panic!("job {job} doesn't get the prebuilt CLI"));
        let check = own
            .iter()
            .position(|s| s.name() == "Check the Tauri CLI is the pinned one")
            .unwrap_or_else(|| panic!("job {job} doesn't check the CLI"));
        let build = own
            .iter()
            .position(|s| {
                s.commands()
                    .iter()
                    .any(|l| l.trim_start().starts_with("cargo tauri build"))
            })
            .unwrap_or_else(|| panic!("job {job} builds no installers"));
        assert!(
            got < check && check < build,
            "job {job}: the CLI is got, then checked, then used"
        );
        let text = own[check].run();
        for must in [
            "cargo tauri --version",
            "TAURI_CLI_VERSION",
            "build-tools",
            "cargo-tauri",
        ] {
            assert!(
                text.contains(must),
                "job {job}'s check lacks {must}: {text}"
            );
        }
        let removes = if job == "windows" {
            "Remove-Item -LiteralPath $shadow"
        } else {
            "rm -f \"$shadow\""
        };
        assert!(text.contains(removes), "job {job}: {text}");
    }
}

/// The release's files are assembled by the bundle job (CI-05,
/// tools/release/bundle_release.py), which needs no key: it names no secret,
/// has no environment and reads only (`contents: read`). A release's bundle
/// follows the sign job, on the sign job's own condition, from its four
/// -signed artifacts, the four build artifacts they were signed from, and
/// the two CLI artifacts, each by its exact name, into one artifact,
/// `gospel-getter-release-bundle`, which the publish takes. After the
/// signing rehearsal the same job runs on its [`REHEARSAL`] artifacts,
/// against its throwaway public key, into `rehearsal-release-bundle`.
#[test]
fn the_bundle_job_assembles_the_release_files_without_the_key() {
    const FROM: &str = "${{ env.FROM }}";
    let dist = workflow("dist.yml");
    let bundle = dist.job("bundle");
    // A plain name: a job cancelled before it starts keeps an expression
    // unevaluated on the commit, a check that never clears (see #233).
    let name = bundle.get("name").expect("the bundle job has a name");
    assert!(!name.contains("${{"), "the bundle job's name: {name}");
    assert_eq!(name, "Assemble the release files");
    assert_eq!(bundle.needs(), ["sign", "sign-rehearsal"], "{bundle:#?}");
    assert_eq!(
        bundle.condition().as_deref(),
        Some(
            "!cancelled() && ((github.event_name == 'workflow_dispatch' && \
             github.ref == 'refs/heads/main' && needs.sign.result == 'success') || \
             needs.sign-rehearsal.result == 'success')"
        ),
        "the bundle follows the sign job on its condition, or the rehearsal"
    );
    assert!(!bundle.has("environment"), "{bundle:#?}");
    assert_eq!(
        grants(&bundle.yaml["permissions"]),
        ["contents: read"],
        "the bundle job reads only"
    );
    for line in code(bundle.yaml) {
        assert!(
            !line.contains("secrets.") && !line.contains("TAURI_SIGNING"),
            "the bundle job names a secret: {line}"
        );
    }
    for (name, value) in [
        (
            "FROM",
            "${{ needs.sign.result != 'success' && 'rehearsal-' || '' }}",
        ),
        (
            "BUNDLE",
            "${{ needs.sign.result == 'success' && 'gospel-getter-release-bundle' || \
             'rehearsal-release-bundle' }}",
        ),
    ] {
        assert_eq!(
            bundle.env(name).as_deref(),
            Some(value),
            "the bundle job's {name}"
        );
    }

    let steps = bundle.steps();
    let mut downloads: Vec<(String, String)> = steps
        .iter()
        .filter(|s| s.downloads())
        .map(|s| {
            assert!(s.with("pattern").is_none(), "{s:#?}");
            (
                s.with("name").unwrap_or_default(),
                s.with("path").unwrap_or_default(),
            )
        })
        .collect();
    downloads.sort();
    let mut expected: Vec<(String, String)> = Vec::new();
    for (_, artifact) in INSTALLER_ARTIFACTS {
        expected.push((
            format!("{FROM}{artifact}{SIGNED}"),
            format!("bundle-in/signed/{artifact}"),
        ));
        expected.push((artifact.to_owned(), format!("bundle-in/built/{artifact}")));
    }
    for artifact in CLI_ARTIFACTS {
        expected.push((artifact.to_owned(), format!("bundle-in/cli/{artifact}")));
    }
    expected.push((
        format!("{REHEARSAL}signing-pubkey"),
        "${{ runner.temp }}/rehearsal-key".to_owned(),
    ));
    expected.sort();
    assert_eq!(downloads, expected, "the bundle job's downloads");
    let pubkey = steps
        .iter()
        .find(|s| s.with("name").as_deref() == Some("rehearsal-signing-pubkey"))
        .expect("the rehearsal's public key");
    assert_eq!(
        pubkey.condition().as_deref(),
        Some("env.FROM == 'rehearsal-'"),
        "only a rehearsal's bundle takes another key: {pubkey:#?}"
    );

    let assemble = steps
        .iter()
        .find(|s| s.run().contains("tools/release/bundle_release.py"))
        .expect("the bundle job runs bundle_release.py");
    let run = assemble.run();
    assert!(
        run.contains(
            "python3 tools/release/bundle_release.py bundle-in release-bundle \"${key[@]}\""
        ) && run.contains("sha256sum -c SHA256SUMS")
            && run.contains("if [ \"${FROM:-}\" = rehearsal- ]; then"),
        "{assemble:#?}"
    );
    let uploads: Vec<&Step> = steps.iter().filter(|s| s.uploads()).collect();
    let [upload] = uploads.as_slice() else {
        panic!("one artifact: {uploads:#?}");
    };
    assert_eq!(
        grants(&upload.yaml["with"]),
        [
            "if-no-files-found: error",
            "name: ${{ env.BUNDLE }}",
            "path: release-bundle/",
            "retention-days: 14",
        ],
        "{upload:#?}"
    );
    // Nothing but the rehearsal and the bundle job's FROM names a rehearsal
    // artifact: never the sign job.
    for step in dist.steps() {
        if step.code().contains(REHEARSAL) && step.job != "sign-rehearsal" {
            assert_eq!(step.job, "bundle", "{step:#?}");
        }
    }
}

/// A push to main never cancels a dispatched run, a release's build (D-05):
/// a dispatched run is a concurrency group of its own, by its run id. Push
/// and pull request runs still cancel the run in progress on their ref.
#[test]
fn a_push_never_cancels_a_dispatched_run() {
    let group = scalar(&workflow("dist.yml").yaml["concurrency"]["group"])
        .expect("dist.yml has a concurrency group");
    assert_eq!(
        group,
        "dist-${{ github.workflow }}-${{ (github.event_name == 'workflow_dispatch' || \
         (github.event.action == 'labeled' && github.event.label.name != 'ci:full')) && \
         github.run_id || github.ref }}"
    );
}

/// A newer push to a pull request cancels its older CI run (CI-03), and
/// nothing else cancels or waits on anything: every run that isn't a pull
/// request's (main's pushes) is a concurrency group of its own, by its run
/// id. One shared group without cancel-in-progress would still let a newer
/// pending run replace an older pending one, cancelling a run on main (a red
/// X, and no cache saved). CI's `pull_request` keeps its default types, so
/// adding the `ci:full` label starts no CI run and cancels none.
#[test]
fn ci_cancels_only_a_pull_requests_superseded_run() {
    let ci = workflow("ci.yml");
    let value = |key: &str| {
        scalar(&ci.yaml["concurrency"][key])
            .unwrap_or_else(|| panic!("ci.yml's concurrency has no {key}"))
    };
    assert_eq!(
        value("group"),
        "ci-${{ github.event_name == 'pull_request' && \
         format('pr-{0}', github.event.pull_request.number) || github.run_id }}",
        "a pull request's runs share a group, and every other run is a group of its own"
    );
    assert_eq!(
        value("cancel-in-progress"),
        "${{ github.event_name == 'pull_request' }}",
        "only a pull request's runs cancel"
    );

    let on = &ci.yaml["on"];
    let triggers: Vec<String> = match on {
        Yaml::Hash(_) => entries(on).into_iter().map(|(k, _)| k).collect(),
        other => list(other),
    };
    assert!(
        triggers.iter().any(|t| t == "pull_request"),
        "{triggers:#?}"
    );
    assert!(
        on["pull_request"]["types"].is_badvalue(),
        "pull_request on its default types (no labeled, so a label cancels nothing): {}",
        flow(on)
    );
}

/// Each platform's upgrade check (tools/ci/upgrade_check.py, whose header
/// says why) runs whenever that platform's GUI checks run, on the
/// installer that platform's build job made, and the release's sign job
/// waits for both, so a failed upgrade blocks the release.
#[test]
fn every_release_build_checks_an_upgrade_from_the_previous_release() {
    let dist = workflow("dist.yml");
    for (upgrade, gui, build, artifact, platform) in [
        (
            "upgrade-linux",
            "gui-linux",
            "linux",
            "gospel-getter-linux-appimage",
            "linux",
        ),
        (
            "upgrade-windows",
            "gui-windows",
            "windows",
            "gospel-getter-windows-nsis-installer",
            "windows",
        ),
    ] {
        let job = dist.job(upgrade);
        assert_eq!(job.needs(), [build], "{upgrade} should wait for {build}");
        assert_eq!(
            job.condition(),
            dist.job(gui).condition(),
            "{upgrade} should run exactly when {gui} does"
        );
        let own = job.steps();
        assert!(
            own.iter()
                .any(|s| s.with("name").as_deref() == Some(artifact)),
            "{upgrade} should check this run's {artifact}: {own:#?}"
        );
        let previous = own
            .iter()
            .find(|s| s.run().contains("tools/ci/upgrade_check.py previous"))
            .unwrap_or_else(|| panic!("{upgrade} gets no previous release: {own:#?}"));
        assert!(
            previous
                .run()
                .contains(&format!("previous --platform {platform} "))
                && previous.env("GH_TOKEN").as_deref() == Some("${{ github.token }}")
                && !previous.has("continue-on-error"),
            "{upgrade}: {previous:#?}"
        );
        // And back the other way, as About's Go back does: this build
        // first, the previous release over it, expecting its version.
        let back = own
            .iter()
            .find(|s| s.run().contains("tools/ci/upgrade_check.py back "))
            .unwrap_or_else(|| panic!("{upgrade} never goes back: {own:#?}"));
        assert!(
            back.env("GOSPEL_GETTER_UPGRADE_CHECK").as_deref() == Some("throwaway-runner")
                && back.env("BACK_TO").as_deref()
                    == Some("${{ steps.previous.outputs.installer }}")
                && back.env("BACK_TAG").as_deref() == Some("${{ steps.previous.outputs.tag }}")
                && back.run().contains("--to \"$BACK_TO\"")
                && back.run().contains("--expect-version \"${BACK_TAG#v}\"")
                && !back.has("continue-on-error")
                && !back.has("if"),
            "{upgrade}: {back:#?}"
        );
        let check = own
            .iter()
            .find(|s| s.run().contains("tools/ci/upgrade_check.py run "))
            .unwrap_or_else(|| panic!("{upgrade} runs no upgrade check: {own:#?}"));
        assert!(
            check.env("GOSPEL_GETTER_UPGRADE_CHECK").as_deref() == Some("throwaway-runner")
                && check
                    .text()
                    .contains("${{ steps.previous.outputs.installer }}")
                && !check.has("continue-on-error")
                && !check.has("if"),
            "{upgrade}: {check:#?}"
        );
    }
    let needs = dist.job("sign").needs();
    for upgrade in ["upgrade-linux", "upgrade-windows"] {
        assert!(
            needs.iter().any(|n| n == upgrade),
            "the sign job doesn't wait for {upgrade}: {needs:?}"
        );
    }
}

/// The `ci:full` label: a pull request that has it runs gui-windows and
/// sign-rehearsal (and their GUI-check test app's job) in its own
/// pull_request run, as a release/ branch's does, in place of a run
/// dispatched by hand that built everything twice; adding the label
/// (`labeled`) starts that run. A run for any other label builds nothing
/// ([`LABEL_GATE`]), cancels nothing, and the jobs behind the required
/// checks never carry a check's name in it, so a skip (which counts as
/// passing) never stands in for a required check (they stay: "Linux
/// (AppImage + deb)", "Windows (NSIS installer)", "GUI checks (Linux)"). The
/// label can never reach the sign job, which has the release's key: its gate
/// is a run dispatched on main alone
/// (`only_the_release_job_has_the_signing_key`).
#[test]
fn ci_full_runs_the_windows_checks_and_rehearsal_but_never_signs() {
    const FULL: &str = "contains(github.event.pull_request.labels.*.name, 'ci:full')";
    let dist = workflow("dist.yml");
    assert_eq!(
        list(&dist.yaml["on"]["pull_request"]["types"]),
        ["opened", "synchronize", "reopened", "labeled"]
    );

    for job in ["gui-windows", "sign-rehearsal"] {
        let condition = dist
            .job(job)
            .condition()
            .unwrap_or_else(|| panic!("job {job} has no if"));
        assert!(
            condition.contains(&format!(
                "github.event_name == 'pull_request' && (startsWith(github.head_ref, 'release/') \
                 || {FULL})"
            )),
            "job {job}: {condition}"
        );
    }
    let gui_windows = dist
        .job("gui-windows")
        .condition()
        .expect("gui-windows's if");
    // Its test app is built whenever it runs, and in no other label's run.
    assert_eq!(
        dist.job(WINDOWS_TEST_APP_JOB).condition(),
        Some(format!("({LABEL_GATE}) && ({gui_windows})")),
        "the test app isn't built when gui-windows runs"
    );

    // Another label's run: every job that runs anything is skipped by the
    // builds' gate, and none behind a required check is named as it is.
    for (job, required) in [
        ("linux", "Linux (AppImage + deb)"),
        ("windows", "Windows (NSIS installer)"),
        ("gui-linux", "GUI checks (Linux)"),
    ] {
        let keys = dist.job(job);
        let name = keys
            .get("name")
            .unwrap_or_else(|| panic!("job {job} has no name"));
        let named = name
            .strip_prefix(&format!("${{{{ ({LABEL_GATE}) && '"))
            .and_then(|rest| rest.strip_suffix("' }}"))
            .and_then(|rest| rest.split_once("' || '"))
            .unwrap_or_else(|| panic!("job {job}'s name doesn't follow the label: {name}"));
        assert_ne!(named.0, named.1, "job {job}");
        assert!(
            !named.1.is_empty() && !named.1.contains("${{"),
            "job {job}: {name}"
        );
        assert_eq!(
            named.0, required,
            "job {job}: a required check's name changed"
        );
        let needs = keys.needs().join(", ");
        let gated = keys
            .condition()
            .is_some_and(|c| c.contains(&format!("({LABEL_GATE})")));
        assert!(
            gated || needs.contains("linux") || needs.contains("windows"),
            "job {job} runs in another label's run: {keys:#?}"
        );
    }
    // The rest run only after the builds, so not in that run either.
    for job in ["gui-windows", "sign-rehearsal", "sign"] {
        assert!(
            dist.job(job).needs().join(", ").contains("windows"),
            "job {job} doesn't wait for the builds"
        );
    }

    // The sign job: dispatched on main only, whatever the labels.
    let sign = dist.job("sign");
    assert_eq!(
        sign.condition().as_deref(),
        Some("github.event_name == 'workflow_dispatch' && github.ref == 'refs/heads/main'")
    );
    for key in sign.own_lines() {
        for word in ["ci:full", "label", "pull_request", "head_ref"] {
            assert!(!key.contains(word), "the sign job's {key} names {word}");
        }
    }
    assert!(
        !sign.steps().iter().any(|s| {
            let code = s.code();
            code.contains("ci:full") || code.contains("label")
        }),
        "a sign step looks at labels"
    );
}

/// Adding `ci:full` makes two runs for the pull request's head commit, and
/// the one that starts second cancels the other, whose gui-linux, cancelled
/// before it started, would stay on the pull request as a failed check
/// under its name's expression. Which survives isn't the label's to say:
/// on #246 the label came a second after the pull request opened, and the
/// opened run cancelled the labeled one. So the run that survives deletes
/// the other (the `supersede` job): in every run of a pull request with
/// the label, but not another label's run ([`LABEL_GATE`]), with a token
/// that can manage runs and do nothing else, nothing waiting for it, never
/// failing the run, and deleting only a Distribution run for the pull
/// request's head commit, older than itself, that ended cancelled.
#[test]
fn the_ci_full_run_deletes_only_the_run_it_superseded() {
    let dist = workflow("dist.yml");
    let supersede = dist.job("supersede");
    assert_eq!(
        supersede.condition(),
        Some(format!(
            "github.event_name == 'pull_request' && \
             contains(github.event.pull_request.labels.*.name, 'ci:full') && ({LABEL_GATE})"
        ))
    );
    assert_eq!(supersede.get("continue-on-error").as_deref(), Some("true"));
    assert!(supersede.has("timeout-minutes"), "{supersede:#?}");
    assert!(!supersede.has("needs"), "{supersede:#?}");

    // actions: write, for this job alone; the workflow's stays contents: read.
    assert_eq!(grants(&supersede.yaml["permissions"]), ["actions: write"]);
    assert_eq!(grants(&dist.yaml["permissions"]), ["contents: read"]);
    for job in dist.jobs() {
        // The bundle job says contents: read itself, and nothing more, so a
        // wider workflow default never reaches the job that assembles the
        // release (the_bundle_job_assembles_the_release_files_without_the_key).
        if job.id == "bundle" {
            assert_eq!(
                grants(&job.yaml["permissions"]),
                ["contents: read"],
                "job {}",
                job.id
            );
        } else if job.id != "supersede" {
            assert!(
                !job.has("permissions"),
                "job {} asks for more than the workflow's",
                job.id
            );
        }
        assert!(
            !job.needs().iter().any(|n| n == "supersede"),
            "job {} waits for it",
            job.id
        );
    }

    // What it deletes, and that it never fails.
    let steps = supersede.steps();
    let [step] = steps.as_slice() else {
        panic!("{steps:#?}");
    };
    assert_eq!(
        step.env("HEAD_SHA").as_deref(),
        Some("${{ github.event.pull_request.head.sha }}")
    );
    assert_eq!(
        step.env("RUN_NUMBER").as_deref(),
        Some("${{ github.run_number }}")
    );
    let run = step.run();
    for must in [
        "actions/workflows/dist.yml/runs?event=pull_request&head_sha=$HEAD_SHA",
        "select(.path == \".github/workflows/dist.yml\" and .event == \"pull_request\"",
        "and .head_sha == env.HEAD_SHA and .run_number < (env.RUN_NUMBER | tonumber))",
        "for id in $(awk '$2 == \"completed\" && $3 == \"cancelled\" { print $1 }' <<<\"$runs\"); do",
        "if gh run delete \"$id\" --repo \"$GITHUB_REPOSITORY\"; then",
    ] {
        assert!(run.contains(must), "the job lacks {must:?}: {run}");
    }
    assert_eq!(run.matches("gh run delete").count(), 1, "{run}");
    let text = step.text();
    for never in ["actions/checkout", "exit 1", "set -e"] {
        assert!(!text.contains(never), "the job has {never:?}: {text}");
    }
}

/// The MSI is released and offered by the updater as much as the
/// setup.exe, so a build without one fails (D-04), and it's installed,
/// checked and removed again (msiexec /i, /x) as the setup.exe is.
#[test]
fn the_msi_is_required_and_installed_and_removed() {
    let dist = workflow("dist.yml");
    let windows = dist.job("windows").steps();
    let locate = windows
        .iter()
        .find(|s| s.name() == "Locate built artifacts")
        .expect("the windows job locates its installers")
        .run();
    assert!(
        locate.contains("Write-Error \"cargo tauri build did not produce an MSI"),
        "{locate}"
    );
    assert!(!locate.contains("Write-Warning"), "{locate}");
    let smoke = windows
        .iter()
        .find(|s| s.name().contains("(MSI)"))
        .expect("the windows job smoke-tests the MSI")
        .run();
    for needle in [
        "msiexec.exe -ArgumentList \"/i\"",
        "msiexec.exe -ArgumentList \"/x\"",
        "\"/qn\"",
        "$_.WindowsInstaller -eq 1",
        "gospel-getter-cli.exe",
        "--version",
    ] {
        assert!(smoke.contains(needle), "{needle}: {smoke}");
    }
    let upload = windows
        .iter()
        .find(|s| s.with("name").as_deref() == Some("gospel-getter-windows-msi-installer"))
        .expect("the windows job uploads the MSI");
    assert!(
        !upload.has("if"),
        "the MSI's upload is conditional: {upload:#?}"
    );
}

/// CI runs the doc tests (the `compile_fail` ones guard what the CLI's
/// `FindingsWriter` can write) on Linux and Windows, and Clippy on the
/// build every installer is, without features, as well as with them all
/// (D-02). The page's crate is built, linted and tested from its lock file
/// as committed (D-03): `--locked`, and Trunk's `locked`. The GUI checks'
/// own crate is linted and unit-tested on both platforms (D-08). The CLI's
/// `#[ignore]`d sweeps (the cross-verse `verify_quote` one, C-3) run in
/// build-and-test after its tests, on pushes to main only, with a time
/// limit of their own: no pull request waits for them.
#[test]
fn ci_runs_the_doc_tests_both_clippys_and_locked_builds() {
    let ci = workflow("ci.yml");
    let steps = ci.steps();
    let runs = |job: &str, command: &str| {
        steps
            .iter()
            .any(|s| s.job == job && s.commands().iter().any(|l| l.trim() == command))
    };
    for job in ["build-and-test", "test-windows"] {
        assert!(runs(job, "cargo test --doc"), "job {job}");
        assert!(runs(job, "cargo test --all-targets"), "job {job}");
    }
    assert!(runs(
        "build-and-test",
        "cargo clippy --all-targets --all-features -- -D warnings"
    ));
    assert!(runs(
        "build-and-test",
        "cargo clippy --all-targets -- -D warnings"
    ));

    let ignored: Vec<(usize, &Step)> = steps
        .iter()
        .enumerate()
        .filter(|(_, s)| s.text().contains("--ignored"))
        .collect();
    let [(at, sweep)] = ignored.as_slice() else {
        panic!("one step should run the ignored tests: {ignored:#?}");
    };
    assert_eq!(sweep.job, "build-and-test", "{sweep:#?}");
    assert!(
        sweep.run().trim() == "cargo test -p gospel-getter-cli -- --ignored"
            && sweep.condition().as_deref()
                == Some("github.event_name == 'push' && github.ref == 'refs/heads/main'")
            && sweep.has("timeout-minutes"),
        "{sweep:#?}"
    );
    let tests = steps
        .iter()
        .position(|s| s.job == "build-and-test" && s.name() == "Test")
        .expect("build-and-test's Test step");
    assert!(tests < *at, "the sweep runs before the other tests");

    let mut ui_steps = 0;
    for step in steps
        .iter()
        .filter(|s| s.get("working-directory").as_deref() == Some("ui-leptos"))
    {
        for line in step.commands().iter().filter(|l| l.contains("cargo ")) {
            ui_steps += 1;
            assert!(
                line.contains("cargo fmt") || line.contains(" --locked"),
                "job {}: {line}",
                step.job
            );
        }
    }
    assert!(ui_steps > 0);
    let build: Vec<&str> = TRUNK_TOML
        .lines()
        .skip_while(|l| *l != "[build]")
        .skip(1)
        .take_while(|l| !l.starts_with('['))
        .collect();
    assert!(build.contains(&"locked = true"), "{build:#?}");
    let conf: Value = serde_json::from_str(TAURI_CONF).unwrap();
    for command in ["beforeDevCommand", "beforeBuildCommand"] {
        // Plain Trunk, which reads Trunk.toml (and so builds locked).
        let run = conf["build"][command].as_str().unwrap_or_default();
        assert!(
            run.starts_with("trunk build") && !run.contains("--config"),
            "{command}: {run}"
        );
    }

    let gui = |file: &GithubFile, job: &str| {
        let step = file
            .job(job)
            .steps()
            .into_iter()
            .find(|s| s.get("working-directory").as_deref() == Some("tests/gui"))
            .unwrap_or_else(|| panic!("job {job} doesn't check tests/gui"));
        let commands = step.commands();
        let has = |command: &str| commands.iter().any(|l| l.trim_end().ends_with(command));
        assert!(
            has("cargo clippy --locked --all-targets -- -D warnings") && has("cargo test --locked"),
            "{step:#?}"
        );
    };
    gui(&ci, "test-windows");
    gui(&workflow("dist.yml"), "gui-linux");
}

/// The page asks Tauri for nothing but the app's own commands
/// (`core.invoke`) and an update's progress (`core.Channel`), which Tauri 2
/// doesn't gate by permissions for the app's own page, so it gets no
/// permissions at all (B-07): not `core:default`, whose event, menu, path,
/// window and webview permissions nothing used. A plugin's command
/// (`plugin:…`) called from the page would need its own permission here.
#[test]
fn the_page_gets_no_permissions() {
    let capability: Value = serde_json::from_str(CAPABILITY).unwrap();
    assert_eq!(capability["windows"], serde_json::json!(["main"]));
    assert_eq!(capability["permissions"], serde_json::json!([]));

    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut sources = vec![root.join("ui/index.html"), root.join("ui/leptos-loader.js")];
    for entry in std::fs::read_dir(root.join("ui-leptos/src")).expect("ui-leptos/src") {
        sources.push(entry.expect("a ui-leptos module").path());
    }
    let mut uses = 0;
    for path in sources {
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path:?}: {e}"));
        assert!(!text.contains("plugin:"), "{path:?} calls a plugin");
        assert!(!text.contains("__TAURI_INTERNALS__"), "{path:?}");
        for (at, _) in text.match_indices("__TAURI__") {
            let rest = &text[at + "__TAURI__".len()..];
            let near = &rest[..rest.len().min(200)];
            assert!(
                rest.starts_with(".core.invoke")
                    || rest.starts_with(".core.Channel")
                    || (rest.starts_with("\", \"core\"]")
                        && (near.contains("js_name = invoke") || near.contains("\"Channel\""))),
                "{path:?} reaches more of Tauri than core.invoke and core.Channel: {near}"
            );
            uses += 1;
        }
    }
    assert!(uses > 0, "the page's calls to Tauri weren't found");
}

/// The committed-bytes check, the step every job that builds or tests the
/// app runs right after its checkout (see tools/ci/committed_bytes.py).
const COMMITTED_BYTES: &str = "./.github/actions/committed-bytes";

/// Every job in ci.yml and dist.yml that checks out the repository runs the
/// committed-bytes check as its very next step, unconditionally, so nothing
/// reads a file before it's checked; the action fails the job when a file
/// differs. Why: .gitattributes' first rule, and CHANGELOG.md's 2.33.1.
#[test]
fn every_build_and_test_job_first_checks_its_checkout_is_the_committed_bytes() {
    // They build and test nothing of the repository's: they sign, check and
    // assemble the files the installer jobs built, and the release's
    // signing path stays as it is.
    const BUILD_NOTHING: [&str; 3] = ["sign", "sign-rehearsal", "bundle"];
    let mut checked = Vec::new();
    for file in ["ci.yml", "dist.yml"] {
        let workflow = workflow(file);
        for job in workflow.jobs() {
            let own = job.steps();
            let checkout = own
                .iter()
                .position(|s| s.action().as_deref() == Some("actions/checkout"));
            let check = own
                .iter()
                .position(|s| s.get("uses").as_deref() == Some(COMMITTED_BYTES));
            if BUILD_NOTHING.contains(&job.id) {
                assert!(checkout.is_some(), "{file}: {} checks out nothing", job.id);
                continue;
            }
            let Some(at) = checkout else {
                assert_eq!(
                    check, None,
                    "{file}: {} checks a checkout it hasn't",
                    job.id
                );
                continue;
            };
            assert_eq!(
                check,
                Some(at + 1),
                "{file}: {}'s next step after its checkout should be `{COMMITTED_BYTES}`",
                job.id
            );
            let step = own[at + 1];
            for key in ["if", "continue-on-error", "with"] {
                assert!(
                    !step.has(key),
                    "{file}: {}'s check has {key}: {step:#?}",
                    job.id
                );
            }
            checked.push(format!("{file}: {}", job.id));
        }
    }
    assert_eq!(
        checked,
        [
            "ci.yml: build-and-test",
            "ci.yml: frontend-tests",
            "ci.yml: test-windows",
            "dist.yml: linux",
            "dist.yml: gui-test-app-linux",
            "dist.yml: windows",
            "dist.yml: gui-test-app-windows",
            "dist.yml: gui-linux",
            "dist.yml: gui-windows",
            "dist.yml: upgrade-linux",
            "dist.yml: upgrade-windows",
        ]
    );

    let action = github_files()
        .into_iter()
        .find(|f| f.name == "actions/committed-bytes/action.yml")
        .expect(".github/actions/committed-bytes/action.yml");
    let commands: Vec<String> = code(&action.yaml)
        .iter()
        .map(|l| l.trim().to_owned())
        .filter(|l| !l.is_empty())
        .collect();
    assert!(
        commands.iter().any(|l| l
            == "\"$py\" \"$GITHUB_WORKSPACE/tools/ci/committed_bytes.py\" \"$GITHUB_WORKSPACE\""),
        "{}",
        action.text
    );
    assert!(
        every(&action.yaml, "continue-on-error").is_empty(),
        "{}",
        action.text
    );
}

/// ci.yml's job that tests each platform, and dist.yml's job that builds
/// that platform's installers.
const TESTED_RELEASES: [(&str, &str); 2] =
    [("build-and-test", "linux"), ("test-windows", "windows")];

/// A way a CI test job may build differently from the release job it
/// stands for: what differs (a key of [`build_setup`]), the value in each
/// (`-` for none, a set's values joined with `; `), and why.
struct AllowedDifference {
    ci_job: &'static str,
    what: &'static str,
    in_ci: &'static str,
    in_release: &'static str,
    why: &'static str,
}

const LINT_TOOLS: &str = "the lint tools CI runs; they change nothing rustc builds";

const CI_CARGO_CONFIG: &str = "it sets only debug and test builds' profile, which no release build \
     reads (ci_tests_what_the_release_jobs_build checks it)";
const PACKAGES: &str = "CI tests the workspace's default members together, the release builds \
     each package alone; no build turns a feature on, and the GUI checks run on the installers \
     and on a test app built -p gospel_getter as they are";
const SELF_UPDATE: &str = "a compile-time switch the installers alone have (see updater.rs); \
     updater::update_mode takes it as an argument, and its tests run both values";

const ALLOWED_DIFFERENCES: &[AllowedDifference] = &[
    AllowedDifference {
        ci_job: "build-and-test",
        what: "runs-on",
        in_ci: "ubuntu-latest",
        in_release: "ubuntu-22.04",
        why: "the installers are built on the oldest Ubuntu Tauri supports (see the linux \
              job), and gui-linux tests them there",
    },
    AllowedDifference {
        ci_job: "build-and-test",
        what: "rust components",
        in_ci: "rustfmt, clippy",
        in_release: "-",
        why: LINT_TOOLS,
    },
    AllowedDifference {
        ci_job: "test-windows",
        what: "rust components",
        in_ci: "clippy",
        in_release: "-",
        why: LINT_TOOLS,
    },
    AllowedDifference {
        ci_job: "build-and-test",
        what: "cargo config",
        in_ci: ".github/ci/cargo-config.toml into CARGO_HOME",
        in_release: "-",
        why: CI_CARGO_CONFIG,
    },
    AllowedDifference {
        ci_job: "test-windows",
        what: "cargo config",
        in_ci: ".github/ci/cargo-config.toml into CARGO_HOME",
        in_release: "-",
        why: CI_CARGO_CONFIG,
    },
    AllowedDifference {
        ci_job: "build-and-test",
        what: "env RUSTDOCFLAGS",
        in_ci: "-D warnings (step: Docs)",
        in_release: "-",
        why: "rustdoc's own flags, for the docs step; nothing is built with them",
    },
    AllowedDifference {
        ci_job: "test-windows",
        what: "env RUSTFLAGS",
        in_ci: "-",
        in_release: "-C target-feature=+crt-static (step: Build the CLI sidecar)",
        why: "links the shipped CLI's C runtime statically, which only its link step sees; \
              the windows job checks its imports itself",
    },
    AllowedDifference {
        ci_job: "build-and-test",
        what: "env GOSPEL_GETTER_SELF_UPDATE",
        in_ci: "-",
        in_release: "1 (step: Build AppImage and deb bundles)",
        why: SELF_UPDATE,
    },
    AllowedDifference {
        ci_job: "test-windows",
        what: "env GOSPEL_GETTER_SELF_UPDATE",
        in_ci: "-",
        in_release: "1 (step: Build Windows installers)",
        why: SELF_UPDATE,
    },
    AllowedDifference {
        ci_job: "build-and-test",
        what: "cargo packages",
        in_ci: "-p gospel-getter-cli; default members",
        in_release: "-p gospel-getter-cli; -p gospel_getter",
        why: PACKAGES,
    },
    AllowedDifference {
        ci_job: "test-windows",
        what: "cargo packages",
        in_ci: "default members",
        in_release: "-p gospel-getter-cli; -p gospel_getter",
        why: PACKAGES,
    },
];

/// What a job builds with, as `what -> values` ([`build_setup`]).
type BuildSetup = std::collections::BTreeMap<String, std::collections::BTreeSet<String>>;

/// How a job prepares and runs its build, as `what -> values`: the runner,
/// the checkout step, the Rust install's action and `with:` options, every
/// `RUST*`, `CARGO*` and `GOSPEL_GETTER_*` variable (the workflow's, the
/// job's and each step's, a step's named), the cargo config it adds, and the
/// packages and features of its app builds (the cargo build, test and tauri
/// build commands it runs in src-tauri). Panics on a git config step, which
/// no build job may have (see every_checkout_keeps_the_committed_line_endings).
fn build_setup(file: &GithubFile, job: &str) -> BuildSetup {
    let mut setup = BuildSetup::default();
    let mut add = |what: &str, value: String| {
        setup.entry(what.to_owned()).or_default().insert(value);
    };
    let job = file.job(job);
    let steps = job.steps();

    if let Some(runner) = job.get("runs-on") {
        add("runs-on", runner);
    }

    let checkouts: Vec<&Step> = steps
        .iter()
        .filter(|s| s.action().as_deref() == Some("actions/checkout"))
        .collect();
    let [checkout] = checkouts.as_slice() else {
        panic!("{}: one checkout: {checkouts:#?}", job.id);
    };
    add("checkout", flow(checkout.yaml));

    let installs: Vec<&Step> = steps
        .iter()
        .filter(|s| s.action().as_deref() == Some("dtolnay/rust-toolchain"))
        .collect();
    let [install] = installs.as_slice() else {
        panic!("{}: one Rust install: {installs:#?}", job.id);
    };
    add("rust action", install.get("uses").unwrap_or_default());
    for (key, value) in entries(&install.yaml["with"]) {
        if let Some(value) = scalar(value) {
            add(&format!("rust {key}"), value);
        }
    }

    let built_with = |name: &str| {
        ["RUST", "CARGO", "GOSPEL_GETTER_"]
            .iter()
            .any(|prefix| name.starts_with(prefix))
    };
    for (name, value) in entries(&file.yaml["env"])
        .into_iter()
        .chain(entries(&job.yaml["env"]))
    {
        if built_with(&name)
            && let Some(value) = scalar(value)
        {
            add(&format!("env {name}"), value);
        }
    }
    for step in &steps {
        for (name, value) in entries(&step.yaml["env"]) {
            if built_with(&name)
                && let Some(value) = scalar(value)
            {
                add(
                    &format!("env {name}"),
                    format!("{value} (step: {})", step.name()),
                );
            }
        }
    }

    let default_dir =
        scalar(&job.yaml["defaults"]["run"]["working-directory"]).unwrap_or_else(|| ".".to_owned());
    for step in &steps {
        let code = step.code();
        assert!(
            !code.contains("git config") && !code.contains("git -c "),
            "{}: a git config step: {step:#?}",
            job.id
        );
        let commands = step.commands();
        for line in &commands {
            if let Some((cat, into)) = line.trim_start().split_once(" >> ")
                && let Some(file) = cat.strip_prefix("cat ")
                && into.contains("CARGO_HOME")
                && into.ends_with("config.toml\"")
            {
                add("cargo config", format!("{file} into CARGO_HOME"));
            }
        }
        let dir = step
            .get("working-directory")
            .unwrap_or_else(|| default_dir.clone());
        for line in &commands {
            // A command, not a message that names one.
            let Some(command) = line.trim_start().strip_prefix("cargo ") else {
                continue;
            };
            // Cargo's own arguments: `cargo tauri build` hands those after
            // `--` to cargo; cargo build and test hand them to the program.
            let args = if let Some(tauri) = command.strip_prefix("tauri build") {
                tauri.split_once(" -- ").map_or("", |(_, cargo)| cargo)
            } else if let Some(rest) = command
                .strip_prefix("build")
                .or(command.strip_prefix("test"))
            {
                rest.split_once(" -- ").map_or(rest, |(cargo, _)| cargo)
            } else {
                continue;
            };
            let args: Vec<&str> = args.split_whitespace().collect();
            let mut features = Vec::new();
            let mut packages = Vec::new();
            for (i, arg) in args.iter().enumerate() {
                let next = args.get(i + 1).copied().unwrap_or_default();
                match *arg {
                    "--config" => add("cargo config", format!("--config {next}")),
                    "--features" | "-F" => features.push(format!("--features {next}")),
                    "--all-features" | "--no-default-features" => features.push((*arg).to_owned()),
                    "-p" | "--package" => packages.push(format!("-p {next}")),
                    "--workspace" => packages.push("--workspace".to_owned()),
                    other if other.starts_with("--config=") => {
                        add("cargo config", other.replacen('=', " ", 1));
                    }
                    other if other.starts_with("--features=") => {
                        features.push(other.replacen('=', " ", 1));
                    }
                    _ => {}
                }
            }
            if dir != "src-tauri" {
                continue;
            }
            add(
                "cargo features",
                if features.is_empty() {
                    "default".to_owned()
                } else {
                    features.join(" ")
                },
            );
            add(
                "cargo packages",
                if packages.is_empty() {
                    "default members".to_owned()
                } else {
                    packages.join(" ")
                },
            );
        }
    }
    setup
}

/// CI tests what the release builds (H3): each platform's test job prepares
/// its build as that platform's release job does (the checkout, the Rust,
/// every variable rustc, cargo or the app's build reads, the cargo config,
/// the features), but for the differences [`ALLOWED_DIFFERENCES`] lists,
/// each with its reason. A new difference fails here until someone writes
/// down why, and an entry that no longer differs fails too. CI's added
/// cargo config only ever touches debug and test builds, and every release
/// build is a release one. The same class of bug as
/// every_checkout_keeps_the_committed_line_endings, in general.
#[test]
fn ci_tests_what_the_release_jobs_build() {
    let mut unexplained = Vec::new();
    let mut used = std::collections::BTreeSet::new();
    let (ci_yml, dist_yml) = (workflow("ci.yml"), workflow("dist.yml"));
    for (ci_job, release_job) in TESTED_RELEASES {
        let ci = build_setup(&ci_yml, ci_job);
        let release = build_setup(&dist_yml, release_job);
        let keys: std::collections::BTreeSet<&String> = ci.keys().chain(release.keys()).collect();
        for what in keys {
            let shown = |setup: &BuildSetup| {
                setup.get(what).map_or("-".to_owned(), |values| {
                    values.iter().cloned().collect::<Vec<_>>().join("; ")
                })
            };
            let (in_ci, in_release) = (shown(&ci), shown(&release));
            if in_ci == in_release {
                continue;
            }
            match ALLOWED_DIFFERENCES.iter().position(|a| {
                a.ci_job == ci_job
                    && a.what == what
                    && a.in_ci == in_ci
                    && a.in_release == in_release
            }) {
                Some(at) => {
                    used.insert(at);
                }
                None => unexplained.push(format!(
                    "{ci_job} (ci.yml) and {release_job} (dist.yml) differ in {what}: \
                     CI has `{in_ci}`, the release `{in_release}`"
                )),
            }
        }
    }
    assert!(
        unexplained.is_empty(),
        "CI's tests would run on a build the release doesn't make. Make them match, or add each \
         to ALLOWED_DIFFERENCES with the reason it's a difference between testing and shipping:\n{}",
        unexplained.join("\n")
    );
    for (at, allowed) in ALLOWED_DIFFERENCES.iter().enumerate() {
        assert!(
            used.contains(&at),
            "ALLOWED_DIFFERENCES allows a difference there is no longer: {} {}",
            allowed.ci_job,
            allowed.what
        );
        assert!(!allowed.why.is_empty());
    }

    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let config = std::fs::read_to_string(root.join(".github/ci/cargo-config.toml"))
        .expect(".github/ci/cargo-config.toml");
    let mut table = None;
    for line in config.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            assert!(
                line == "[profile.dev]" || line.starts_with("[profile.dev.package."),
                "CI's cargo config reaches past debug and test builds: {line}"
            );
            table = Some(line);
        }
        assert!(
            table.is_some(),
            "CI's cargo config sets {line} outside a profile"
        );
    }

    for (_, release_job) in TESTED_RELEASES {
        for step in dist_yml.job(release_job).steps() {
            for line in step.commands() {
                if line.contains("cargo build") {
                    assert!(line.contains("--release"), "{release_job}: {line}");
                }
                if line.contains("cargo tauri build") {
                    assert!(!line.contains("--debug"), "{release_job}: {line}");
                }
            }
        }
    }
}

/// CI's tests and the installers must run on the same checkout, the
/// committed bytes on every platform: no workflow or action sets git's
/// `core.autocrlf`, and .gitattributes' first rule keeps every file as
/// committed (the reason is above that rule).
#[test]
fn every_checkout_keeps_the_committed_line_endings() {
    for file in github_files() {
        assert!(
            !file.text.to_ascii_lowercase().contains("autocrlf"),
            "{} sets core.autocrlf",
            file.name
        );
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let attributes = std::fs::read_to_string(root.join(".gitattributes")).expect(".gitattributes");
    let first_rule = attributes
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('#'));
    assert_eq!(first_rule, Some("* text=auto eol=lf"), ".gitattributes");
}
