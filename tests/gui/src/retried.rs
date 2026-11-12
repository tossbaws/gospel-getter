//! A suite that ran twice (`GUI_CHECK_RETRY`, CI only): when the whole
//! suite fails, `scripts/gui-check.sh` runs it once more, from the start on
//! fresh data, keeping the first attempt in `attempt-1/`, and
//! `gui-check --retried` judges each check from both attempts' results.txt
//! ([`judge`]). Attempt 2's verdict stands, except that a check that failed
//! in attempt 1 and passed in attempt 2 is FLAKY: never a clean pass. It
//! rewrites the final results.txt with those verdicts, prints a GitHub
//! `::warning` for each flaky check, and writes the job summary's section
//! (`retry-summary.md`). The run passes only if attempt 2 has no FAIL.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use anyhow::{Context, Result};

use crate::report::Check;

/// A check's verdict in one attempt's results.txt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Seen {
    Pass,
    Skip,
    Fail,
}

/// A check's verdict over both attempts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Judged {
    Pass,
    /// Failed in attempt 1 (or had no result there), passed in attempt 2.
    Flaky,
    Skip,
    Fail,
}

/// A check's verdict over both attempts, from what each attempt's
/// results.txt says (`None`: it has no line for the check). Attempt 2's
/// stands: a FAIL there is a FAIL, whatever attempt 1 said, and so is no
/// result; a SKIP stays a SKIP. A pass in attempt 2 is FLAKY when attempt 1
/// failed (or had no result), and a clean PASS only when it didn't.
pub fn judge(first: Option<Seen>, second: Option<Seen>) -> Judged {
    match second {
        None | Some(Seen::Fail) => Judged::Fail,
        Some(Seen::Skip) => Judged::Skip,
        Some(Seen::Pass) => match first {
            Some(Seen::Pass | Seen::Skip) => Judged::Pass,
            Some(Seen::Fail) | None => Judged::Flaky,
        },
    }
}

/// One attempt's results.txt: each check's verdict and its detail, by
/// number, and any other line (such as a setup failure's), as it is.
#[derive(Debug, Default)]
struct Attempt {
    checks: BTreeMap<u8, (Seen, String)>,
    other: Vec<String>,
}

impl Attempt {
    /// Reads results.txt's lines: `PASS|SKIP|FAIL <number> <name>: <detail>`.
    fn parse(text: &str) -> Self {
        let mut attempt = Self::default();
        for line in text.lines().filter(|l| !l.trim().is_empty()) {
            match parse_line(line) {
                Some((number, seen, detail)) => {
                    attempt
                        .checks
                        .entry(number)
                        .or_insert_with(|| (seen, detail.to_owned()));
                }
                None => attempt.other.push(line.to_owned()),
            }
        }
        attempt
    }

    fn get(&self, check: Check) -> Option<&(Seen, String)> {
        self.checks.get(&check.number())
    }
}

/// A check's line: its number, verdict and detail.
fn parse_line(line: &str) -> Option<(u8, Seen, &str)> {
    let (verdict, rest) = line.split_once(' ')?;
    let seen = match verdict {
        "PASS" => Seen::Pass,
        "SKIP" => Seen::Skip,
        "FAIL" => Seen::Fail,
        _ => return None,
    };
    let (number, rest) = rest.split_once(' ')?;
    let number: u8 = number.parse().ok()?;
    let (_name, detail) = rest.split_once(": ").unwrap_or((rest, ""));
    Some((number, seen, detail))
}

/// A check that was flaky, and why it failed in attempt 1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Flake {
    pub check: Check,
    pub first: String,
}

/// A check that failed in attempt 2: why, and what attempt 1 said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    pub check: Check,
    pub why: String,
    pub first: &'static str,
}

/// Both attempts, judged.
#[derive(Debug, Default)]
pub struct Retried {
    /// The final results.txt.
    pub results: String,
    pub passed: usize,
    pub flaky: Vec<Flake>,
    pub skipped: usize,
    pub failed: Vec<Failure>,
    /// Attempt 2's lines that aren't a check's (a setup failure's): each a
    /// FAIL too when it says FAIL.
    pub other: Vec<String>,
}

/// What results.txt says for no result.
const NO_RESULT: &str = "no result";

fn word(seen: Option<Seen>) -> &'static str {
    match seen {
        Some(Seen::Pass) => "PASS",
        Some(Seen::Skip) => "SKIP",
        Some(Seen::Fail) => "FAIL",
        None => NO_RESULT,
    }
}

