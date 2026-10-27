//! The command palette's commands and how the box finds them, free of the
//! DOM so they are tested natively (`cargo test`): every command with its
//! label, the other words it's found by, the shortcut it shows (a row of
//! `shortcut_table`), why it can't run now, and what running it does; the
//! few the empty box suggests; and the box's words, matched as prefixes in
//! any order, ignoring case and accents.
//!
//! What a command can and can't do now depends on the page: whether it's
//! comparing, reading mode, Aa's settings, the chapter shown, the passage
//! history, a busy export. That's read as a [`PaletteState`]
//! each time the palette is drawn or a command chosen (see
//! `bridge::palette_state`), so the reasons are always the page's state at
//! that moment, as they were.
//!
//! The commands, labels, words, reasons and order are the page's
//! `paletteCommands` (`ui/index.html` at 201ab8f), byte for byte.

use serde::Deserialize;
use unicode_properties::{GeneralCategoryGroup, UnicodeGeneralCategory};

use crate::shortcut_table::shortcut;

/// One choice of one of Aa's settings (an `<option>` of its record): the
/// value and what it's called.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
pub struct Choice {
    pub value: String,
    pub label: String,
}

/// What the page knows that the commands need, taken when the palette is
/// drawn or a command is chosen (`bridge::palette_state`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PaletteState {
    /// Comparing two translations.
    pub comparing: bool,
    /// The translation compared with (or that would be), if there's one.
    pub partner: Option<String>,
    /// Reading mode is on.
    pub reader_mode: bool,
    /// The book and chapter picker is the sidebar.
    pub sidebar: bool,
    /// The window is wide enough for the sidebar.
    pub wide: bool,
    /// Aa's settings: each one's choices and the one in use.
    pub layouts: Vec<Choice>,
    pub layout: String,
    pub widths: Vec<Choice>,
    pub width: String,
    pub text_sizes: Vec<Choice>,
    pub text_size: String,
    pub themes: Vec<Choice>,
    pub theme: String,
    /// The translations (code and name) and the one being read.
    pub translations: Vec<Choice>,
    pub translation: String,
    /// Export notes… is on the page and enabled (there are notes).
    pub notes: bool,
    /// ☰'s Export data… and Import data… are disabled (one is running).
    pub export_busy: bool,
    pub import_busy: bool,
    /// About's Check now is checking.
    pub checking: bool,
    /// The chapter's text is ready to copy.
    pub chapter_copy: bool,
    /// The chapter's Copy as… button is on the page.
    pub chapter_shown: bool,
    /// There's a chapter after (before) the one shown, and a book.
    pub next_chapter: bool,
    pub previous_chapter: bool,
    pub next_book: bool,
    pub previous_book: bool,
    /// There's somewhere to go back (forward) to in the passage history.
    pub back: bool,
    pub forward: bool,
}

