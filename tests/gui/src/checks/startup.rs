//! Check 74: a start-up that fails says why. Before the first session, the
//! app is started on a data folder of its own (in the run's out-dir, kept
//! for the record) whose database is a folder, so it can't be opened, the
//! way a damaged install or a full disk stops it:
//!
//! - a native error message says it couldn't start (its title, on the run's
//!   private display);
//! - the log the app keeps in that folder has the failure down to its root
//!   cause, SQLite's "unable to open database file", not only its outer
//!   words;
//! - once the message is answered (Enter), the app exits with code 1,
//!   within a step, rather than panicking.
//!
//! Nothing else runs meanwhile, so the app can't hand over to another copy
//! of itself (`window::single_instance_plugin`). Linux only: on Windows the
//! app's data folder is the account's own, which a run can't give a place
//! of its own, so the check is a SKIP there.

use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail, ensure};
use tokio::process::Command;

use crate::config::Config;
use crate::report::{Platform, skipped};
use crate::ui::log_line;

/// The native message's title (`START_FAILED_TITLE` in the app's lib.rs).
const TITLE: &str = "Gospel Getter couldn't start";

/// The app's identifier, its data folder's name (`tauri.conf.json`).
const IDENTIFIER: &str = "com.tossbaws.gospel-getter";

/// What the log must have: the failure, and the cause at its root.
const LOGGED: &str = "Gospel Getter couldn't start: ";
const ROOT_CAUSE: &str = "unable to open database file";

/// What the app's log at `log` says, or why it can't be read.
fn read_log(log: &Path) -> String {
    std::fs::read_to_string(log).unwrap_or_else(|e| format!("({} unread: {e})", log.display()))
}

/// Why check 74 isn't run on `platform`, if it isn't: a SKIP there.
fn not_run_on(platform: Platform) -> Option<&'static str> {
    (platform == Platform::Windows).then_some(
        "not run on Windows: the app's data folder there is the account's own, which a run \
         can't give a place of its own to stage a failure in",
    )
}

/// Check 74 (see the module's docs).
pub(super) async fn startup_failure(config: &Config, started: Instant) -> Result<String> {
    if let Some(why) = not_run_on(Platform::current()) {
        return Err(skipped(why));
    }
    let xvfb = config
        .xvfb
        .as_ref()
        .context("no private display to find the message on")?;
    let home = config.out_dir.join("startup-failure");
    let _ = std::fs::remove_dir_all(&home);
    let data = home.join("data");
    let app_dir = data.join(IDENTIFIER);
    let database = app_dir.join("gospel_getter.db");
    std::fs::create_dir_all(&database)
        .with_context(|| format!("couldn't make a folder at {}", database.display()))?;
    let log = app_dir.join("logs").join("gospel-getter.log");

    log_line(
        started,
        &format!(
            "start the app with a folder where its database goes ({})",
            database.display()
        ),
    );
    // Stopped when this returns, if it hasn't exited by then.
    let mut app = Command::new(&config.app)
        .env("XDG_DATA_HOME", &data)
        .stdin(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .with_context(|| format!("couldn't start {}", config.app))?;

    let deadline = Instant::now() + config.session_timeout;
    let message = loop {
        if let Some(status) = app.try_wait().context("couldn't wait for the app")? {
            bail!(
                "the app exited ({status}) without a message saying it couldn't start; its log: \
                 {}",
                read_log(&log)
            );
        }
        if let Some((id, _)) = xvfb.windows_named(TITLE).await?.into_iter().next() {
            break id;
        }
        if Instant::now() >= deadline {
            bail!(
                "no \"{TITLE}\" message within {:?}; the app's log: {}",
                config.session_timeout,
                read_log(&log)
            );
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    };
    log_line(started, &format!("  ok: \"{TITLE}\" is shown"));
    let shot = config
        .out_dir
        .join("screenshots")
        .join("74-startup-failure.png");
    if let Err(e) = xvfb.screenshot_root(&shot).await {
        log_line(started, &format!("  no screenshot of it: {e:#}"));
    }

    let logged = read_log(&log);
    let line = logged
        .lines()
        .find(|line| line.contains(LOGGED))
        .with_context(|| format!("the log doesn't say it couldn't start: {logged}"))?;
    ensure!(
        line.contains(ROOT_CAUSE),
        "the log says it couldn't start, but not why ({ROOT_CAUSE:?}): {line}"
    );

    xvfb.press_enter(&message).await?;
    let status = tokio::time::timeout(config.step_timeout, app.wait())
        .await
        .with_context(|| {
            format!(
                "the app didn't exit within {:?} of its message being answered",
                config.step_timeout
            )
        })?
        .context("couldn't wait for the app")?;
    ensure!(
        status.code() == Some(1),
        "the app exited with {status}, not code 1"
    );
    Ok(format!(
        "\"{TITLE}\" shown; the log has the root cause ({ROOT_CAUSE}); exit code 1 once it was \
         answered"
    ))
}

#[cfg(test)]
mod tests {
    use super::not_run_on;
    use crate::report::{Check, Platform};

    /// Check 74 is a SKIP exactly where the checks' table says it never
    /// runs.
    #[test]
    fn it_skips_where_the_table_says() {
        for platform in [Platform::Linux, Platform::Windows] {
            assert_eq!(
                not_run_on(platform).is_some(),
                Check::StartupFailure.skip().always_on(platform),
                "{platform}"
            );
            assert_eq!(
                not_run_on(platform).is_some(),
                Check::StartupFailure.skip().allowed_on(platform),
                "{platform}"
            );
        }
    }
}
