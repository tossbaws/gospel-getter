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
const CI_YML: &str = include_str!("../../.github/workflows/ci.yml");
const TRUNK_TOML: &str = include_str!("../../Trunk.toml");
const INSTALL_SH: &str = include_str!("../../packaging/install.sh");
const WINDOWS_PACKAGING: &str = include_str!("../../packaging/windows/README.md");
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
    assert_eq!(capability["permissions"], serde_json::json!([]));
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

/// What builds the test app in dist.yml, and the artifacts it goes into.
const TEST_BUILD_STEP: &str = "Build the GUI-check test app";
const TEST_ARTIFACT: &str = "gui-check-test-app-";

/// One step of a workflow's job: its job, its name (if it has one) and its
/// text. Crude, as `cargo_package_version` is, but enough for the shape
/// these workflows keep: jobs at two spaces, steps at `      - `.
#[derive(Debug)]
struct Step {
    job: String,
    name: String,
    text: String,
}

fn dist_steps() -> Vec<Step> {
    workflow_steps(DIST_YML)
}

fn workflow_steps(workflow: &str) -> Vec<Step> {
    let mut steps: Vec<Step> = Vec::new();
    let mut job = String::new();
    let mut in_jobs = false;
    for line in workflow.lines() {
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

/// A dist.yml job's own keys (`name:`, `if:`, `needs:` and so on), up to its
/// `steps:`, trimmed, without comments or what's nested under them.
fn dist_job_keys(job: &str) -> Vec<&'static str> {
    job_keys(DIST_YML, job)
}

/// A workflow job's own keys, as for [`dist_job_keys`].
fn job_keys<'a>(workflow: &'a str, job: &str) -> Vec<&'a str> {
    let header = format!("  {job}:");
    workflow
        .lines()
        .skip_while(|l| *l != header)
        .skip(1)
        .take_while(|l| *l != "    steps:")
        .filter(|l| l.starts_with("    ") && !l.starts_with("     "))
        .map(str::trim)
        .filter(|l| !l.starts_with('#'))
        .collect()
}