/// What running a command does: what its own control does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Run {
    /// Clicks the control with this id (Aa's toggles, the ☰ menu's items…).
    Click(&'static str),
    /// Clicks the `[data-value]` choice `value` in the group with this id
    /// (Aa's swatches and segments).
    Choice { group: &'static str, value: String },
    /// Opens the Library (unless it's open) on this tab, focused.
    Library(&'static str),
    /// About, then its Check now.
    CheckUpdates,
    /// Aa's text size step (`aa::text_size_shortcut`), as its keys take
    /// it: a step larger (1) or smaller (-1), or back to Medium (0).
    TextSize(i8),
    /// Reads this translation, as its row in the translation button's
    /// popover does.
    Translation(String),
    /// Compares with this translation, as Compare's "with" does.
    CompareWith(String),
    /// The code that was the page's (`page::run_command`): the command's
    /// name there.
    Page(&'static str),
}

/// One command, as the palette lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    pub id: String,
    pub label: String,
    /// The first way to press its shortcut ("Ctrl+="), or empty.
    pub key: String,
    /// All of it, as `aria-keyshortcuts` says it, or empty.
    pub keys: &'static str,
    /// Other words it's found by.
    pub words: String,
    /// Why it can't run now, if it can't: it's listed, disabled, with this.
    pub reason: Option<&'static str>,
    pub run: Run,
}

/// The reason a choice already chosen can't be chosen.
pub const IN_USE: &str = "In use";

/// The commands the empty box suggests, in this order.
pub const SUGGESTED: [&str; 5] = [
    "compare",
    "reader-mode",
    "random-chapter",
    "copy-chapter",
    "export-notes",
];

/// The Library's tabs the palette opens: their ids and names.
const LIBRARY_TABS: [(&str, &str); 3] = [
    ("bookmarks", "Bookmarks"),
    ("highlights", "Highlights"),
    ("notes", "Notes"),
];

fn click(id: &'static str) -> Run {
    Run::Click(id)
}

fn page(command: &'static str) -> Run {
    Run::Page(command)
}

/// The reason when `in_use`, else none.
fn in_use(in_use: bool) -> Option<&'static str> {
    in_use.then_some(IN_USE)
}

/// Every command, for the page as `state` says it is, in the palette's
/// order. A command whose control toggles (Compare, Reading mode, the
/// sidebar) is named for what it would do.
pub fn commands(state: &PaletteState) -> Vec<Command> {
    let mut list = Vec::new();
    let mut add = |id: String,
                   label: String,
                   shortcut_id: &str,
                   words: &str,
                   reason: Option<&'static str>,
                   run: Run| {
        let row = (!shortcut_id.is_empty())
            .then(|| shortcut(shortcut_id))
            .flatten();
        list.push(Command {
            id,
            label,
            key: row.map(|r| r.first_keys()).unwrap_or_default(),
            keys: row.map_or("", |r| r.aria),
            words: words.to_string(),
            reason,
            run,
        });
    };
    let comparing = state.comparing;
    let reader_mode = state.reader_mode;

    add(
        "compare".into(),
        if comparing {
            "Compare off"
        } else {
            "Compare on"
        }
        .into(),
        "compare",
        "translations side by side",
        (!comparing && state.partner.is_none()).then_some("There’s only one translation"),
        click("compare-btn"),
    );
    add(
        "reader-mode".into(),
        if reader_mode {
            "Exit reading mode"
        } else {
            "Reading mode"
        }
        .into(),
        if reader_mode { "reader-exit" } else { "" },
        "distraction free focus",
        None,
        click(if reader_mode {
            "reader-exit"
        } else {
            "reader-mode-btn"
        }),
    );
    for o in &state.layouts {
        add(
            format!("layout-{}", o.value),
            format!("Layout: {}", o.label),
            "",
            "",
            if comparing {
                Some("Compare always shows a verse to a row")
            } else {
                in_use(o.value == state.layout)
            },
            Run::Choice {
                group: "layout-choices",
                value: o.value.clone(),
            },
        );
    }
    for o in &state.widths {
        add(
            format!("width-{}", o.value),
            format!("Width: {}", o.label),
            "",
            "column measure",
            in_use(o.value == state.width),
            Run::Choice {
                group: "width-choices",
                value: o.value.clone(),
            },
        );
    }
    // Where the size in use is among the sizes (none: before the first).
    let size = state
        .text_sizes
        .iter()
        .position(|o| o.value == state.text_size)
        .map_or(-1, |i| i as isize);
    let sizes = state.text_sizes.len() as isize;
    add(
        "text-larger".into(),
        "Text size: Larger".into(),
        "text-larger",
        "bigger increase font",
        (size >= sizes - 1).then_some("Already the largest"),
        Run::TextSize(1),
    );
    add(
        "text-smaller".into(),
        "Text size: Smaller".into(),
        "text-smaller",
        "decrease font",
        (size <= 0).then_some("Already the smallest"),
        Run::TextSize(-1),
    );
    add(
        "text-medium".into(),
        "Text size: Medium".into(),
        "text-medium",
        "reset default font",
        in_use(state.text_size == "medium"),
        Run::TextSize(0),
    );
    for o in &state.themes {
        add(
            format!("theme-{}", o.value),
            format!("Theme: {}", o.label),
            "",
            "colours colors look",
            in_use(o.value == state.theme),
            Run::Choice {
                group: "theme-choices",
                value: o.value.clone(),
            },
        );
    }
    let reading = &state.translation;
    for o in &state.translations {
        add(
            format!("translation-{}", o.value),
            format!("Translation: {}", o.value.to_uppercase()),
            "",
            &o.label,
            in_use(&o.value == reading),
            Run::Translation(o.value.clone()),
        );
    }
    for o in state.translations.iter().filter(|o| &o.value != reading) {
        add(
            format!("compare-with-{}", o.value),
            format!("Compare with: {}", o.value.to_uppercase()),
            "",
            &o.label,
            if comparing {
                in_use(state.partner.as_ref() == Some(&o.value))
            } else {
                Some("Only while comparing")
            },
            Run::CompareWith(o.value.clone()),
        );
    }
    add(
        "random-chapter".into(),
        "Random chapter".into(),
        "",
        "surprise",
        None,
        click("random-chapter-btn"),
    );
    add(
        "sidebar".into(),
        if state.sidebar {
            "Hide sidebar"
        } else {
            "Show sidebar"
        }
        .into(),
        "",
        "books chapters picker",
        if reader_mode {
            Some("Not in reading mode")
        } else {
            (!state.wide).then_some("The window is too narrow for it")
        },
        click("sidebar-toggle"),
    );
    for (tab, name) in LIBRARY_TABS {
        add(
            format!("library-{tab}"),
            format!("Open Library: {name}"),
            "library",
            "",
            reader_mode.then_some("Not in reading mode"),
            Run::Library(tab),
        );
    }
    add(
        "export-notes".into(),
        "Export notes…".into(),
        "",
        "markdown file",
        (!state.notes).then_some("There are no notes yet"),
        click("export-notes-btn"),
    );
    add(
        "export-data".into(),
        "Export data…".into(),
        "",
        "backup save file",
        state.export_busy.then_some("Busy with an export or import"),
        click("export-data-btn"),
    );
    add(
        "import-data".into(),
        "Import data…".into(),
        "",
        "restore backup file",
        state.import_busy.then_some("Busy with an export or import"),
        click("import-data-btn"),
    );
    add(
        "welcome".into(),
        "Show welcome".into(),
        "",
        "start help",
        None,
        click("welcome-btn"),
    );
    add(
        "about".into(),
        "About Gospel Getter".into(),
        "",
        "version credits",
        None,
        click("about-btn"),
    );
    add(
        "check-updates".into(),
        "Check for updates".into(),
        "",
        "update version new",
        state.checking.then_some("Already checking"),
        Run::CheckUpdates,
    );
    add(
        "copy-chapter".into(),
        "Copy chapter".into(),
        "",
        "clipboard text",
        (!state.chapter_copy).then_some("No chapter is shown"),
        page("copy-chapter"),
    );
    add(
        "copy-chapter-as".into(),
        "Copy chapter as…".into(),
        "",
        "clipboard verse numbers markdown quote plain text",
        (!state.chapter_shown).then_some("No chapter is shown"),
        page("copy-chapter-as"),
    );
    add(
        "next-chapter".into(),
        "Next chapter".into(),
        "next-chapter",
        "",
        (!state.next_chapter).then_some("This is the Bible’s last chapter"),
        page("next-chapter"),
    );
    add(
        "previous-chapter".into(),
        "Previous chapter".into(),
        "previous-chapter",
        "",
        (!state.previous_chapter).then_some("This is the Bible’s first chapter"),
        page("previous-chapter"),
    );
    add(
        "next-book".into(),
        "Next book".into(),
        "next-book",
        "",
        (!state.next_book).then_some("This is the Bible’s last book"),
        page("next-book"),
    );
    add(
        "previous-book".into(),
        "Previous book".into(),
        "previous-book",
        "",
        (!state.previous_book).then_some("This is the Bible’s first book"),
        page("previous-book"),
    );
    add(
        "back".into(),
        "Back".into(),
        "back",
        "history previous passage",
        (!state.back).then_some("Nothing earlier in this session"),
        page("back"),
    );
    add(
        "forward".into(),
        "Forward".into(),
        "forward",
        "history next passage",
        (!state.forward).then_some("Nothing later in this session"),
        page("forward"),
    );
    add(
        "shortcuts".into(),
        "Keyboard shortcuts".into(),
        "shortcuts",
        "keys help",
        None,
        page("shortcuts"),
    );
    list
}

// ---- Matching is by words, in any order, ignoring case and accents:
// every word typed must begin a word of the command's name or its other
// words ("rea mo" is Reading mode, "theme ma" Theme: Matrix). The list
// keeps its own order.

/// `s` decomposed (NFD). In the webview by the engine's own
/// `String.prototype.normalize`, which the page's `foldWord` used, so the
/// module needn't carry the tables (about 64 KB compressed); natively (the
/// tests) by `unicode-normalization`, the same Unicode algorithm.
fn decomposed(s: &str) -> String {
    #[cfg(target_arch = "wasm32")]
    {
        String::from(js_sys::JsString::from(s).normalize("NFD"))
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        unicode_normalization::UnicodeNormalization::nfd(s).collect()
    }
}

/// A word folded for matching, as the page's `foldWord`: decomposed (NFD),
/// its marks dropped, lowercased.
pub fn fold_word(s: &str) -> String {
    decomposed(s)
        .chars()
        .filter(|c| c.general_category_group() != GeneralCategoryGroup::Mark)
        .collect::<String>()
        .to_lowercase()
}

/// A letter or a digit, as `\p{L}` and `\p{N}` say.
fn letter_or_digit(c: char) -> bool {
    matches!(
        c.general_category_group(),
        GeneralCategoryGroup::Letter | GeneralCategoryGroup::Number
    )
}

/// The folded words of `s`: its runs of letters and digits.
pub fn words_of(s: &str) -> Vec<String> {
    fold_word(s)
        .split(|c: char| !letter_or_digit(c))
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

/// Whether every word `typed` begins a word of the command's label or its
/// other words.
pub fn command_matches(command: &Command, typed: &[String]) -> bool {
    let words = words_of(&format!("{} {}", command.label, command.words));
    typed
        .iter()
        .all(|t| words.iter().any(|w| w.starts_with(t.as_str())))
}

/// What the box holds.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Query {
    /// Nothing but spaces.
    pub empty: bool,
    /// It begins with ">": commands only, no search.
    pub commands_only: bool,
    /// Its words (after the ">"), folded.
    pub words: Vec<String>,
}

/// `s` without the white space JavaScript's `trim` takes off either end
/// (Rust's `trim`, and the byte order mark).
pub fn js_trim(s: &str) -> &str {
    s.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}')
}

