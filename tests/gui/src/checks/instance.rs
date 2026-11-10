//! Check 73: one app at a time (`window::single_instance_plugin`). While the
//! first session's app runs, the app is started again the way the driver
//! starts it: the same launcher, in the checks' own environment (on Linux,
//! the run's own display and session bus, which is where the app looks for
//! another copy of itself, so nothing outside the run is seen or reached).
//!
//! - The second launch exits, successfully, within a step: it handed over
//!   to the app already open instead of starting a second app on the same
//!   database (one that started would run until stopped).
//! - The app already open logged that it was opened again, once (it shows
//!   its window, unminimised and focused, then).
//! - It's still there and answering, on the same page.

use std::path::Path;
use std::process::Stdio;
use std::time::Instant;

use anyhow::{Context, Result, ensure};
use tokio::process::Command;

use super::Run;

/// What the app already open logs when it's opened again (`window.rs`).
const OPENED_AGAIN: &str = "Gospel Getter was opened again; showing the window already open";

/// The page's title and heading, to see it's the same page after.
const PAGE_JS: &str = "const h = document.querySelector('.chapter-main .chapter-heading');
    return { title: document.title, heading: h ? h.textContent.trim() : null,
        visible: document.visibilityState, focused: document.hasFocus() };";

/// How many times `log` (app.log) says the app was opened again.
fn times_opened_again(log: &Path) -> Result<usize> {
    let text = std::fs::read(log).with_context(|| format!("couldn't read {}", log.display()))?;
    Ok(String::from_utf8_lossy(&text).matches(OPENED_AGAIN).count())
}

impl Run<'_> {
    /// Check 73 (see the module's docs).
    pub(super) async fn second_launch(&self) -> Result<String> {
        let log = self.config.out_dir.join("app.log");
        let before = times_opened_again(&log)?;
        let page = self.ui.js(PAGE_JS, Vec::new()).await?;

        let begun = Instant::now();
        let status = self
            .ui
            .step("start the app again while it's open", async {
                // A second app that doesn't exit is stopped with the step.
                let mut second = Command::new(&self.config.app)
                    .stdin(Stdio::null())
                    .kill_on_drop(true)
                    .spawn()
                    .with_context(|| format!("couldn't start {}", self.config.app))?;
                second
                    .wait()
                    .await
                    .context("couldn't wait for the second launch")
            })
            .await
            .context("the second launch didn't exit: it may have started a second app")?;
        let took = begun.elapsed();
        ensure!(status.success(), "the second launch exited with {status}");

        let after = times_opened_again(&log)?;
        ensure!(
            after == before + 1,
            "the app already open should log {OPENED_AGAIN:?} once for the second launch; \
             app.log has it {before} time(s) before and {after} after"
        );
        let now = self.ui.js(PAGE_JS, Vec::new()).await?;
        ensure!(
            now["title"] == page["title"] && now["heading"] == page["heading"],
            "the app already open should still show the same page: {page} before, {now} after"
        );
        Ok(format!(
            "the second launch exited 0 in {:.2}s; the app already open logged being opened \
             again and still shows {} (visibility {}, focused {})",
            took.as_secs_f64(),
            now["title"],
            now["visible"],
            now["focused"],
        ))
    }
}