/// A release's installers come from the run dispatched on its release commit
/// after the merge (`gh workflow run dist.yml`), which joined the same
/// concurrency group as the merge's push run and cancelled it: a red X on
/// every release commit. So the push of a release-prep merge builds nothing,
/// and a dispatched run never cancels the run in progress (it is now a group
/// of its own, too: `a_push_never_cancels_a_dispatched_run`). Pull requests
/// (a release/ branch's too) and other pushes build as before.
#[test]
fn a_release_merge_never_cancels_its_own_build() {
    for job in ["linux", "windows"] {
        let keys = dist_job_keys(job);
        let condition = keys
            .iter()
            .find_map(|k| k.strip_prefix("if: "))
            .unwrap_or_else(|| panic!("job {job} has no `if:`: {keys:#?}"));
        assert!(
            condition.contains("startsWith(github.event.head_commit.message, 'chore(release):')"),
            "job {job} builds on a release-prep merge's push: {condition}"
        );
        // Only a push skips: a pull request and a dispatched run always build.
        assert!(
            condition.starts_with("github.event_name != 'push' || "),
            "job {job} can skip more than a push: {condition}"
        );
        assert!(
            !condition.contains("pull_request") && !condition.contains("workflow_dispatch"),
            "job {job}: {condition}"
        );
    }

    let concurrency: Vec<&str> = DIST_YML
        .lines()
        .skip_while(|l| *l != "concurrency:")
        .skip(1)
        .take_while(|l| l.starts_with("  "))
        .map(str::trim)
        .collect();
    let cancel = concurrency
        .iter()
        .find_map(|l| l.strip_prefix("cancel-in-progress: "))
        .unwrap_or_else(|| panic!("no cancel-in-progress: {concurrency:#?}"));
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
    let steps = dist_steps();
    for job in ["linux", "windows"] {
        let job_steps: Vec<&Step> = steps.iter().filter(|s| s.job == job).collect();
        let built_at = job_steps
            .iter()
            .position(|s| s.name == SIDECAR_STEP)
            .unwrap_or_else(|| panic!("job {job} has no \"{SIDECAR_STEP}\" step"));
        let build = &job_steps[built_at].text;
        assert!(
            build.contains("cargo build --release --locked -p gospel-getter-cli"),
            "{build}"
        );
        assert!(!build.contains("--features"), "{build}");
        assert!(build.contains("binaries/gospel-getter-cli-"), "{build}");
        let mut installer_builds = 0;
        for (at, step) in job_steps.iter().enumerate() {
            for line in step
                .text
                .lines()
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

/// AppImage users can't reach a sidecar (F8), so the program goes up on its
/// own too, from the job that built it, before the job's test app is built.
#[test]
fn the_cli_is_also_uploaded_on_its_own() {
    let steps = dist_steps();
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
        let uploads: Vec<&Step> = steps
            .iter()
            .filter(|s| s.job == job && s.text.contains(&format!("name: {artifact}\n")))
            .collect();
        assert_eq!(uploads.len(), 1, "job {job} uploads {artifact} once");
        let upload = &uploads[0].text;
        assert!(upload.contains("actions/upload-artifact"), "{upload}");
        assert!(upload.contains(&format!("path: {file}\n")), "{upload}");
        assert!(upload.contains("if-no-files-found: error"), "{upload}");
    }
}

/// Each kind of installer is checked for it, once installed.
#[test]
fn the_smoke_tests_check_the_sidecar_is_there() {
    let steps = dist_steps();
    let smoke = |job: &str, name: &str| -> String {
        steps
            .iter()
            .find(|s| s.job == job && s.name.contains(name))
            .unwrap_or_else(|| panic!("no {name} step in {job}"))
            .text
            .clone()
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
/// binary, which the windows job also builds as the test app, never the
/// first program its folder lists: the CLI sidecar sits beside it (GG-19).
#[test]
fn the_windows_gui_checks_launch_the_app_by_its_name() {
    let package = CARGO_TOML
        .lines()
        .find_map(|l| l.strip_prefix("name = \""))
        .and_then(|rest| rest.strip_suffix('"'))
        .expect("Cargo.toml's [package] name");
    let exe = format!("{package}.exe");
    let keys = dist_job_keys("gui-windows");
    let env_line = format!("GUI_EXE: {exe}");
    assert!(
        DIST_YML.lines().any(|l| l.trim() == env_line),
        "gui-windows should name the app's program, {exe}: {keys:#?}"
    );
    let steps = dist_steps();
    let install = steps
        .iter()
        .find(|s| s.job == "gui-windows" && s.name.starts_with("Install the app"))
        .expect("gui-windows installs the app");
    assert!(install.text.contains("$env:GUI_EXE"), "{install:#?}");
    assert!(
        !install
            .text
            .lines()
            .skip_while(|l| !l.contains("$location"))
            .any(|l| l.contains("Select-Object -First 1")),
        "the app's program is picked by the folder's order: {install:#?}"
    );
    for name in ["gospel-getter-cli.exe", "uninstall.exe"] {
        assert!(
            install.text.contains(name),
            "{name} isn't known: {install:#?}"
        );
    }
    let test_app = steps
        .iter()
        .find(|s| s.job == "windows" && s.name == TEST_BUILD_STEP)
        .expect("the windows job builds the test app");
    assert!(
        test_app.text.contains(&format!("target/release/{exe}")),
        "{test_app:#?}"
    );
}

/// The repository's workflows and its own actions, as (file name, text):
/// every one in their folders, so a new one is checked too.
fn github_files() -> Vec<(String, String)> {
    let github = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.github");
    let read = |path: &std::path::Path| {
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
    };
    let mut files = Vec::new();
    let workflows = std::fs::read_dir(github.join("workflows")).expect(".github/workflows");
    for entry in workflows {
        let path = entry.expect("a workflow").path();
        if path
            .extension()
            .is_some_and(|ext| ext == "yml" || ext == "yaml")
        {
            files.push((format!("workflows/{}", file_name(&path)), read(&path)));
        }
    }
    let actions = std::fs::read_dir(github.join("actions")).expect(".github/actions");
    for entry in actions {
        let dir = entry.expect("an action").path();
        for name in ["action.yml", "action.yaml"] {
            let path = dir.join(name);
            if path.is_file() {
                files.push((format!("actions/{}/{name}", file_name(&dir)), read(&path)));
            }
        }
    }
    files.sort();
    files
}

fn file_name(path: &std::path::Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// A workflow's jobs, as (id, text): from each `  id:` under `jobs:` to the
/// next.
fn workflow_jobs(workflow: &str) -> Vec<(String, String)> {
    let mut jobs: Vec<(String, String)> = Vec::new();
    let mut in_jobs = false;
    for line in workflow.lines() {
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
            jobs.push((line.trim().trim_end_matches(':').to_owned(), String::new()));
            continue;
        }
        if let Some((_, text)) = jobs.last_mut() {
            text.push_str(line);
            text.push('\n');
        }
    }
    jobs
}

/// Every action a workflow or the repository's own action uses is pinned to
/// a commit, not a tag or branch that can be moved under it (D-01), with
/// the release it is in a comment, which Dependabot keeps up to date
/// (`.github/dependabot.yml`). The repository's own actions (`./…`) are
/// the commit's own. dtolnay/rust-toolchain has no releases, only branches:
/// its comment says which.
#[test]
fn every_action_is_pinned_to_a_commit() {
    let files = github_files();
    assert!(
        files.iter().any(|(f, _)| f == "workflows/dist.yml")
            && files.iter().any(|(f, _)| f.starts_with("actions/")),
        "{:?}",
        files.iter().map(|(f, _)| f).collect::<Vec<_>>()
    );
    let mut pinned = 0;
    for (file, text) in &files {
        for (at, line) in text.lines().enumerate() {
            let key = line.trim_start();
            let key = key.strip_prefix("- ").unwrap_or(key);
            let Some(uses) = key.strip_prefix("uses:") else {
                continue;
            };
            let uses = uses.trim();
            if uses.starts_with("./") {
                continue;
            }
            let place = format!("{file}:{}: {}", at + 1, line.trim());
            let (action, comment) = uses
                .split_once(" # ")
                .unwrap_or_else(|| panic!("{place}: no comment naming its release"));
            let (name, commit) = action
                .split_once('@')
                .unwrap_or_else(|| panic!("{place}: not pinned"));
            assert!(
                commit.len() == 40
                    && commit
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
                "{place}: pinned to `{commit}`, not a full commit SHA"
            );
            if name != "dtolnay/rust-toolchain" {
                let release = comment.trim().strip_prefix('v').unwrap_or_default();
                let parts: Vec<&str> = release.split('.').collect();
                assert!(
                    parts.len() == 3
                        && parts
                            .iter()
                            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit())),
                    "{place}: the comment should name its release, as `# vX.Y.Z`"
                );
            }
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
    let workflows: Vec<(String, String)> = github_files()
        .into_iter()
        .filter(|(file, _)| file.starts_with("workflows/"))
        .collect();
    assert!(
        workflows.iter().any(|(f, _)| f == "workflows/dist.yml"),
        "{:?}",
        workflows.iter().map(|(f, _)| f).collect::<Vec<_>>()
    );
    let mut apt = 0;
    for (file, text) in &workflows {
        let jobs = workflow_jobs(text);
        assert!(!jobs.is_empty(), "{file} has no jobs");
        for (job, _) in &jobs {
            let keys = job_keys(text, job);
            assert!(
                keys.iter().any(|k| k.starts_with("timeout-minutes: ")),
                "{file}: job {job} has no timeout-minutes: {keys:#?}"
            );
        }
        for step in workflow_steps(text) {
            let installs: Vec<&str> = step
                .text
                .lines()
                .filter(|l| !l.trim_start().starts_with('#'))
                .filter(|l| l.contains("apt-get update") || l.contains("apt-get install"))
                .collect();
            if installs.is_empty() {
                continue;
            }
            for line in &installs {
                assert!(
                    line.contains(" -o Acquire::Retries=")
                        && line.contains(" -o Acquire::http::Timeout="),
                    "{file}: job {}: apt-get without its retries: {line}",
                    step.job
                );
                apt += 1;
            }
            assert!(
                step.text
                    .lines()
                    .any(|l| l.starts_with("        timeout-minutes: ")),
                "{file}: job {}: the step running apt-get has no timeout-minutes of its own: {step:#?}",
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
#[test]
fn the_gui_checks_retry_once_in_ci_and_never_force_a_failure() {
    let steps = dist_steps();
    for job in ["gui-linux", "gui-windows"] {
        let keys = job_keys(DIST_YML, job);
        assert!(keys.contains(&"env:"), "job {job} has no env: {keys:#?}");
        let body = workflow_jobs(DIST_YML)
            .into_iter()
            .find(|(id, _)| id == job)
            .map(|(_, body)| body)
            .unwrap_or_else(|| panic!("no job {job}"));
        assert!(
            body.lines().any(|l| l == "      GUI_CHECK_RETRY: \"1\""),
            "job {job} doesn't retry the suite"
        );
        let run = steps
            .iter()
            .find(|s| s.job == job && s.text.contains("scripts/gui-check.sh"))
            .unwrap_or_else(|| panic!("job {job} doesn't run the checks"));
        let out = run
            .text
            .lines()
            .find_map(|l| l.split_once("scripts/gui-check.sh ").map(|(_, rest)| rest))
            .expect("the checks' arguments");
        assert!(out.contains("/gui-check\""), "job {job}: {out}");
        let upload = steps
            .iter()
            .find(|s| s.job == job && s.text.contains("actions/upload-artifact@"))
            .unwrap_or_else(|| panic!("job {job} uploads nothing"));
        assert_eq!(
            with_value(upload, "path"),
            Some("${{ runner.temp }}/gui-check"),
            "job {job} uploads the whole out-dir, attempt-1/ too"
        );
        assert!(
            upload.text.contains("        if: always()\n"),
            "{upload:#?}"
        );
    }
    for (file, text) in github_files() {
        for name in ["force-fail", "GUI_CHECK_FORCE_FAIL"] {
            assert!(!text.contains(name), "{file} names {name}");
        }
    }
}

/// Release builds use one Rust, set in one place (`RUST_TOOLCHAIN` in
/// dist.yml), an exact release: not whatever is stable on the day.
#[test]
fn release_builds_use_one_pinned_rust() {
    let pinned = DIST_YML
        .lines()
        .find_map(|l| l.strip_prefix("  RUST_TOOLCHAIN: "))
        .expect("dist.yml sets RUST_TOOLCHAIN")
        .trim_matches('"');
    let parts: Vec<&str> = pinned.split('.').collect();
    assert!(
        parts.len() == 3 && parts.iter().all(|p| p.parse::<u32>().is_ok()),
        "RUST_TOOLCHAIN should be an exact release, X.Y.Z: {pinned}"
    );
    let installs: Vec<Step> = dist_steps()
        .into_iter()
        .filter(|s| s.text.contains("dtolnay/rust-toolchain@"))
        .collect();
    assert!(!installs.is_empty());
    for install in &installs {
        assert!(
            install
                .text
                .contains("          toolchain: ${{ env.RUST_TOOLCHAIN }}\n"),
            "job {} installs another Rust: {install:#?}",
            install.job
        );
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
    for (file, text) in github_files() {
        for everything in ["secrets: inherit", "toJSON(secrets)", "toJson(secrets)"] {
            assert!(!text.contains(everything), "{file}: {everything}");
        }
        let outside_jobs = text.split("\njobs:\n").next().unwrap_or_default();
        assert!(
            !outside_jobs
                .lines()
                .any(|l| l.contains(KEY) && !l.trim_start().starts_with('#')),
            "{file} names {KEY} outside its jobs"
        );
        if file.starts_with("actions/") {
            assert!(!text.contains(KEY), "{file} names {KEY}");
            continue;
        }
        for (job, body) in workflow_jobs(&text) {
            if !body.contains(KEY) {
                continue;
            }
            let keys = job_keys(&text, &job);
            assert!(
                keys.contains(&"environment: release"),
                "{file}: job {job} has the signing key outside the release environment: {keys:#?}"
            );
            assert!(
                keys.contains(
                    &"if: github.event_name == 'workflow_dispatch' && github.ref == 'refs/heads/main'"
                ),
                "{file}: job {job} has the signing key on more than a run dispatched on main: {keys:#?}"
            );
            holders.push(format!("{file} {job}"));
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

const RELEASE_README: &str = include_str!("../../tools/release/README.md");

/// The value of a step's `with:` key (`name`, `path`, `pattern`), if it
/// has one.
fn with_value<'a>(step: &'a Step, key: &str) -> Option<&'a str> {
    let prefix = format!("          {key}: ");
    step.text
        .lines()
        .find_map(|l| l.strip_prefix(prefix.as_str()))
        .map(str::trim)
}

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
/// does the same with a throwaway key, uploading nothing.
#[test]
fn only_a_release_run_signs_and_it_signs_what_was_checked() {
    let steps = dist_steps();
    let builds: Vec<&str> = steps
        .iter()
        .filter(|s| s.job == "linux" || s.job == "windows")
        .flat_map(|s| s.text.lines())
        .filter(|l| {
            let l = l.trim_start();
            l.strip_prefix("run: ")
                .unwrap_or(l)
                .starts_with("cargo tauri build")
        })
        .collect();
    assert!(
        builds.len() >= 2,
        "both jobs' installer builds: {builds:#?}"
    );
    for build in builds {
        assert!(
            build.contains("--config tauri.updater.conf.json") && build.ends_with(" --no-sign"),
            "an installer build that isn't the release's, unsigned: {build}"
        );
    }

    let keys = dist_job_keys("sign");
    assert!(
        keys.contains(&"needs: [linux, windows, gui-linux, gui-windows]"),
        "the sign job should wait for the builds and both GUI checks: {keys:#?}"
    );
    let job = |name: &str| -> Vec<&Step> { steps.iter().filter(|s| s.job == name).collect() };
    let sign = job("sign");
    let signs: Vec<&&Step> = sign
        .iter()
        .filter(|s| s.text.contains("tools/release/sign_installers.py"))
        .collect();
    assert_eq!(signs.len(), 1, "{sign:#?}");
    assert!(
        signs[0]
            .text
            .contains("TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}"),
        "{signs:#?}"
    );
    let uploads: Vec<&&Step> = sign
        .iter()
        .filter(|s| s.text.contains("actions/upload-artifact@"))
        .collect();
    for (built_by, artifact) in INSTALLER_ARTIFACTS {
        let name = format!("          name: {artifact}\n");
        assert!(
            job(built_by)
                .iter()
                .any(|s| s.text.contains("actions/upload-artifact@") && s.text.contains(&name)),
            "job {built_by} doesn't upload {artifact}"
        );
        // Never under the build artifact's own name: it stays as built.
        assert!(
            !uploads
                .iter()
                .any(|s| with_value(s, "name") == Some(artifact)),
            "the sign job replaces the build's {artifact}: {uploads:#?}"
        );
        let signed = format!("{artifact}{SIGNED}");
        let up: Vec<&&&Step> = uploads
            .iter()
            .filter(|s| with_value(s, "name") == Some(signed.as_str()))
            .collect();
        assert_eq!(up.len(), 1, "the sign job uploads {signed} once");
        let text = &up[0].text;
        assert!(
            text.contains(&format!("          path: installers/{artifact}/\n"))
                && text.contains("          overwrite: true\n")
                && text.contains("          if-no-files-found: error\n"),
            "{text}"
        );
        // The release procedure takes the signed one.
        assert!(
            RELEASE_README.contains(&format!("`{signed}`")),
            "tools/release/README.md doesn't name {signed}"
        );
    }
    // Uploaded to nowhere else: the release's artifacts are the four above,
    // each a -signed name.
    assert_eq!(uploads.len(), INSTALLER_ARTIFACTS.len(), "{uploads:#?}");
    for upload in &uploads {
        let name = with_value(upload, "name").unwrap_or_default();
        assert!(
            name.ends_with(SIGNED)
                && INSTALLER_ARTIFACTS
                    .iter()
                    .any(|(_, artifact)| name.strip_suffix(SIGNED) == Some(artifact)),
            "the sign job uploads {name:?}, not a -signed installer artifact: {upload:#?}"
        );
    }
    // overwrite: on the -signed names only, anywhere in dist.yml.
    for step in &steps {
        if step.text.contains("          overwrite: true\n") {
            let name = with_value(step, "name").unwrap_or_default();
            assert!(
                step.job == "sign" && name.ends_with(SIGNED),
                "job {} overwrites {name:?}: {step:#?}",
                step.job
            );
        }
    }

    let keys = dist_job_keys("sign-rehearsal");
    let condition = keys
        .iter()
        .find_map(|k| k.strip_prefix("if: "))
        .expect("the rehearsal has an if:");
    assert!(
        condition.contains(
            "github.event_name == 'workflow_dispatch' && github.ref != 'refs/heads/main'"
        ) && condition.contains("startsWith(github.head_ref, 'release/')"),
        "{condition}"
    );
    assert!(!keys.iter().any(|k| k.starts_with("environment:")));
    let rehearsal = job("sign-rehearsal");
    assert!(
        rehearsal.iter().any(|s| s
            .text
            .contains("python3 tools/release/sign_installers.py installers \\")
            && s.text
                .contains("--private-key-path \"$RUNNER_TEMP/rehearsal.key\"")
            && s.text
                .contains("--pubkey-file \"$RUNNER_TEMP/rehearsal.key.pub\"")),
        "{rehearsal:#?}"
    );
    assert!(
        !rehearsal
            .iter()
            .any(|s| s.text.contains("actions/upload-artifact@")),
        "the rehearsal's signatures are worthless: it uploads nothing"
    );
    // Both take the build's four artifacts by their exact names, each into
    // its own folder, and nothing else: never a -signed one (a pattern such
    // as gospel-getter-linux-* would take gospel-getter-linux-appimage-signed
    // too, which a re-run of the sign job would then sign again).
    for name in ["sign", "sign-rehearsal"] {
        let downloads: Vec<&Step> = job(name)
            .into_iter()
            .filter(|s| s.text.contains("actions/download-artifact@"))
            .collect();
        for download in &downloads {
            assert!(
                with_value(download, "pattern").is_none(),
                "job {name} downloads by a pattern, which can take a -signed artifact: {download:#?}"
            );
            let artifact = with_value(download, "name").unwrap_or_default();
            assert!(
                !artifact.is_empty() && !artifact.ends_with(SIGNED),
                "job {name} downloads {artifact:?}: {download:#?}"
            );
        }
        let mut names: Vec<&str> = downloads
            .iter()
            .filter_map(|s| with_value(s, "name"))
            .collect();
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
                .find(|s| with_value(s, "name") == Some(artifact))
                .and_then(|s| with_value(s, "path"));
            assert_eq!(
                into,
                Some(format!("installers/{artifact}").as_str()),
                "job {name}: {artifact} goes into its own folder"
            );
        }
    }
    // Both get their signer the same way, from crates.io, in the job.
    for name in ["sign", "sign-rehearsal"] {
        assert!(
            job(name).iter().any(|s| s.text.contains(
                "cargo install tauri-cli --version \"${{ env.TAURI_CLI_VERSION }}\" --locked"
            )),
            "job {name}"
        );
        assert!(
            !job(name).iter().any(|s| s.text.contains("rust-cache")),
            "job {name} restores a cache"
        );
    }
}

/// A push to main never cancels a dispatched run, a release's build (D-05):
/// a dispatched run is a concurrency group of its own, by its run id. Push
/// and pull request runs still cancel the run in progress on their ref.
#[test]
fn a_push_never_cancels_a_dispatched_run() {
    let group = DIST_YML
        .lines()
        .skip_while(|l| *l != "concurrency:")
        .find_map(|l| l.trim().strip_prefix("group: "))
        .expect("dist.yml has a concurrency group");
    assert_eq!(
        group,
        "dist-${{ github.workflow }}-${{ github.event_name == 'workflow_dispatch' && github.run_id || github.ref }}"
    );
}

/// The MSI is released and offered by the updater as much as the
/// setup.exe, so a build without one fails (D-04), and it's installed,
/// checked and removed again (msiexec /i, /x) as the setup.exe is.
#[test]
fn the_msi_is_required_and_installed_and_removed() {
    let steps = dist_steps();
    let windows: Vec<&Step> = steps.iter().filter(|s| s.job == "windows").collect();
    let locate = windows
        .iter()
        .find(|s| s.name == "Locate built artifacts")
        .expect("the windows job locates its installers");
    assert!(
        locate
            .text
            .contains("Write-Error \"cargo tauri build did not produce an MSI"),
        "{locate:#?}"
    );
    assert!(!locate.text.contains("Write-Warning"), "{locate:#?}");
    let smoke = windows
        .iter()
        .find(|s| s.name.contains("(MSI)"))
        .expect("the windows job smoke-tests the MSI");
    for needle in [
        "msiexec.exe -ArgumentList \"/i\"",
        "msiexec.exe -ArgumentList \"/x\"",
        "\"/qn\"",
        "$_.WindowsInstaller -eq 1",
        "gospel-getter-cli.exe",
        "--version",
    ] {
        assert!(smoke.text.contains(needle), "{needle}: {smoke:#?}");
    }
    let upload = windows
        .iter()
        .find(|s| {
            s.text
                .contains("name: gospel-getter-windows-msi-installer\n")
        })
        .expect("the windows job uploads the MSI");
    assert!(
        !upload.text.contains("        if:"),
        "the MSI's upload is conditional: {upload:#?}"
    );
}

/// CI runs the doc tests (the `compile_fail` ones guard what the CLI's
/// `FindingsWriter` can write) on Linux and Windows, and Clippy on the
/// build every installer is, without features, as well as with them all
/// (D-02). The page's crate is built, linted and tested from its lock file
/// as committed (D-03): `--locked`, and Trunk's `locked`. The GUI checks'
/// own crate is linted and unit-tested on both platforms (D-08).
#[test]
fn ci_runs_the_doc_tests_both_clippys_and_locked_builds() {
    let ci = workflow_steps(CI_YML);
    let runs = |job: &str, command: &str| {
        ci.iter()
            .any(|s| s.job == job && s.text.lines().any(|l| l.trim_end().ends_with(command)))
    };
    for job in ["build-and-test", "test-windows"] {
        assert!(runs(job, "run: cargo test --doc"), "job {job}");
        assert!(runs(job, "run: cargo test --all-targets"), "job {job}");
    }
    assert!(runs(
        "build-and-test",
        "run: cargo clippy --all-targets --all-features -- -D warnings"
    ));
    assert!(runs(
        "build-and-test",
        "run: cargo clippy --all-targets -- -D warnings"
    ));

    let mut ui_steps = 0;
    for step in ci
        .iter()
        .filter(|s| s.text.contains("working-directory: ui-leptos"))
    {
        for line in step.text.lines().filter(|l| l.contains("cargo ")) {
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

    let gui = |steps: &[Step], job: &str| {
        let step = steps
            .iter()
            .find(|s| s.job == job && s.text.contains("working-directory: tests/gui"))
            .unwrap_or_else(|| panic!("job {job} doesn't check tests/gui"));
        assert!(
            step.text
                .contains("cargo clippy --locked --all-targets -- -D warnings\n")
                && step.text.contains("cargo test --locked\n"),
            "{step:#?}"
        );
    };
    gui(&ci, "test-windows");
    gui(&dist_steps(), "gui-linux");
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
