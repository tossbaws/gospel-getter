//! results.txt: one line per check, PASS, SKIP or FAIL and why, rewritten
//! as each result comes in so a run that is cut short still says how far
//! it got. SKIP is a check that can't run on this platform (on Windows,
//! checks 26, 45 and 65, and the sidebar's checks when the window can't be
//! made wide enough): not a failure, so it doesn't fail the run, and never
//! a pass, so it isn't counted as one.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use anyhow::Result;

/// The checks, numbered as results.txt lists them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Check {
    Driver = 1,
    Csp = 2,
    ExportNotes = 3,
    Screenshots = 4,
    Welcome = 5,
    NoteEditor = 6,
    Shortcuts = 7,
    ImportPreview = 8,
    UpdateBanner = 9,
    UpdateAbout = 10,
    UpdateDownload = 11,
    UpdateLinkOnly = 12,
    UpdateOffline = 13,
    LibraryBookmarks = 14,
    LibraryHighlights = 15,
    LibraryNotes = 16,
    Search = 17,
    SearchGoTo = 18,
    PaletteCommand = 19,
    PickerPopover = 20,
    PickerSidebar = 21,
    RandomChapter = 22,
    PickerKeyboard = 23,
    SidebarPlace = 24,
    AaControls = 25,
    AaRestart = 26,
    TranslationSwitch = 27,
    Compare = 28,
    ReadingMode = 29,
    OnePanel = 30,
    EscapeChain = 31,
    OutsideClick = 32,
    MenuKeyboard = 33,
    AboutTrap = 34,
    GSidebar = 35,
    ResizePlace = 36,
    ScrollThenResize = 37,
    ChapterSteps = 38,
    CompareGrid = 39,
    Disclosures = 40,
    BackForward = 41,
    LongChapter = 42,
    TryAgain = 43,
    PlainText = 44,
    DisclosuresKept = 45,
    SelectVerses = 46,
    CopyFormats = 47,
    CardBookmark = 48,
    CardHighlight = 49,
    CardNote = 50,
    XrefPreview = 51,
    ShiftGhost = 52,
    KeyTurns = 53,
    KeySelection = 54,
    KeyCopy = 55,
    KeyHistoryNote = 56,
    PalettePageCommands = 57,
    ColdStart = 58,
    ReopenAtVerse = 59,
    MarginNotes = 60,
    MarginSwitch = 61,
    MarginNarrow = 62,
    MarginPsalm119 = 63,
    NotesSearch = 64,
    NoteDraft = 65,
    NoteMarkdown = 66,
    BarHistory = 67,
    BarChapters = 68,
    NotesBeside = 69,
    AiFindings = 70,
    ExportChoice = 71,
    SingleKeys = 72,
    SecondLaunch = 73,
    StartupFailure = 74,
}

impl Check {
    /// The checks on the app as shipped (the first session).
    pub const SHIPPED: [Check; 63] = [
        Check::Driver,
        Check::Csp,
        Check::ExportNotes,
        Check::ExportChoice,
        Check::Screenshots,
        Check::Welcome,
        Check::NoteEditor,
        Check::Shortcuts,
        Check::ImportPreview,
        Check::LibraryBookmarks,
        Check::LibraryHighlights,
        Check::LibraryNotes,
        Check::Search,
        Check::SearchGoTo,
        Check::PaletteCommand,
        Check::PickerPopover,
        Check::PickerSidebar,
        Check::RandomChapter,
        Check::PickerKeyboard,
        Check::SidebarPlace,
        Check::AaControls,
        Check::TranslationSwitch,
        Check::Compare,
        Check::ReadingMode,
        Check::OnePanel,
        Check::EscapeChain,
        Check::OutsideClick,
        Check::MenuKeyboard,
        Check::AboutTrap,
        Check::GSidebar,
        Check::ResizePlace,
        Check::ScrollThenResize,
        Check::ChapterSteps,
        Check::CompareGrid,
        Check::Disclosures,
        Check::BackForward,
        Check::BarHistory,
        Check::BarChapters,
        Check::LongChapter,
        Check::TryAgain,
        Check::SelectVerses,
        Check::CopyFormats,
        Check::CardBookmark,
        Check::CardHighlight,
        Check::CardNote,
        Check::XrefPreview,
        Check::ShiftGhost,
        Check::KeyTurns,
        Check::KeySelection,
        Check::KeyCopy,
        Check::KeyHistoryNote,
        Check::PalettePageCommands,
        Check::SingleKeys,
        Check::MarginNotes,
        Check::MarginSwitch,
        Check::MarginNarrow,
        Check::MarginPsalm119,
        Check::NotesBeside,
        Check::NotesSearch,
        Check::NoteMarkdown,
        Check::AiFindings,
        Check::SecondLaunch,
        Check::PlainText,
    ];

