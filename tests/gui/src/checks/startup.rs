//! Check 74: a start-up that fails says why. Before the first session, the
//! app is started on a data folder whose database is a folder, so it can't
//! be opened, the way a damaged install or a full disk stops it:
//!
//! - a native error message says it couldn't start (its title: on the
//!   run's private display on Linux, through UI Automation on Windows);
//! - the log the app keeps has the failure down to its root cause,
//!   SQLite's "unable to open database file", not only its outer words;
//! - once the message is answered (Enter; its OK button on Windows), the
//!   app exits with code 1, within a step, rather than panicking.
//!
//! Nothing else runs meanwhile, so the app can't hand over to another copy
//! of itself (`window::single_instance_plugin`).
//!
//! Where the failure is staged: on Linux, a data folder of the run's own
//! (`XDG_DATA_HOME`, in the run's out-dir, kept for the record). On Windows
//! the app's folders are the account's own (`%APPDATA%` for its data,
//! `%LOCALAPPDATA%` for its log), which nothing lets a run move, so the
//! failure is staged in them: only when CI asks ([`STAGING`], which
//! dist.yml's gui-windows job sets, on a runner thrown away after the job)
//! and never on anyone's own computer, where the check is a SKIP. Even
//! there, whatever the folders held is moved aside first and put back
//! after ([`Staged`]), and the app's log is copied to the out-dir.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail, ensure};
use tokio::process::Command;

use crate::config::Config;
use crate::native;
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

/// CI's word that check 74 may stage its failure in the account's own
/// folders on Windows (see the module's docs): `1`, with GitHub Actions'
/// own `GITHUB_ACTIONS=true` as well.
const STAGING: &str = "GUI_CHECK_STAGE_IN_APPDATA";

/// Whether CI has asked for check 74 to be staged in the account's own
/// folders ([`STAGING`]).
fn staging_asked() -> bool {
    std::env::var(STAGING).is_ok_and(|v| v == "1")
        && std::env::var("GITHUB_ACTIONS").is_ok_and(|v| v == "true")
}

/// Why check 74 isn't run on `platform`, if it isn't: a SKIP there. On
/// Windows it runs only when CI asks (`staging`).
fn not_run_on(platform: Platform, staging: bool) -> Option<&'static str> {
    (platform == Platform::Windows && !staging).then_some(
        "not run on Windows outside CI: the app's folders there are the account's own \
         (%APPDATA% and %LOCALAPPDATA%), which only a CI runner thrown away after the job can \
         spare to stage a failed start in (GUI_CHECK_STAGE_IN_APPDATA, set by dist.yml's \
         gui-windows job)",
    )
}

/// Check 74 (see the module's docs).
pub(super) async fn startup_failure(config: &Config, started: Instant) -> Result<String> {
    let platform = Platform::current();
    if let Some(why) = not_run_on(platform, staging_asked()) {
        return Err(skipped(why));
    }
    let home = config.out_dir.join("startup-failure");
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).with_context(|| format!("couldn't make {}", home.display()))?;
    if platform == Platform::Windows {
        let staged = Staged::new(&home)?;
        let result = failed_start(config, started, &staged.app_dir, &staged.log, &home).await;
        let put_back = staged.put_back();
        let said = result?;
        put_back?;
        return Ok(said);
    }
    let app_dir = home.join("data").join(IDENTIFIER);
    let log = app_dir.join("logs").join("gospel-getter.log");
    failed_start(config, started, &app_dir, &log, &home).await
}

/// The account's own app folders on Windows, each moved aside (if there
/// was one) for check 74 to stage its failure in, and put back after.
struct Staged {
    /// The app's data folder (`%APPDATA%`), where the database goes.
    app_dir: PathBuf,
    /// The log the app keeps (`%LOCALAPPDATA%`).
    log: PathBuf,
    /// Where the log is copied before the folders are put back.
    kept_log: PathBuf,
    /// Each folder the check uses, and where what it held was moved, if
    /// it held anything.
    folders: Vec<(PathBuf, Option<PathBuf>)>,
}

impl Staged {
    fn new(home: &Path) -> Result<Self> {
        let mut staged = Self {
            app_dir: PathBuf::new(),
            log: PathBuf::new(),
            kept_log: home.join("gospel-getter.log"),
            folders: Vec::new(),
        };
        for base in ["APPDATA", "LOCALAPPDATA"] {
            let base = PathBuf::from(
                std::env::var_os(base).with_context(|| format!("{base} is not set"))?,
            );
            let folder = base.join(IDENTIFIER);
            let aside = base.join(format!("{IDENTIFIER}.gui-check-74"));
            // Left by a run that couldn't put it back: never overwritten.
            // An error here drops `staged`, which puts back what it moved.
            ensure!(
                !aside.exists(),
                "{} is already there (an earlier check 74 didn't put it back)",
                aside.display()
            );
            let moved = if folder.exists() {
                std::fs::rename(&folder, &aside).with_context(|| {
                    format!(
                        "couldn't move {} aside to {}",
                        folder.display(),
                        aside.display()
                    )
                })?;
                Some(aside)
            } else {
                None
            };
            staged.folders.push((folder, moved));
        }
        staged.app_dir = staged.folders[0].0.clone();
        staged.log = staged.folders[1].0.join("logs").join("gospel-getter.log");
        Ok(staged)
    }

