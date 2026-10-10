//! Updates: checking GitHub Releases for a newer version and, for release
//! installers, installing it after the reader agrees.
//!
//! The manifest is a static `latest.json` attached to each release (see
//! `tools/release/make_latest_json.py`), read through `tauri-plugin-updater`
//! from the endpoint in `tauri.conf.json`. Every download is checked against
//! the public key there, and with `requireSignedVersion` the signature must
//! also name the version the manifest announces.
//!
//! Whether a build may install updates itself is decided when it's built:
//! dist.yml sets `GOSPEL_GETTER_SELF_UPDATE=1` for the release installers.
//! At run time the bundler's marker says which installer this copy came
//! from: AppImage, .deb, NSIS or MSI. The plugin then picks the manifest
//! entry for exactly that installer (`linux-x86_64-deb`, ...). Any other
//! build (packaging/install.sh, the AUR recipe, `cargo build`) only says
//! that a new version exists and links to the releases page.
//!
//! The webview gets no updater, process or opener permission: it calls the
//! three commands here, and only `open_download_page` opens a URL, always
//! [`RELEASES_PAGE`].

use serde::Serialize;

/// Where a build that can't update itself sends the reader.
pub const RELEASES_PAGE: &str = "https://github.com/tossbaws/gospel-getter/releases/latest";

/// Set (to "1") at compile time by dist.yml, for the release installers only.
const SELF_UPDATE_MARKER: Option<&str> = option_env!("GOSPEL_GETTER_SELF_UPDATE");

/// The kind of package this copy of the app was installed from, as the
/// Tauri bundler marked it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bundle {
    AppImage,
    Deb,
    Rpm,
    Msi,
    Nsis,
    MacApp,
}

/// What an available update can offer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateMode {
    /// A release installer: download, verify and install in place.
    SelfUpdate,
    /// Any other build: say a new version exists and link to it.
    LinkOnly,
}

/// Only the four kinds of installer dist.yml builds, signs and lists in
/// `latest.json`, from a build dist.yml made, update themselves.
pub fn update_mode(marker: Option<&str>, bundle: Option<Bundle>) -> UpdateMode {
    match (marker, bundle) {
        (Some("1"), Some(Bundle::AppImage | Bundle::Deb | Bundle::Msi | Bundle::Nsis)) => {
            UpdateMode::SelfUpdate
        }
        _ => UpdateMode::LinkOnly,
    }
}

/// The manifest entry a link-only build reads the new version from. It
/// never downloads anything, so any entry for its OS will do; asking for
/// one by name keeps the plugin from looking for this build's own bundle
/// type, which a link-only build usually doesn't have.
pub fn link_only_target(os: &str) -> Option<&'static str> {
    match os {
        "linux" => Some("linux-x86_64-appimage"),
        "windows" => Some("windows-x86_64-nsis"),
        _ => None,
    }
}

/// Whether to offer `remote` to a reader running `current`: only a newer,
/// final release. Pre-releases are never offered (there's no beta track),
/// and never an older or equal version. Build metadata (`+...`) doesn't
/// count, as SemVer says.
pub fn is_offered(current: &semver::Version, remote: &semver::Version) -> bool {
    remote.pre.is_empty() && remote.cmp_precedence(current).is_gt()
}

/// An available update, for the banner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    /// The release notes, as plain text.
    pub notes: Option<String>,
    /// When it was published, as `YYYY-MM-DD`.
    pub date: Option<String>,
    /// Whether this build can install it (otherwise it links to it).
    pub can_self_update: bool,
}

/// Download progress, sent to the banner while an update downloads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "event", content = "data")]
pub enum DownloadEvent {
    #[serde(rename_all = "camelCase")]
    Started { content_length: Option<u64> },
    #[serde(rename_all = "camelCase")]
    Progress { chunk_length: usize },
    /// Downloaded and verified; installing, then restarting.
    Finished,
}

