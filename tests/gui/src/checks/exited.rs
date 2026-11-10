//! Before each session starts, every app started before it has exited
//! (Windows; see `gui_check::launched`). One still running would hold the
//! one-app-at-a-time guard, and the app the session starts would hand over
//! to it and exit at once (Edge WebDriver: "Chrome instance exited").
//!
//! Edge WebDriver ends a session by terminating gui-launch, which takes the
//! app with it; this makes sure it did. An app still running gets
//! [`EXIT_WAIT`] to go, polled. Then it's stopped, but only if it's the
//! very process gui-launch recorded (the same pid, created at the same
//! moment, running the same program), never one found by name. One that
//! won't stop fails the session's start, saying so.
//!
//! On Linux nothing is recorded: WebKitWebDriver ends the app itself at the
//! end of a session (the script's wrapper execs it), and the guard there is
//! a name on the run's own session bus. But WebKit's helper processes go a
//! moment after the app, and its network process writes what the page put
//! in local storage in its last moments to disk only as it shuts down
//! (`gui_check::helpers`): check 65's note draft, kept as typing paused
//! just before the app quit. An app started before then reads local storage
//! without it. So each session waits, up to [`HELPERS_WAIT`], until the
//! run's WebKit helpers from the session before have exited (never stopping
//! them); one still running then fails the session's start, saying so.

use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use gui_check::launched::Launched;

use crate::config::Config;
use crate::ui::log_line;

#[cfg(windows)]
use windows::exited;

/// How long the run's WebKit helpers may take to exit once their app has
/// (Linux).
const HELPERS_WAIT: Duration = Duration::from_secs(30);

/// How often they're looked for again.
const HELPERS_POLL: Duration = Duration::from_millis(20);

/// Waits until every app gui-launch has started in the run has exited, or
/// on Linux every WebKit helper an earlier app left (see the module's
/// docs), and logs what it found.
pub(super) async fn earlier_apps_exited(config: &Config, started: Instant) -> Result<()> {
    if let Some(runtime_dir) = &config.runtime_dir {
        return helpers_exited(runtime_dir, started, started + config.budget).await;
    }
    let Some(record) = &config.launched else {
        return Ok(());
    };
    let apps = match std::fs::read_to_string(record) {
        Ok(text) => Launched::parse_all(&text).with_context(|| {
            format!("reading the apps gui-launch started ({})", record.display())
        })?,
        // Nothing has been started yet.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => {
            return Err(e).with_context(|| {
                format!("reading the apps gui-launch started ({})", record.display())
            });
        }
    };
    if apps.is_empty() {
        return Ok(());
    }
    let deadline = started + config.budget;
    let mut gone = Vec::with_capacity(apps.len());
    for app in &apps {
        gone.push(exited(app, started, deadline).await?);
    }
    log_line(
        started,
        &format!("the apps started before have exited: {}", gone.join("; ")),
    );
    Ok(())
}

