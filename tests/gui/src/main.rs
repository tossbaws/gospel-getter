//! Gospel Getter's GUI smoke checks: the real app, driven through its own
//! webview by tauri-driver (WebDriver), with no one at the keyboard. Run
//! through `scripts/gui-check.sh`, which starts the private display, the
//! driver and this, and cleans up after; see that script.
//!
//! A handful of smoke checks, not a second test suite (the jsdom tests in
//! tests/frontend are the behaviour suite): the page boots, nothing is
//! blocked by the Content Security Policy, and each dialog the Rust pieces
//! (ui-leptos) own works through the real UI: Export notes end to end with
//! the native save dialog, the welcome on a fresh install, the note editor,
//! the keyboard shortcuts sheet, and the import preview after Export data…
//! and Import data… through the native dialogs; with screenshots of the
//! reading view and each dialog. Then the updater (About's Check now, the
//! launch check's banner, a download's progress, a build that can only link
//! to the download page, and a check that runs out of time), on the GUI
//! checks' test build of the app against the run's own update server
//! (`update_server`), stopping before the app would replace itself. And
//! the Library, search and the palette, the book and chapter picker, and
//! Aa with the translations and the saved settings, which survive the app
//! being closed and started again.

mod checks;
mod config;
mod native;
mod report;
mod ui;
mod update_server;

use std::process::ExitCode;

use config::Config;
use report::Report;

/// The stack the checks run with. Each check is one long async function of
/// steps, and in the debug build `scripts/gui-check.sh` makes their poll
/// frames are large; nested, they outgrew the 1 MB Windows gives a
/// program's main thread (a stack overflow in check 66). So the checks run
/// on a thread of their own, and the runtime's workers, with this much.
const CHECKS_STACK: usize = 32 * 1024 * 1024;

fn main() -> ExitCode {
    let checks = std::thread::Builder::new()
        .name("gui-check".to_owned())
        .stack_size(CHECKS_STACK)
        .spawn(|| {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .thread_stack_size(CHECKS_STACK)
                .build();
            match runtime {
                Ok(runtime) => runtime.block_on(run()),
                Err(e) => {
                    eprintln!("gui-check: no async runtime: {e}");
                    ExitCode::from(2)
                }
            }
        });
    match checks.map(std::thread::JoinHandle::join) {
        Ok(Ok(code)) => code,
        Ok(Err(_)) => {
            eprintln!("gui-check: the checks' thread panicked");
            ExitCode::FAILURE
        }
        Err(e) => {
            eprintln!("gui-check: the checks' thread couldn't start: {e}");
            ExitCode::from(2)
        }
    }
}

async fn run() -> ExitCode {
    let config = match Config::from_env() {
        Ok(config) => config,
        Err(e) => {
            eprintln!("gui-check: {e:#}");
            return ExitCode::from(2);
        }
    };
    let mut report = Report::new(&config.out_dir);
    checks::run(&config, &mut report).await;
    if report.all_passed() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