/// The messages the reader sees. The frontend tests use these too, through
/// the test bridge, so the wording is checked in one place.
pub mod messages {
    pub const OFFLINE: &str = "Couldn\u{2019}t reach GitHub to check for updates. Check your internet connection and try again.";
    pub const NO_RELEASE_INFO: &str =
        "Couldn\u{2019}t get the latest version from GitHub. Try again later.";
    pub const NOT_FOR_THIS_INSTALL: &str =
        "The latest version isn\u{2019}t available for this kind of installation yet.";
    pub const CHECK_FAILED: &str = "Couldn\u{2019}t check for updates.";
    pub const DOWNLOAD_FAILED: &str =
        "The update couldn\u{2019}t be downloaded. Check your internet connection and try again.";
    pub const NOT_VERIFIED: &str = "The update couldn\u{2019}t be verified as a genuine Gospel Getter release, so it wasn\u{2019}t installed.";
    pub const NOT_AUTHORIZED: &str = "Installing the update needs your password, and it wasn\u{2019}t given. Nothing was changed.";
    pub const INSTALL_FAILED: &str = "The update couldn\u{2019}t be installed.";
    pub const NOTHING_TO_INSTALL: &str =
        "There\u{2019}s no update to install. Check for updates first.";
    pub const LINK_ONLY: &str = "This copy of Gospel Getter can\u{2019}t update itself. Download the new version from the releases page.";
    pub const OPEN_FAILED: &str = "Couldn\u{2019}t open the releases page in your browser.";
}

/// What went wrong, in the terms the reader needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Problem {
    /// No connection to GitHub, or it timed out.
    Offline,
    /// GitHub answered, but not with a usable manifest (none published
    /// yet, a server error, or malformed JSON).
    NoReleaseInfo,
    /// The manifest has no entry for this installer.
    NotForThisInstall,
    /// The download's signature or signed version didn't check out.
    NotVerified,
    /// The reader cancelled or failed the password prompt (.deb).
    NotAuthorized,
    Other,
}

/// What the reader is told when checking fails.
pub fn check_message(problem: Problem) -> &'static str {
    match problem {
        Problem::Offline => messages::OFFLINE,
        Problem::NoReleaseInfo => messages::NO_RELEASE_INFO,
        Problem::NotForThisInstall => messages::NOT_FOR_THIS_INSTALL,
        _ => messages::CHECK_FAILED,
    }
}

/// What the reader is told when downloading or installing fails.
pub fn install_message(problem: Problem) -> &'static str {
    match problem {
        Problem::Offline => messages::DOWNLOAD_FAILED,
        Problem::NotVerified => messages::NOT_VERIFIED,
        Problem::NotAuthorized => messages::NOT_AUTHORIZED,
        _ => messages::INSTALL_FAILED,
    }
}

#[cfg(desktop)]
pub use desktop::*;

#[cfg(desktop)]
mod desktop {
    use std::sync::Mutex;

    use tauri::ipc::Channel;
    use tauri::{AppHandle, Manager, Runtime, State};
    use tauri_plugin_opener::OpenerExt;
    use tauri_plugin_updater::{Error, Update, UpdaterExt};

    use super::{
        Bundle, DownloadEvent, Problem, RELEASES_PAGE, SELF_UPDATE_MARKER, UpdateInfo, UpdateMode,
        check_message, install_message, is_offered, link_only_target, messages, update_mode,
    };
    use crate::commands::AppState;

    /// The update the last check found, for `install_update`.
    #[derive(Default)]
    pub struct PendingUpdate(Mutex<Option<Update>>);

    /// The bundle type the Tauri bundler marked this binary with.
    fn current_bundle() -> Option<Bundle> {
        use tauri::utils::config::BundleType;
        Some(match tauri::utils::platform::bundle_type()? {
            BundleType::AppImage => Bundle::AppImage,
            BundleType::Deb => Bundle::Deb,
            BundleType::Rpm => Bundle::Rpm,
            BundleType::Msi => Bundle::Msi,
            BundleType::Nsis => Bundle::Nsis,
            BundleType::App | BundleType::Dmg => Bundle::MacApp,
        })
    }

    pub fn current_mode() -> UpdateMode {
        update_mode(SELF_UPDATE_MARKER, current_bundle())
    }