    /// Keeps the log, removes what the check made and puts back what the
    /// folders held.
    fn put_back(mut self) -> Result<()> {
        if self.log.exists() {
            std::fs::copy(&self.log, &self.kept_log).with_context(|| {
                format!(
                    "couldn't keep {} as {}",
                    self.log.display(),
                    self.kept_log.display()
                )
            })?;
        }
        self.restore()
    }

    fn restore(&mut self) -> Result<()> {
        let mut failed = Vec::new();
        for (folder, moved) in self.folders.drain(..) {
            if folder.exists()
                && let Err(e) = std::fs::remove_dir_all(&folder)
            {
                failed.push(format!("removing {}: {e}", folder.display()));
                continue;
            }
            if let Some(moved) = moved
                && let Err(e) = std::fs::rename(&moved, &folder)
            {
                failed.push(format!(
                    "moving {} back to {}: {e}",
                    moved.display(),
                    folder.display()
                ));
            }
        }
        ensure!(
            failed.is_empty(),
            "couldn't put the app's folders back: {}",
            failed.join("; ")
        );
        Ok(())
    }
}

impl Drop for Staged {
    /// Whatever went wrong before [`Staged::put_back`], the folders go back.
    fn drop(&mut self) {
        if let Err(e) = self.restore() {
            eprintln!("check 74: {e:#}");
        }
    }
}

/// Check 74's run, on the app's data folder `app_dir` (its log at `log`),
/// with its screenshot and anything else kept in `home`.
async fn failed_start(
    config: &Config,
    started: Instant,
    app_dir: &Path,
    log: &Path,
    home: &Path,
) -> Result<String> {
    let windows = config.xvfb.is_none();
    let database = app_dir.join("gospel_getter.db");
    std::fs::create_dir_all(&database)
        .with_context(|| format!("couldn't make a folder at {}", database.display()))?;
    // On Windows the checks start the app through gui-launch; this starts
    // the app itself, with nothing between.
    let program = if windows {
        std::env::var("GUI_LAUNCH_TARGET").context("GUI_LAUNCH_TARGET is not set")?
    } else {
        config.app.clone()
    };

    log_line(
        started,
        &format!(
            "start the app with a folder where its database goes ({})",
            database.display()
        ),
    );
    // Stopped when this returns, if it hasn't exited by then.
    let mut command = Command::new(&program);
    if !windows {
        command.env("XDG_DATA_HOME", app_dir.parent().context("no data folder")?);
    }
    let mut app = command
        .stdin(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .with_context(|| format!("couldn't start {program}"))?;

    let deadline = Instant::now() + config.session_timeout;
    let message = match &config.xvfb {
        Some(xvfb) => loop {
            if let Some(status) = app.try_wait().context("couldn't wait for the app")? {
                bail!(
                    "the app exited ({status}) without a message saying it couldn't start; its \
                     log: {}",
                    read_log(log)
                );
            }
            if let Some((id, _)) = xvfb.windows_named(TITLE).await?.into_iter().next() {
                break id;
            }
            if Instant::now() >= deadline {
                bail!(
                    "no \"{TITLE}\" message within {:?}; the app's log: {}",
                    config.session_timeout,
                    read_log(log)
                );
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        },
        None => match native::windows_message(TITLE, config.session_timeout, false, home).await {
            Ok(class) => class,
            Err(e) => {
                let exited = app.try_wait().context("couldn't wait for the app")?;
                bail!(
                    "no \"{TITLE}\" message within {:?} ({e:#}; the app {}); its log: {}",
                    config.session_timeout,
                    exited.map_or("still running".to_owned(), |s| format!("exited: {s}")),
                    read_log(log)
                );
            }
        },
    };
    log_line(started, &format!("  ok: \"{TITLE}\" is shown"));
    if let Some(xvfb) = &config.xvfb {
        let shot = config
            .out_dir
            .join("screenshots")
            .join("74-startup-failure.png");
        if let Err(e) = xvfb.screenshot_root(&shot).await {
            log_line(started, &format!("  no screenshot of it: {e:#}"));
        }
    }

    let logged = read_log(log);
    let line = logged
        .lines()
        .find(|line| line.contains(LOGGED))
        .with_context(|| format!("the log doesn't say it couldn't start: {logged}"))?;
    ensure!(
        line.contains(ROOT_CAUSE),
        "the log says it couldn't start, but not why ({ROOT_CAUSE:?}): {line}"
    );

    match &config.xvfb {
        Some(xvfb) => xvfb.press_enter(&message).await?,
        None => {
            let pressed = native::windows_message(TITLE, config.step_timeout, true, home).await?;
            log_line(started, &format!("  answered: {pressed}"));
        }
    }
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

    /// Check 74 is a SKIP exactly where the checks' table says it may be,
    /// and only on Windows outside CI: never where the table says it's
    /// never run.
    #[test]
    fn it_skips_where_the_table_says() {
        for platform in [Platform::Linux, Platform::Windows] {
            for staging in [false, true] {
                let skips = not_run_on(platform, staging).is_some();
                assert_eq!(
                    skips,
                    platform == Platform::Windows && !staging,
                    "{platform}, staging {staging}"
                );
                if skips {
                    assert!(
                        Check::StartupFailure.skip().allowed_on(platform),
                        "{platform}"
                    );
                }
                assert!(
                    !Check::StartupFailure.skip().always_on(platform),
                    "{platform}"
                );
            }
        }
    }
}
