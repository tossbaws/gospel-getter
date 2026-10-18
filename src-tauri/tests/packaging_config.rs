//! Regression checks on the packaging configuration Tauri and the AUR
//! `PKGBUILD` actually read — `tauri.conf.json` and `PKGBUILD` are data,
//! not code, so a drift between them and the crate (a version bump that
//! misses one of them, an icon path that no longer exists, a placeholder
//! left in place) fails silently at bundle/build time rather than at
//! `cargo check` time. These pin that down.

use serde_json::Value;

const TAURI_CONF: &str = include_str!("../tauri.conf.json");
const CARGO_TOML: &str = include_str!("../Cargo.toml");
const PKGBUILD: &str = include_str!("../../packaging/PKGBUILD");
const LIB_RS: &str = include_str!("../src/lib.rs");
const CAPABILITY: &str = include_str!("../capabilities/default.json");

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
    let expected = format!("pkgver={cargo_ver}");
    assert!(
        PKGBUILD.contains(&expected),
        "PKGBUILD's pkgver should track Cargo.toml's version ({cargo_ver}); \
         expected a `{expected}` line"
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
/// any window permission for the title (the page's own title is copied to
/// the window).
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
    assert_eq!(
        capability["permissions"],
        serde_json::json!(["core:default"])
    );
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