    /// Sort an updater error into what the reader needs to know.
    pub fn classify(error: &Error) -> Problem {
        match error {
            Error::Reqwest(e) if e.is_connect() || e.is_timeout() => Problem::Offline,
            Error::Reqwest(e) if e.is_decode() || e.is_status() => Problem::NoReleaseInfo,
            Error::Reqwest(_) => Problem::Offline,
            Error::Network(_) | Error::ReleaseNotFound | Error::Serialization(_) => {
                Problem::NoReleaseInfo
            }
            Error::TargetNotFound(_) | Error::TargetsNotFound(_) => Problem::NotForThisInstall,
            Error::Minisign(_)
            | Error::Base64(_)
            | Error::SignatureUtf8(_)
            | Error::SignedVersionMismatch { .. }
            | Error::MissingSignedVersion => Problem::NotVerified,
            Error::AuthenticationFailed => Problem::NotAuthorized,
            _ => Problem::Other,
        }
    }

    /// Close the database, so nothing is left half-written when the app
    /// exits for an installer or restarts. Safe to call from synchronous
    /// code on a runtime thread (the Windows `on_before_exit` hook): the
    /// pool is closed from a separate thread.
    fn close_database<R: Runtime>(app: &AppHandle<R>) {
        let store = app.state::<AppState>().store.clone();
        let closed =
            std::thread::spawn(move || tauri::async_runtime::block_on(store.close())).join();
        if closed.is_err() {
            tracing::error!("Failed to close the database before updating");
        }
    }

    fn updater<R: Runtime>(
        app: &AppHandle<R>,
        mode: UpdateMode,
    ) -> Result<tauri_plugin_updater::Updater, Error> {
        let mut builder = app
            .updater_builder()
            .version_comparator(|current, release| is_offered(&current, &release.version));
        match mode {
            UpdateMode::SelfUpdate => {
                // Windows only: runs right before the installer starts and
                // the app exits.
                let handle = app.clone();
                builder = builder.on_before_exit(move || {
                    close_database(&handle);
                    handle.cleanup_before_exit();
                });
            }
            UpdateMode::LinkOnly => {
                if let Some(target) = link_only_target(std::env::consts::OS) {
                    builder = builder.target(target);
                }
            }
        }
        builder.build()
    }

    fn update_info(update: &Update, mode: UpdateMode) -> UpdateInfo {
        UpdateInfo {
            version: update.version.clone(),
            notes: update.body.clone().filter(|n| !n.trim().is_empty()),
            date: update
                .date
                .map(|d| format!("{:04}-{:02}-{:02}", d.year(), u8::from(d.month()), d.day())),
            can_self_update: mode == UpdateMode::SelfUpdate,
        }
    }

    /// The latest release, if it's newer than this one.
    #[tauri::command]
    pub async fn check_for_update<R: Runtime>(
        app: AppHandle<R>,
        pending: State<'_, PendingUpdate>,
    ) -> Result<Option<UpdateInfo>, String> {
        let mode = current_mode();
        let found = match updater(&app, mode) {
            Ok(updater) => updater.check().await,
            Err(e) => Err(e),
        };
        let found = found.map_err(|e| {
            tracing::error!("Failed to check for updates: {e}");
            check_message(classify(&e)).to_string()
        })?;
        let info = found.as_ref().map(|u| update_info(u, mode));
        *pending.0.lock().unwrap_or_else(|e| e.into_inner()) = found;
        Ok(info)
    }

    /// Download the update the last check found, verify it, install it and
    /// restart. On Windows the installer takes over and relaunches the app;
    /// elsewhere this restarts once it's installed. Either way it only
    /// returns on failure.
    #[tauri::command]
    pub async fn install_update<R: Runtime>(
        app: AppHandle<R>,
        pending: State<'_, PendingUpdate>,
        on_event: Channel<DownloadEvent>,
    ) -> Result<(), String> {
        if current_mode() != UpdateMode::SelfUpdate {
            return Err(messages::LINK_ONLY.to_string());
        }
        let update = pending
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .ok_or_else(|| messages::NOTHING_TO_INSTALL.to_string())?;

        let mut started = false;
        let progress = on_event.clone();
        let bytes = update
            .download(
                move |chunk_length, content_length| {
                    if !started {
                        started = true;
                        let _ = progress.send(DownloadEvent::Started { content_length });
                    }
                    let _ = progress.send(DownloadEvent::Progress { chunk_length });
                },
                || {},
            )
            .await
            .map_err(|e| {
                tracing::error!("Failed to download update {}: {e}", update.version);
                install_message(classify(&e)).to_string()
            })?;
        let _ = on_event.send(DownloadEvent::Finished);

        // On Windows this runs `on_before_exit`, starts the installer and
        // exits.
        update.install(bytes).map_err(|e| {
            tracing::error!("Failed to install update {}: {e}", update.version);
            install_message(classify(&e)).to_string()
        })?;
        tracing::info!("Installed update {}; restarting", update.version);
        close_database(&app);
        app.restart();
    }

