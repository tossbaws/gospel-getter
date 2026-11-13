//! What scripts/gui-check.sh hands the checks, through the environment.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};

use crate::native::Xvfb;
use crate::repeat::Repeat;

#[derive(Debug)]
pub struct Config {
    /// tauri-driver, e.g. http://127.0.0.1:4444.
    pub webdriver_url: String,
    /// What tauri-driver starts: the app (on Linux, a wrapper that sends
    /// its output to app.log).
    pub app: String,
    /// The run's out-dir: results.txt and screenshots/.
    pub out_dir: PathBuf,
    /// A folder of the run's own for the exported file.
    pub export_dir: PathBuf,
    /// gospel-getter-cli, which check 70 saves an AI finding with while
    /// the app runs (beside the app, or the AppImage's own), if there is one.
    pub cli: Option<PathBuf>,
    /// What is left of the run's time budget.
    pub budget: Duration,
    /// The longest any one step may take.
    pub step_timeout: Duration,
    /// How long starting the app (the WebDriver session) may take: the app
    /// seeds its database and builds its window first.
    pub session_timeout: Duration,
    /// Linux: the private X display, the only one xdotool may touch.
    pub xvfb: Option<Xvfb>,
    /// Linux: the run's own `XDG_RUNTIME_DIR`, which everything the script
    /// starts has: how the app's WebKit helpers are told apart from any
    /// other process (`gui_check::helpers`).
    pub runtime_dir: Option<String>,
    /// What tauri-driver starts for the updater checks (9 to 13): the GUI
    /// checks' test build of the app (feature `gui-test-updater`), through
    /// its own wrapper. Without one, those checks fail as not run.
    pub updater_app: Option<String>,
    /// The test build's updater settings file, which it reads before each
    /// check (`GOSPEL_GETTER_TEST_UPDATER`, set for the app by the script),
    /// and the checks write.
    pub updater_settings: Option<PathBuf>,
    /// Windows: where gui-launch records each app it starts
    /// (`GUI_LAUNCH_PIDS`, see `gui_check::launched`), each to have exited
    /// before the next session starts.
    pub launched: Option<PathBuf>,
    /// By hand only, never in CI: one check run many times in a row in its
    /// session, to chase a flaky one (GUI_CHECK_REPEAT, `crate::repeat`).
    pub repeat: Option<Repeat>,
}

/// Starting the app (a cold start seeds its database) may take longer than
/// an ordinary step.
const DEFAULT_SESSION_TIMEOUT: Duration = Duration::from_secs(30);

fn var(name: &str) -> Result<String> {
    std::env::var(name)
        .with_context(|| format!("{name} is not set (run this through scripts/gui-check.sh)"))
}

fn seconds(name: &str) -> Result<Duration> {
    let value = var(name)?;
    let secs: u64 = value
        .parse()
        .with_context(|| format!("{name} is not a number of seconds: {value:?}"))?;
    Ok(Duration::from_secs(secs))
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let xvfb = if cfg!(target_os = "linux") {
            Some(Xvfb::new(&var("GUI_XVFB_DISPLAY")?, &var("GUI_XVFB_PID")?)?)
        } else {
            None
        };
        Ok(Self {
            webdriver_url: var("GUI_WEBDRIVER_URL")?,
            app: var("GUI_APP")?,
            out_dir: PathBuf::from(var("GUI_OUT_DIR")?),
            export_dir: PathBuf::from(var("GUI_EXPORT_DIR")?),
            cli: std::env::var("GUI_CLI")
                .ok()
                .filter(|v| !v.is_empty())
                .map(PathBuf::from),
            budget: seconds("GUI_BUDGET_SECS")?,
            step_timeout: seconds("GUI_STEP_TIMEOUT_SECS")?,
            session_timeout: if std::env::var_os("GUI_SESSION_TIMEOUT_SECS").is_some() {
                seconds("GUI_SESSION_TIMEOUT_SECS")?
            } else {
                DEFAULT_SESSION_TIMEOUT
            },
            xvfb,
            runtime_dir: if cfg!(target_os = "linux") {
                Some(var("XDG_RUNTIME_DIR")?)
            } else {
                None
            },
            updater_app: std::env::var("GUI_UPDATER_APP")
                .ok()
                .filter(|v| !v.is_empty()),
            updater_settings: std::env::var_os("GUI_TEST_UPDATER_FILE").map(PathBuf::from),
            launched: if cfg!(windows) {
                Some(PathBuf::from(var("GUI_LAUNCH_PIDS")?))
            } else {
                None
            },
            repeat: Repeat::parse(&std::env::var("GUI_CHECK_REPEAT").unwrap_or_default())?,
        })
    }
}