/// Judges every check from attempt 1's results.txt (`first`) and attempt
/// 2's (`second`), in the checks' order, with attempt 2's other lines after.
pub fn retried(first: &str, second: &str) -> Retried {
    let (first, second) = (Attempt::parse(first), Attempt::parse(second));
    let mut out = Retried::default();
    for check in Check::all() {
        let one = first.get(check);
        let two = second.get(check);
        let (number, name) = (check.number(), check.name());
        let first_why = || {
            one.map(|(_, detail)| detail.clone())
                .unwrap_or_else(|| NO_RESULT.to_owned())
        };
        let line = match judge(one.map(|(s, _)| *s), two.map(|(s, _)| *s)) {
            Judged::Pass => {
                out.passed += 1;
                format!("PASS {number} {name}: {}", two.map_or("", |(_, d)| d))
            }
            Judged::Flaky => {
                let why = first_why();
                let line = format!(
                    "FLAKY {number} {name}: passed on retry: {}; attempt 1: {why}",
                    two.map_or("", |(_, d)| d)
                );
                out.flaky.push(Flake { check, first: why });
                line
            }
            Judged::Skip => {
                out.skipped += 1;
                format!("SKIP {number} {name}: {}", two.map_or("", |(_, d)| d))
            }
            Judged::Fail => {
                let why = match two {
                    Some((_, detail)) => detail.clone(),
                    None => format!("{NO_RESULT} in attempt 2"),
                };
                let first = word(one.map(|(s, _)| *s));
                let line = format!("FAIL {number} {name}: {why}; attempt 1: {first}");
                out.failed.push(Failure { check, why, first });
                line
            }
        };
        out.results.push_str(&line);
        out.results.push('\n');
    }
    for line in &second.other {
        out.results.push_str(line);
        out.results.push('\n');
    }
    out.other = second.other;
    out
}

impl Retried {
    /// Whether the run passed: no FAIL in attempt 2 (a FLAKY isn't one).
    pub fn passed(&self) -> bool {
        self.failed.is_empty() && !self.other.iter().any(|l| l.starts_with("FAIL "))
    }

    /// "N passed, N flaky (passed on retry), N skipped, N failed".
    pub fn totals(&self) -> String {
        let failed =
            self.failed.len() + self.other.iter().filter(|l| l.starts_with("FAIL ")).count();
        format!(
            "{} passed, {} flaky (passed on retry), {} skipped, {failed} failed",
            self.passed,
            self.flaky.len(),
            self.skipped
        )
    }

    /// A GitHub `::warning` for each flaky check: its number, its name and
    /// why it failed in attempt 1.
    pub fn annotations(&self) -> Vec<String> {
        self.flaky
            .iter()
            .map(|flake| {
                format!(
                    "::warning title={}::{}",
                    escape_property(&format!(
                        "Flaky GUI check {} {}",
                        flake.check.number(),
                        flake.check.name()
                    )),
                    escape_data(&format!(
                        "Check {} {} failed in attempt 1 and passed when the whole suite ran again \
                         (FLAKY in results.txt). Attempt 1: {}",
                        flake.check.number(),
                        flake.check.name(),
                        shortened(&flake.first, 600)
                    ))
                )
            })
            .collect()
    }

    /// The job summary's section: the totals, every flaky check with why it
    /// failed in attempt 1, and every failure.
    pub fn summary(&self, platform: &str) -> String {
        let mut text = String::new();
        let _ = writeln!(text, "### GUI checks ({platform}): the suite ran twice\n");
        let _ = writeln!(
            text,
            "Attempt 1 failed, so the whole suite ran once more on fresh data \
             (`GUI_CHECK_RETRY`). Each check's verdict is attempt 2's; one that failed in \
             attempt 1 and passed in attempt 2 is FLAKY, not a pass. Attempt 1's results.txt \
             and screenshots are in the artifact's `attempt-1/`.\n"
        );
        let _ = writeln!(text, "**{}**\n", self.totals());
        let _ = writeln!(text, "#### Flaky (passed on retry)\n");
        if self.flaky.is_empty() {
            let _ = writeln!(text, "None.\n");
        } else {
            let _ = writeln!(
                text,
                "| Check | Why it failed in attempt 1 |\n| --- | --- |"
            );
            for flake in &self.flaky {
                let _ = writeln!(
                    text,
                    "| {} {} | {} |",
                    flake.check.number(),
                    flake.check.name(),
                    cell(&flake.first)
                );
            }
            text.push('\n');
        }
        if !self.failed.is_empty() || !self.other.is_empty() {
            let _ = writeln!(
                text,
                "#### Failed\n\n| Check | Attempt 2 | Attempt 1 |\n| --- | --- | --- |"
            );
            for failure in &self.failed {
                let _ = writeln!(
                    text,
                    "| {} {} | {} | {} |",
                    failure.check.number(),
                    failure.check.name(),
                    cell(&failure.why),
                    failure.first
                );
            }
            for line in &self.other {
                let _ = writeln!(text, "| (results.txt) | {} | |", cell(line));
            }
            text.push('\n');
        }
        text
    }
}

