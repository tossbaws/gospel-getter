//! What the frontend tests' bridge (`examples/frontend_bridge.rs`) answers
//! in place of the system: the native file dialogs, the system browser, the
//! computer's clock and the updater (Go back's `previous.json` and its
//! backup's folder too).
//!
//! The bridge serves the app's own commands (`invoke_handler`) on Tauri's
//! mock runtime, which has none of these, so it scripts them through a
//! [`BridgeScript`] it sets on the state (`AppState::with_bridge_script`).
//! The app never sets one.

use std::path::PathBuf;
use std::time::Duration;

use chrono::{DateTime, FixedOffset, Utc};

use crate::updater::{DownloadEvent, Problem, UpdateInfo, UpdateMode};

/// The frontend tests' bridge's stand-ins for what its mock app doesn't
/// have (see the module docs). Never set in the app.
#[doc(hidden)]
pub trait BridgeScript: Send + Sync {
    /// The file or folder the reader chooses in a file dialog, or `None`
    /// for cancelling it. An error is the command's answer.
    fn choose_file(&self) -> Result<Option<PathBuf>, String>;

    /// Now, and the time zone the notes export reads dates in.
    fn clock(&self) -> (DateTime<Utc>, FixedOffset);

    /// Open `url` in the system browser.
    fn open_in_browser(&self, url: &str) -> std::io::Result<()>;

    /// What a check for updates finds, and how long it may take.
    fn check_for_update(&self) -> ScriptedCheck;

    /// What installing the update does, and installing Go back's release.
    fn install_update(&self) -> ScriptedInstall;

    /// What this release's `previous.json` names, if it has one (else
    /// offline or a 404 alike). The app's own rules still decide whether
    /// it's offered (`updater::check_previous_version`).
    fn check_previous(&self) -> Option<ScriptedPrevious>;

    /// The folder Go back saves its backup in, in place of the database's;
    /// `None` for the database's.
    fn backup_folder(&self) -> Option<PathBuf>;

    /// Whether this build installs updates itself (`updater::update_mode`'s
    /// answer in the app), which Go back needs.
    fn update_mode(&self) -> UpdateMode;

    /// Where the app would restart, an update installed: the install never
    /// answers after this.
    fn restart(&self);
}

/// A check for updates as a [`BridgeScript`] scripts it.
#[doc(hidden)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptedCheck {
    /// How long the check may take (the app's is `updater::CHECK_TIMEOUT`).
    pub limit: Duration,
    /// What it finds (an update, or none) or the problem it meets; `None`
    /// if it never answers.
    pub answer: Option<Result<Option<UpdateInfo>, Problem>>,
}

/// Installing an update as a [`BridgeScript`] scripts it.
#[doc(hidden)]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScriptedInstall {
    /// The download's progress, sent in this order.
    pub events: Vec<DownloadEvent>,
    /// The problem it fails with; `None` if it installs and restarts.
    pub problem: Option<Problem>,
}

/// What a [`BridgeScript`]'s `previous.json` names: a release and its
/// `schemaVersion`.
#[doc(hidden)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptedPrevious {
    pub version: String,
    pub schema_version: i64,
}
