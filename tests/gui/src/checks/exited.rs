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
//! a name on the run's own session bus.

use std::time::Instant;

use anyhow::{Context, Result};
use gui_check::launched::Launched;

use crate::config::Config;
use crate::ui::log_line;

#[cfg(windows)]
use windows::exited;

/// Waits until every app gui-launch has started in the run has exited
/// (see the module's docs), and logs what it found.
pub(super) async fn earlier_apps_exited(config: &Config, started: Instant) -> Result<()> {
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
