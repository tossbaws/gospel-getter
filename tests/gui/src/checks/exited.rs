//! Before each session starts, every app started before it has exited
//! (Windows; see `gui_check::launched`). One still running would hold the
//! one-app-at-a-time guard, and the app the session starts would hand over
//! to it and exit at once (Edge WebDriver: "Chrome instance exited").
//!
//! Edge WebDriver ends a session by terminating gui-launch, which takes the
//! app with it; this makes sure it did. The app is still exiting when the
//! session's end is reported, and Windows won't say what program an exiting
//! process runs, so each recorded pid's process is held open (its pid can't
//! go to another meanwhile) and given [`SETTLE`] to exit before it's asked
//! what it is: one that has exited isn't running, whatever it was. One
//! still running is the app only by the pid, the creation time and the
//! program gui-launch recorded (the program unless Windows refuses to say,
//! as for an exiting process), never by name, and is told apart from a
//! process given the pid since only by a creation time or a program that
//! differs, never by a refusal. The app gets [`EXIT_WAIT`] in all to go,
//! polled; then it's stopped. One that won't stop fails the session's
//! start, saying so, as does a process that can't be told apart. Windows
//! refusing to open the app (by its creation time) or to say what the
//! process holding its pid is, as it can while a process exits (run
//! 42879721170: "couldn't tell what pid 7896 is: Access is denied"), is
//! waited through within the same [`EXIT_WAIT`], never taken as an answer.
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
    use gui_check::launched::process::Process;
    use gui_check::launched::{Identity, Launched};

    use crate::ui::log_line;

    /// How long an app may take to exit once its session has ended.
    const EXIT_WAIT: Duration = Duration::from_secs(30);

    /// How long a recorded pid's process gets to exit before it's asked what
    /// it is: an app ended with its session exits within about 0.1s.
    const SETTLE: Duration = Duration::from_secs(1);

    /// How long a stopped app may take to be gone.
    const STOP_WAIT: Duration = Duration::from_secs(10);

    /// How often it looks again.
    const POLL: Duration = Duration::from_millis(100);

    /// Waits for `app` to have exited, stopping it if it doesn't, and says
    /// how it went. Windows refusing to say what the process holding its pid
    /// is, as it can while a process exits, is waited through, never taken
    /// as an answer, within the same [`EXIT_WAIT`].
    pub(super) async fn exited(
        app: &Launched,
        started: Instant,
        deadline: Instant,
    ) -> Result<String> {
        let name = format!("pid {} ({})", app.pid, app.path);
        let begun = Instant::now();
        let wait = EXIT_WAIT.min(deadline.saturating_duration_since(begun));
        let until = begun + wait;
        let Some(process) = held(app, &name, started, until).await? else {
            return Ok(format!("{name} gone"));
        };
        // Whatever holds the pid, once it has exited nothing recorded is
        // running under it.
        if gone_within(
            &process,
            SETTLE.min(until.saturating_duration_since(Instant::now())),
        )
        .await?
        {
            return Ok(format!(
                "{name} exited after {:.2}s",
                begun.elapsed().as_secs_f64()
            ));
        }
        let Some(identity) = identified(&process, app, &name, started, until).await? else {
            return Ok(format!(
                "{name} exited after {:.2}s (Windows wouldn't say what it was meanwhile)",
                begun.elapsed().as_secs_f64()
            ));
        };
        let how = match identity {
            Identity::App => "the same pid, creation time and program",
            Identity::AppExiting => {
                "the same pid and creation time; Windows no longer says what an exiting process runs"
            }
            Identity::CreatedOtherwise => {
                return Ok(format!("{name} gone (the pid is another process's now)"));
            }
            Identity::OtherProgram => {
                return Ok(format!("{name} gone (the pid runs another program now)"));
            }
        };
        log_line(
            started,
            &format!("waiting for {name}, started by an earlier session, to exit"),
        );
        if gone_within(&process, until.saturating_duration_since(Instant::now())).await? {
            return Ok(format!(
                "{name} exited after {:.2}s",
                begun.elapsed().as_secs_f64()
            ));
        }
        log_line(
            started,
            &format!(
                "{name} is still running {:.0}s after its session ended: stopping it (the app \
                 gui-launch started: {how})",
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

    /// The process holding `app`'s pid, if one does: opened, so that its
    /// pid can't go to another process while it's watched. `None` when no
    /// process has the pid, or when Windows won't let it be opened and its
    /// creation time shows it's another process. When Windows won't let the
    /// app itself (by its creation time) be opened, as it can refuse a
    /// process that is exiting, it's asked again every [`POLL`] until the
    /// app is gone or opens, up to `until`; one that does neither fails,
    /// saying so, as before.
    async fn held(
        app: &Launched,
        name: &str,
        started: Instant,
        until: Instant,
    ) -> Result<Option<Process>> {
        let mut said = false;
        loop {
            match Process::open(app.pid) {
                Ok(process) => return Ok(process),
                // The app is the checks' own account's, which can always
                // open it this way, unless it's exiting; so is this another
                // process? Only its creation time says, never the refusal
                // itself.
                Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                    let created = Process::created_at(app.pid)
                        .with_context(|| format!("couldn't tell what pid {} is ({e})", app.pid))?;
                    if !app.is_by_creation(created) {
                        // Gone, or another process created since.
                        return Ok(None);
                    }
                    if Instant::now() >= until {
                        bail!(
                            "pid {} is the app gui-launch recorded (created at the moment \
                             recorded), but couldn't be opened to wait for it: {e}",
                            app.pid
                        );
                    }
                    if !said {
                        said = true;
                        log_line(
                            started,
                            &format!(
                                "{name} won't open to be waited for ({e}), as Windows can refuse \
                                 an exiting process: asking again until it's gone"
                            ),
                        );
                    }
                    tokio::time::sleep(POLL).await;
                }
                Err(e) => {
                    return Err(e).with_context(|| format!("couldn't look for pid {}", app.pid));
                }
            }
        }
    }

    /// What `process`, which holds `app`'s pid, is (`Process::identify`).
    /// When Windows won't say even when it was created, as it can refuse an
    /// exiting process, it's asked again every [`POLL`] while the process
    /// runs, up to `until`: `None` if it exits meanwhile (nothing recorded
    /// is running under the pid then, whatever it was). One that neither
    /// exits nor says can't be told apart from a process given the pid
    /// since, so it's never stopped: that fails, saying so.
    async fn identified(
        process: &Process,
        app: &Launched,
        name: &str,
        started: Instant,
        until: Instant,
    ) -> Result<Option<Identity>> {
        let mut said = false;
        loop {
            let identity = process
                .identify(app)
                .with_context(|| format!("couldn't tell what pid {} is", app.pid))?;
            if let Some(identity) = identity {
                return Ok(Some(identity));
            }
            if !said {
                said = true;
                log_line(
                    started,
                    &format!(
                        "{name}: Windows won't say when it was created (as it can refuse an exiting \
                         process); waiting for it to exit or say"
                    ),
                );
            }
            if gone_within(process, POLL).await? {
                return Ok(None);
            }
            if Instant::now() >= until {
                bail!(
                    "couldn't tell what pid {} is: Windows still won't say when the process holding \
                     it was created, and it hasn't exited, so it can't be told apart from a process \
                     given the pid since (and isn't stopped)",
                    app.pid
                );
            }
        }
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
