//! results.txt: one line per check, PASS, SKIP or FAIL and why, rewritten
//! as each result comes in so a run that is cut short still says how far
//! it got. SKIP is a check that can't run on this platform (where the
//! checks' table below says, as `gui-check --list` prints it): not a
//! failure, so it doesn't fail the run, and never a pass, so it isn't
//! counted as one.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use anyhow::Result;

/// The checks' table: everything about each check but how it's run, one
/// row per check, by number. A row is `(number, Variant, "results.txt
/// name", Group, Skip)`: its number and name in results.txt, the session
/// it runs in ([`Group`]) and where it's a SKIP ([`Skip`]). The [`Check`]
/// enum, [`Check::name`], the groups' arrays (their lengths counted from
/// it, not by hand) and `gui-check --list` are all made from it.
///
/// Adding a check: a row here, in number order (the build fails on a gap
/// or a row out of order), and its run in `checks.rs`, in its group's
/// session (that module's tests fail on a check with no dispatch arm, or
/// two). A check still skips itself, at run time (`skipped`); the table
/// says where it may, and [`Report::record`] fails a SKIP it doesn't list,
/// or a pass where it says the check never runs.
macro_rules! checks {
    ($(($number:literal, $variant:ident, $name:literal, $group:ident, $skip:ident),)+) => {
        /// The checks, numbered as results.txt lists them.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
        pub enum Check {
            $($variant = $number,)+
        }

        impl Check {
            /// Every check, by number: the table's rows, in order.
            const ALL: &[Check] = &[$(Check::$variant,)+];

            /// Its name in results.txt.
            pub fn name(self) -> &'static str {
                match self {
                    $(Check::$variant => $name,)+
                }
            }

            /// The session it runs in.
            pub const fn group(self) -> Group {
                match self {
                    $(Check::$variant => Group::$group,)+
                }
            }

            /// Where it's a SKIP.
            pub const fn skip(self) -> Skip {
                match self {
                    $(Check::$variant => Skip::$skip,)+
                }
            }

            /// Its variant's name, as the dispatch code names it (the
            /// dispatch tests in `checks.rs` look for it there).
            #[cfg(test)]
            pub fn variant(self) -> &'static str {
                match self {
                    $(Check::$variant => stringify!($variant),)+
                }
            }
        }
    };
}

