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
        }
    }

    /// Its number in results.txt.
    pub fn number(self) -> u8 {
        self as u8
    }

    /// Every check, by number: the app's as shipped, started again, and
    /// the updater's.
    pub fn all() -> Vec<Check> {
        let mut all: Vec<Check> = Check::SHIPPED
            .iter()
            .chain(&Check::RELAUNCH)
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

    /// Fails every one of `checks` that has no result yet, with `why`.
    pub async fn fail_the_rest(&mut self, checks: &[Check], why: &str) {
        for &check in checks {
            self.results
                .entry(check)
                .or_insert_with(|| Err(why.to_owned()));
        }
        self.write().await;
    }

    /// Whether every check ran and passed.
    pub fn all_passed(&self) -> bool {
        Check::all()
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
            report.results.insert(check, Ok("fine".to_owned()));
        }
        assert!(report.all_passed());
        let last = *Check::all().last().expect("checks");
        report.results.remove(&last);
        assert!(!report.all_passed());
    }
}
