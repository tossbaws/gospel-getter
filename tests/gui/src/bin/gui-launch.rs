//! Windows only: what Edge WebDriver starts in place of the app, so the
//! app's own log is kept. The release app is a GUI-subsystem program, so its
//! output (tracing, to stdout) goes nowhere when anything else starts it,
//! and tauri-driver only starts an `.exe`. This starts the real app
//! (`GUI_LAUNCH_TARGET`) with the same arguments and environment, its
//! output appended to `GUI_LAUNCH_LOG`, and waits for it. A copy named
//! `gui-launch-updater.exe` starts the GUI checks' test build instead
//! (`GUI_LAUNCH_UPDATER_TARGET`, `GUI_LAUNCH_UPDATER_LOG`), for the updater
//! checks' session.
//!
//! The app as shipped (not the test build) gets `GUI_LAUNCH_CUT_OFF_PROXY`
//! as its HTTPS_PROXY, HTTP_PROXY and ALL_PROXY, with no NO_PROXY but
//! 127.0.0.1, so its launch check never reaches the real update server
//! (see scripts/gui-check.sh).
//!
//! The app ends with this launcher. Edge WebDriver ends a session by
//! terminating the program it started, which is this, and on Windows a
//! child outlives its parent: the app would go on running, holding the
//! one-app-at-a-time guard, and every app started after it would hand over
//! to it and exit. So this first puts itself in a job object that ends
//! everything in it when the job's last handle closes (this process's, as
//! it's terminated); the app, started after, is in the job too. Each app
//! started is recorded in `GUI_LAUNCH_PIDS` (its pid, when it was created
//! and its program's path), and the checks make sure it has exited before
//! they start the next one (gui_check::launched).
//!
//! The log starts with what the app was given that matters to WebDriver:
//! its arguments and the WebView2 and Tauri automation variables.

use std::process::ExitCode;

#[cfg(windows)]
fn main() -> ExitCode {
    match launcher::run() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("gui-launch: {e}");
            ExitCode::from(101)
        }
    }
}

#[cfg(not(windows))]
fn main() -> ExitCode {
    eprintln!(
        "gui-launch: Windows only (on Linux scripts/gui-check.sh's own wrapper starts the app)"
    );
    ExitCode::from(2)
}

#[cfg(windows)]
mod launcher {
    use std::fs::{File, OpenOptions};
    use std::io::Write;
    use std::process::{Command, ExitCode, Stdio};

    use gui_check::launched::process::launched;

    use super::job::Job;

    /// The variables Edge WebDriver and tauri-driver set for the app.
    const SHOWN: [&str; 4] = [
        "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS",
        "WEBVIEW2_USER_DATA_FOLDER",
        "WEBVIEW2_BROWSER_EXECUTABLE_FOLDER",
        "TAURI_WEBVIEW_AUTOMATION",
    ];

    /// Appends `line` to the app's log, as best it can: the log is for
    /// reading after, and a line lost there changes nothing else.
    fn note(log: &mut File, line: &str) {
        let _ = writeln!(log, "gui-launch: {line}");
    }

    pub fn run() -> Result<ExitCode, String> {
        // Which app, by this launcher's own name.
        let updater = std::env::current_exe()
            .ok()
            .and_then(|exe| {
                exe.file_stem()
                    .map(|s| s.to_string_lossy().ends_with("-updater"))
            })
            .unwrap_or(false);
        let (target_var, log_var) = if updater {
            ("GUI_LAUNCH_UPDATER_TARGET", "GUI_LAUNCH_UPDATER_LOG")
        } else {
            ("GUI_LAUNCH_TARGET", "GUI_LAUNCH_LOG")
        };
        let target = std::env::var_os(target_var).ok_or(format!("{target_var} is not set"))?;
        let log_path = std::env::var_os(log_var).ok_or(format!("{log_var} is not set"))?;
        let pids_path = std::env::var_os("GUI_LAUNCH_PIDS").ok_or("GUI_LAUNCH_PIDS is not set")?;
        let args: Vec<_> = std::env::args_os().skip(1).collect();
        let cut_off_proxy = if updater {
            None
        } else {
            Some(
                std::env::var_os("GUI_LAUNCH_CUT_OFF_PROXY")
                    .ok_or("GUI_LAUNCH_CUT_OFF_PROXY is not set")?,
            )
        };

        let mut log = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .map_err(|e| format!("opening {}: {e}", log_path.to_string_lossy()))?;
        let mut header = format!(
            "gui-launch: starting {} with arguments {args:?}\n",
            target.to_string_lossy()
        );
        for name in SHOWN {
            let value = std::env::var_os(name);
            header.push_str(&format!(
                "gui-launch:   {name}={}\n",
                value
                    .as_deref()
                    .map_or("(not set)".into(), |v| v.to_string_lossy())
            ));
        }
        if let Some(proxy) = &cut_off_proxy {
            header.push_str(&format!(
                "gui-launch:   HTTPS_PROXY, HTTP_PROXY and ALL_PROXY={} (no real update server)\n",
                proxy.to_string_lossy()
            ));
        }
        log.write_all(header.as_bytes())
            .map_err(|e| format!("writing the log: {e}"))?;

        // Before the app starts, so it's in the job from its first moment.
        // Without one the app may outlive the session; the checks then
        // stop it themselves, and say so.
        let job = match Job::ending_with_this_process() {
            Ok(job) => Some(job),
            Err(e) => {
                note(
                    &mut log,
                    &format!(
                        "WARNING: couldn't tie the app to this launcher (a job object): {e}; \
                         it may outlive the session"
                    ),
                );
                None
            }
        };

        let stdout = log
            .try_clone()
            .map_err(|e| format!("sharing the log: {e}"))?;
        let stderr = log
            .try_clone()
            .map_err(|e| format!("sharing the log: {e}"))?;
        let mut command = Command::new(&target);
        command.args(&args);
        if let Some(proxy) = &cut_off_proxy {
            for name in ["https_proxy", "http_proxy", "all_proxy", "no_proxy"] {
                command.env_remove(name);
            }
            command
                .env("HTTPS_PROXY", proxy)
                .env("HTTP_PROXY", proxy)
                .env("ALL_PROXY", proxy)
                .env("NO_PROXY", "127.0.0.1,localhost,::1");
        }
        let mut app = command
            .stdin(Stdio::null())
            .stdout(stdout)
            .stderr(stderr)
            .spawn()
            .map_err(|e| format!("starting {}: {e}", target.to_string_lossy()))?;

        // The record the checks go by. An app they couldn't account for
        // isn't left running.
        let recorded = launched(app.id(), &app)
            .map_err(|e| format!("reading the app's pid {}: {e}", app.id()))
            .and_then(|this| {
                OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&pids_path)
                    .and_then(|mut pids| pids.write_all(this.to_line().as_bytes()))
                    .map_err(|e| format!("recording it in {}: {e}", pids_path.to_string_lossy()))
            });
        if let Err(e) = recorded {
            note(&mut log, &format!("stopping the app: {e}"));
            let _ = app.kill();
            let _ = app.wait();
            return Err(e);
        }
        note(
            &mut log,
            &format!(
                "the app is pid {}{}",
                app.id(),
                if job.is_some() {
                    ", tied to this launcher: it ends when this does"
                } else {
                    ""
                }
            ),
        );