checks! {
    (1,  Driver,              "driver",                 Shipped,  Never),
    (2,  Csp,                 "csp",                    Shipped,  Never),
    (3,  ExportNotes,         "export-notes",           Shipped,  Never),
    (4,  Screenshots,         "screenshots",            Shipped,  Never),
    (5,  Welcome,             "welcome",                Shipped,  Never),
    (6,  NoteEditor,          "note-editor",            Shipped,  Never),
    (7,  Shortcuts,           "shortcuts",              Shipped,  Never),
    (8,  ImportPreview,       "import-preview",         Shipped,  Never),
    (9,  UpdateBanner,        "update-banner",          Updater,  Never),
    (10, UpdateAbout,         "update-about",           Updater,  Never),
    (11, UpdateDownload,      "update-download",        Updater,  Never),
    (12, UpdateLinkOnly,      "update-link-only",       Updater,  Never),
    (13, UpdateOffline,       "update-offline",         Updater,  Never),
    (14, LibraryBookmarks,    "library-bookmarks",      Shipped,  Never),
    (15, LibraryHighlights,   "library-highlights",     Shipped,  Never),
    (16, LibraryNotes,        "library-notes",          Shipped,  Never),
    (17, Search,              "search",                 Shipped,  Never),
    (18, SearchGoTo,          "search-go-to",           Shipped,  Never),
    (19, PaletteCommand,      "palette-command",        Shipped,  Never),
    (20, PickerPopover,       "picker-popover",         Shipped,  Never),
    (21, PickerSidebar,       "picker-sidebar",         Shipped,  OnNarrowWindows),
    (22, RandomChapter,       "random-chapter",         Shipped,  Never),
    (23, PickerKeyboard,      "picker-keyboard",        Shipped,  Never),
    (24, SidebarPlace,        "sidebar-reading-place",  Shipped,  OnNarrowWindows),
    (25, AaControls,          "aa-controls",            Shipped,  Never),
    (26, AaRestart,           "aa-restart",             Relaunch, OnWindows),
    (27, TranslationSwitch,   "translation-switch",     Shipped,  Never),
    (28, Compare,             "compare",                Shipped,  Never),
    (29, ReadingMode,         "reading-mode",           Shipped,  Never),
    (30, OnePanel,            "one-panel",              Shipped,  Never),
    (31, EscapeChain,         "escape-chain",           Shipped,  Never),
    (32, OutsideClick,        "outside-click",          Shipped,  Never),
    (33, MenuKeyboard,        "menu-keyboard",          Shipped,  Never),
    (34, AboutTrap,           "about-trap",             Shipped,  Never),
    (35, GSidebar,            "g-and-sidebar",          Shipped,  OnNarrowWindows),
    (36, ResizePlace,         "resize-reading-place",   Shipped,  OnNarrowWindows),
    (37, ScrollThenResize,    "scroll-then-resize",     Shipped,  OnNarrowWindows),
    (38, ChapterSteps,        "chapter-steps",          Shipped,  Never),
    (39, CompareGrid,         "compare-grid",           Shipped,  Never),
    (40, Disclosures,         "disclosures",            Shipped,  Never),
    (41, BackForward,         "back-forward",           Shipped,  Never),
    (42, LongChapter,         "long-chapter",           Shipped,  Never),
    (43, TryAgain,            "try-again",              Shipped,  Never),
    (44, PlainText,           "plain-text-fallback",    Shipped,  Never),
    (45, DisclosuresKept,     "disclosures-kept",       Relaunch, OnWindows),
    (46, SelectVerses,        "select-verses",          Shipped,  Never),
    (47, CopyFormats,         "copy-and-copy-as",       Shipped,  Never),
    (48, CardBookmark,        "card-bookmark",          Shipped,  Never),
    (49, CardHighlight,       "card-highlight",         Shipped,  Never),
    (50, CardNote,            "card-note",              Shipped,  Never),
    (51, XrefPreview,         "xref-preview",           Shipped,  Never),
    (52, ShiftGhost,          "shift-ghost",            Shipped,  Never),
    (53, KeyTurns,            "key-turns",              Shipped,  Never),
    (54, KeySelection,        "key-selection",          Shipped,  Never),
    (55, KeyCopy,             "key-copy",               Shipped,  Never),
    (56, KeyHistoryNote,      "key-history-and-note",   Shipped,  Never),
    (57, PalettePageCommands, "palette-page-commands",  Shipped,  Never),
    (58, ColdStart,           "cold-start",             Relaunch, Never),
    (59, ReopenAtVerse,       "reopen-at-verse",        Relaunch, Never),
    (60, MarginNotes,         "margin-notes",           Shipped,  Never),
    (61, MarginSwitch,        "margin-switch",          Shipped,  Never),
    (62, MarginNarrow,        "margin-narrow",          Shipped,  Never),
    (63, MarginPsalm119,      "margin-psalm-119",       Shipped,  Never),
    (64, NotesSearch,         "notes-search",           Shipped,  Never),
    (65, NoteDraft,           "note-draft",             Relaunch, OnWindows),
    (66, NoteMarkdown,        "note-markdown",          Shipped,  Never),
    (67, BarHistory,          "bar-back-forward-recent",Shipped,  Never),
    (68, BarChapters,         "bar-chapter-arrows",     Shipped,  Never),
    (69, NotesBeside,         "notes-beside",           Shipped,  Never),
    (70, AiFindings,          "ai-findings",            Shipped,  Never),
    (71, ExportChoice,        "export-choice",          Shipped,  Never),
    (72, SingleKeys,          "single-key-switch",      Shipped,  Never),
    (73, SecondLaunch,        "second-launch",          Shipped,  Never),
    (74, StartupFailure,      "startup-failure",        Startup,  OnWindows),
}

// The table is numbered 1, 2, 3… in order, with no gap: results.txt and
// `--list` go by number, and `Check::all` by the table's order.
const _: () = {
    let mut i = 0;
    while i < Check::ALL.len() {
        assert!(
            Check::ALL[i] as usize == i + 1,
            "the checks' table must be numbered 1, 2, 3… in order, with no gap"
        );
        i += 1;
    }
};

