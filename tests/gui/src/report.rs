//! results.txt: one line per check, PASS or FAIL and why, rewritten as
//! each result comes in so a run that is cut short still says how far it
//! got.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::Result;

/// The checks, numbered as results.txt lists them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Check {
    Driver = 1,
    Csp = 2,
    ExportNotes = 3,
    Screenshots = 4,
}

impl Check {
    pub const ALL: [Check; 4] = [
        Check::Driver,
        Check::Csp,
        Check::ExportNotes,
        Check::Screenshots,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Check::Driver => "driver",
            Check::Csp => "csp",
            Check::ExportNotes => "export-notes",
            Check::Screenshots => "screenshots",
        }
    }

    /// Its number in results.txt.
    pub fn number(self) -> u8 {
        self as u8
    }
}

/// The run's results so far.
#[derive(Debug)]
pub struct Report {
    path: PathBuf,
    results: BTreeMap<Check, std::result::Result<String, String>>,
}

impl Report {
    pub fn new(out_dir: &Path) -> Self {
        Self {
            path: out_dir.join("results.txt"),
            results: BTreeMap::new(),
        }
    }

    /// Records `check`'s outcome (the first one only) and rewrites
    /// results.txt.
    pub async fn record(&mut self, check: Check, outcome: Result<String>) {
        let outcome = outcome.map_err(|e| format!("{e:#}"));
        self.results.entry(check).or_insert(outcome);
        self.write().await;
    }

    /// Fails every check that has no result yet, with `why`.
    pub async fn fail_the_rest(&mut self, why: &str) {
        for check in Check::ALL {
            self.results
                .entry(check)
                .or_insert_with(|| Err(why.to_owned()));
        }
        self.write().await;
    }

    /// Whether every check ran and passed.
    pub fn all_passed(&self) -> bool {
        Check::ALL
            .iter()
            .all(|check| matches!(self.results.get(check), Some(Ok(_))))
    }

    fn lines(&self) -> String {
        let mut text = String::new();
        for (check, outcome) in &self.results {
            let (verdict, detail) = match outcome {
                Ok(detail) => ("PASS", detail),
                Err(reason) => ("FAIL", reason),
            };
            // One line per check, whatever the detail holds.
            let detail = detail.replace('\n', " ");
            text.push_str(&format!(
                "{verdict} {} {}: {detail}\n",
                check.number(),
                check.name()
            ));
        }
        text
    }

    async fn write(&self) {
        if let Err(e) = tokio::fs::write(&self.path, self.lines()).await {
            eprintln!("gui-check: could not write {}: {e}", self.path.display());
        }
    }
}
