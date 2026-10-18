//! Driving the page through WebDriver, one named step at a time. Every
//! step has a timeout (the step timeout, or what is left of the run's
//! budget, whichever is less), waits poll a condition rather than sleep for
//! a fixed time, and nothing is retried: a WebDriver error fails the step.

use std::future::Future;
use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use serde_json::Value;
use thirtyfour::prelude::*;
use tokio::time::timeout;

/// How often a wait looks at its condition again.
const POLL: Duration = Duration::from_millis(50);

/// Settled: no finite CSS animation or transition is still running (a
/// dialog fading in, a toast fading out). Infinite ones never finish, so
/// they don't count.
const SETTLED_JS: &str = "
    const running = document.getAnimations().filter((a) => a.playState === 'running'
        && a.effect && a.effect.getComputedTiming().iterations !== Infinity);
    return [running.length === 0, running.map((a) => a.animationName || a.transitionProperty || 'animation')];
";

/// A PNG file's first eight bytes.
const PNG_MAGIC: &[u8] = b"\x89PNG\r\n\x1a\n";

/// The WebDriver session, the clock and the limits.
pub struct Ui {
    driver: WebDriver,
    started: Instant,
    deadline: Instant,
    step_timeout: Duration,
}

impl Ui {
    pub fn new(
        driver: WebDriver,
        started: Instant,
        budget: Duration,
        step_timeout: Duration,
    ) -> Self {
        Self {
            driver,
            started,
            deadline: started + budget,
            step_timeout,
        }
    }

    pub fn driver(&self) -> &WebDriver {
        &self.driver
    }

    pub fn into_driver(self) -> WebDriver {
        self.driver
    }

    /// One line in the run's step log (stderr, which the script keeps).
    pub fn log(&self, message: &str) {
        log_line(self.started, message);
    }