/// `text`, at most `max` characters, with "…" where it was cut.
fn shortened(text: &str, max: usize) -> String {
    let text = text.replace(['\r', '\n'], " ");
    if text.chars().count() <= max {
        return text;
    }
    let mut cut: String = text.chars().take(max).collect();
    cut.push('…');
    cut
}

/// A Markdown table cell's text: one line, its pipes escaped, cut short.
fn cell(text: &str) -> String {
    shortened(text, 300).replace('|', "\\|")
}

/// A workflow command's message, escaped as GitHub reads it.
fn escape_data(text: &str) -> String {
    text.replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
}

/// A workflow command's property (`title=`), escaped as GitHub reads it.
fn escape_property(text: &str) -> String {
    escape_data(text).replace(':', "%3A").replace(',', "%2C")
}

/// `gui-check --retried <attempt 1's results.txt> <attempt 2's results.txt>
/// <summary.md>`: judges both, rewrites attempt 2's results.txt with the
/// verdicts, writes the summary, prints the `::warning`s on stdout and the
/// totals on stderr. Whether the run passed.
pub fn run(first: &Path, second: &Path, summary: &Path) -> Result<bool> {
    // Attempt 1 always writes one, but a missing one still judges: every
    // check that passes in attempt 2 is then FLAKY, never a clean pass.
    let one = match std::fs::read_to_string(first) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e).with_context(|| format!("reading {}", first.display())),
    };
    let two =
        std::fs::read_to_string(second).with_context(|| format!("reading {}", second.display()))?;
    let judged = retried(&one, &two);
    std::fs::write(second, &judged.results)
        .with_context(|| format!("writing {}", second.display()))?;
    let platform = if cfg!(windows) { "Windows" } else { "Linux" };
    std::fs::write(summary, judged.summary(platform))
        .with_context(|| format!("writing {}", summary.display()))?;
    for annotation in judged.annotations() {
        println!("{annotation}");
    }
    eprintln!("gui-check: after the retry: {}", judged.totals());
    Ok(judged.passed())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rules: attempt 2's verdict stands, and a failure that passed on
    /// retry is FLAKY, never PASS.
    #[test]
    fn each_pair_of_verdicts_is_judged() {
        use Judged as J;
        use Seen as S;
        assert_eq!(judge(Some(S::Fail), Some(S::Pass)), J::Flaky, "fail, pass");
        assert_eq!(judge(Some(S::Fail), Some(S::Fail)), J::Fail, "fail, fail");
        assert_eq!(judge(Some(S::Pass), Some(S::Fail)), J::Fail, "pass, fail");
        assert_eq!(judge(Some(S::Skip), Some(S::Skip)), J::Skip, "skip, skip");
        assert_eq!(judge(Some(S::Pass), Some(S::Pass)), J::Pass, "pass, pass");
        assert_eq!(judge(Some(S::Skip), Some(S::Pass)), J::Pass, "skip, pass");
        assert_eq!(judge(Some(S::Fail), Some(S::Skip)), J::Skip, "fail, skip");
        // No line in attempt 2 is a FAIL, whatever attempt 1 said.
        for first in [Some(S::Pass), Some(S::Skip), Some(S::Fail), None] {
            assert_eq!(judge(first, None), J::Fail, "{first:?}, missing");
        }
        // No line in attempt 1 (no result) that passes in attempt 2.
        assert_eq!(judge(None, Some(S::Pass)), J::Flaky, "missing, pass");
    }

    /// A results.txt of these lines.
    fn attempt(lines: &[(&str, Check, &str)]) -> String {
        lines
            .iter()
            .map(|(verdict, check, detail)| {
                format!("{verdict} {} {}: {detail}\n", check.number(), check.name())
            })
            .collect()
    }

    /// A results.txt with every check `verdict` but those in `but`.
    fn every(verdict: &str, but: &[(&str, Check, &str)]) -> String {
        let mut lines: Vec<(&str, Check, &str)> = Vec::new();
        for check in Check::all() {
            match but.iter().find(|(_, c, _)| *c == check) {
                Some(&line) => lines.push(line),
                None => lines.push((verdict, check, "fine")),
            }
        }
        attempt(&lines)
    }

    /// Every check in attempt 2 passed or was skipped, and one of them
    /// failed in attempt 1: FLAKY with attempt 1's reason, a warning, a
    /// summary row, and the run passes.
    #[test]
    fn a_failure_that_passed_on_retry_is_flaky_and_said() {
        let first = every(
            "PASS",
            &[
                (
                    "FAIL",
                    Check::DisclosuresKept,
                    "\"Psalms 119 …\" timed out | after 20s",
                ),
                ("SKIP", Check::StartupFailure, "not run on Windows"),
            ],
        );
        let second = every(
            "PASS",
            &[("SKIP", Check::StartupFailure, "not run on Windows")],
        );
        let judged = retried(&first, &second);
        assert!(judged.passed(), "{}", judged.results);
        assert_eq!(
            judged.flaky,
            [Flake {
                check: Check::DisclosuresKept,
                first: "\"Psalms 119 …\" timed out | after 20s".to_owned()
            }]
        );
        assert!(
            judged.results.contains(
                "\nFLAKY 45 disclosures-kept: passed on retry: fine; attempt 1: \"Psalms 119 …\" \
                 timed out | after 20s\n"
            ),
            "{}",
            judged.results
        );
        assert!(
            judged
                .results
                .contains("\nSKIP 74 startup-failure: not run on Windows\n")
        );
        assert_eq!(judged.results.lines().count(), Check::all().len());
        assert!(
            !judged.results.contains("PASS 45 "),
            "a flake never reads as a pass"
        );
        let totals = judged.totals();
        assert_eq!(
            totals,
            format!(
                "{} passed, 1 flaky (passed on retry), 1 skipped, 0 failed",
                Check::all().len() - 2
            )
        );
        let warnings = judged.annotations();
        assert_eq!(warnings.len(), 1);
        assert!(
            warnings[0]
                .starts_with("::warning title=Flaky GUI check 45 disclosures-kept::Check 45"),
            "{}",
            warnings[0]
        );
        assert!(warnings[0].contains("Attempt 1: \"Psalms 119 …\" timed out | after 20s"));
        let summary = judged.summary("Linux");
        assert!(
            summary.contains("#### Flaky (passed on retry)"),
            "{summary}"
        );
        assert!(
            summary.contains("| 45 disclosures-kept | \"Psalms 119 …\" timed out \\| after 20s |"),
            "{summary}"
        );
        assert!(!summary.contains("#### Failed"), "{summary}");
    }

    /// A FAIL in attempt 2 fails the run, whatever attempt 1 said; so does
    /// a check with no line in attempt 2, and a setup failure's line.
    #[test]
    fn a_failure_in_attempt_2_fails_the_run() {
        let first = every("PASS", &[("FAIL", Check::NoteDraft, "first")]);
        let second = every(
            "PASS",
            &[
                ("FAIL", Check::NoteDraft, "second"),
                ("FAIL", Check::Csp, "blocked"),
            ],
        );
        let judged = retried(&first, &second);
        assert!(!judged.passed());
        assert!(judged.flaky.is_empty());
        assert!(
            judged
                .results
                .contains("\nFAIL 2 csp: blocked; attempt 1: PASS\n")
        );
        assert!(
            judged
                .results
                .contains("\nFAIL 65 note-draft: second; attempt 1: FAIL\n")
        );
        assert!(
            judged
                .summary("Windows")
                .contains("| 2 csp | blocked | PASS |")
        );

        // Attempt 2 cut short: the checks it never reached fail.
        let short = attempt(&[("PASS", Check::Driver, "booted")]);
        let judged = retried(&every("FAIL", &[]), &short);
        assert!(!judged.passed());
        assert_eq!(judged.flaky.len(), 1, "check 1 passed on retry");
        assert_eq!(judged.failed.len(), Check::all().len() - 1);
        assert!(
            judged
                .results
                .contains("FAIL 2 csp: no result in attempt 2; attempt 1: FAIL\n")
        );

        // Attempt 2 couldn't even list its checks.
        let setup = "FAIL setup: no check ran, and the checks couldn't be listed: why\n";
        let judged = retried(&every("FAIL", &[]), setup);
        assert!(!judged.passed());
        assert!(judged.results.ends_with(setup), "{}", judged.results);
    }

    /// Nothing failed in attempt 1 that attempt 2 passed: no FLAKY, and
    /// skips stay skips.
    #[test]
    fn a_clean_attempt_2_has_no_flakes() {
        let first = every("PASS", &[("SKIP", Check::AaRestart, "not run on Windows")]);
        let judged = retried(&first, &first);
        assert!(judged.passed());
        assert!(judged.flaky.is_empty() && judged.annotations().is_empty());
        assert!(
            judged
                .results
                .contains("\nSKIP 26 aa-restart: not run on Windows\n")
        );
        assert!(
            judged
                .summary("Linux")
                .contains("#### Flaky (passed on retry)\n\nNone.")
        );
    }

    /// A workflow command's text can't end it early or break its line.
    #[test]
    fn annotations_are_escaped() {
        assert_eq!(escape_data("50% a\r\nb"), "50%25 a%0D%0Ab");
        assert_eq!(escape_property("a: b, c"), "a%3A b%2C c");
        assert_eq!(shortened("abcdef", 3), "abc…");
        assert_eq!(cell("a|b\nc"), "a\\|b c");
    }
}