        let status = match app.wait() {
            Ok(status) => status,
            Err(e) => {
                let e = format!("waiting for {}: {e}", target.to_string_lossy());
                note(&mut log, &format!("stopping the app: {e}"));
                let _ = app.kill();
                return Err(e);
            }
        };
        // The app has exited by itself: this launcher's own exit, next,
        // takes nothing with it.
        drop(job);

        note(&mut log, &format!("the app exited: {status}"));
        Ok(match status.code() {
            Some(0) => ExitCode::SUCCESS,
            Some(code) => ExitCode::from(u8::try_from(code).unwrap_or(1)),
            None => ExitCode::FAILURE,
        })
    }
}

/// The job object the launcher and the app it starts are in.
#[cfg(windows)]
mod job {
    use std::io;
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};

    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT,
        JOB_OBJECT_LIMIT_BREAKAWAY_OK, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        SetInformationJobObject,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    /// A job that ends every process in it when its last handle closes,
    /// with this process in it: once this process is gone (Edge WebDriver
    /// terminates it to end the session), so is everything it started.
    /// Dropping it (after the app has exited by itself) lets this process
    /// exit without ending anything.
    #[derive(Debug)]
    pub struct Job(OwnedHandle);

    impl Job {
        pub fn ending_with_this_process() -> io::Result<Self> {
            // SAFETY: no security attributes (so the handle isn't
            // inherited by the app) and no name are both allowed; a null
            // handle is checked for before the handle is used.
            let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
            if handle.is_null() {
                return Err(io::Error::last_os_error());
            }
            // SAFETY: `handle` is a job handle just created, owned by
            // nothing else, and closed only by the `OwnedHandle`.
            let job = Self(unsafe { OwnedHandle::from_raw_handle(handle) });
            // Breakaway stays allowed, for any helper the WebView2 runtime
            // starts that asks for it; the app is started without asking,
            // so it stays in the job.
            job.limit(JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_BREAKAWAY_OK)?;
            // SAFETY: the job handle is open (`job` owns it), and
            // GetCurrentProcess's pseudo-handle is always valid and needs
            // no closing.
            if unsafe { AssignProcessToJobObject(job.0.as_raw_handle(), GetCurrentProcess()) } == 0
            {
                return Err(io::Error::last_os_error());
            }
            Ok(job)
        }

        fn limit(&self, flags: JOB_OBJECT_LIMIT) -> io::Result<()> {
            let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            info.BasicLimitInformation.LimitFlags = flags;
            let size = size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32;
            // SAFETY: the job handle is open (`self` owns it); `info` is
            // the structure JobObjectExtendedLimitInformation names, `size`
            // is its size, and it outlives the call.
            let ok = unsafe {
                SetInformationJobObject(
                    self.0.as_raw_handle(),
                    JobObjectExtendedLimitInformation,
                    (&raw const info).cast(),
                    size,
                )
            };
            if ok == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        }
    }

    impl Drop for Job {
        fn drop(&mut self) {
            // No limits, before the handle closes: closing it would
            // otherwise end this process, and anything left in the job.
            if let Err(e) = self.limit(0) {
                eprintln!("gui-launch: couldn't lift the job's limits: {e}");
            }
        }
    }
}
