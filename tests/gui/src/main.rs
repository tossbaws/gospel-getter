//! Gospel Getter's GUI smoke checks: the real app, driven through its own
//! webview by tauri-driver (WebDriver), with no one at the keyboard. Run
//! through `scripts/gui-check.sh`, which starts the private display, the
//! driver and this, and cleans up after; see that script.
//!
//! A handful of smoke checks, not a second test suite (the jsdom tests in
//! tests/frontend are the behaviour suite): the page boots, nothing is
//! blocked by the Content Security Policy, Export notes works end to end
//! through the real UI and the native save dialog, and screenshots of the
//! reading view and the dialog in Chi Rho and Vellum.

mod checks;
mod config;
mod native;
mod report;
mod ui;

use std::process::ExitCode;

use config::Config;
use report::Report;

#[tokio::main]
async fn main() -> ExitCode {
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