/// Waits until none of the run's WebKit helpers is running (Linux; see the
/// module's docs), up to [`HELPERS_WAIT`] or `deadline`, and logs what it
/// waited for. Nothing running, it says nothing.
async fn helpers_exited(runtime_dir: &str, started: Instant, deadline: Instant) -> Result<()> {
    let left = |what: &str| {
        gui_check::helpers::running(runtime_dir)
            .with_context(|| format!("looking for {what} (/proc)"))
    };
    let first = left("WebKit's helpers from the session before")?;
    if first.is_empty() {
        return Ok(());
    }
    let named = |found: &[(u32, String)]| {
        found
            .iter()
            .map(|(pid, comm)| format!("{comm} (pid {pid})"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    log_line(
        started,
        &format!(
            "waiting for the last session's WebKit helpers to exit: {}",
            named(&first)
        ),
    );
    let begun = Instant::now();
    let until = begun + HELPERS_WAIT.min(deadline.saturating_duration_since(begun));
    loop {
        let now = left("WebKit's helpers still running")?;
        if now.is_empty() {
            log_line(
                started,
                &format!(
                    "  ok: they have exited ({:.2}s)",
                    begun.elapsed().as_secs_f64()
                ),
            );
            return Ok(());
        }
        if Instant::now() >= until {
            anyhow::bail!(
                "WebKit's helpers from the session before are still running {:.0}s after it \
                 ended: {}; an app started now could read local storage before they've \
                 written it",
                begun.elapsed().as_secs_f64(),
                named(&now)
            );
        }
        tokio::time::sleep(HELPERS_POLL).await;
    }
}

/// gui-launch only runs on Windows, so there's never a record to go by.
#[cfg(not(windows))]
async fn exited(app: &Launched, _started: Instant, _deadline: Instant) -> Result<String> {
    anyhow::bail!(
        "pid {} was recorded by gui-launch, which only runs on Windows",
        app.pid
    )
}

/// The waiting, on Windows.
#[cfg(windows)]
mod windows {
    use std::time::{Duration, Instant};

    use anyhow::{Context, Result, bail};
    use gui_check::launched::Launched;
    use gui_check::launched::process::Process;

    use crate::ui::log_line;

    /// How long an app may take to exit once its session has ended.
    const EXIT_WAIT: Duration = Duration::from_secs(30);

    /// How long a stopped app may take to be gone.
    const STOP_WAIT: Duration = Duration::from_secs(10);

    /// How often it looks again.
    const POLL: Duration = Duration::from_millis(100);

    /// Waits for `app` to have exited, stopping it if it doesn't, and says
    /// how it went.
    pub(super) async fn exited(
        app: &Launched,
        started: Instant,
        deadline: Instant,
    ) -> Result<String> {
        let name = format!("pid {} ({})", app.pid, app.path);
        let Some(process) = running(app)? else {
            return Ok(format!("{name} gone"));
        };
        log_line(
            started,
            &format!("waiting for {name}, started by an earlier session, to exit"),
        );
        let begun = Instant::now();
        let wait = EXIT_WAIT.min(deadline.saturating_duration_since(begun));
        if gone_within(&process, wait).await? {
            return Ok(format!(
                "{name} exited after {:.2}s",
                begun.elapsed().as_secs_f64()
            ));
        }
        log_line(
            started,
            &format!(
                "{name} is still running {:.0}s after its session ended: stopping it (the app \
                 gui-launch started: the same pid, creation time and program)",
                wait.as_secs_f64()
            ),
        );
        process
            .terminate()
            .with_context(|| format!("couldn't stop {name}"))?;
        if gone_within(&process, STOP_WAIT).await? {
            return Ok(format!(
                "{name} stopped, still running {:.0}s after its session ended",
                wait.as_secs_f64()
            ));
        }
        bail!(
            "{name}, started by an earlier session, is still running {:.0}s after that session \
             ended and {:.0}s after being stopped: the app started next would hand over to it and \
             exit (one app at a time)",
            wait.as_secs_f64(),
            STOP_WAIT.as_secs_f64()
        )
    }

    /// `app`, if it's still running: opened, so that its pid can't go to
    /// another process while it's watched.
    fn running(app: &Launched) -> Result<Option<Process>> {
        let process = match Process::open(app.pid) {
            Ok(Some(process)) => process,
            Ok(None) => return Ok(None),
            // Not a process the run started: those are the checks' own user's,
            // which they can always open. The app's pid went to another.
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => return Ok(None),
            Err(e) => return Err(e).with_context(|| format!("couldn't look for pid {}", app.pid)),
        };
        if process
            .exited()
            .with_context(|| format!("couldn't tell whether pid {} has exited", app.pid))?
        {
            return Ok(None);
        }
        // A process given the pid since isn't the app: the app is gone.
        let same = process
            .is(app)
            .with_context(|| format!("couldn't tell what pid {} is", app.pid))?;
        Ok(same.then_some(process))
    }

    /// Whether `process` has exited within `limit`, looking every [`POLL`].
    async fn gone_within(process: &Process, limit: Duration) -> Result<bool> {
        let until = Instant::now() + limit;
        loop {
            if process
                .exited()
                .context("couldn't tell whether the app has exited")?
            {
                return Ok(true);
            }
            if Instant::now() >= until {
                return Ok(false);
            }
            tokio::time::sleep(POLL).await;
        }
    }
}