/// The session a check runs in, in this order: [`Group::Startup`] with no
/// session first, then [`Group::Shipped`], [`Group::Relaunch`] and
/// [`Group::Updater`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    /// The app as shipped started on a data folder it can't open, before
    /// the first session: no WebDriver session, since it never has a
    /// window.
    Startup,
    /// The checks on the app as shipped (the first session).
    Shipped,
    /// The checks on the app as shipped started again, on the first
    /// session's data (the second session, and for check 65 a third,
    /// after the second quits with a draft typed).
    Relaunch,
    /// The updater's checks, on the GUI checks' test build against the
    /// run's own update server (the last session).
    Updater,
}

impl Group {
    /// How many checks the table puts in it.
    const fn len(self) -> usize {
        let mut count = 0;
        let mut i = 0;
        while i < Check::ALL.len() {
            if Check::ALL[i].group() as u8 == self as u8 {
                count += 1;
            }
            i += 1;
        }
        count
    }

    /// Its checks, by number; `N` is [`Group::len`].
    const fn checks<const N: usize>(self) -> [Check; N] {
        let mut checks = [Check::ALL[0]; N];
        let mut found = 0;
        let mut i = 0;
        while i < Check::ALL.len() {
            if Check::ALL[i].group() as u8 == self as u8 {
                checks[found] = Check::ALL[i];
                found += 1;
            }
            i += 1;
        }
        assert!(found == N, "a group's length is its count in the table");
        checks
    }
}

impl fmt::Display for Group {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Group::Startup => "startup",
            Group::Shipped => "shipped",
            Group::Relaunch => "relaunch",
            Group::Updater => "updater",
        })
    }
}

/// A platform the checks run on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Linux,
    Windows,
}

impl Platform {
    /// The one this is running on (`scripts/gui-check.sh` runs on no
    /// other).
    pub fn current() -> Self {
        if cfg!(windows) {
            Platform::Windows
        } else {
            Platform::Linux
        }
    }
}

impl fmt::Display for Platform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Platform::Linux => "Linux",
            Platform::Windows => "Windows",
        })
    }
}

/// Where a check is a SKIP. The check decides, as it runs (`skipped`);
/// this is what the table says it may decide, for `--list` and
/// [`Report::record`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Skip {
    /// Run everywhere: never a SKIP.
    Never,
    /// Never run on Windows: a SKIP there, every time.
    OnWindows,
    /// A SKIP on Windows when the window can't be made wide enough for the
    /// sidebar (1200 px; the CI runner's screen can be), run otherwise.
    OnNarrowWindows,
}

impl Skip {
    /// Whether the check may be a SKIP on `platform`.
    pub fn allowed_on(self, platform: Platform) -> bool {
        match self {
            Skip::Never => false,
            Skip::OnWindows | Skip::OnNarrowWindows => platform == Platform::Windows,
        }
    }

    /// Whether the check is a SKIP on `platform` every time: never run
    /// there.
    pub fn always_on(self, platform: Platform) -> bool {
        self == Skip::OnWindows && platform == Platform::Windows
    }
}

impl fmt::Display for Skip {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Skip::Never => "-",
            Skip::OnWindows => "windows",
            Skip::OnNarrowWindows => "windows-when-narrow",
        })
    }
}

impl Check {
    /// The checks on the app as shipped (the first session).
    pub const SHIPPED: [Check; Group::Shipped.len()] = Group::Shipped.checks();

    /// The checks on the app as shipped started again, on the first
    /// session's data (the second session, and for check 65 a third).
    pub const RELAUNCH: [Check; Group::Relaunch.len()] = Group::Relaunch.checks();

    /// The updater's checks, on the GUI checks' test build (the last
    /// session).
    pub const UPDATER: [Check; Group::Updater.len()] = Group::Updater.checks();

    /// Its number in results.txt.
    pub fn number(self) -> u8 {
        self as u8
    }

    /// Every check, by number: the app's as shipped, started again, and
    /// failing to start, and the updater's.
    pub fn all() -> Vec<Check> {
        Check::ALL.to_vec()
    }
}

