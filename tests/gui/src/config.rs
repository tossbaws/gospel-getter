//! What scripts/gui-check.sh hands the checks, through the environment.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};

use crate::native::Xvfb;

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
    /// What is left of the run's time budget.
    pub budget: Duration,
    /// The longest any one step may take.
    pub step_timeout: Duration,
    /// How long starting the app (the WebDriver session) may take: the app
    /// seeds its database and builds its window first.
    pub session_timeout: Duration,
    /// Linux: the private X display, the only one xdotool may touch.
    pub xvfb: Option<Xvfb>,
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
            budget: seconds("GUI_BUDGET_SECS")?,
            step_timeout: seconds("GUI_STEP_TIMEOUT_SECS")?,
            session_timeout: if std::env::var_os("GUI_SESSION_TIMEOUT_SECS").is_some() {
                seconds("GUI_SESSION_TIMEOUT_SECS")?
            } else {
                DEFAULT_SESSION_TIMEOUT
            },
            xvfb,
        })
    }
}
