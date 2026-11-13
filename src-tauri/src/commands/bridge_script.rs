//! What the frontend tests' bridge (`examples/frontend_bridge.rs`) answers
//! in place of the system: the native file dialogs, the system browser, the
//! computer's clock and the updater.
//!
//! The bridge serves the app's own commands (`invoke_handler`) on Tauri's
//! mock runtime, which has none of these, so it scripts them through a
//! [`BridgeScript`] it sets on the state (`AppState::with_bridge_script`).
//! The app never sets one: each command that asks finds none and goes on
//! exactly as it did before the bridge could ask.

use std::path::PathBuf;
use std::time::Duration;

use chrono::{DateTime, FixedOffset, Utc};

use crate::updater::{DownloadEvent, Problem, UpdateInfo};

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

    /// What installing the update does.
    fn install_update(&self) -> ScriptedInstall;

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