    /// The checks on the app as shipped started again, on the first
    /// session's data (the second session, and for check 65 a third,
    /// after the second quits with a draft typed).
    pub const RELAUNCH: [Check; 5] = [
        Check::ColdStart,
        Check::ReopenAtVerse,
        Check::AaRestart,
        Check::DisclosuresKept,
        Check::NoteDraft,
    ];

    /// The app as shipped started on a data folder it can't open, before
    /// the first session: no WebDriver session, since it never has a window.
    pub const STARTUP: [Check; 1] = [Check::StartupFailure];

    /// The updater's checks, on the GUI checks' test build against the
    /// run's own update server (the third session).
    pub const UPDATER: [Check; 5] = [
        Check::UpdateBanner,
        Check::UpdateAbout,
        Check::UpdateDownload,
        Check::UpdateLinkOnly,
        Check::UpdateOffline,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Check::Driver => "driver",
            Check::Csp => "csp",
            Check::ExportNotes => "export-notes",
            Check::Screenshots => "screenshots",
            Check::Welcome => "welcome",
            Check::NoteEditor => "note-editor",
            Check::Shortcuts => "shortcuts",
            Check::ImportPreview => "import-preview",
            Check::UpdateBanner => "update-banner",
            Check::UpdateAbout => "update-about",
            Check::UpdateDownload => "update-download",
            Check::UpdateLinkOnly => "update-link-only",
            Check::UpdateOffline => "update-offline",
            Check::LibraryBookmarks => "library-bookmarks",
            Check::LibraryHighlights => "library-highlights",
            Check::LibraryNotes => "library-notes",
            Check::Search => "search",
            Check::SearchGoTo => "search-go-to",
            Check::PaletteCommand => "palette-command",
            Check::PickerPopover => "picker-popover",
            Check::PickerSidebar => "picker-sidebar",
            Check::RandomChapter => "random-chapter",
            Check::PickerKeyboard => "picker-keyboard",
            Check::SidebarPlace => "sidebar-reading-place",
            Check::AaControls => "aa-controls",
            Check::AaRestart => "aa-restart",
            Check::TranslationSwitch => "translation-switch",
            Check::Compare => "compare",
            Check::ReadingMode => "reading-mode",
            Check::OnePanel => "one-panel",
            Check::EscapeChain => "escape-chain",
            Check::OutsideClick => "outside-click",
            Check::MenuKeyboard => "menu-keyboard",
            Check::AboutTrap => "about-trap",
            Check::GSidebar => "g-and-sidebar",
            Check::ResizePlace => "resize-reading-place",
            Check::ScrollThenResize => "scroll-then-resize",
            Check::ChapterSteps => "chapter-steps",
            Check::CompareGrid => "compare-grid",
            Check::Disclosures => "disclosures",
            Check::BackForward => "back-forward",
            Check::LongChapter => "long-chapter",
            Check::TryAgain => "try-again",
            Check::PlainText => "plain-text-fallback",
            Check::DisclosuresKept => "disclosures-kept",
            Check::SelectVerses => "select-verses",
            Check::CopyFormats => "copy-and-copy-as",
            Check::CardBookmark => "card-bookmark",
            Check::CardHighlight => "card-highlight",
            Check::CardNote => "card-note",
            Check::XrefPreview => "xref-preview",
            Check::ShiftGhost => "shift-ghost",
            Check::KeyTurns => "key-turns",
            Check::KeySelection => "key-selection",
            Check::KeyCopy => "key-copy",
            Check::KeyHistoryNote => "key-history-and-note",
            Check::PalettePageCommands => "palette-page-commands",
            Check::ColdStart => "cold-start",
            Check::ReopenAtVerse => "reopen-at-verse",
            Check::MarginNotes => "margin-notes",
            Check::MarginSwitch => "margin-switch",
            Check::MarginNarrow => "margin-narrow",
            Check::MarginPsalm119 => "margin-psalm-119",
            Check::NotesSearch => "notes-search",
            Check::NoteDraft => "note-draft",
            Check::NoteMarkdown => "note-markdown",
            Check::BarHistory => "bar-back-forward-recent",
            Check::BarChapters => "bar-chapter-arrows",
            Check::NotesBeside => "notes-beside",
            Check::AiFindings => "ai-findings",
            Check::ExportChoice => "export-choice",
            Check::SingleKeys => "single-key-switch",
            Check::SecondLaunch => "second-launch",
            Check::StartupFailure => "startup-failure",
        }
    }

    /// Its number in results.txt.
    pub fn number(self) -> u8 {
        self as u8
    }

    /// Every check, by number: the app's as shipped, started again, and
    /// failing to start, and the updater's.
    pub fn all() -> Vec<Check> {
        let mut all: Vec<Check> = Check::SHIPPED
            .iter()
            .chain(&Check::RELAUNCH)
            .chain(&Check::STARTUP)
            .chain(&Check::UPDATER)
            .copied()
            .collect();
        all.sort_unstable();
        all
    }
}

