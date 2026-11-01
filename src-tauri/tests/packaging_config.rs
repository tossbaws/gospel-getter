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
const DIST_YML: &str = include_str!("../../.github/workflows/dist.yml");
const INSTALL_SH: &str = include_str!("../../packaging/install.sh");
const WINDOWS_PACKAGING: &str = include_str!("../../packaging/windows/README.md");

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

/// The GUI checks' test build of the app (feature `gui-test-updater`: its
/// updater reads a test settings file, accepts a plain-HTTP server and
/// never installs; see `src/updater/test_hook.rs`).
const TEST_FEATURE: &str = "gui-test-updater";

/// What builds the test app in dist.yml, and the artifacts it goes into.
const TEST_BUILD_STEP: &str = "Build the GUI-check test app";
const TEST_ARTIFACT: &str = "gui-check-test-app-";

/// One step of a dist.yml job: its job, its name (if it has one) and its
/// text. Crude, as `cargo_package_version` is, but enough for the shape
/// this workflow keeps: jobs at two spaces, steps at `      - `.
#[derive(Debug)]
struct Step {
    job: String,
    name: String,
    text: String,
}

fn dist_steps() -> Vec<Step> {
    let mut steps: Vec<Step> = Vec::new();
    let mut job = String::new();
    let mut in_jobs = false;
    for line in DIST_YML.lines() {
        if line == "jobs:" {
            in_jobs = true;
            continue;
        }
        if !in_jobs {
            continue;
        }
        if line.len() > 2
            && line.starts_with("  ")
            && !line.starts_with("   ")
            && line.trim_end().ends_with(':')
            && !line.trim_start().starts_with('#')
        {
            job = line.trim().trim_end_matches(':').to_string();
            continue;
        }
        if let Some(rest) = line.strip_prefix("      - ") {
            let name = rest
                .strip_prefix("name: ")
                .map(|n| n.trim_matches('"').to_string())
                .unwrap_or_default();
            steps.push(Step {
                job: job.clone(),
                name,
                text: format!("{line}\n"),
            });
            continue;
        }
        if let Some(step) = steps.last_mut() {
            if step.name.is_empty()
                && let Some(name) = line.trim_start().strip_prefix("name: ")
                && line.starts_with("        name: ")
            {
                step.name = name.trim_matches('"').to_string();
            }
            step.text.push_str(line);
            step.text.push('\n');
        }
    }
    steps
}

/// The test build can never be a release, or an installer, or anything a
/// packager builds: the feature is off by default; the AUR recipe, the
/// install script and the Windows packaging notes never name it (or turn
/// on every feature); and in dist.yml the installers' builds take no
/// features, while the test app is built only in its own steps, after its
/// job's installers are uploaded, into its own artifacts, which the release
/// procedure never takes (it takes the four installers by name).
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
    let steps = dist_steps();
    assert!(
        steps
            .iter()
            .filter(|s| s.text.contains("cargo tauri build"))
            .count()
            >= 2,
        "the installer builds weren't found: {steps:#?}"
    );
    for step in &steps {
        if step.text.contains("cargo tauri build") {
            assert!(
                !step.text.contains("--features")
                    && !step.text.contains("--all-features")
                    && !step.text.contains(TEST_FEATURE),
                "an installer build takes features: {step:#?}"
            );
        }
        if step.text.contains(TEST_FEATURE) && step.name != TEST_BUILD_STEP {
            // Only the comment at the top of the workflow, and the GUI
            // jobs reading the test app, may name it otherwise.
            assert!(
                !step.text.contains("cargo ") || step.text.contains("scripts/gui-check.sh"),
                "{TEST_FEATURE} built outside \"{TEST_BUILD_STEP}\": {step:#?}"
            );
        }
        if step.text.contains("actions/upload-artifact") && step.text.contains("gui-check-test-app")
        {
            assert!(
                step.text.contains(&format!("name: {TEST_ARTIFACT}")),
                "the test app goes into its own artifact: {step:#?}"
            );
        }
        if step.text.contains("actions/upload-artifact")
            && step.text.contains("name: gospel-getter-")
        {
            assert!(
                !step.text.contains("gui-check-test-app"),
                "an installer artifact takes the test app: {step:#?}"
            );
        }
    }
    let builds: Vec<&Step> = steps.iter().filter(|s| s.name == TEST_BUILD_STEP).collect();
    assert_eq!(
        builds.iter().map(|s| s.job.as_str()).collect::<Vec<_>>(),
        ["linux", "windows"],
        "the test app is built by the Linux and Windows jobs, once each"
    );
    for build in builds {
        assert!(
            build.text.contains(&format!(
                "cargo build --release --locked --features {TEST_FEATURE}"
            )),
            "{build:#?}"
        );
        let job: Vec<&Step> = steps.iter().filter(|s| s.job == build.job).collect();
        let at = job
            .iter()
            .position(|s| std::ptr::eq(*s, build))
            .expect("the build step is in its job");
        for (i, step) in job.iter().enumerate() {
            if step.text.contains("actions/upload-artifact")
                && step.text.contains("name: gospel-getter-")
            {
                assert!(
                    i < at,
                    "job {}: an installer is uploaded after the test app is built: {step:#?}",
                    build.job
                );
            }
        }
    }
}