    /// How long the step `what` may take, and whether the budget (rather
    /// than the step timeout) is what limits it.
    fn limit(&self, what: &str) -> Result<(Duration, bool)> {
        let left = self.deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            bail!("the run's time budget ran out before \"{what}\"");
        }
        Ok((self.step_timeout.min(left), left < self.step_timeout))
    }

    fn timed_out(
        what: &str,
        limit: Duration,
        by_budget: bool,
        seen: Option<&Value>,
    ) -> anyhow::Error {
        let why = if by_budget {
            format!("the run's time budget ran out during \"{what}\"")
        } else {
            format!("\"{what}\" timed out after {:.0}s", limit.as_secs_f64())
        };
        match seen {
            Some(seen) if !seen.is_null() => anyhow!("{why}; last seen: {seen}"),
            _ => anyhow!(why),
        }
    }

    /// Runs `action` as the step `what`, within its time limit.
    pub async fn step<T>(&self, what: &str, action: impl Future<Output = Result<T>>) -> Result<T> {
        self.step_within(what, None, action).await
    }

    /// As [`Ui::step`], with a longer limit than the usual step timeout
    /// (still within the budget): for starting the app.
    pub async fn step_within<T>(
        &self,
        what: &str,
        longer: Option<Duration>,
        action: impl Future<Output = Result<T>>,
    ) -> Result<T> {
        let (mut limit, mut by_budget) = self.limit(what)?;
        if let Some(longer) = longer {
            let left = self.deadline.saturating_duration_since(Instant::now());
            limit = longer.min(left);
            by_budget = left < longer;
        }
        self.log(what);
        let begun = Instant::now();
        match timeout(limit, action).await {
            Ok(Ok(value)) => {
                self.log(&format!("  ok ({:.2}s)", begun.elapsed().as_secs_f64()));
                Ok(value)
            }
            Ok(Err(e)) => {
                let e = e.context(format!("\"{what}\" failed"));
                self.log(&format!("  FAILED: {e:#}"));
                Err(e)
            }
            Err(_) => {
                let e = Self::timed_out(what, limit, by_budget, None);
                self.log(&format!("  FAILED: {e:#}"));
                Err(e)
            }
        }
    }

    /// Runs `script` in the page and returns what it returns.
    pub async fn js(&self, script: &str, args: Vec<Value>) -> Result<Value> {
        let ret = self.driver.execute(script, args).await?;
        Ok(ret.json().clone())
    }

    /// Waits until `script` (which returns `[ready, seen]`) says it is
    /// ready, and returns what it saw then. On a timeout, the error has what
    /// it saw last.
    pub async fn wait(&self, what: &str, script: &str) -> Result<Value> {
        self.wait_with(what, script, Vec::new()).await
    }

    /// [`Ui::wait`], passing `args` to the script.
    pub async fn wait_with(&self, what: &str, script: &str, args: Vec<Value>) -> Result<Value> {
        self.wait_within(what, script, args, None).await
    }

    /// [`Ui::wait_with`], with a longer limit than the usual step timeout
    /// (still within the budget), for what the app does once, slowly.
    pub async fn wait_within(
        &self,
        what: &str,
        script: &str,
        args: Vec<Value>,
        longer: Option<Duration>,
    ) -> Result<Value> {
        let (mut limit, mut by_budget) = self.limit(what)?;
        if let Some(longer) = longer {
            let left = self.deadline.saturating_duration_since(Instant::now());
            limit = longer.min(left);
            by_budget = left < longer;
        }
        self.log(&format!("wait: {what}"));
        let begun = Instant::now();
        let mut last = Value::Null;
        loop {
            let left = limit.saturating_sub(begun.elapsed());
            if left.is_zero() {
                let e = Self::timed_out(what, limit, by_budget, Some(&last));
                self.log(&format!("  FAILED: {e:#}"));
                return Err(e);
            }
            match timeout(left, self.js(script, args.clone())).await {
                Err(_) => {
                    let e = Self::timed_out(what, limit, by_budget, Some(&last));
                    self.log(&format!("  FAILED: {e:#}"));
                    return Err(e);
                }
                Ok(Err(e)) => {
                    let e = e.context(format!("WebDriver failed while waiting for \"{what}\""));
                    self.log(&format!("  FAILED: {e:#}"));
                    return Err(e);
                }
                Ok(Ok(value)) => {
                    let ready = value.get(0).and_then(Value::as_bool).unwrap_or(false);
                    let seen = value.get(1).cloned().unwrap_or(Value::Null);
                    if ready {
                        self.log(&format!(
                            "  ok ({:.2}s): {seen}",
                            begun.elapsed().as_secs_f64()
                        ));
                        return Ok(seen);
                    }
                    last = seen;
                }
            }
            tokio::time::sleep(POLL).await;
        }
    }

    /// Clicks the element `css` finds, as a pointer would (WebDriver's
    /// element click: scrolled into view, refused if something covers it).
    pub async fn click(&self, what: &str, css: &str) -> Result<()> {
        self.step(what, async {
            let element = self
                .driver
                .find(By::Css(css))
                .await
                .with_context(|| format!("no element {css}"))?;
            element
                .click()
                .await
                .with_context(|| format!("clicking {css}"))?;
            Ok(())
        })
        .await
    }

    /// Presses `keys` on the keyboard, to whatever has focus.
    pub async fn press(&self, what: &str, keys: impl Into<TypingData>) -> Result<()> {
        let keys = keys.into();
        self.step(what, async {
            self.driver.action_chain().send_keys(keys).perform().await?;
            Ok(())
        })
        .await
    }

    /// Types `text` into the element `css` finds.
    pub async fn type_into(&self, what: &str, css: &str, text: &str) -> Result<()> {
        self.step(what, async {
            let element = self
                .driver
                .find(By::Css(css))
                .await
                .with_context(|| format!("no element {css}"))?;
            element.send_keys(text).await?;
            Ok(())
        })
        .await
    }

    /// Saves the webview's own screenshot (taken by WebKit or WebView2) as
    /// `path`, once the page has settled, after checking it is a PNG.
    pub async fn screenshot(&self, what: &str, path: &Path) -> Result<u64> {
        self.wait("animations to finish, for the screenshot", SETTLED_JS)
            .await?;
        self.step(what, async {
            let png = self.driver.screenshot_as_png().await?;
            if !png.starts_with(PNG_MAGIC) {
                bail!("the screenshot is not a PNG ({} bytes)", png.len());
            }
            tokio::fs::write(path, &png)
                .await
                .with_context(|| format!("writing {}", path.display()))?;
            Ok(png.len() as u64)
        })
        .await
    }
}

/// One line in the run's step log, timed from `started`.
pub fn log_line(started: Instant, message: &str) {
    eprintln!(
        "[gui-check {:>6.2}s] {message}",
        started.elapsed().as_secs_f64()
    );
}