impl Query {
    /// What `value` (the box's text) asks for.
    pub fn parse(value: &str) -> Self {
        let trimmed = js_trim(value);
        let commands_only = trimmed.starts_with('>');
        let text = if commands_only {
            &trimmed[1..]
        } else {
            trimmed
        };
        Self {
            empty: trimmed.is_empty(),
            commands_only,
            words: words_of(text),
        }
    }

    /// Whether this is a search (words to look for, or a reference).
    pub fn searches(&self) -> bool {
        !self.empty && !self.commands_only
    }
}

/// The commands to list for `query`, from `all` (in its order): every one
/// after a bare ">", the suggestions in the empty box, otherwise those the
/// words match.
pub fn shown(query: &Query, all: Vec<Command>) -> Vec<Command> {
    if query.commands_only && query.words.is_empty() {
        return all;
    }
    if query.empty {
        return SUGGESTED
            .iter()
            .filter_map(|id| all.iter().find(|c| c.id == *id).cloned())
            .collect();
    }
    if query.words.is_empty() {
        return Vec::new();
    }
    all.into_iter()
        .filter(|c| command_matches(c, &query.words))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn choices(values: &[(&str, &str)]) -> Vec<Choice> {
        values
            .iter()
            .map(|(value, label)| Choice {
                value: value.to_string(),
                label: label.to_string(),
            })
            .collect()
    }

    /// The page at John 3 in the KJV, as the frontend tests boot it.
    fn john_3() -> PaletteState {
        PaletteState {
            partner: Some("web".into()),
            wide: true,
            sidebar: true,
            layouts: choices(&[("verse", "Verse by verse"), ("flowing", "Flowing")]),
            layout: "verse".into(),
            widths: choices(&[("narrow", "Narrow"), ("wide", "Wide")]),
            width: "narrow".into(),
            text_sizes: choices(&[("small", "Small"), ("medium", "Medium"), ("large", "Large")]),
            text_size: "medium".into(),
            themes: choices(&[("chi-rho", "Chi Rho"), ("matrix", "Matrix")]),
            theme: "chi-rho".into(),
            translations: choices(&[
                ("kjv", "King James Version"),
                ("web", "World English Bible"),
                ("bsb", "Berean Standard Bible"),
            ]),
            translation: "kjv".into(),
            chapter_copy: true,
            chapter_shown: true,
            next_chapter: true,
            previous_chapter: true,
            next_book: true,
            previous_book: true,
            ..PaletteState::default()
        }
    }

    fn find<'a>(list: &'a [Command], id: &str) -> &'a Command {
        list.iter()
            .find(|c| c.id == id)
            .unwrap_or_else(|| panic!("{id}"))
    }

    fn labels(list: &[Command]) -> Vec<&str> {
        list.iter().map(|c| c.label.as_str()).collect()
    }

    fn query(value: &str) -> Query {
        Query::parse(value)
    }

    #[test]
    fn every_command_once_in_the_palettes_order() {
        let list = commands(&john_3());
        assert_eq!(list.first().unwrap().label, "Compare on");
        assert_eq!(list.last().unwrap().label, "Keyboard shortcuts");
        for (i, c) in list.iter().enumerate() {
            assert!(list[..i].iter().all(|o| o.id != c.id), "{} twice", c.id);
        }
        // 2 + 2 layouts + 2 widths + 3 sizes + 2 themes + 3 translations
        // + 2 to compare with + 20 more.
        assert_eq!(list.len(), 36);
        let ids: Vec<&str> = list.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(
            &ids[..9],
            [
                "compare",
                "reader-mode",
                "layout-verse",
                "layout-flowing",
                "width-narrow",
                "width-wide",
                "text-larger",
                "text-smaller",
                "text-medium"
            ]
        );
        assert_eq!(
            labels(&list[11..16]),
            [
                "Translation: KJV",
                "Translation: WEB",
                "Translation: BSB",
                "Compare with: WEB",
                "Compare with: BSB"
            ]
        );
    }

    #[test]
    fn keys_come_from_the_shortcut_table() {
        let list = commands(&john_3());
        for (id, key, keys) in [
            ("compare", "C", "C"),
            ("text-larger", "Ctrl+=", "Control+="),
            ("library-notes", "Ctrl+B", "Control+B"),
            ("next-chapter", "→", "ArrowRight"),
            ("back", "Alt+←", "Alt+ArrowLeft"),
            ("shortcuts", "?", "?"),
        ] {
            let c = find(&list, id);
            assert_eq!((c.key.as_str(), c.keys), (key, keys), "{id}");
        }
        let random = find(&list, "random-chapter");
        assert_eq!((random.key.as_str(), random.keys), ("", ""));
        // Reading mode shows its key only once it's on: Escape leaves it.
        assert_eq!(find(&list, "reader-mode").key, "");
        let on = commands(&PaletteState {
            reader_mode: true,
            ..john_3()
        });
        let exit = find(&on, "reader-mode");
        assert_eq!(
            (exit.label.as_str(), exit.key.as_str()),
            ("Exit reading mode", "Esc")
        );
        assert_eq!(exit.run, Run::Click("reader-exit"));
    }

    #[test]
    fn reasons_follow_the_page() {
        let list = commands(&john_3());
        let reason = |list: &[Command], id: &str| find(list, id).reason;
        assert_eq!(reason(&list, "compare"), None);
        assert_eq!(
            reason(&list, "compare-with-web"),
            Some("Only while comparing")
        );
        assert_eq!(reason(&list, "theme-chi-rho"), Some(IN_USE));
        assert_eq!(reason(&list, "theme-matrix"), None);
        assert_eq!(reason(&list, "translation-kjv"), Some(IN_USE));
        assert_eq!(reason(&list, "text-medium"), Some(IN_USE));
        assert_eq!(
            reason(&list, "export-notes"),
            Some("There are no notes yet")
        );
        assert_eq!(
            reason(&list, "back"),
            Some("Nothing earlier in this session")
        );
        assert_eq!(
            reason(&list, "forward"),
            Some("Nothing later in this session")
        );
        assert_eq!(reason(&list, "sidebar"), None);
        assert_eq!(find(&list, "sidebar").label, "Hide sidebar");

        let comparing = commands(&PaletteState {
            comparing: true,
            ..john_3()
        });
        assert_eq!(find(&comparing, "compare").label, "Compare off");
        assert_eq!(reason(&comparing, "compare-with-web"), Some(IN_USE));
        assert_eq!(reason(&comparing, "compare-with-bsb"), None);
        assert_eq!(
            reason(&comparing, "layout-flowing"),
            Some("Compare always shows a verse to a row")
        );

        let alone = commands(&PaletteState {
            partner: None,
            ..john_3()
        });
        assert_eq!(
            reason(&alone, "compare"),
            Some("There’s only one translation")
        );

        let end = commands(&PaletteState {
            next_chapter: false,
            next_book: false,
            chapter_copy: false,
            chapter_shown: false,
            ..john_3()
        });
        assert_eq!(
            reason(&end, "next-chapter"),
            Some("This is the Bible’s last chapter")
        );
        assert_eq!(
            reason(&end, "next-book"),
            Some("This is the Bible’s last book")
        );
        assert_eq!(reason(&end, "previous-chapter"), None);
        assert_eq!(reason(&end, "copy-chapter"), Some("No chapter is shown"));
        assert_eq!(reason(&end, "copy-chapter-as"), Some("No chapter is shown"));

        let reading_mode = commands(&PaletteState {
            reader_mode: true,
            ..john_3()
        });
        assert_eq!(
            reason(&reading_mode, "sidebar"),
            Some("Not in reading mode")
        );
        assert_eq!(
            reason(&reading_mode, "library-bookmarks"),
            Some("Not in reading mode")
        );
        let narrow = commands(&PaletteState {
            wide: false,
            sidebar: false,
            ..john_3()
        });
        assert_eq!(find(&narrow, "sidebar").label, "Show sidebar");
        assert_eq!(
            reason(&narrow, "sidebar"),
            Some("The window is too narrow for it")
        );

        let busy = commands(&PaletteState {
            export_busy: true,
            import_busy: true,
            checking: true,
            notes: true,
            ..john_3()
        });
        assert_eq!(
            reason(&busy, "export-data"),
            Some("Busy with an export or import")
        );
        assert_eq!(
            reason(&busy, "import-data"),
            Some("Busy with an export or import")
        );
        assert_eq!(reason(&busy, "check-updates"), Some("Already checking"));
        assert_eq!(reason(&busy, "export-notes"), None);
    }

    #[test]
    fn text_size_stops_at_either_end() {
        let at = |size: &str| {
            let list = commands(&PaletteState {
                text_size: size.into(),
                ..john_3()
            });
            (
                find(&list, "text-larger").reason,
                find(&list, "text-smaller").reason,
            )
        };
        assert_eq!(at("large"), (Some("Already the largest"), None));
        assert_eq!(at("small"), (None, Some("Already the smallest")));
        assert_eq!(at("medium"), (None, None));
        // A size the list doesn't have is below the first.
        assert_eq!(at("huge"), (None, Some("Already the smallest")));
    }

    #[test]
    fn each_command_runs_its_own_control() {
        let list = commands(&john_3());
        assert_eq!(find(&list, "compare").run, Run::Click("compare-btn"));
        assert_eq!(
            find(&list, "theme-matrix").run,
            Run::Choice {
                group: "theme-choices",
                value: "matrix".into()
            }
        );
        assert_eq!(
            find(&list, "translation-bsb").run,
            Run::Translation("bsb".into())
        );
        assert_eq!(
            find(&list, "compare-with-web").run,
            Run::CompareWith("web".into())
        );
        assert_eq!(find(&list, "text-larger").run, Run::TextSize(1));
        assert_eq!(find(&list, "text-medium").run, Run::TextSize(0));
        assert_eq!(find(&list, "library-notes").run, Run::Library("notes"));
        assert_eq!(find(&list, "check-updates").run, Run::CheckUpdates);
        assert_eq!(find(&list, "back").run, page("back"));
    }

    #[test]
    fn folding_drops_case_and_accents() {
        assert_eq!(fold_word("Thème"), "theme");
        assert_eq!(fold_word("ÉLAN Ångström"), "elan angstrom");
        assert_eq!(
            words_of("Theme: Classic Dark"),
            ["theme", "classic", "dark"]
        );
        assert_eq!(words_of("Export notes…"), ["export", "notes"]);
        assert_eq!(words_of("  --  "), Vec::<String>::new());
        assert_eq!(words_of("Ψαλμός 23"), ["ψαλμος", "23"]);
    }

    #[test]
    fn words_match_as_prefixes_in_any_order() {
        let list = commands(&john_3());
        let matching = |typed: &str| {
            labels(&shown(&query(typed), list.clone()))
                .into_iter()
                .map(str::to_string)
                .collect::<Vec<_>>()
        };
        assert_eq!(matching("rea mo"), ["Reading mode"]);
        assert_eq!(matching("mo rea"), ["Reading mode"]);
        assert_eq!(matching("READING"), ["Reading mode"]);
        assert_eq!(matching("theme ma"), ["Theme: Matrix"]);
        assert_eq!(matching("thème ma"), ["Theme: Matrix"]);
        assert_eq!(matching("translation berean"), ["Translation: BSB"]);
        assert_eq!(matching("next ch"), ["Next chapter"]);
        assert_eq!(matching("eading"), Vec::<String>::new());
        assert_eq!(matching("---"), Vec::<String>::new());
    }

    #[test]
    fn the_box_reads_as_commands_suggestions_or_words() {
        assert_eq!(
            query("  "),
            Query {
                empty: true,
                commands_only: false,
                words: vec![]
            }
        );
        assert_eq!(
            query(" > Theme ma "),
            Query {
                empty: false,
                commands_only: true,
                words: vec!["theme".into(), "ma".into()]
            }
        );
        assert!(query("john 3").searches());
        assert!(!query(">john").searches());
        assert!(!query("\u{feff} ").searches());

        let list = commands(&john_3());
        assert_eq!(shown(&query(">"), list.clone()).len(), list.len());
        assert_eq!(
            labels(&shown(&query(""), list.clone())),
            [
                "Compare on",
                "Reading mode",
                "Random chapter",
                "Copy chapter",
                "Export notes…"
            ]
        );
        assert_eq!(
            labels(&shown(&query(">theme ma"), list.clone())),
            ["Theme: Matrix"]
        );
        assert!(shown(&query(">nothing like this"), list).is_empty());
    }
}
