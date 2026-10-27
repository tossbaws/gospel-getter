//! The GUI checks' updater (feature `gui-test-updater`, which only the
//! GUI-check test build enables): where a test build looks for updates and
//! what it may do, read from a small JSON file named by
//! [`SETTINGS_VAR`], so `tests/gui` can point it at its own update server,
//! with a key made for the run, and change it between checks.
//!
//! ```json
//! { "endpoint": "http://127.0.0.1:4567/latest.json", "pubkey": "dW50cn…",
//!   "mode": "self", "timeoutMs": 2000 }
//! ```
//!
//! `mode` is `self` (a release installer: Update now downloads and
//! verifies) or `link` (any other build: Download page); `timeoutMs` is the
//! check's time limit, [`CHECK_TIMEOUT`] if left out. The file is read
//! before every check and install, and with the variable unset the build
//! checks as a release would. Either way a test build never installs what
//! it downloads: once the update is downloaded and verified it says
//! [`DOESNT_INSTALL`], so it can't replace itself.
//!
//! None of this is in any other build. No release, installer or packaging
//! build enables the feature (`tests/packaging_config.rs` fails if one
//! does), and without it `updater::settings` is `tauri.conf.json`'s and the
//! app's context is `tauri.conf.json` as it is (unit tests in `updater.rs`
//! and `lib.rs`).

use std::time::Duration;

use serde::Deserialize;

use super::desktop::Settings;
use super::{CHECK_TIMEOUT, UpdateMode};

/// Names the test settings file.
pub const SETTINGS_VAR: &str = "GOSPEL_GETTER_TEST_UPDATER";

/// What a test build says instead of installing an update.
pub const DOESNT_INSTALL: &str =
    "Test build: the update was downloaded and verified, and not installed.";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TestSettings {
    endpoint: String,
    pubkey: String,
    mode: TestMode,
    timeout_ms: Option<u64>,
}

#[derive(Debug, Deserialize)]
enum TestMode {
    #[serde(rename = "self")]
    SelfUpdate,
    #[serde(rename = "link")]
    LinkOnly,
}

/// The test settings, if [`SETTINGS_VAR`] names a file; an error (shown
/// where the check's result would be) if it can't be read.
pub fn settings() -> Result<Option<Settings>, String> {
    let Some(path) = std::env::var_os(SETTINGS_VAR) else {
        return Ok(None);
    };
    let text = std::fs::read_to_string(&path).map_err(|e| {
        format!(
            "Test build: couldn’t read the updater settings {}: {e}",
            path.to_string_lossy()
        )
    })?;
    parse(&text).map(Some)
}

/// `text` (the file's JSON) as [`Settings`]. Never installs.
pub fn parse(text: &str) -> Result<Settings, String> {
    let file: TestSettings = serde_json::from_str(text)
        .map_err(|e| format!("Test build: the updater settings aren’t usable: {e}"))?;
    let endpoint = file
        .endpoint
        .parse::<tauri::Url>()
        .map_err(|e| format!("Test build: the updater endpoint isn’t a URL: {e}"))?;
    Ok(Settings {
        mode: match file.mode {
            TestMode::SelfUpdate => UpdateMode::SelfUpdate,
            TestMode::LinkOnly => UpdateMode::LinkOnly,
        },
        timeout: file.timeout_ms.map_or(CHECK_TIMEOUT, Duration::from_millis),
        endpoint: Some(endpoint),
        pubkey: Some(file.pubkey),
        installs: false,
    })
}

/// Lets the updater reach the checks' local server, which speaks plain
/// HTTP: `dangerousInsecureTransportProtocol` in the updater plugin's
/// configuration, which a release build refuses an `http` endpoint
/// without. Only the test build's context gets it.
pub fn prepare_context<R: tauri::Runtime>(context: &mut tauri::Context<R>) {
    tracing::warn!(
        "GUI-check test build: the updater reads its settings from {SETTINGS_VAR} and never installs"
    );
    if let Some(updater) = context
        .config_mut()
        .plugins
        .0
        .get_mut("updater")
        .and_then(serde_json::Value::as_object_mut)
    {
        updater.insert(
            "dangerousInsecureTransportProtocol".to_string(),
            serde_json::Value::Bool(true),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_settings_name_the_server_key_mode_and_limit_and_never_install() {
        let settings = parse(
            r#"{ "endpoint": "http://127.0.0.1:4567/latest.json", "pubkey": "key",
                 "mode": "link", "timeoutMs": 1500 }"#,
        )
        .unwrap();
        assert_eq!(settings.mode, UpdateMode::LinkOnly);
        assert_eq!(settings.timeout, Duration::from_millis(1500));
        assert_eq!(
            settings.endpoint.unwrap().as_str(),
            "http://127.0.0.1:4567/latest.json"
        );
        assert_eq!(settings.pubkey.as_deref(), Some("key"));
        assert!(!settings.installs);

        let settings = parse(
            r#"{ "endpoint": "http://127.0.0.1:1/latest.json", "pubkey": "k", "mode": "self" }"#,
        )
        .unwrap();
        assert_eq!(settings.mode, UpdateMode::SelfUpdate);
        assert_eq!(settings.timeout, CHECK_TIMEOUT);
        assert!(!settings.installs);
    }

    #[test]
    fn unusable_settings_say_so() {
        for bad in [
            "",
            "{}",
            r#"{ "endpoint": "not a url", "pubkey": "k", "mode": "self" }"#,
            r#"{ "endpoint": "http://x/", "pubkey": "k", "mode": "maybe" }"#,
            r#"{ "endpoint": "http://x/", "pubkey": "k", "mode": "self", "installs": true }"#,
        ] {
            assert!(parse(bad).unwrap_err().starts_with("Test build:"), "{bad}");
        }
    }
}