    /// Open the releases page in the system browser (for builds that can't
    /// update themselves).
    #[tauri::command]
    pub fn open_download_page<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
        app.opener()
            .open_url(RELEASES_PAGE, None::<&str>)
            .map_err(|e| {
                tracing::error!("Failed to open {RELEASES_PAGE}: {e}");
                messages::OPEN_FAILED.to_string()
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> semver::Version {
        semver::Version::parse(s).unwrap()
    }

    #[test]
    fn only_release_installers_update_themselves() {
        use Bundle::*;
        for bundle in [AppImage, Deb, Msi, Nsis] {
            assert_eq!(update_mode(Some("1"), Some(bundle)), UpdateMode::SelfUpdate);
            // install.sh's AppImage, the AUR's deb-style install, a local
            // `cargo tauri build`: no marker, so link-only.
            assert_eq!(update_mode(None, Some(bundle)), UpdateMode::LinkOnly);
        }
        // Not a bundle dist.yml signs, or no bundle at all (`cargo build`).
        for bundle in [Some(Rpm), Some(MacApp), None] {
            assert_eq!(update_mode(Some("1"), bundle), UpdateMode::LinkOnly);
        }
        // Only the exact marker.
        assert_eq!(update_mode(Some("0"), Some(AppImage)), UpdateMode::LinkOnly);
        assert_eq!(update_mode(Some(""), Some(Deb)), UpdateMode::LinkOnly);
    }

    #[test]
    fn link_only_builds_read_a_target_the_manifest_has() {
        assert_eq!(link_only_target("linux"), Some("linux-x86_64-appimage"));
        assert_eq!(link_only_target("windows"), Some("windows-x86_64-nsis"));
        assert_eq!(link_only_target("macos"), None);
    }

    #[test]
    fn only_newer_final_releases_are_offered() {
        assert!(is_offered(&v("2.5.0"), &v("2.6.0")));
        assert!(is_offered(&v("2.5.0"), &v("2.5.1")));
        assert!(is_offered(&v("2.5.0"), &v("3.0.0")));
        assert!(
            is_offered(&v("2.9.0"), &v("2.10.0")),
            "compared as numbers, not text"
        );
        assert!(!is_offered(&v("2.5.0"), &v("2.5.0")));
        assert!(!is_offered(&v("2.5.0"), &v("2.4.9")), "never a downgrade");
        assert!(
            !is_offered(&v("2.5.0"), &v("2.6.0-beta.1")),
            "no beta track"
        );
        // Build metadata doesn't make a release newer.
        assert!(!is_offered(&v("2.5.0"), &v("2.5.0+rebuild")));
        // A pre-release build is offered its final release.
        assert!(is_offered(&v("2.6.0-rc.1"), &v("2.6.0")));
    }

    #[test]
    fn problems_have_messages_for_the_reader() {
        assert_eq!(check_message(Problem::Offline), messages::OFFLINE);
        assert_eq!(
            check_message(Problem::NoReleaseInfo),
            messages::NO_RELEASE_INFO
        );
        assert_eq!(
            check_message(Problem::NotForThisInstall),
            messages::NOT_FOR_THIS_INSTALL
        );
        assert_eq!(check_message(Problem::Other), messages::CHECK_FAILED);
        assert_eq!(install_message(Problem::Offline), messages::DOWNLOAD_FAILED);
        assert_eq!(
            install_message(Problem::NotVerified),
            messages::NOT_VERIFIED
        );
        assert_eq!(
            install_message(Problem::NotAuthorized),
            messages::NOT_AUTHORIZED
        );
        assert_eq!(install_message(Problem::Other), messages::INSTALL_FAILED);
    }

    /// The updater reads the release's `latest.json`, checks every download
    /// against the committed public key and the version it was signed for,
    /// and is only signed for in dist.yml (through the overlay), so local
    /// and install.sh builds need no private key. The webview gets no
    /// updater, process or opener permission.
    #[test]
    fn updater_config_and_permissions() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let updater = &config["plugins"]["updater"];
        assert_eq!(
            updater["endpoints"],
            serde_json::json!([
                "https://github.com/tossbaws/gospel-getter/releases/latest/download/latest.json"
            ])
        );
        assert_eq!(updater["requireSignedVersion"], true);
        assert!(updater.get("dangerousInsecureTransportProtocol").is_none());
        let pubkey = updater["pubkey"].as_str().unwrap();
        assert!(pubkey.starts_with("dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6"));
        assert!(config["bundle"].get("createUpdaterArtifacts").is_none());

        let overlay: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.updater.conf.json")).unwrap();
        assert_eq!(
            overlay,
            serde_json::json!({
                "$schema": "https://schema.tauri.app/config/2",
                "bundle": { "createUpdaterArtifacts": true }
            })
        );

        let capability: serde_json::Value =
            serde_json::from_str(include_str!("../capabilities/default.json")).unwrap();
        assert_eq!(
            capability["permissions"],
            serde_json::json!(["core:default"])
        );
        assert_eq!(
            RELEASES_PAGE,
            "https://github.com/tossbaws/gospel-getter/releases/latest"
        );
    }

    #[test]
    fn update_info_and_progress_serialize_as_the_frontend_reads_them() {
        let info = UpdateInfo {
            version: "2.6.0".into(),
            notes: Some("Notes".into()),
            date: Some("2026-10-20".into()),
            can_self_update: true,
        };
        assert_eq!(
            serde_json::to_value(info).unwrap(),
            serde_json::json!({
                "version": "2.6.0", "notes": "Notes", "date": "2026-10-20", "canSelfUpdate": true
            })
        );
        assert_eq!(
            serde_json::to_value(DownloadEvent::Started {
                content_length: Some(10)
            })
            .unwrap(),
            serde_json::json!({ "event": "started", "data": { "contentLength": 10 } })
        );
        assert_eq!(
            serde_json::to_value(DownloadEvent::Progress { chunk_length: 4 }).unwrap(),
            serde_json::json!({ "event": "progress", "data": { "chunkLength": 4 } })
        );
        assert_eq!(
            serde_json::to_value(DownloadEvent::Finished).unwrap(),
            serde_json::json!({ "event": "finished" })
        );
    }

    #[cfg(desktop)]
    #[test]
    fn updater_errors_are_classified() {
        use tauri_plugin_updater::Error;
        let cases = [
            (Error::ReleaseNotFound, Problem::NoReleaseInfo),
            (Error::Network("status 404".into()), Problem::NoReleaseInfo),
            (
                Error::TargetsNotFound(vec!["linux-x86_64-deb".into()]),
                Problem::NotForThisInstall,
            ),
            (
                Error::SignedVersionMismatch {
                    signed: "2.5.0".into(),
                    announced: "2.6.0".into(),
                },
                Problem::NotVerified,
            ),
            (Error::MissingSignedVersion, Problem::NotVerified),
            (Error::SignatureUtf8("x".into()), Problem::NotVerified),
            (Error::AuthenticationFailed, Problem::NotAuthorized),
            (Error::DebInstallFailed, Problem::Other),
            (Error::InsecureTransportProtocol, Problem::Other),
        ];
        for (error, problem) in cases {
            assert_eq!(classify(&error), problem, "{error}");
        }
        let bad_json: Error = serde_json::from_str::<serde_json::Value>("{")
            .unwrap_err()
            .into();
        assert_eq!(classify(&bad_json), Problem::NoReleaseInfo);
    }

    /// No network (here, nothing listening on the port): the reader is
    /// told to check their connection.
    #[cfg(desktop)]
    #[tokio::test]
    async fn a_refused_connection_means_offline() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        // As the updater does before its own requests.
        let _ = rustls::crypto::ring::default_provider().install_default();
        let error = reqwest::get(format!("http://127.0.0.1:{port}/latest.json"))
            .await
            .unwrap_err();
        let error = tauri_plugin_updater::Error::Reqwest(error);
        assert_eq!(classify(&error), Problem::Offline);
        assert_eq!(check_message(classify(&error)), messages::OFFLINE);
    }
}
