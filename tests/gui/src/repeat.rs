//! GUI_CHECK_REPEAT, for chasing a flaky check by hand: one check run many
//! times in a row in its own session (`52:40` runs check 52 forty times),
//! which is far cheaper than looping the whole suite to catch a 1-in-15
//! failure. Off unless set; CI never sets it (tests/packaging_config.rs
//! fails if a workflow names it). Its line in results.txt counts the runs:
//! a PASS only if every one passed, else a FAIL saying how many failed and
//! why each did; steps.log has a line for each run.

use anyhow::{Result, anyhow, bail};

use crate::report::{Check, is_skip};

/// Which check to run again, and how many times in all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Repeat {
    check: Check,
    times: u32,
}

impl Repeat {
    /// GUI_CHECK_REPEAT's value: `N:TIMES` (check N, TIMES runs in all, at
    /// least 1), or empty for none.
    pub fn parse(spec: &str) -> Result<Option<Self>> {
        let spec = spec.trim();
        if spec.is_empty() {
            return Ok(None);
        }
        let Some((number, times)) = spec.split_once(':') else {
            bail!("GUI_CHECK_REPEAT is <check>:<times> (e.g. 52:40), not {spec:?}");
        };
        let number: u8 = number
            .trim()
            .parse()
            .map_err(|_| anyhow!("GUI_CHECK_REPEAT: no check numbered {number:?}"))?;
        let Some(check) = Check::all().into_iter().find(|c| c.number() == number) else {
            bail!("GUI_CHECK_REPEAT: no check numbered {number} (gui-check --list lists them)");
        };
        let times: u32 = times
            .trim()
            .parse()
            .ok()
            .filter(|&times| times >= 1)
            .ok_or_else(|| {
                anyhow!("GUI_CHECK_REPEAT: {times:?} isn't a number of runs (1 or more)")
            })?;
        Ok(Some(Self { check, times }))
    }

    /// The check that's run again.
    pub fn check(self) -> Check {
        self.check
    }

    /// How many times `check` runs: this many if it's the one, else once.
    pub fn times_for(repeat: Option<Self>, check: Check) -> u32 {
        repeat.filter(|r| r.check == check).map_or(1, |r| r.times)
    }
}

/// The runs of a repeated check so far, folded into one result.
#[derive(Debug, Default)]
pub struct Runs {
    passed: u32,
    first: Option<Result<String>>,
    failures: Vec<String>,
}

impl Runs {
    /// Run `n`'s result, and what steps.log says of it.
    pub fn add(&mut self, n: u32, times: u32, result: Result<String>) -> String {
        let line = match &result {
            Ok(_) => {
                self.passed += 1;
                format!("run {n} of {times}: PASS")
            }
            Err(e) if is_skip(e) => format!("run {n} of {times}: SKIP: {e:#}"),
            Err(e) => {
                self.failures.push(format!("run {n}: {e:#}"));
                format!("run {n} of {times}: FAIL: {e:#}")
            }
        };
        if self.first.is_none() {
            self.first = Some(result);
        }
        line
    }

    /// The check's result over every run: a failure if any run failed,
    /// saying how many and why; else the first run's (a pass, with the
    /// count, or a skip).
    pub fn verdict(self) -> Result<String> {
        let runs = self.passed + u32::try_from(self.failures.len()).unwrap_or(u32::MAX);
        if !self.failures.is_empty() {
            bail!(
                "{} of {runs} runs failed (GUI_CHECK_REPEAT): {}",
                self.failures.len(),
                self.failures.join("; ")
            );
        }
        match self.first {
            Some(Ok(detail)) => Ok(format!(
                "{detail} [GUI_CHECK_REPEAT: {} of {runs} runs passed]",
                self.passed
            )),
            Some(Err(skip)) => Err(skip),
            None => bail!("not run: GUI_CHECK_REPEAT ran it no times"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::skipped;

    #[test]
    fn parses_a_check_and_its_runs() {
        assert_eq!(Repeat::parse("").unwrap(), None);
        assert_eq!(Repeat::parse("  ").unwrap(), None);
        let repeat = Repeat::parse("52:40").unwrap();
        assert_eq!(
            repeat,
            Some(Repeat {
                check: Check::ShiftGhost,
                times: 40
            })
        );
        assert_eq!(Repeat::times_for(repeat, Check::ShiftGhost), 40);
        assert_eq!(Repeat::times_for(repeat, Check::KeyTurns), 1);
        assert_eq!(Repeat::times_for(None, Check::ShiftGhost), 1);
        assert_eq!(
            Repeat::parse(" 7 : 1 ").unwrap(),
            Some(Repeat {
                check: Check::Shortcuts,
                times: 1
            })
        );
        for bad in [
            "52", "52:", ":40", "52:0", "52:-1", "52:x", "0:3", "250:3", "x:3",
        ] {
            assert!(Repeat::parse(bad).is_err(), "{bad:?} parsed");
        }
    }

    #[test]
    fn every_run_counts() {
        let mut runs = Runs::default();
        assert_eq!(runs.add(1, 3, Ok("fine".into())), "run 1 of 3: PASS");
        assert_eq!(runs.add(2, 3, Ok("also fine".into())), "run 2 of 3: PASS");
        assert_eq!(runs.add(3, 3, Ok("fine".into())), "run 3 of 3: PASS");
        assert_eq!(
            runs.verdict().unwrap(),
            "fine [GUI_CHECK_REPEAT: 3 of 3 runs passed]"
        );

        // One failure fails the check, whichever run it was, with why.
        let mut runs = Runs::default();
        runs.add(1, 3, Ok("fine".into()));
        assert_eq!(
            runs.add(2, 3, Err(anyhow!("timed out"))),
            "run 2 of 3: FAIL: timed out"
        );
        runs.add(3, 3, Ok("fine".into()));
        assert_eq!(
            format!("{:#}", runs.verdict().unwrap_err()),
            "1 of 3 runs failed (GUI_CHECK_REPEAT): run 2: timed out"
        );

        let mut runs = Runs::default();
        runs.add(1, 2, Err(anyhow!("a")));
        runs.add(2, 2, Err(anyhow!("b")));
        assert_eq!(
            format!("{:#}", runs.verdict().unwrap_err()),
            "2 of 2 runs failed (GUI_CHECK_REPEAT): run 1: a; run 2: b"
        );
    }

    #[test]
    fn a_skip_stays_a_skip() {
        let mut runs = Runs::default();
        assert_eq!(
            runs.add(1, 2, Err(skipped("not run on Windows"))),
            "run 1 of 2: SKIP: not run on Windows"
        );
        runs.add(2, 2, Err(skipped("not run on Windows")));
        assert!(is_skip(&runs.verdict().unwrap_err()));
        assert!(Runs::default().verdict().is_err());
    }
}