/// Every check, one to a line, by number: its number and name as
/// results.txt names it (`3 export-notes`), then its group and where it's
/// a SKIP (`26 aa-restart relaunch windows`; `-` for nowhere). What
/// `gui-check --list` prints, and `scripts/gui-check.sh` reads (the first
/// two columns), so its results.txt names every check even when the run is
/// cut short.
pub fn list() -> String {
    Check::all()
        .into_iter()
        .map(|check| {
            format!(
                "{} {} {} {}\n",
                check.number(),
                check.name(),
                check.group(),
                check.skip()
            )
        })
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

/// `check`'s outcome on `platform`, held to the checks' table: a SKIP on a
/// platform its row doesn't list, or a pass where its row says it's never
/// run, is a FAIL saying so (the check or its row is wrong). Any other
/// outcome is as it was. A unit test can't see a check skip (it decides
/// as it runs, on the real app), so every verdict is checked here, where
/// each one passes.
fn as_listed(check: Check, platform: Platform, outcome: Result<String>) -> Result<String> {
    match outcome {
        Err(e) if is_skip(&e) && !check.skip().allowed_on(platform) => Err(anyhow::anyhow!(
            "a SKIP on {platform}, which the checks' table (tests/gui src/report.rs) doesn't \
             list for it: {e:#}"
        )),
        Ok(detail) if check.skip().always_on(platform) => Err(anyhow::anyhow!(
            "it ran on {platform}, where the checks' table (tests/gui src/report.rs) says it's \
             a SKIP: {detail}"
        )),
        outcome => outcome,
    }
}

/// A test of `scripts/gui-check.sh`'s retry, by hand: a check that passed
/// recorded as failed, when GUI_CHECK_FORCE_FAIL names it ([`forces`]) in a
/// build with the `force-fail` feature. Without the feature (every build CI
/// or anyone else makes unless they ask for it) the variable is never read.
/// The checks never ship in an installer, with or without it.
fn forced(check: Check) -> Option<String> {
    if !cfg!(feature = "force-fail") {
        return None;
    }
    let spec = std::env::var("GUI_CHECK_FORCE_FAIL").ok()?;
    let attempt = std::env::var("GUI_CHECK_ATTEMPT")
        .ok()
        .and_then(|a| a.parse().ok())
        .unwrap_or(1);
    forces(&spec, check.number(), attempt).then(|| {
        format!(
            "forced to fail by GUI_CHECK_FORCE_FAIL={spec} in attempt {attempt} (a test of the \
             retry, in a gui-check built with the force-fail feature)"
        )
    })
}

/// Whether GUI_CHECK_FORCE_FAIL's `spec` fails check `number` in `attempt`:
/// a comma-separated list of checks, each `N` (in every attempt) or `N@A`
/// (in attempt A only).
fn forces(spec: &str, number: u8, attempt: u32) -> bool {
    spec.split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .any(|item| {
            let (check, only) = match item.split_once('@') {
                Some((check, only)) => (check, Some(only)),
                None => (item, None),
            };
            check.parse::<u8>().ok() == Some(number)
                && only.is_none_or(|only| only.parse::<u32>().ok() == Some(attempt))
        })
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
    /// results.txt: a pass, a skip ([`skipped`]) or a failure, held to
    /// the table first ([`as_listed`]).
    pub async fn record(&mut self, check: Check, outcome: Result<String>) {
        let outcome = as_listed(check, Platform::current(), outcome);
        let outcome = match (outcome, forced(check)) {
            (Ok(_), Some(why)) => Err(anyhow::anyhow!(why)),
            (outcome, _) => outcome,
        };
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

    /// One check to a line, its first two columns as results.txt names it
    /// (all `scripts/gui-check.sh` reads), then its group and where it's a
    /// SKIP.
    #[test]
    fn the_list_is_one_check_to_a_line_as_results_txt_names_it() {
        let list = list();
        let lines: Vec<&str> = list.lines().collect();
        assert_eq!(lines.len(), Check::all().len());
        assert_eq!(lines[0], "1 driver shipped -");
        for line in [
            "9 update-banner updater -",
            "21 picker-sidebar shipped windows-when-narrow",
            "26 aa-restart relaunch windows",
            "74 startup-failure startup windows",
        ] {
            assert!(lines.contains(&line), "{line:?} in {list}");
        }
        for (line, check) in lines.iter().zip(Check::all()) {
            let columns: Vec<&str> = line.split(' ').collect();
            assert_eq!(columns.len(), 4, "{line:?}");
            assert_eq!(
                columns[..2].join(" "),
                format!("{} {}", check.number(), check.name())
            );
        }
        assert!(list.ends_with('\n'));
    }

    /// The groups are the table's, each by number, their lengths counted
    /// from it: 1 check failing to start (no array: nothing but `all` ever
    /// needed one), 63 on the app as shipped, 5 started again, 5 on the
    /// updater.
    #[test]
    fn the_groups_are_the_tables() {
        for (group, checks) in [
            (Group::Startup, &[Check::StartupFailure][..]),
            (Group::Shipped, &Check::SHIPPED[..]),
            (Group::Relaunch, &Check::RELAUNCH[..]),
            (Group::Updater, &Check::UPDATER[..]),
        ] {
            let listed: Vec<Check> = Check::all()
                .into_iter()
                .filter(|check| check.group() == group)
                .collect();
            assert_eq!(checks, listed, "{group}");
        }
        assert_eq!(
            [
                Check::SHIPPED.len(),
                Check::RELAUNCH.len(),
                Check::UPDATER.len()
            ],
            [63, 5, 5]
        );
        assert_eq!(
            Check::UPDATER,
            [
                Check::UpdateBanner,
                Check::UpdateAbout,
                Check::UpdateDownload,
                Check::UpdateLinkOnly,
                Check::UpdateOffline
            ]
        );
    }

    /// Where the checks skip themselves today: 26, 45, 65 and 74 never run
    /// on Windows (`checks::display::NOT_RUN_ON_WINDOWS` and
    /// `checks::startup`), and the sidebar's checks (21, 24, 35, 36 and 37,
    /// through `Run::not_wide`) don't when the window can't be made wide
    /// enough there. Nothing skips on Linux.
    #[test]
    fn the_skips_are_where_the_checks_skip() {
        let on = |skip: Skip| -> Vec<u8> {
            Check::all()
                .into_iter()
                .filter(|check| check.skip() == skip)
                .map(Check::number)
                .collect()
        };
        assert_eq!(on(Skip::OnWindows), [26, 45, 65, 74]);
        assert_eq!(on(Skip::OnNarrowWindows), [21, 24, 35, 36, 37]);
        for check in Check::all() {
            assert!(!check.skip().allowed_on(Platform::Linux), "{check:?}");
        }
    }

    /// The table holds every verdict to it: a SKIP where the row doesn't
    /// allow one, or a pass where the row says the check never runs, is a
    /// FAIL saying so; anything else is as it was.
    #[test]
    fn a_skip_or_a_pass_the_table_does_not_expect_fails() {
        let verdict = |check: Check, platform: Platform, outcome: Result<String>| {
            let mut report = Report::new(std::path::Path::new("unused"));
            report.note(check, as_listed(check, platform, outcome));
            report.lines()
        };
        let windows = Platform::Windows;
        let linux = Platform::Linux;

        // As the table says: unchanged.
        let why = "not run on Windows: why";
        assert_eq!(
            verdict(Check::AaRestart, windows, Err(skipped(why))),
            format!("SKIP 26 aa-restart: {why}\n")
        );
        assert_eq!(
            verdict(Check::AaRestart, linux, Ok("kept".to_owned())),
            "PASS 26 aa-restart: kept\n"
        );
        assert_eq!(
            verdict(Check::PickerSidebar, windows, Err(skipped("narrow"))),
            "SKIP 21 picker-sidebar: narrow\n"
        );
        assert_eq!(
            verdict(Check::PickerSidebar, windows, Ok("wide".to_owned())),
            "PASS 21 picker-sidebar: wide\n"
        );
        assert_eq!(
            verdict(Check::Csp, windows, Err(anyhow::anyhow!("blocked"))),
            "FAIL 2 csp: blocked\n"
        );
        assert_eq!(
            verdict(Check::StartupFailure, windows, Err(anyhow::anyhow!("no"))),
            "FAIL 74 startup-failure: no\n"
        );

        // A SKIP the table doesn't list: a FAIL, with the skip's reason.
        assert_eq!(
            verdict(Check::Csp, windows, Err(skipped("not here"))),
            "FAIL 2 csp: a SKIP on Windows, which the checks' table (tests/gui src/report.rs) \
             doesn't list for it: not here\n"
        );
        for check in [
            Check::AaRestart,
            Check::PickerSidebar,
            Check::StartupFailure,
        ] {
            let lines = verdict(check, linux, Err(skipped("no")));
            assert!(
                lines.starts_with(&format!("FAIL {} ", check.number()))
                    && lines.contains("a SKIP on Linux"),
                "{lines}"
            );
        }
        // A pass where the table says it never runs: a FAIL too.
        assert_eq!(
            verdict(Check::NoteDraft, windows, Ok("offered".to_owned())),
            "FAIL 65 note-draft: it ran on Windows, where the checks' table (tests/gui \
             src/report.rs) says it's a SKIP: offered\n"
        );
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

        // After a retry: a FLAKY is a result, neither filled in as a FAIL
        // nor failing the run, and the totals count it on its own.
        std::fs::write(
            &results,
            "PASS 1 driver: booted\nFLAKY 2 csp: passed on retry: fine; attempt 1: blocked\n",
        )
        .expect("results.txt");
        let (said, file) = collect(&[Check::Driver, Check::Csp], 0);
        assert_eq!(
            said,
            "1 passed, 0 skipped, 0 failed, 1 flaky (passed on retry)\n0\n"
        );
        assert!(!file.contains("FAIL "), "{file}");

        std::fs::remove_dir_all(&dir).expect("the scratch folder goes");
    }

    /// scripts/gui-check.sh's retry (retry_suite in gui-check-results.sh),
    /// no display: a stub attempt writes results.txt and a screenshot and
    /// exits as told, and a stub checks binary stands in for `gui-check
    /// --retried` (tested in `retried`), noting how it was called. Attempt
    /// 1 passing ends it there; failing, it's kept in attempt-1/, the app
    /// gets fresh data, attempt 2 runs into the out-dir, and the judgement's
    /// warnings reach stdout and its summary the job's. The run is green only
    /// when attempt 2 and the judgement both pass.
    #[cfg(unix)]
    #[test]
    fn gui_check_sh_runs_the_suite_once_more_when_it_fails() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        let dir = std::env::temp_dir().join(format!(
            "gui-check-retry-test-{}-{}",
            std::process::id(),
            now.as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("a scratch folder");
        let script = |name: &str, text: &str| {
            let path = dir.join(name);
            std::fs::write(&path, text).expect("a stub");
            let mut perms = std::fs::metadata(&path).expect("the stub").permissions();
            std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o755);
            std::fs::set_permissions(&path, perms).expect("an executable stub");
            path
        };
        // One attempt: $1 is its out-dir; ATTEMPT_<n> its exit code.
        let attempt = script(
            "attempt",
            r#"#!/usr/bin/env bash
            out=$1; mkdir -p "$out/screenshots"
            echo "attempt $GUI_CHECK_ATTEMPT" >"$out/screenshots/shot.png"
            code_var="ATTEMPT_$GUI_CHECK_ATTEMPT"; code=${!code_var}
            if ((code == 0)); then echo "PASS 7 shortcuts: fine" >"$out/results.txt"
            else echo "FAIL 7 shortcuts: attempt $GUI_CHECK_ATTEMPT" >"$out/results.txt"; fi
            exit "$code""#,
        );
        // gui-check --retried: notes its arguments, rewrites the results,
        // writes a summary, warns, and judges as JUDGED says.
        let runner = script(
            "gui-check",
            r#"#!/usr/bin/env bash
            echo "$*" >"$(dirname "$0")/called"
            [[ $1 == --retried ]] || exit 2
            echo "FLAKY 7 shortcuts: passed on retry" >"$3"
            echo "Flaky (passed on retry)" >"$4"
            echo "::warning title=Flaky GUI check 7 shortcuts::attempt 1"
            exit "$JUDGED""#,
        );
        let library =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/gui-check-results.sh");
        let run = |name: &str, codes: [u8; 3]| {
            let out = dir.join(name);
            std::fs::create_dir_all(&out).expect("an out-dir");
            let summary = dir.join(format!("{name}-summary.md"));
            let _ = std::fs::remove_file(dir.join("called"));
            let done = std::process::Command::new("bash")
                .arg("-c")
                .arg(
                    r#"set -euo pipefail
                    source "$1"; out="$2"; runner="$3"; attempt="$4"
                    fresh_app_data() { echo fresh >>"$out/../fresh-$(basename "$out")"; }
                    status=0; retry_suite "$attempt" || status=$?
                    echo "exit $status""#,
                )
                .arg("bash")
                .arg(&library)
                .arg(&out)
                .arg(&runner)
                .arg(&attempt)
                .env("ATTEMPT_1", codes[0].to_string())
                .env("ATTEMPT_2", codes[1].to_string())
                .env("JUDGED", codes[2].to_string())
                .env("GITHUB_STEP_SUMMARY", &summary)
                .output()
                .expect("bash runs");
            assert!(done.status.success(), "{done:?}");
            let read = |path: &Path| std::fs::read_to_string(path).unwrap_or_default();
            (
                String::from_utf8_lossy(&done.stdout).into_owned(),
                read(&out.join("results.txt")),
                read(&out.join("attempt-1/results.txt")),
                read(&summary),
                read(&dir.join("called")),
                read(&dir.join(format!("fresh-{name}"))),
                read(&out.join("screenshots/shot.png")),
            )
        };

        // Attempt 1 passes: nothing more.
        let (said, results, first, summary, called, fresh, _) = run("clean", [0, 9, 9]);
        assert_eq!(said, "exit 0\n");
        assert_eq!(results, "PASS 7 shortcuts: fine\n");
        assert!(first.is_empty() && summary.is_empty() && called.is_empty() && fresh.is_empty());

        // Attempt 1 fails, attempt 2 passes: kept apart, judged, said.
        let (said, results, first, summary, called, fresh, shot) = run("flaky", [1, 0, 0]);
        assert_eq!(
            said,
            "::warning title=Flaky GUI check 7 shortcuts::attempt 1\nexit 0\n"
        );
        assert_eq!(first, "FAIL 7 shortcuts: attempt 1\n");
        assert_eq!(shot, "attempt 2\n");
        let out = dir.join("flaky");
        assert_eq!(
            std::fs::read_to_string(out.join("attempt-1/screenshots/shot.png")).ok(),
            Some("attempt 1\n".to_owned())
        );
        assert_eq!(results, "FLAKY 7 shortcuts: passed on retry\n");
        assert_eq!(summary, "Flaky (passed on retry)\n");
        assert_eq!(fresh, "fresh\n", "the app's data, fresh once");
        assert_eq!(
            called,
            format!(
                "--retried {0}/attempt-1/results.txt {0}/results.txt {0}/retry-summary.md\n",
                out.display()
            )
        );

        // A FAIL in attempt 2 fails the run, whatever the judgement says.
        let (said, ..) = run("twice", [1, 1, 1]);
        assert!(said.ends_with("exit 1\n"), "{said}");
        let (said, ..) = run("twice-judged-clean", [1, 1, 0]);
        assert!(said.ends_with("exit 1\n"), "{said}");
        // Attempt 2 passing isn't enough if the judgement couldn't pass it.
        let (said, ..) = run("unjudged", [1, 0, 2]);
        assert!(said.ends_with("exit 2\n"), "{said}");

        std::fs::remove_dir_all(&dir).expect("the scratch folder goes");
    }

    /// GUI_CHECK_FORCE_FAIL names checks, each in every attempt or in one.
    #[test]
    fn a_forced_failure_names_its_check_and_attempt() {
        assert!(forces("7", 7, 1) && forces("7", 7, 2));
        assert!(forces("7@1", 7, 1) && !forces("7@1", 7, 2));
        assert!(forces("45, 7@2", 7, 2) && forces("45, 7@2", 45, 1));
        assert!(!forces("7", 8, 1) && !forces("", 7, 1) && !forces("x@1", 7, 1));
        assert!(!forces("7@x", 7, 1));
        // Without the feature, nothing is forced, whatever the variable says.
        if !cfg!(feature = "force-fail") {
            assert_eq!(forced(Check::Shortcuts), None);
        }
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
