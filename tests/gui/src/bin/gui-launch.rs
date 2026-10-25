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
//! The log starts with what the app was given that matters to WebDriver:
//! its arguments and the WebView2 and Tauri automation variables.

use std::fs::OpenOptions;
use std::io::Write;
use std::process::{Command, ExitCode, Stdio};

/// The variables Edge WebDriver and tauri-driver set for the app.
const SHOWN: [&str; 4] = [
    "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS",
    "WEBVIEW2_USER_DATA_FOLDER",
    "WEBVIEW2_BROWSER_EXECUTABLE_FOLDER",
    "TAURI_WEBVIEW_AUTOMATION",
];

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("gui-launch: {e}");
            ExitCode::from(101)
        }
    }
}

fn run() -> Result<ExitCode, String> {
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
    let status = command
        .stdin(Stdio::null())
        .stdout(log)
        .stderr(stderr)
        .status()
        .map_err(|e| format!("starting {}: {e}", target.to_string_lossy()))?;

    let mut log = OpenOptions::new()
        .append(true)
        .open(&log_path)
        .map_err(|e| format!("reopening the log: {e}"))?;
    let _ = writeln!(log, "gui-launch: the app exited: {status}");
    Ok(match status.code() {
        Some(0) => ExitCode::SUCCESS,
        Some(code) => ExitCode::from(u8::try_from(code).unwrap_or(1)),
        None => ExitCode::FAILURE,
    })
}