/// Every check, one to a line as results.txt names it (`3 export-notes`),
/// by number: what `gui-check --list` prints, and `scripts/gui-check.sh`
/// reads, so its results.txt names every check even when the run is cut
/// short.
pub fn list() -> String {
    Check::all()
        .into_iter()
        .map(|check| format!("{} {}\n", check.number(), check.name()))
        .collect()
}

/// Why a check wasn't run here: what a check that can't run on this
/// platform returns, as its error (so its signature and `?` stay as they
/// are), for [`Report::record`] to write as SKIP, never PASS or FAIL. Made
/// by [`skipped`].
#[derive(Debug)]
pub struct Skipped(String);

impl fmt::Display for Skipped {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Skipped {}

/// A check's result that says it wasn't run here, and why (`not run on
/// Windows: …`): SKIP in results.txt.
pub fn skipped(why: impl Into<String>) -> anyhow::Error {
    anyhow::Error::new(Skipped(why.into()))
}

/// Whether a check's error is a skip ([`skipped`]), not a failure: no
/// failure screenshot for it.
pub fn is_skip(error: &anyhow::Error) -> bool {
    error.downcast_ref::<Skipped>().is_some()
}

/// A check's verdict, as results.txt writes it.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Verdict {
    Pass(String),
    Skip(String),
    Fail(String),
}

/// How many checks passed, were skipped and failed, and how many have no
/// result yet.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Totals {
    pub passed: usize,
    pub skipped: usize,
    pub failed: usize,
    pub missing: usize,
}

impl fmt::Display for Totals {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} passed, {} skipped, {} failed",
            self.passed, self.skipped, self.failed
        )?;
        if self.missing > 0 {
            write!(f, ", {} with no result", self.missing)?;
        }
        Ok(())
    }
}

/// The run's results so far.
#[derive(Debug)]
pub struct Report {
    path: PathBuf,
    results: BTreeMap<Check, Verdict>,
}

impl Report {
    pub fn new(out_dir: &Path) -> Self {
        Self {
            path: out_dir.join("results.txt"),
            results: BTreeMap::new(),
        }
    }

    /// Records `check`'s outcome (the first one only) and rewrites
    /// results.txt: a pass, a skip ([`skipped`]) or a failure.
    pub async fn record(&mut self, check: Check, outcome: Result<String>) {
        self.note(check, outcome);
        self.write().await;
    }

    /// `check`'s outcome, the first one only, as its verdict.
    fn note(&mut self, check: Check, outcome: Result<String>) {
        let verdict = match outcome {
            Ok(detail) => Verdict::Pass(detail),
            Err(e) if is_skip(&e) => Verdict::Skip(format!("{e:#}")),
            Err(e) => Verdict::Fail(format!("{e:#}")),
        };
        self.results.entry(check).or_insert(verdict);
    }

    /// Fails every one of `checks` that has no result yet, with `why`.
    pub async fn fail_the_rest(&mut self, checks: &[Check], why: &str) {
        for &check in checks {
            self.results
                .entry(check)
                .or_insert_with(|| Verdict::Fail(why.to_owned()));
        }
        self.write().await;
    }

    /// How many of the checks passed, were skipped and failed, and how many
    /// have no result.
    pub fn totals(&self) -> Totals {
        let mut totals = Totals::default();
        for check in Check::all() {
            match self.results.get(&check) {
                Some(Verdict::Pass(_)) => totals.passed += 1,
                Some(Verdict::Skip(_)) => totals.skipped += 1,
                Some(Verdict::Fail(_)) => totals.failed += 1,
                None => totals.missing += 1,
            }
        }
        totals
    }

    /// Whether the run passed: every check has a result, and none failed.
    /// A SKIP isn't a failure, so it doesn't fail the run, but it's never
    /// counted as passed ([`Report::totals`]).
    pub fn all_passed(&self) -> bool {
        let totals = self.totals();
        totals.failed == 0 && totals.missing == 0
    }

    fn lines(&self) -> String {
        let mut text = String::new();
        for (check, outcome) in &self.results {
            let (verdict, detail) = match outcome {
                Verdict::Pass(detail) => ("PASS", detail),
                Verdict::Skip(reason) => ("SKIP", reason),
                Verdict::Fail(reason) => ("FAIL", reason),
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Each check is in one group, once, and they're numbered 1 to the
    /// last with no gap, each with a name of its own: results.txt and
    /// `gui-check --list` name every one.
    #[test]
    fn every_check_is_listed_once_by_number() {
        let all = Check::all();
        let numbers: Vec<u8> = all.iter().map(|check| check.number()).collect();
        let last = u8::try_from(all.len()).expect("fewer than 256 checks");
        assert_eq!(numbers, (1..=last).collect::<Vec<_>>(), "{all:?}");
        let mut names: Vec<&str> = all.iter().map(|check| check.name()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), all.len(), "two checks share a name");
        for name in names {
            assert!(
                !name.is_empty() && !name.contains(char::is_whitespace),
                "{name:?}"
            );
        }
    }

    #[test]
    fn the_list_is_one_check_to_a_line_as_results_txt_names_it() {
        let list = list();
        let lines: Vec<&str> = list.lines().collect();
        assert_eq!(lines.len(), Check::all().len());
        assert_eq!(lines[0], "1 driver");
        assert!(lines.contains(&"9 update-banner"), "{list}");
        assert!(list.ends_with('\n'));
    }

    /// A run passes only when every listed check passed.
    #[test]
    fn all_passed_needs_every_listed_check() {
        let mut report = Report::new(std::path::Path::new("unused"));
        for check in Check::all() {
            report
                .results
                .insert(check, Verdict::Pass("fine".to_owned()));
        }
        assert!(report.all_passed());
        let last = *Check::all().last().expect("checks");
        report.results.remove(&last);
        assert!(!report.all_passed());
    }

    /// One line for each verdict: PASS with what it saw, SKIP and FAIL
    /// with why, by number, each on one line.
    #[test]
    fn each_verdict_has_its_own_line() {
        let mut report = Report::new(std::path::Path::new("unused"));
        report.note(Check::Driver, Ok("booted".to_owned()));
        report.note(
            Check::AaRestart,
            Err(skipped("not run on Windows: a fresh\nuser data folder")),
        );
        report.note(Check::Csp, Err(anyhow::anyhow!("blocked")));
        assert_eq!(
            report.lines(),
            "PASS 1 driver: booted\n\
             FAIL 2 csp: blocked\n\
             SKIP 26 aa-restart: not run on Windows: a fresh user data folder\n"
        );
    }

    /// A skip found under context is still a skip, with all of its reason.
    #[test]
    fn a_skip_under_context_is_still_a_skip() {
        let mut report = Report::new(std::path::Path::new("unused"));
        let error = skipped("not run on Windows: too narrow").context("check 21");
        assert!(is_skip(&error));
        assert!(!is_skip(&anyhow::anyhow!(
            "not run on Windows: said, not skipped"
        )));
        report.note(Check::PickerSidebar, Err(error));
        assert_eq!(
            report.lines(),
            "SKIP 21 picker-sidebar: check 21: not run on Windows: too narrow\n"
        );
    }

    /// A skip doesn't fail the run, and isn't counted as passed: the
    /// totals count skips on their own.
    #[test]
    fn a_skip_is_neither_a_pass_nor_a_failure() {
        let mut report = Report::new(std::path::Path::new("unused"));
        let all = Check::all();
        for &check in &all {
            report.note(check, Ok("fine".to_owned()));
        }
        let mut skipping = Report::new(std::path::Path::new("unused"));
        for &check in &all {
            if check == Check::NoteDraft {
                skipping.note(check, Err(skipped("not run on Windows")));
            } else {
                skipping.note(check, Ok("fine".to_owned()));
            }
        }
        assert!(skipping.all_passed(), "a skip isn't a failure");
        let (every, some) = (report.totals(), skipping.totals());
        assert_eq!(some.passed, every.passed - 1, "a skip isn't a pass");
        assert_eq!(
            (some.skipped, some.failed, some.missing),
            (1, 0, 0),
            "{some}"
        );
        assert_eq!((every.skipped, every.failed, every.missing), (0, 0, 0));
        assert!(
            some.to_string().ends_with(" passed, 1 skipped, 0 failed"),
            "{some}"
        );

        // A failure still fails it, skips or not, and so does a check
        // with no result.
        skipping
            .results
            .insert(Check::Csp, Verdict::Fail("blocked".to_owned()));
        assert!(!skipping.all_passed());
        assert_eq!(
            (skipping.totals().skipped, skipping.totals().failed),
            (1, 1)
        );
        let mut short = Report::new(std::path::Path::new("unused"));
        short.note(Check::Driver, Err(skipped("not run")));
        assert!(!short.all_passed());
        let totals = short.totals();
        assert_eq!((totals.skipped, totals.missing), (1, all.len() - 1));
        assert!(totals.to_string().contains(" with no result"), "{totals}");
    }

    /// scripts/gui-check.sh reads results.txt as [`Report`] writes it
    /// (its scripts/gui-check-results.sh, sourced here, no display): a
    /// SKIP is a result line, so a skipped check isn't filled in as a FAIL
    /// and doesn't fail the run, and the totals count it apart. A check
    /// with no line gets a FAIL, which does fail it.
    #[cfg(unix)]
    #[test]
    fn gui_check_sh_takes_a_skip_as_a_result_but_not_a_failure() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        let dir = std::env::temp_dir().join(format!(
            "gui-check-results-test-{}-{}",
            std::process::id(),
            now.as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("a scratch folder");
        let results = dir.join("results.txt");
        let mut report = Report::new(&dir);
        report.note(Check::Driver, Ok("booted".to_owned()));
        report.note(Check::AaRestart, Err(skipped("not run on Windows: why")));
        std::fs::write(&results, report.lines()).expect("results.txt");
        let library =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/gui-check-results.sh");
        let collect = |checks: &[Check], checks_status: u8| {
            let out = std::process::Command::new("bash")
                .arg("-c")
                .arg(
                    r#"set -euo pipefail
                    source "$1"; results="$2"; status="$3"; shift 3; checks=("$@")
                    complete_results "no result"
                    result_totals
                    run_status "$status""#,
                )
                .arg("bash")
                .arg(&library)
                .arg(&results)
                .arg(checks_status.to_string())
                .args(
                    checks
                        .iter()
                        .map(|c| format!("{} {}", c.number(), c.name())),
                )
                .output()
                .expect("bash runs");
            assert!(out.status.success(), "{out:?}");
            (
                String::from_utf8_lossy(&out.stdout).into_owned(),
                std::fs::read_to_string(&results).expect("results.txt"),
            )
        };

        // Every check has a line: nothing filled in, and the run passes.
        let (said, file) = collect(&[Check::Driver, Check::AaRestart], 0);
        assert_eq!(said, "1 passed, 1 skipped, 0 failed\n0\n");
        assert_eq!(file, report.lines(), "the skip was filled in as a FAIL");
        // The checks' own failure stays theirs.
        let (said, _) = collect(&[Check::Driver, Check::AaRestart], 5);
        assert!(said.ends_with("\n5\n"), "{said}");
        // A check with no line: a FAIL for it, which fails the run.
        let (said, file) = collect(&[Check::Driver, Check::Csp, Check::AaRestart], 0);
        assert_eq!(said, "1 passed, 1 skipped, 1 failed\n1\n");
        assert!(file.ends_with("FAIL 2 csp: no result\n"), "{file}");
        assert_eq!(file.matches("FAIL ").count(), 1, "{file}");

        std::fs::remove_dir_all(&dir).expect("the scratch folder goes");
    }

    /// Only the first outcome counts: a later failure doesn't overwrite a
    /// skip (as when the relaunch's session fails after 26, 45 and 65 were
    /// skipped on Windows), nor a skip a failure.
    #[test]
    fn the_first_outcome_stays() {
        let mut report = Report::new(std::path::Path::new("unused"));
        report.note(Check::AaRestart, Err(skipped("not run on Windows")));
        report.note(Check::AaRestart, Err(anyhow::anyhow!("no session")));
        report
            .results
            .entry(Check::DisclosuresKept)
            .or_insert_with(|| Verdict::Fail("no session".to_owned()));
        report.note(Check::DisclosuresKept, Err(skipped("not run on Windows")));
        assert_eq!(
            report.lines(),
            "SKIP 26 aa-restart: not run on Windows\n\
             FAIL 45 disclosures-kept: no session\n"
        );
    }
}
