//! Lightweight regression checks on `ui/index.html`'s CSS and script,
//! without a browser or JS test framework — plain substring/brace-matching
//! assertions against the bundled static asset, the same file Tauri loads
//! into the webview at `frontendDist`.

const HTML: &str = include_str!("../../ui/index.html");

use gospel_getter_lib::preferences::{self, Preference, Values};

/// The page's pieces written in Rust (`ui-leptos/src`), which own the
/// modal dialogs' markup and keyboard handling, the Library's tabs and
/// lists, the search box and its palette, the table of shortcuts, the
/// book and chapter picker's books and chapters, Aa with the translations
/// and the saved settings, the header's surfaces with the picker's
/// shapes, the reader (the chapter's markup, its loading and the passage
/// history), the selection card (copying, and the card's bookmark,
/// highlight and notes), and what was the page's main script (the keyboard
/// and click dispatchers, boot, and the page's glue): checks of those read
/// them where they now live.
const PIECES: [(&str, &str); 31] = [
    (
        "export_notes.rs",
        include_str!("../../ui-leptos/src/export_notes.rs"),
    ),
    (
        "shortcuts.rs",
        include_str!("../../ui-leptos/src/shortcuts.rs"),
    ),
    ("welcome.rs", include_str!("../../ui-leptos/src/welcome.rs")),
    (
        "import_preview.rs",
        include_str!("../../ui-leptos/src/import_preview.rs"),
    ),
    (
        "note_editor.rs",
        include_str!("../../ui-leptos/src/note_editor.rs"),
    ),
    ("bridge.rs", include_str!("../../ui-leptos/src/bridge.rs")),
    ("about.rs", include_str!("../../ui-leptos/src/about.rs")),
    ("updates.rs", include_str!("../../ui-leptos/src/updates.rs")),
    ("library.rs", include_str!("../../ui-leptos/src/library.rs")),
    ("search.rs", include_str!("../../ui-leptos/src/search.rs")),
    (
        "shortcut_table.rs",
        include_str!("../../ui-leptos/src/shortcut_table.rs"),
    ),
    ("picker.rs", include_str!("../../ui-leptos/src/picker.rs")),
    (
        "picker_text.rs",
        include_str!("../../ui-leptos/src/picker_text.rs"),
    ),
    ("aa.rs", include_str!("../../ui-leptos/src/aa.rs")),
    ("aa_text.rs", include_str!("../../ui-leptos/src/aa_text.rs")),
    (
        "surfaces.rs",
        include_str!("../../ui-leptos/src/surfaces.rs"),
    ),
    (
        "surfaces_text.rs",
        include_str!("../../ui-leptos/src/surfaces_text.rs"),
    ),
    ("reader.rs", include_str!("../../ui-leptos/src/reader.rs")),
    (
        "reader_text.rs",
        include_str!("../../ui-leptos/src/reader_text.rs"),
    ),
    ("card.rs", include_str!("../../ui-leptos/src/card.rs")),
    (
        "card_text.rs",
        include_str!("../../ui-leptos/src/card_text.rs"),
    ),
    ("margin.rs", include_str!("../../ui-leptos/src/margin.rs")),
    (
        "margin_text.rs",
        include_str!("../../ui-leptos/src/margin_text.rs"),
    ),
    (
        "note_markdown.rs",
        include_str!("../../ui-leptos/src/note_markdown.rs"),
    ),
    (
        "note_links.rs",
        include_str!("../../ui-leptos/src/note_links.rs"),
    ),
    (
        "keyboard.rs",
        include_str!("../../ui-leptos/src/keyboard.rs"),
    ),
    (
        "keyboard_text.rs",
        include_str!("../../ui-leptos/src/keyboard_text.rs"),
    ),
    ("clicks.rs", include_str!("../../ui-leptos/src/clicks.rs")),
    ("boot.rs", include_str!("../../ui-leptos/src/boot.rs")),
    ("page.rs", include_str!("../../ui-leptos/src/page.rs")),
    (
        "page_text.rs",
        include_str!("../../ui-leptos/src/page_text.rs"),
    ),
];

/// One piece's source, by file name.
fn piece(name: &str) -> &'static str {
    PIECES
        .iter()
        .find(|(file, _)| *file == name)
        .map(|(_, source)| *source)
        .unwrap_or_else(|| panic!("no piece {name}"))
}

/// Walk forward from a `{` at `start`, tracking nesting depth, to find the
/// index of the brace that closes that same block (not just the first
/// nested selector/statement inside it).
fn matching_brace_end(src: &str, open_brace_at: usize) -> usize {
    let mut depth = 0usize;
    for (i, ch) in src[open_brace_at..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return open_brace_at + i;
                }
            }
            _ => {}
        }
    }
    panic!("unclosed block starting at byte {open_brace_at}");
}

/// The block (from its `{`, without the `}` that closes it) after the first
/// `needle` in `src`, with every run of whitespace made one space, so a
/// check reads the same however the stylesheet or source is indented and
/// wrapped.
fn block_after(src: &str, needle: &str) -> String {
    let start = src
        .find(needle)
        .unwrap_or_else(|| panic!("expected to find `{needle}` in ui/index.html"));
    let open = src[start..]
        .find('{')
        .map(|i| start + i)
        .unwrap_or_else(|| panic!("expected `{{` after `{needle}`"));
    let close = matching_brace_end(src, open);
    one_space(&src[open..close])
}

/// `text` with each run of whitespace (spaces, tabs, line breaks) one
/// space, and none at either end.
fn one_space(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Regression guard for GH-1: on narrow screens the reader must only see
/// the current chapter, not scroll past the full previous one first. The
/// neighbors are hidden (never re-stacked under `.chapter-main`): the
/// previous one first, below 848px, then the next, below 648px (see
/// `side_chapters_never_squeeze_the_column_below_50_characters`).
#[test]
fn mobile_breakpoint_hides_chapter_side_neighbors() {
    for (query, side) in [
        ("@media (max-width: 847px)", ".chapter-side-prev"),
        ("@media (max-width: 647px)", ".chapter-side-next"),
    ] {
        let block = block_after(HTML, query);
        let rule = block
            .find(side)
            .unwrap_or_else(|| panic!("{query} should hide {side}"));
        assert!(
            block[rule..].starts_with(&format!("{side} {{ display: none;")),
            "{query} should hide {side} instead of re-stacking it"
        );
    }
}

/// Regression guard for GH-6: printing a chapter should hide all page
/// chrome (the top bar and its popovers, the Library, the dialogs and the
/// toast, the book and chapter picker, neighboring chapter panes,
/// nav hint, the chapter's end buttons, the selection card) but must
/// never target the reading content itself — the chapter heading and
/// verses are the whole point of printing the page.
#[test]
fn print_stylesheet_hides_chrome_but_not_reading_content() {
    let block = block_after(HTML, "@media print");

    assert!(
        block.contains("display: none"),
        "print stylesheet should actually hide the page chrome it targets"
    );
    for chrome_selector in [
        ".top-bar",
        ".popover",
        ".library-panel",
        ".import-overlay",
        ".welcome-overlay",
        ".toast",
        ".picker",
        ".chapter-side",
        ".nav-hint",
        ".chapter-end",
        ".selection-card",
    ] {
        assert!(
            block.contains(chrome_selector),
            "print stylesheet should hide `{chrome_selector}` — it's page \
             chrome, not reading content"
        );
    }

    for reading_selector in ["#reading-pane {", ".chapter-main {", ".verse {"] {
        assert!(
            !block.contains(reading_selector),
            "print stylesheet must not hide `{reading_selector}` — the \
             chapter heading and verses are exactly what should remain \
             visible when printed"
        );
    }
}

/// Margin notes in reading mode (ui-leptos/src/margin.rs): beside the
/// column, never in its flow (so they can't move the text), on screen only
/// (printing leaves them out, and the markers that stand in for them), in
/// the UI's sans and smaller than the verses, in tested theme colors. Aa's
/// "Notes in the margin" is a pressed toggle like Reading mode's, shown with
/// it, and the setting is this computer's, never in a backup. The margin
/// never scrolls the page: markers, which reflow the text, go in and out
/// through the reading place logic.
#[test]
fn margin_notes_sit_beside_the_text_on_screen_only() {
    let rules = rules();
    let screen = |selector: &str| {
        rules
            .iter()
            .find(|r| r.at_rule.as_deref() == Some("@media screen") && r.selector == selector)
            .unwrap_or_else(|| panic!("expected an `@media screen` rule for `{selector}`"))
            .body
            .clone()
    };
    let margin = screen(".margin-notes");
    for declaration in [
        "position: absolute;",
        "left: calc(100% + 1.5rem);",
        "width: var(--margin-width, 14rem);",
        "font-family: var(--font-ui);",
        "font-size: 0.8rem;",
        "color: var(--text);",
    ] {
        assert!(margin.contains(declaration), ".margin-notes: {declaration}");
    }
    assert!(screen(".margin-note").contains("position: absolute;"));
    assert!(screen(".margin-note-ref").contains("color: var(--accent-2);"));
    assert!(screen(".margin-note-where").contains("color: var(--text-dim);"));
    assert!(screen(".margin-rule").contains("background: var(--accent-dim);"));
    assert!(screen(".margin-marker").contains("color: var(--verse-num-color);"));
    let body = screen(".margin-note-body");
    assert!(
        body.contains("max-height: calc(var(--margin-fold-lines, 6) * 1.45em);")
            && body.contains("overflow: hidden;")
    );
    // Its words as the card's: Markdown, line breaks as `<br>`, so no
    // `pre-wrap` to break them twice.
    assert!(!body.contains("white-space"));
    assert!(piece("margin_text.rs").contains("note_markdown::note_html(&note.body, &note.links"));
    assert!(
        piece("margin.rs")
            .contains(r#"set_property("--margin-fold-lines", &text::FOLD_LINES.to_string())"#)
    );
    let print = block_after(HTML, "@media print");
    assert!(print.contains(".margin-notes, .margin-marker { display: none !important; }"));

    assert!(HTML.contains(
        r#"<button type="button" id="margin-notes-btn" class="reader-mode-btn margin-notes-btn" aria-pressed="true""#
    ));
    assert!(
        screen_rule(
            &rules,
            r#":root:not([data-reader-mode="on"]) .margin-notes-btn"#
        )
        .body
        .contains("display: none;")
    );
    assert_eq!(preferences::MARGIN_NOTES.fallback, "on");
    assert_eq!(
        preferences::MARGIN_NOTES.file_name,
        None,
        "never in a backup"
    );

    let margin_rs = piece("margin.rs");
    assert!(margin_rs.contains("surfaces::keep_reading_place("));
    assert!(!margin_rs.contains("scroll_by") && !margin_rs.contains("scroll_to"));
}

/// Regression guard for GH-5: the "Random chapter" feature picks a chapter
/// entirely client-side, so every rendered `.book-item` button must carry
/// its `chapter_count` as a data attribute, or the client has no valid
/// upper bound to pick from. Book buttons are built by one shared view,
/// `book_view` in the picker's piece (`ui-leptos/src/picker.rs`), for both
/// the Old and New Testament lists, rather than two server-rendered loops,
/// so this checks that view and that both lists are built from it.
#[test]
fn book_item_buttons_expose_chapter_count() {
    let picker = piece("picker.rs");
    assert!(
        picker.contains(
            "data-book-id=book.id.to_string()\n                data-chapter-count=book.chapter_count.to_string()"
        ),
        "book-item buttons must expose chapterCount as a data attribute for \
         client-side random selection"
    );
    assert!(
        picker.contains(r#"for (id, books) in [("ot-book-list", &old), ("nt-book-list", &new)] {"#),
        "both testaments' lists should be drawn by the same loop"
    );
    assert!(
        picker.contains("books.iter().map(book_view)"),
        "each testament's books should render through the shared book-button \
         view"
    );
}

/// Regression guard for GH-5: the random chapter control must be a real,
/// labeled `<button>` (not a link or a bare clickable `<div>`) so it's
/// reachable and announced correctly by assistive tech.
#[test]
fn random_chapter_control_is_a_real_labeled_button() {
    let id_pos = HTML
        .find(r#"id="random-chapter-btn""#)
        .expect("a #random-chapter-btn control should exist in ui/index.html");

    let tag_start = HTML[..id_pos]
        .rfind("<button")
        .expect("the random chapter control should be a <button> element");
    let tag_end = HTML[tag_start..]
        .find('>')
        .map(|i| tag_start + i)
        .expect("the random chapter button's opening tag should be closed");
    let opening_tag = &HTML[tag_start..=tag_end];
    assert!(
        opening_tag.contains(r#"type="button""#),
        "random chapter control should be an explicit type=\"button\", not a \
         form-submitting default"
    );

    let close_pos = HTML[tag_end..]
        .find("</button>")
        .map(|i| tag_end + i)
        .expect("the random chapter button should have a matching </button>");
    let label = HTML[tag_end + 1..close_pos].trim();
    assert!(
        !label.is_empty(),
        "random chapter button should have a non-empty visible label so its \
         accessible name isn't blank"
    );
}

/// Regression guard for GH-5: the random pick (`random_pick` in
/// `ui-leptos/src/picker_text.rs`, unit-tested there) must choose from the
/// books the picker lists (uniform choice of book, then 1..=chapter_count
/// for that book) and must bail out rather than read a chapter when the
/// count isn't positive — it must never fabricate a request to an invalid
/// chapter.
#[test]
fn random_chapter_script_guards_against_malformed_data() {
    let rules = piece("picker_text.rs");
    assert!(
        piece("picker.rs").contains("text::random_pick(&s.books, js_sys::Math::random)"),
        "random pick should choose uniformly from the books the picker lists"
    );
    assert!(
        rules.contains("if book.chapter_count < 1 {\n        return None;"),
        "random pick should guard against a non-positive chapter count \
         instead of issuing an invalid request"
    );
    assert!(
        rules.contains("floor_times(random(), count)).ok()? + 1"),
        "chapter pick should be uniform over 1..=chapter_count"
    );
}

/// The frontend must talk to the backend only through Tauri's typed
/// `invoke()` bridge — no `fetch()`/HTTP calls, and no reliance on
/// `@tauri-apps/api` being bundled by a Node build step (the app must load
/// straight from these local files with no build tooling in between).
#[test]
fn frontend_uses_invoke_not_http() {
    assert!(
        !HTML.contains("fetch("),
        "the desktop app must not make HTTP requests — all data access \
         should go through invoke()"
    );
    assert!(
        HTML.contains("window.__TAURI__.core.invoke"),
        "the frontend should call Tauri commands via the global __TAURI__ \
         bridge (app.withGlobalTauri), not an npm-bundled API import"
    );
    // The picker builds a book's chapters from its chapter count in
    // `get_home`, so it doesn't call `get_chapters`.
    for command in ["get_home", "get_reading"] {
        assert!(
            HTML.contains(&format!("invoke('{command}'")),
            "expected a call to the `{command}` Tauri command"
        );
    }
    // The verses' notes are the selection card's (ui-leptos/src/card.rs),
    // through the same global `invoke` (ui-leptos/src/ipc.rs).
    assert!(
        piece("card.rs").contains(r#"send("list_notes", &args)"#),
        "expected a call to the `list_notes` Tauri command"
    );
}

// ---- Reader comfort, themes and reading mode -------------------------

/// The contents of the page's `<style>` element with `/* ... */` comments
/// removed, so prose inside comments can't satisfy or break a check.
fn stylesheet() -> String {
    let start = HTML
        .find("<style>")
        .expect("ui/index.html should have a <style>")
        + 7;
    let end = HTML.find("</style>").expect("<style> should be closed");
    let mut css = String::new();
    let mut rest = &HTML[start..end];
    while let Some(open) = rest.find("/*") {
        css.push_str(&rest[..open]);
        let close = rest[open..].find("*/").expect("unclosed CSS comment") + open;
        rest = &rest[close + 2..];
    }
    css.push_str(rest);
    css
}

/// One style rule: its selector list, its declarations, and the at-rule
/// (e.g. `@media print`) it's nested in, if any. `body` is the declarations
/// as parsed (`declarations`), each written `name: value;` and separated by
/// one space, so a check like `body.contains("color: var(--text)")` compares
/// declarations, not the stylesheet's spacing or line breaks.
struct Rule {
    at_rule: Option<String>,
    selector: String,
    body: String,
}

/// A rule's `name: value` declarations, in order: split at the `;`s outside
/// quotes and brackets, each name and value trimmed and their whitespace
/// runs made one space (see `one_space`).
fn declarations(body: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let (mut depth, mut quote, mut start) = (0usize, None, 0);
    let mut push = |decl: &str| {
        if let Some((name, value)) = decl.split_once(':') {
            out.push((one_space(name), one_space(value)));
        }
    };
    for (i, c) in body.char_indices() {
        match (quote, c) {
            (Some(q), _) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '\'' | '"') => quote = Some(c),
            (None, '(' | '[') => depth += 1,
            (None, ')' | ']') => depth = depth.saturating_sub(1),
            (None, ';') if depth == 0 => {
                push(&body[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    push(&body[start..]);
    out
}

/// `declarations` written back one way: `name: value;`, one space apart.
fn written(declarations: &[(String, String)]) -> String {
    declarations
        .iter()
        .map(|(name, value)| format!("{name}: {value};"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn rules_in(css: &str, at_rule: Option<&str>, out: &mut Vec<Rule>) {
    let mut cursor = 0;
    while let Some(i) = css[cursor..].find('{') {
        let open = cursor + i;
        let close = matching_brace_end(css, open);
        let selector = css[cursor..open].trim().to_owned();
        let body = &css[open + 1..close];
        if selector.starts_with('@') {
            rules_in(body, Some(&one_space(&selector)), out);
        } else {
            out.push(Rule {
                at_rule: at_rule.map(str::to_owned),
                selector: one_space(&selector),
                body: written(&declarations(body)),
            });
        }
        cursor = close + 1;
    }
}

/// The checks read declarations, not the stylesheet's spacing: the same
/// rules, indented and wrapped or run together on one line, parse alike, and
/// a `;` inside quotes or brackets doesn't end a declaration.
#[test]
fn css_checks_ignore_how_the_stylesheet_is_spaced() {
    let wrapped = ".a,\n        .b {\n    color: var(--text);\n    content: ';';\n    \
                   background:\n        url(\"x;y\")\n}\n@media   print {\n    .a {\n        \
                   display:  none !important;\n    }\n}\n";
    let one_line = ".a, .b{color:var(--text);content:';';background:url(\"x;y\");}\
                    @media print{.a{display:none !important}}";
    let parsed = |css: &str| {
        let mut out = Vec::new();
        rules_in(css, None, &mut out);
        out.into_iter()
            .map(|r| (r.at_rule, r.selector, r.body))
            .collect::<Vec<_>>()
    };
    let expected = vec![
        (
            None,
            ".a, .b".to_owned(),
            "color: var(--text); content: ';'; background: url(\"x;y\");".to_owned(),
        ),
        (
            Some("@media print".to_owned()),
            ".a".to_owned(),
            "display: none !important;".to_owned(),
        ),
    ];
    assert_eq!(parsed(wrapped), expected);
    assert_eq!(parsed(one_line), expected);
    assert_eq!(
        block_after(wrapped, "@media"),
        block_after(
            "@media print {\n.a { display: none !important; }\n}",
            "@media"
        ),
    );
}

fn rules() -> Vec<Rule> {
    let mut out = Vec::new();
    rules_in(&stylesheet(), None, &mut out);
    out
}

/// The rule outside any at-rule whose selector list is `selector` (its
/// whitespace runs one space, as `Rule::selector`'s are).
fn screen_rule<'a>(rules: &'a [Rule], selector: &str) -> &'a Rule {
    let selector = one_space(selector);
    rules
        .iter()
        .find(|r| r.at_rule.is_none() && r.selector == selector)
        .unwrap_or_else(|| panic!("expected a `{selector}` rule"))
}

/// A theme's block: the default theme's on bare `:root`, every other one's
/// on `:root[data-theme="..."]`. Each also styles that theme's swatch in
/// Aa (`[data-theme-preview="..."]`). System shares the default's block,
/// so that block names System too; in a light OS a
/// `@media (prefers-color-scheme: light)` block gives System Vellum's
/// palette instead (see `system_follows_the_os_between_chi_rho_and_vellum`).
fn theme_rule<'a>(rules: &'a [Rule], theme: &str, default_theme: &str) -> &'a Rule {
    let page = if theme == default_theme {
        ":root".to_owned()
    } else {
        format!(r#":root[data-theme="{theme}"]"#)
    };
    let preview = format!(r#"[data-theme-preview="{theme}"]"#);
    rules
        .iter()
        .find(|r| {
            r.at_rule.is_none()
                && selectors(r).any(|s| s == page)
                && selectors(r).any(|s| s == preview)
        })
        .unwrap_or_else(|| panic!("expected a `{page}, {preview}` rule"))
}

/// System's block in a light OS.
fn system_light_rule(rules: &[Rule]) -> &Rule {
    rules
        .iter()
        .find(|r| {
            r.at_rule.as_deref() == Some("@media (prefers-color-scheme: light)")
                && selectors(r).eq([
                    r#":root[data-theme="system"]"#,
                    r#"[data-theme-preview="system"]"#,
                ])
        })
        .expect("System should switch to Vellum's palette in a light OS")
}

fn default_theme() -> String {
    script_preset("theme-select").1
}

fn selectors(rule: &Rule) -> impl Iterator<Item = &str> {
    rule.selector.split(',').map(str::trim)
}

/// Every `--name: value` custom property a rule declares.
fn custom_properties(body: &str) -> Vec<(String, String)> {
    declarations(body)
        .into_iter()
        .filter(|(name, _)| name.starts_with("--"))
        .collect()
}

fn property<'a>(props: &'a [(String, String)], name: &str) -> &'a str {
    props
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, v)| v.as_str())
        .unwrap_or_else(|| panic!("expected `{name}` to be declared"))
}

/// The setting whose hidden `<select id>` records it: Aa's settings, in
/// the saved settings' one table (`shared/preferences.rs`, which the page's
/// Rust pieces fill those `<select>`s from).
fn preference(select_id: &str) -> &'static Preference {
    preferences::PREFERENCES
        .iter()
        .find(|p| p.select == Some(select_id))
        .unwrap_or_else(|| panic!("no setting is recorded in #{select_id}"))
}

/// The options of a setting's `<select id="...">`, `(value, label)` in
/// order: its choices in the one table.
fn select_options(id: &str) -> Vec<(String, String)> {
    let Values::Choices(choices) = preference(id).values else {
        panic!("#{id}'s setting has no choices");
    };
    choices
        .iter()
        .map(|c| (c.value.to_owned(), c.label.to_owned()))
        .collect()
}

/// The values and default a saved choice for the `<select id>` is checked
/// against (the store, and the pre-paint script's table generated from
/// it): the one table's.
fn script_preset(select_id: &str) -> (Vec<String>, String) {
    let pref = preference(select_id);
    (
        pref.values.list().into_iter().map(str::to_owned).collect(),
        pref.fallback.to_owned(),
    )
}

fn relative_luminance(hex: &str) -> f64 {
    let hex = hex.trim_start_matches('#');
    assert_eq!(hex.len(), 6, "expected a #rrggbb color, got `{hex}`");
    let channel = |i: usize| {
        let c = f64::from(u8::from_str_radix(&hex[i..i + 2], 16).unwrap()) / 255.0;
        if c <= 0.039_28 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(0) + 0.7152 * channel(2) + 0.0722 * channel(4)
}

fn contrast_ratio(a: &str, b: &str) -> f64 {
    let (la, lb) = (relative_luminance(a), relative_luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// Palette for each theme the picker offers (see `theme_rule`), System
/// twice: as it is in a dark OS and in a light one.
fn theme_palettes(rules: &[Rule]) -> Vec<(String, Vec<(String, String)>)> {
    theme_blocks(rules)
        .into_iter()
        .map(|(theme, rule)| (theme, custom_properties(&rule.body)))
        .collect()
}

/// Each theme's block, by the names `theme_palettes` gives them.
fn theme_blocks(rules: &[Rule]) -> Vec<(String, &Rule)> {
    let default_theme = default_theme();
    select_options("theme-select")
        .into_iter()
        .flat_map(|(value, _)| {
            if value == "system" {
                vec![
                    (
                        "system (dark)".to_owned(),
                        theme_rule(rules, &value, &default_theme),
                    ),
                    ("system (light)".to_owned(), system_light_rule(rules)),
                ]
            } else {
                let rule = theme_rule(rules, &value, &default_theme);
                vec![(value, rule)]
            }
        })
        .collect()
}

/// Every theme in the picker must define the full set of color custom
/// properties the default theme does — a theme missing e.g. `--option-bg`
/// would fall through to the default's dark popup color and could end up
/// unreadable against its own text.
#[test]
fn every_theme_option_defines_a_complete_palette() {
    let rules = rules();
    let default_rule = theme_rule(&rules, &default_theme(), &default_theme());
    let color_props: Vec<String> = custom_properties(&default_rule.body)
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    for name in ["--accent-2", "--glow", "--verse-num-color"] {
        assert!(color_props.iter().any(|n| n == name), "{name}");
    }

    for (theme, props) in theme_palettes(&rules) {
        for name in &color_props {
            assert!(
                props.iter().any(|(n, _)| n == name),
                "theme `{theme}` should define `{name}`"
            );
        }
    }
    for (theme, rule) in theme_blocks(&rules) {
        assert!(
            rule.body.contains("color-scheme:"),
            "theme `{theme}` should set color-scheme so native controls use \
             a matching light/dark palette"
        );
    }
}

/// Text, dimmed text and accent-colored text must all be legible on the
/// opaque popup/select background, and text drawn on an accent-filled
/// control (active book/chapter) must be legible on the accent — WCAG AA
/// (4.5:1) for every theme, which is what keeps Hot Pink from being pink
/// on pink.
#[test]
fn theme_palettes_meet_wcag_aa_contrast() {
    let rules = rules();
    for (theme, props) in theme_palettes(&rules) {
        let option_bg = property(&props, "--option-bg");
        for fg in ["--text", "--text-dim", "--accent"] {
            let ratio = contrast_ratio(property(&props, fg), option_bg);
            assert!(
                ratio >= 4.5,
                "theme `{theme}`: {fg} on --option-bg is {ratio:.2}:1, below 4.5:1"
            );
        }
        let ratio = contrast_ratio(
            property(&props, "--on-accent"),
            property(&props, "--accent"),
        );
        assert!(
            ratio >= 4.5,
            "theme `{theme}`: --on-accent on --accent is {ratio:.2}:1, below 4.5:1"
        );
    }
}

#[test]
fn hot_pink_theme_is_offered_and_distinct() {
    assert!(
        select_options("theme-select").contains(&("hot-pink".to_owned(), "Hot Pink".to_owned())),
        "the theme picker should offer Hot Pink"
    );
    let rules = rules();
    let palettes = theme_palettes(&rules);
    let accent_of = |theme: &str| {
        let (_, props) = palettes.iter().find(|(t, _)| t == theme).unwrap();
        property(props, "--accent").to_owned()
    };
    let hot_pink = accent_of("hot-pink");

    // Pink is the accent, not the body text: verse text must read as
    // clearly separate from the pink headings/verse numbers around it.
    let (_, props) = palettes.iter().find(|(t, _)| t == "hot-pink").unwrap();
    let separation = contrast_ratio(property(props, "--text"), &hot_pink);
    assert!(
        separation >= 2.5,
        "Hot Pink body text should stand apart from its pink accent \
         ({separation:.2}:1), not be pink on pink"
    );
    for (theme, _) in palettes.iter().filter(|(t, _)| t != "hot-pink") {
        assert_ne!(
            accent_of(theme),
            hot_pink,
            "Hot Pink should have its own accent, not reuse `{theme}`'s"
        );
    }
}

/// The Monster Hunter theme is now Beast Slayer: the visible label and the
/// stored key both change, the palette doesn't, and a reader who had
/// saved the old key is migrated to the new one instead of silently
/// falling back to the default theme.
#[test]
fn beast_slayer_replaces_monster_hunter_and_migrates_saved_choice() {
    let options = select_options("theme-select");
    assert!(
        options.contains(&("beast-slayer".to_owned(), "Beast Slayer".to_owned())),
        "the theme picker should offer Beast Slayer"
    );
    assert!(
        !options
            .iter()
            .any(|(v, l)| v == "monster-hunter" || l == "Monster Hunter"),
        "the old Monster Hunter name should no longer be offered"
    );

    let rules = rules();
    let props = custom_properties(&theme_rule(&rules, "beast-slayer", &default_theme()).body);
    for (name, value) in [
        ("--text", "#f0e4d0"),
        ("--text-dim", "#c2a583"),
        ("--accent", "#d4a017"),
        ("--option-bg", "#241a12"),
        ("--on-accent", "#1a1410"),
    ] {
        assert_eq!(
            property(&props, name),
            value,
            "Beast Slayer should keep the original {name}"
        );
    }

    assert_eq!(
        preferences::THEME.renamed_to("monster-hunter"),
        Some("beast-slayer"),
        "a saved `monster-hunter` theme should be migrated to `beast-slayer`"
    );
    assert!(
        HTML.contains("{ 'monster-hunter': 'beast-slayer' }"),
        "and shown as `beast-slayer` from the first frame"
    );
    let (values, _) = script_preset("theme-select");
    assert!(
        !values.iter().any(|v| v == "monster-hunter"),
        "the old key should be migrated, not accepted as-is"
    );
}

/// Each preference `<select>` must offer exactly the values the store
/// accepts from storage (otherwise a saved choice could be un-selectable,
/// or a selectable one silently reset on restart), and its default must be
/// one of them. The `<select>`s are hidden records of each choice, which
/// Aa fills from the one table and sets to what's saved; Aa's buttons,
/// labelled groups built from the same choices, are what the reader uses.
#[test]
fn preference_selects_match_the_values_the_script_persists() {
    for id in [
        "theme-select",
        "text-size-select",
        "line-spacing-select",
        "width-select",
    ] {
        let offered: Vec<String> = select_options(id).into_iter().map(|(v, _)| v).collect();
        let (accepted, fallback) = script_preset(id);
        assert_eq!(
            offered, accepted,
            "#{id} options should match what the script persists"
        );
        assert!(
            accepted.contains(&fallback),
            "#{id}'s default `{fallback}` should be offered"
        );
        assert!(
            HTML.contains(&format!(r#"<select id="{id}" hidden></select>"#)),
            "#{id} should be a hidden record behind Aa's buttons, filled by Aa"
        );
    }
    let draw = block_after(piece("aa.rs"), "fn draw_choices(");
    assert!(
        draw.contains(
            "let (Some(id), Values::Choices(choices)) = (pref.select, pref.values) else {"
        )
    );
    assert!(draw.contains("select.set_value(&prefs::load(&pref));"));
}

/// The saved look is on the page before its first frame: the first script
/// in `<head>` is the pre-paint script generated from the one table
/// (`preferences::prepaint_script`), byte for byte, and it only reads what's
/// saved. Everything that saves a setting is the page's Rust pieces'.
#[test]
fn the_pre_paint_script_applies_the_saved_look_from_the_one_table() {
    let script = preferences::prepaint_script();
    let at = HTML.find(&script).unwrap_or_else(|| {
        panic!("ui/index.html's pre-paint script should be, byte for byte:\n{script}")
    });
    let head = &HTML[..HTML.find("</head>").unwrap()];
    assert_eq!(
        head.find("<script>"),
        Some(at + "    ".len()),
        "the first script"
    );
    assert!(head[..at].contains("</style>"), "after the stylesheet");
    for key in preferences::PREFERENCES.map(|p| p.key) {
        assert!(script.contains(&format!("['{key}', ")), "{key}");
    }
    assert!(
        !head.contains("setItem"),
        "the head scripts never save a setting"
    );
    assert!(piece("aa.rs").contains("prefs::set(&preferences::UPDATE_CHECK, value);"));
}

/// With nothing saved, verse typography must be exactly what it was before
/// the presets existed, and every non-default preset must actually change
/// something.
#[test]
fn reading_presets_default_to_the_original_typography() {
    let rules = rules();
    let root = custom_properties(&screen_rule(&rules, ":root").body);
    assert_eq!(property(&root, "--reading-scale"), "1");
    assert_eq!(property(&root, "--reading-leading"), "1.7");

    let verse = &screen_rule(&rules, ".verse").body;
    assert!(verse.contains("font-size: calc(1.05rem * var(--reading-scale))"));
    assert!(verse.contains("line-height: var(--reading-leading)"));

    for (id, attr, var) in [
        ("text-size-select", "text-size", "--reading-scale"),
        ("line-spacing-select", "line-spacing", "--reading-leading"),
    ] {
        let (values, fallback) = script_preset(id);
        for value in values.iter().filter(|v| **v != fallback) {
            let selector = format!(r#":root[data-{attr}="{value}"]"#);
            let props = custom_properties(&screen_rule(&rules, &selector).body);
            let set = property(&props, var);
            assert_ne!(set, property(&root, var), "`{value}` should change {var}");
        }
    }
}

/// Text size/spacing apply to the reading content only — verses, verse
/// numbers, chapter headings, compare mode's "not numbered" cells (which
/// sit in a row beside a verse), and in the note panel the verses quoted
/// and the note being written about them — never to the rest of the UI
/// (so the settings menu, book list etc. don't grow with it).
#[test]
fn reading_presets_scale_only_reading_content() {
    for rule in rules() {
        if !rule.body.contains("var(--reading-") {
            continue;
        }
        for selector in selectors(&rule) {
            assert!(
                [
                    ".verse",
                    ".chapter-side .verse",
                    r#":root[data-layout="flowing"] .chapter-text"#,
                    r#":root[data-layout="flowing"] .chapter-side .chapter-text"#,
                    ".verse-num",
                    ".chapter-heading",
                    ".compare-gap",
                    ".note-passage",
                    ".note-input"
                ]
                .contains(&selector),
                "`{selector}` uses a reading preset, but only reading content should"
            );
        }
    }
}

/// Reading mode hides navigation and chrome (the Library and ☰ buttons
/// among it) but must keep the chapter being read, Aa, search, and its own
/// exit control on screen.
#[test]
fn reader_mode_hides_chrome_but_keeps_reading_content_and_exit() {
    let rules = rules();
    // Scoped to screen, so reading mode never changes a printed page.
    let hidden: Vec<&str> = rules
        .iter()
        .filter(|r| r.at_rule.as_deref() == Some("@media screen"))
        .filter(|r| r.body.contains("display: none"))
        .flat_map(selectors)
        .filter_map(|s| s.strip_prefix(r#":root[data-reader-mode="on"] "#))
        .collect();

    for chrome in [
        ".sidebar-toggle",
        ".chapter-side",
        ".nav-hint",
        "#library-toggle",
        "#menu-toggle",
    ] {
        assert!(
            hidden.contains(&chrome),
            "reading mode should hide `{chrome}`"
        );
    }
    for kept in [
        "#reading-pane",
        "#reading-pane-inner",
        ".chapter-main",
        ".chapter-heading",
        ".verse",
        ".top-bar",
        "#translation-toggle",
        "#aa-toggle",
        ".search-toggle",
        ".ref-btn",
        ".reader-exit",
    ] {
        assert!(
            !hidden.contains(&kept),
            "reading mode must not hide `{kept}`"
        );
    }

    // The exit control is only hidden when reading mode is *off*.
    let exit_hiders: Vec<&str> = rules
        .iter()
        .filter(|r| r.at_rule.is_none() && r.body.contains("display: none"))
        .flat_map(selectors)
        .filter(|s| s.ends_with(".reader-exit"))
        .collect();
    assert_eq!(
        exit_hiders,
        [r#":root:not([data-reader-mode="on"]) .reader-exit"#]
    );
}

/// The exit control must be a real, labeled, keyboard-focusable button,
/// and Escape must also leave reading mode. None of it was ever the main
/// script's (now gone): the exit and the toggle are Aa's (the page's Rust
/// pieces), Escape's last step is the surfaces' (Rust too), which asks Aa,
/// and the pre-paint script doesn't touch the Tauri bridge. Without the
/// Rust pieces reading mode can't be left (the exit and Escape do nothing).
#[test]
fn reader_mode_exit_is_accessible_and_independent_of_the_main_script() {
    let tag_start = HTML
        .find(r#"<button type="button" id="reader-exit""#)
        .expect("reading mode's exit should be a type=\"button\" <button>");
    let label_end = HTML[tag_start..].find("</button>").unwrap() + tag_start;
    let label = &HTML[HTML[tag_start..].find('>').unwrap() + tag_start + 1..label_end];
    assert!(
        label.contains("Exit reading mode"),
        "exit button needs a clear visible label"
    );

    let toggle = HTML
        .find(r#"id="reader-mode-btn""#)
        .expect("Aa should have a reading-mode toggle");
    let toggle_tag = &HTML[HTML[..toggle].rfind("<button").unwrap()..toggle + 80];
    assert!(toggle_tag.contains(r#"type="button""#));
    assert!(
        toggle_tag.contains("aria-pressed="),
        "reading-mode toggle should expose its state"
    );

    // The pre-paint script, the head's first (boot's prefetch, after it,
    // asks the backend; the look never waits for it).
    let head_start = HTML.find("<script>").unwrap();
    let head_script = &HTML[head_start..head_start + HTML[head_start..].find("</script>").unwrap()];
    assert!(
        !head_script.contains("__TAURI__.core"),
        "the pre-paint script must not depend on the Tauri bridge"
    );
    let escape = block_after(piece("surfaces.rs"), "fn on_escape(");
    for wiring in [
        r#"event.key() != "Escape""#,
        "aa::set_reader_mode(false);",
        "'gospel-getter-reader-mode', 'readerMode', 'off'",
    ] {
        assert!(
            head_script.contains(wiring) || escape.contains(wiring),
            "the pre-paint script or the surfaces' Escape should contain `{wiring}`"
        );
    }
    let aa = piece("aa.rs");
    for wiring in [
        r#"by_id("reader-exit")"#,
        r#"by_id(if on { "reader-exit" } else { "aa-toggle" })"#,
        "dom::on_click(&exit, |_| set_reader_mode(false));",
        "pub fn set_reader_mode(on: bool) {",
        "prefs::set(&preferences::READER_MODE,",
    ] {
        assert!(aa.contains(wiring), "Aa should contain `{wiring}`");
    }
}

/// The opening tag of the element with `id`: in the page's markup, or, for
/// the dialogs the Rust pieces own, in their `view!` markup (the same
/// attributes, one to a line).
fn tag_of(id: &str) -> &'static str {
    let needle = format!(r#"id="{id}""#);
    let source = std::iter::once(HTML)
        .chain(PIECES.iter().map(|(_, source)| *source))
        .find(|source| source.contains(&needle))
        .unwrap_or_else(|| panic!("expected an element #{id}"));
    let at = source.find(&needle).unwrap();
    let tag = &source[source[..at].rfind('<').unwrap()..];
    &tag[..tag.find('>').unwrap()]
}

/// Each header button announces what it opens and whether it's open, and
/// what it opens says what it is.
#[test]
fn header_buttons_announce_what_they_open() {
    for (toggle, surface, role, label) in [
        ("aa-toggle", "aa-popover", r#"role="dialog""#, "Display"),
        ("menu-toggle", "app-menu", r#"role="menu""#, "Menu"),
    ] {
        let tag = tag_of(toggle);
        assert!(tag.contains(&format!(r#"aria-controls="{surface}""#)));
        assert!(tag.contains(r#"aria-expanded="false""#));
        assert!(tag.contains(&format!(r#"aria-label="{label}""#)));
        let surface_tag = tag_of(surface);
        assert!(surface_tag.contains(role), "#{surface} should be {role}");
        assert!(surface_tag.contains(&format!(r#"aria-label="{label}""#)));
    }
    // The translation button names the translation once one is read (the
    // main script sets "Translation: King James Version").
    let translation = tag_of("translation-toggle");
    assert!(translation.contains(r#"aria-controls="translation-popover""#));
    assert!(translation.contains(r#"aria-expanded="false""#));
    assert!(translation.contains(r#"aria-haspopup="dialog""#));
    assert!(translation.contains(r#"aria-keyshortcuts="T""#));
    let popover = tag_of("translation-popover");
    assert!(popover.contains(r#"role="dialog""#));
    assert!(popover.contains(r#"aria-label="Translation and compare""#));
    let library = tag_of("library-toggle");
    assert!(library.contains(r#"aria-controls="library-panel""#));
    assert!(library.contains(r#"aria-expanded="false""#));
    assert!(library.contains(r#"aria-label="Library: bookmarks, highlights and notes""#));
    assert!(tag_of("search-toggle").contains(r#"aria-controls="search-panel""#));
    let about = tag_of("about-panel");
    assert!(about.contains(r#"role="dialog""#) && about.contains(r#"aria-modal="true""#));
    let toast = tag_of("data-status");
    assert!(toast.contains(r#"role="status""#) && toast.contains(r#"aria-live="polite""#));
    for item in [
        "export-data-btn",
        "import-data-btn",
        "welcome-btn",
        "about-btn",
    ] {
        assert!(tag_of(item).contains(r#"role="menuitem""#), "#{item}");
    }
}

/// The top bar, the popovers, the Library and the toast add only these
/// colors to the theme palettes: the bar's opaque `--bar-bg`, with the bar
/// buttons (ghost icons in `--text-dim`, `--text` on a wash under the
/// pointer, `--accent` on it while open) AA on it in every theme; and they
/// otherwise reuse the AA-tested pairs (`--accent` on `--panel-bg` over
/// `--option-bg`, `--on-accent` on `--accent`, `--text`/`--text-dim` on
/// `--option-bg`). The chosen theme swatch's ring is `--accent`, at least
/// 3:1 (a non-text contrast) on `--option-bg`.
#[test]
fn header_surfaces_use_tested_theme_colors() {
    let rules = rules();
    let bar = screen_rule(&rules, ".top-bar");
    assert!(bar.body.contains("background: var(--bar-bg)"));
    let open_alpha: f64 = screen_rule(&rules, r#".bar-btn[aria-expanded="true"]"#)
        .body
        .split_once("rgba(var(--accent-rgb), ")
        .and_then(|(_, rest)| rest.split_once(')'))
        .map(|(alpha, _)| alpha.trim().parse().unwrap())
        .expect("an open bar button is tinted with --accent");
    let ghost = screen_rule(&rules, ".bar-btn");
    for declaration in ["background: none", "border: none", "color: var(--text-dim)"] {
        assert!(ghost.body.contains(declaration), ".bar-btn: {declaration}");
    }
    let hover = &screen_rule(&rules, ".bar-btn:hover").body;
    assert!(hover.contains("color: var(--text)"));
    let hover_alpha: f64 = hover
        .split_once("rgba(var(--accent-rgb), ")
        .and_then(|(_, rest)| rest.split_once(')'))
        .map(|(alpha, _)| alpha.trim().parse().unwrap())
        .expect("a bar button under the pointer gets a wash");
    assert!(
        screen_rule(&rules, r#".bar-btn[aria-expanded="true"]"#)
            .body
            .contains("color: var(--accent)")
    );
    for (theme, props) in theme_palettes(&rules) {
        let bar_bg = property(&props, "--bar-bg");
        let accent = property(&props, "--accent");
        let accent_rgb = property(&props, "--accent-rgb");
        let option_bg = property(&props, "--option-bg");
        for (what, fg, bg) in [
            (
                "--text on --bar-bg",
                property(&props, "--text").to_owned(),
                bar_bg.to_owned(),
            ),
            ("--accent on --bar-bg", accent.to_owned(), bar_bg.to_owned()),
            (
                "a bar button",
                property(&props, "--text-dim").to_owned(),
                bar_bg.to_owned(),
            ),
            (
                "a bar button under the pointer",
                property(&props, "--text").to_owned(),
                composite(&format!("rgba({accent_rgb}, {hover_alpha})"), bar_bg),
            ),
            (
                "an open bar button",
                accent.to_owned(),
                composite(&format!("rgba({accent_rgb}, {open_alpha})"), bar_bg),
            ),
            (
                "a menu item under the pointer",
                property(&props, "--text").to_owned(),
                composite(&format!("rgba({accent_rgb}, 0.15)"), option_bg),
            ),
        ] {
            let ratio = contrast_ratio(&fg, &bg);
            assert!(
                ratio >= 4.5,
                "theme `{theme}`: {what} is {ratio:.2}:1, below 4.5:1"
            );
        }
        let ring = contrast_ratio(accent, option_bg);
        assert!(
            ring >= 3.0,
            "theme `{theme}`: the chosen swatch's ring is {ring:.2}:1, below 3:1"
        );
    }

    let segment = screen_rule(&rules, ".segment");
    assert!(segment.body.contains("color: var(--accent)"));
    assert!(segment.body.contains("background: var(--panel-bg)"));
    let chosen = screen_rule(&rules, r#".segment[aria-pressed="true"]"#);
    assert!(chosen.body.contains("background: var(--accent)"));
    assert!(chosen.body.contains("color: var(--on-accent)"));
    let swatch = screen_rule(&rules, ".theme-swatch");
    assert!(swatch.body.contains("border: 1px solid var(--text-dim)"));
    let ring = screen_rule(&rules, r#".theme-swatch[aria-pressed="true"]"#);
    assert!(ring.body.contains("var(--accent)"));
    for selector in [".popover", ".library-panel:not([hidden])", ".toast"] {
        let rule = screen_rule(&rules, selector);
        assert!(
            rule.body.contains("background: var(--option-bg)"),
            "{selector}"
        );
        assert!(rule.body.contains("color: var(--text)"), "{selector}");
    }
    let tab = screen_rule(&rules, ".library-tab");
    assert!(tab.body.contains("color: var(--text-dim)"));
    let menu_hover = screen_rule(&rules, ".menu-item:hover,\n        .menu-item:focus");
    assert!(menu_hover.body.contains("rgba(var(--accent-rgb), 0.15)"));
}

/// Export notes' Written list and its chapter and date fields are themed,
/// not the platform's grey: `--text` on the opaque `--option-bg` (AA in
/// every theme), a border and radius like the app's other controls, and
/// for the list no native appearance and a chevron in `--accent`; each has
/// a focus ring in `--accent` (at least 3:1, a non-text contrast, on
/// `--option-bg`), as do the books' checkboxes and All / None. The list's
/// options are the same pair, and each theme's `color-scheme` keeps the
/// list it opens, and the date fields' calendar, legible.
#[test]
fn export_notes_book_list_uses_tested_theme_colors() {
    let rules = rules();
    let select = screen_rule(&rules, ".export-notes-select");
    for declaration in [
        "appearance: none",
        "color: var(--text)",
        "background-color: var(--option-bg)",
        "border: 1px solid var(--accent-dim)",
        "border-radius: var(--radius-m)",
    ] {
        assert!(select.body.contains(declaration), "{declaration}");
    }
    let chevron = select
        .body
        .split_once("background-image:")
        .and_then(|(_, rest)| rest.split_once(';'))
        .map(|(value, _)| value)
        .expect("a chevron");
    assert_eq!(chevron.matches("var(--accent)").count(), 2, "{chevron}");
    let option = screen_rule(&rules, ".export-notes-select option");
    assert!(option.body.contains("color: var(--text)"));
    assert!(option.body.contains("background-color: var(--option-bg)"));
    let field = screen_rule(&rules, ".export-notes-field");
    for declaration in [
        "color: var(--text)",
        "background-color: var(--option-bg)",
        "border: 1px solid var(--accent-dim)",
        "border-radius: var(--radius-m)",
    ] {
        assert!(field.body.contains(declaration), "{declaration}");
    }
    assert!(
        screen_rule(&rules, ".export-notes-book-count")
            .body
            .contains("color: var(--text-dim)")
    );
    let focus = screen_rule(
        &rules,
        ".export-notes-books input:focus-visible,\n        .export-notes-select:focus-visible,\n        \
         .export-notes-field:focus-visible,\n        .export-notes-tools .reader-mode-btn:focus-visible",
    );
    assert!(focus.body.contains("outline: 2px solid var(--accent)"));

    for (theme, props) in theme_palettes(&rules) {
        let option_bg = property(&props, "--option-bg");
        let text = contrast_ratio(property(&props, "--text"), option_bg);
        assert!(
            text >= 4.5,
            "theme `{theme}`: the book list's text is {text:.2}:1, below 4.5:1"
        );
        let count = contrast_ratio(property(&props, "--text-dim"), option_bg);
        assert!(
            count >= 4.5,
            "theme `{theme}`: the books' counts are {count:.2}:1, below 4.5:1"
        );
        let accent = contrast_ratio(property(&props, "--accent"), option_bg);
        assert!(
            accent >= 3.0,
            "theme `{theme}`: the book list's chevron and focus ring are {accent:.2}:1, below 3:1"
        );
    }
}

/// The theme swatches in Aa are drawn from the theme definitions
/// themselves, not from copied colors: one per theme, each previewing its
/// theme through `[data-theme-preview]`.
#[test]
fn theme_swatches_come_from_the_theme_definitions() {
    let rules = rules();
    let preview = screen_rule(&rules, ".theme-swatch-preview");
    assert!(preview.body.contains("background: var(--bg)"));
    assert!(preview.body.contains("color: var(--accent)"));
    // Aa draws one swatch per theme in the one table, previewing it.
    let aa = piece("aa.rs");
    assert!(aa.contains(
        r#"<span class="theme-swatch-preview" data-theme-preview=value aria-hidden="true">"#
    ));
    assert!(aa.contains("append(&group, swatch_view(choice.value, choice.label));"));
}

/// About's Release notes link shows the same URL the backend opens. About
/// is the Rust pieces' (`ui-leptos/src/about.rs`), which build the URL.
#[test]
fn release_notes_link_matches_the_backend() {
    let prefix = gospel_getter_lib::updater::RELEASE_NOTES_PREFIX;
    let about = piece("about.rs");
    assert!(about.contains(&format!("format!(\"{prefix}{{version}}\")")));
    assert!(about.contains(r#"ipc::invoke_value("open_release_notes""#));
}

/// Arrow keys pressed on a focused settings `<select>` change that
/// select, and must not also turn the page to another chapter.
#[test]
fn arrow_key_navigation_ignores_focused_form_controls() {
    assert!(
        piece("keyboard.rs")
            .contains(r#"matches!(target.tag_name().as_str(), "INPUT" | "TEXTAREA" | "SELECT")"#)
    );
}

/// Printing stays black on white at the default size and spacing whatever
/// theme or reading presets are saved, and never shows reading mode's
/// on-screen exit control.
#[test]
fn print_ignores_saved_reader_preferences() {
    let rules = rules();
    let print_root = rules
        .iter()
        .find(|r| r.at_rule.as_deref() == Some("@media print") && r.selector == ":root")
        .expect("print stylesheet should reset :root custom properties");
    let props = custom_properties(&print_root.body);
    for (name, value) in [
        ("--text", "#000000 !important"),
        ("--bg", "#ffffff !important"),
        ("--reading-scale", "1 !important"),
        ("--reading-leading", "1.7 !important"),
    ] {
        assert_eq!(property(&props, name), value, "print should force {name}");
    }

    let print_hidden: Vec<&str> = rules
        .iter()
        .filter(|r| {
            r.at_rule.as_deref() == Some("@media print") && r.body.contains("display: none")
        })
        .flat_map(selectors)
        .collect();
    assert!(print_hidden.contains(&".reader-exit"));

    for rule in rules
        .iter()
        .filter(|r| selectors(r).any(|s| s.starts_with(r#":root[data-reader-mode="on"]"#)))
    {
        assert_eq!(
            rule.at_rule.as_deref(),
            Some("@media screen"),
            "reading-mode layout `{}` should be screen-only so it can't alter print",
            rule.selector
        );
    }
}

// ---- Highlights

const HIGHLIGHT_COLORS: [&str; 4] = ["yellow", "green", "blue", "pink"];

/// Highlighted verse text must stay readable: `--text` on each of a
/// theme's highlighter colors meets WCAG AA (4.5:1), in every theme, and
/// the four colors are distinct.
#[test]
fn highlight_colors_keep_verse_text_at_wcag_aa_in_every_theme() {
    let rules = rules();
    for (theme, props) in theme_palettes(&rules) {
        let text = property(&props, "--text");
        let mut seen = Vec::new();
        for color in HIGHLIGHT_COLORS {
            let highlight = property(&props, &format!("--hl-{color}"));
            let ratio = contrast_ratio(text, highlight);
            assert!(
                ratio >= 4.5,
                "theme `{theme}`: --text on --hl-{color} is {ratio:.2}:1, below 4.5:1"
            );
            assert!(
                !seen.contains(&highlight),
                "theme `{theme}`: --hl-{color} repeats another color"
            );
            seen.push(highlight);
        }
    }
}

/// The hue of a `#rrggbb` color, in degrees.
fn hue(hex: &str) -> f64 {
    let hex = hex.trim_start_matches('#');
    let [r, g, b] = [0, 2, 4].map(|i| f64::from(u8::from_str_radix(&hex[i..i + 2], 16).unwrap()));
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let d = max - min;
    let h = if d == 0.0 {
        0.0
    } else if max == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    h * 60.0
}

/// Matrix's yellow highlighter reads as yellow on its black-green, not
/// olive (it was #4a4708, hue 57°): a warm amber, hue 40-50°, as dark as
/// AA for its pale green text allows (7.06:1), and lighter than the page
/// so the band shows.
#[test]
fn matrix_yellow_is_warm_not_olive() {
    let rules = rules();
    let (_, props) = theme_palettes(&rules)
        .into_iter()
        .find(|(theme, _)| theme == "matrix")
        .expect("the Matrix theme");
    let yellow = property(&props, "--hl-yellow");
    assert_eq!(yellow, "#664e00");
    let h = hue(yellow);
    assert!((40.0..=50.0).contains(&h), "hue {h:.1}");
    assert!(hue("#4a4708") > 55.0, "the old one was olive");
    let text = contrast_ratio(property(&props, "--text"), yellow);
    assert!(text >= 7.0, "{text:.2}:1");
    let band = contrast_ratio(yellow, property(&props, "--bg"));
    assert!(band >= 2.5, "against the page: {band:.2}:1");
}

/// Each color has a verse style and a swatch style, and the highlight sits
/// on the verse text (a band behind the words), not the whole verse block
/// the selection tints.
#[test]
fn every_highlight_color_is_styled_on_the_verse_text() {
    let rules = rules();
    for color in HIGHLIGHT_COLORS {
        let verse = screen_rule(
            &rules,
            &format!(r#".verse[data-highlight="{color}"] .verse-text"#),
        );
        assert!(
            verse.body.contains(&format!("var(--hl-{color})")),
            "{color} verses should use --hl-{color}"
        );
        let swatch = screen_rule(
            &rules,
            &format!(r#".highlight-swatch[data-color="{color}"]"#),
        );
        assert!(swatch.body.contains(&format!("var(--hl-{color})")));
    }
    // The pressed swatch is marked by more than its color.
    let pressed = screen_rule(&rules, r#".highlight-swatch[aria-pressed="true"]::after"#);
    assert!(pressed.body.contains("content:"));
}

/// The Highlights list in Settings adds no colors of its own: each
/// passage's swatch is its highlighter color, outlined in `--text-dim` (AA
/// on `--option-bg` in every theme, see the palette tests) so it shows
/// even where the highlighter color is close to the menu's background,
/// and its name is shown in `--text-dim` too. The pressed filter is
/// `--on-accent` on `--accent`, also AA in every theme; the unpressed one
/// is checked here.
#[test]
fn highlight_list_swatches_and_filter_use_tested_theme_colors() {
    let rules = rules();
    for color in HIGHLIGHT_COLORS {
        let dot = screen_rule(&rules, &format!(r#".highlight-dot[data-color="{color}"]"#));
        assert!(dot.body.contains(&format!("var(--hl-{color})")));
    }
    let dot = screen_rule(&rules, ".highlight-dot");
    assert!(dot.body.contains("border: 1px solid var(--text-dim)"));
    let name = screen_rule(&rules, ".highlight-color");
    assert!(name.body.contains("color: var(--text-dim)"));
    let pressed = screen_rule(&rules, r#".highlight-filter-btn[aria-pressed="true"]"#);
    assert!(pressed.body.contains("background: var(--accent)"));
    assert!(pressed.body.contains("color: var(--on-accent)"));
    let filter = screen_rule(&rules, ".highlight-filter-btn");
    assert!(filter.body.contains("color: var(--accent)"));

    // Unpressed, a filter is --accent on --panel-bg, a translucent tint
    // over the menu's opaque --option-bg.
    for (theme, props) in theme_palettes(&rules) {
        let behind = composite(
            property(&props, "--panel-bg"),
            property(&props, "--option-bg"),
        );
        let ratio = contrast_ratio(property(&props, "--accent"), &behind);
        assert!(
            ratio >= 4.5,
            "theme `{theme}`: an unpressed highlight filter is {ratio:.2}:1, below 4.5:1"
        );
    }
}

/// An `rgba(r, g, b, a)` color laid over an opaque `#rrggbb`, as `#rrggbb`.
fn composite(rgba: &str, under: &str) -> String {
    let inner = rgba
        .strip_prefix("rgba(")
        .and_then(|s| s.strip_suffix(')'))
        .unwrap_or_else(|| panic!("expected rgba(...), got `{rgba}`"));
    let parts: Vec<f64> = inner
        .split(',')
        .map(|p| p.trim().parse().unwrap())
        .collect();
    let [r, g, b, a] = parts[..] else {
        panic!("expected four rgba components in `{rgba}`")
    };
    let under = under.trim_start_matches('#');
    let channel = |top: f64, i: usize| {
        let bottom = f64::from(u8::from_str_radix(&under[i..i + 2], 16).unwrap());
        (top * a + bottom * (1.0 - a)).round() as u8
    };
    format!(
        "#{:02x}{:02x}{:02x}",
        channel(r, 0),
        channel(g, 2),
        channel(b, 4)
    )
}

/// The update banner and the Updates section add no colors of their own:
/// the banner is --text on the opaque --option-bg, its secondary text
/// --text-dim, and its controls the existing button styles, all AA in
/// every theme (see the palette tests). It isn't printed.
#[test]
fn update_banner_uses_tested_theme_colors_and_is_not_printed() {
    let rules = rules();
    let banner = screen_rule(&rules, ".update-banner:not([hidden])");
    assert!(banner.body.contains("background: var(--option-bg)"));
    assert!(banner.body.contains("color: var(--text)"));
    assert!(banner.body.contains("position: fixed"));
    let status = screen_rule(&rules, ".update-status");
    assert!(status.body.contains("color: var(--text-dim)"));
    let summary = screen_rule(&rules, ".update-banner summary");
    assert!(summary.body.contains("color: var(--accent)"));
    let print_hidden = rules
        .iter()
        .find(|r| {
            r.at_rule.as_deref() == Some("@media print")
                && r.selector.split(',').any(|s| s.trim() == ".update-banner")
        })
        .expect("print stylesheet should hide the update banner");
    assert!(print_hidden.body.contains("display: none !important"));
}

/// An update found while About is open shows in About: its notes are
/// --text in a box with an --accent-dim border, their headings and file
/// names --accent, its status --text-dim (`.update-status`, as the
/// banner's), all on the About panel's opaque --option-bg, and AA there in
/// every theme. The notes scroll inside their box.
#[test]
fn about_update_uses_tested_theme_colors() {
    let rules = rules();
    let panel = screen_rule(&rules, ".import-panel");
    assert!(panel.body.contains("background: var(--option-bg)"));
    for (selector, declarations) in [
        (
            ".about-update-notes",
            &[
                "color: var(--text)",
                "border: 1px solid var(--accent-dim)",
                "overflow-y: auto",
                "max-height: min(14rem, 40vh)",
            ][..],
        ),
        (".about-update-notes h5", &["color: var(--accent)"][..]),
        (
            ".about-update-notes code",
            &["color: var(--accent)", "font-family: inherit"][..],
        ),
    ] {
        let rule = screen_rule(&rules, selector);
        for declaration in declarations {
            assert!(rule.body.contains(declaration), "{selector}: {declaration}");
        }
    }
    for (theme, props) in theme_palettes(&rules) {
        let option = property(&props, "--option-bg");
        for token in ["--text", "--text-dim", "--accent"] {
            let ratio = contrast_ratio(property(&props, token), option);
            assert!(
                ratio >= 4.5,
                "theme `{theme}`: {token} on --option-bg is {ratio:.2}:1"
            );
        }
    }
}

/// Printing stays black on white: the print stylesheet clears every
/// highlighter color.
#[test]
fn highlights_are_not_printed() {
    let rules = rules();
    let print_root = rules
        .iter()
        .find(|r| r.at_rule.as_deref() == Some("@media print") && r.selector == ":root")
        .expect("print stylesheet should reset :root custom properties");
    let props = custom_properties(&print_root.body);
    for color in HIGHLIGHT_COLORS {
        assert_eq!(
            property(&props, &format!("--hl-{color}")),
            "transparent !important",
            "print should clear --hl-{color}"
        );
    }
}

/// Compare mode's column headers stick just below the top bar while the
/// columns scroll. Every rule that places them must agree, since a later
/// `top: 0` would win the cascade and slide them under the opaque bar.
#[test]
fn compare_headers_stick_below_the_top_bar() {
    let rules = rules();
    let tops: Vec<&str> = rules
        .iter()
        .filter(|r| r.at_rule.as_deref() != Some("@media print"))
        .filter(|r| selectors(r).any(|s| s == ".compare-head"))
        .flat_map(|r| r.body.split(';'))
        .filter_map(|decl| decl.trim().strip_prefix("top:"))
        .map(str::trim)
        .collect();
    assert_eq!(tops, ["var(--bar-height)"]);
    let head = screen_rule(&rules, ".compare-head");
    assert!(head.body.contains("position: sticky"));
}

/// The note marker is the bookmark star's kind of mark — a pseudo-element
/// after the verse number, never part of the text — but a different
/// glyph, beside the star when a verse has both. It's the verse number's
/// color (`--accent`, or Vaporwave's brighter aqua), AA on `--option-bg` in
/// every theme (see the palette tests), and at least 3:1
/// (a non-text contrast) on a selected verse's tint too. Like the star, it
/// isn't printed.
#[test]
fn note_marker_is_distinct_tested_and_not_printed() {
    let rules = rules();
    let star = screen_rule(
        &rules,
        ".chapter-main .verse.is-bookmarked .verse-num::after",
    );
    let marker = screen_rule(&rules, ".chapter-main .verse.has-notes .verse-num::after");
    assert!(star.body.contains(r"content: '\2605'"));
    assert!(marker.body.contains(r"content: '\270E'"));
    assert!(marker.body.contains("color: var(--verse-num-color)"));
    let both = screen_rule(
        &rules,
        ".chapter-main .verse.is-bookmarked.has-notes .verse-num::after",
    );
    assert!(both.body.contains(r"'\2605\2009\270E'"));

    let tint_alpha: f64 = screen_rule(&rules, ".chapter-main .verse.verse-selected")
        .body
        .split_once("background: rgba(var(--accent-rgb), ")
        .and_then(|(_, rest)| rest.split_once(')'))
        .map(|(alpha, _)| alpha.trim().parse().unwrap())
        .expect("a selected verse is tinted with --accent");
    for (theme, props) in theme_palettes(&rules) {
        // The verse number's color: --accent-2, or Vaporwave's own.
        let accent = match property(&props, "--verse-num-color") {
            "var(--accent-2)" => property(&props, "--accent-2"),
            color => color,
        };
        let tint = composite(
            &format!("rgba({}, {tint_alpha})", property(&props, "--accent-rgb")),
            property(&props, "--option-bg"),
        );
        let ratio = contrast_ratio(accent, &tint);
        assert!(
            ratio >= 3.0,
            "theme `{theme}`: the note marker on a selected verse is {ratio:.2}:1, below 3:1"
        );
    }

    for selector in [
        ".chapter-main .verse.is-bookmarked .verse-num::after",
        ".chapter-main .verse.has-notes .verse-num::after",
    ] {
        let print = rules
            .iter()
            .find(|r| r.at_rule.as_deref() == Some("@media print") && r.selector == selector)
            .unwrap_or_else(|| panic!("print should clear `{selector}`"));
        assert!(print.body.contains("content: none !important"));
    }
}

/// The Notes section and the note panel add no colors of their own: the
/// section is the actions' `--panel-bg` tint, with `--text` notes,
/// `--text-dim` dates and `--accent` citations and links, all AA on it in
/// every theme; the panel is the welcome's modal, on `--option-bg`.
#[test]
fn notes_use_tested_theme_colors() {
    let rules = rules();
    let section = screen_rule(&rules, ".verse-notes");
    assert!(section.body.contains("background: var(--panel-bg)"));
    for (selector, color) in [
        (".note-card-body", "--text"),
        (".note-card-date", "--text-dim"),
        (".verse-notes-title", "--text-dim"),
        (".note-card-ref", "--accent"),
        (".note-link", "--accent-2"),
        (".note-text a", "--accent-2"),
        (".note-text code", "--accent"),
    ] {
        assert!(
            screen_rule(&rules, selector)
                .body
                .contains(&format!("color: var({color})")),
            "{selector} should be {color}"
        );
    }
    for (theme, props) in theme_palettes(&rules) {
        let behind = composite(
            property(&props, "--panel-bg"),
            property(&props, "--option-bg"),
        );
        for fg in ["--text", "--text-dim", "--accent", "--accent-2"] {
            let ratio = contrast_ratio(property(&props, fg), &behind);
            assert!(
                ratio >= 4.5,
                "theme `{theme}`: {fg} in the Notes section is {ratio:.2}:1, below 4.5:1"
            );
        }
    }

    let panel = screen_rule(&rules, ".modal-panel");
    assert!(panel.body.contains("background: var(--option-bg)"));
    assert!(panel.body.contains("color: var(--text)"));
    let input = screen_rule(&rules, ".note-input");
    assert!(input.body.contains("background: var(--option-bg)"));
    assert!(input.body.contains("color: var(--text)"));
    for selector in [".note-text-shown", ".note-label", ".note-preview"] {
        assert!(
            screen_rule(&rules, selector)
                .body
                .contains("color: var(--text-dim)"),
            "{selector}"
        );
    }
    // The welcome and the note panel share one modal style; the welcome
    // keeps its own ids and classes.
    for class in ["welcome-overlay modal-overlay", "welcome-panel modal-panel"] {
        assert!(
            piece("welcome.rs").contains(&format!(r#"class="{class}""#)),
            "{class}"
        );
    }
    let note_overlay = tag_of("note-overlay");
    assert!(note_overlay.contains(r#"class="modal-overlay""#));
    assert!(
        note_overlay.lines().any(|line| line.trim() == "hidden"),
        "hidden until opened"
    );
    assert_eq!(
        rules
            .iter()
            .filter(|r| r.at_rule.is_none() && r.body.contains("background: rgba(0, 0, 0, 0.6)"))
            .count(),
        2,
        "one dimmed overlay style for the import preview and About, one for the modals"
    );
}

/// A verse's hidden "has notes" cue is for screen readers only: it can't
/// be selected, so copying a verse's text never copies it (checked in
/// WebKitGTK: without this, a native selection and copy of a verse with
/// notes put "(has notes)" on the clipboard).
#[test]
fn the_has_notes_cue_is_never_copied() {
    let rules = rules();
    // The same goes for a KJV verse's "(has footnotes)".
    let cue = rules
        .iter()
        .find(|r| r.at_rule.is_none() && selectors(r).eq([".note-cue", ".footnote-cue"]))
        .expect("a `.note-cue, .footnote-cue` rule");
    assert!(cue.body.contains("-webkit-user-select: none"));
    assert!(cue.body.contains("user-select: none"));
    let hidden = screen_rule(&rules, ".visually-hidden");
    assert!(hidden.body.contains("clip: rect(0 0 0 0)"));
}

/// KJV footnote markers and the chapter's Footnotes section are drawn in
/// colors every theme keeps legible (see
/// `theme_palettes_meet_wcag_aa_contrast`): markers in the verse numbers'
/// color (see `verse_numbers_and_footnote_markers_are_legible_on_every_theme`),
/// and the footnotes' text in the dimmed text color, like the
/// cross-reference list. Markers are never copied with a verse.
#[test]
fn kjv_footnotes_use_tested_theme_colors() {
    let rules = rules();
    let marker = screen_rule(&rules, ".fn-marker");
    for declaration in [
        "color: var(--verse-num-color)",
        "font-weight: normal",
        "user-select: none",
    ] {
        assert!(marker.body.contains(declaration), "{declaration}");
    }
    assert!(
        screen_rule(&rules, ".footnote-ref")
            .body
            .contains("color: var(--accent-2)")
    );
    assert!(
        screen_rule(&rules, ".footnote-text")
            .body
            .contains("color: var(--text-dim)")
    );
    assert!(
        screen_rule(&rules, ".footnote-label")
            .body
            .contains("color: var(--accent-2)")
    );
    assert!(
        screen_rule(&rules, ".supplied")
            .body
            .contains("font-style: italic")
    );
    for (theme, props) in theme_palettes(&rules) {
        let option_bg = property(&props, "--option-bg");
        for fg in ["--accent", "--accent-2", "--text-dim"] {
            let ratio = contrast_ratio(property(&props, fg), option_bg);
            assert!(ratio >= 4.5, "theme `{theme}`: {fg} is {ratio:.2}:1");
        }
    }
}

/// Printing keeps the chapter's Footnotes section and the markers in the
/// verses, as it keeps the Cross References section; the selection card,
/// which shows the selected verses' footnotes on screen, isn't printed.
#[test]
fn kjv_footnotes_print_with_the_chapter() {
    let block = block_after(HTML, "@media print");
    for selector in [
        ".footnote-summary",
        ".footnote-row",
        ".fn-marker",
        ".supplied",
    ] {
        assert!(
            !block.contains(selector),
            "the print stylesheet mustn't touch `{selector}`"
        );
    }
    assert!(block.contains(".selection-card"));
    assert!(!block.contains(".xref-summary"));
}

/// The chapter's Cross References and Footnotes lists are two columns: the
/// verse labels in one, and each row's citations or footnote wrapping in
/// the other, beside its label (a hanging indent), never under it. Both
/// lists share one rule, so they line up alike, and each row takes its
/// list's columns, so every label in a list is as wide as the widest.
/// Printing keeps the same layout.
#[test]
fn chapter_end_lists_wrap_beside_their_labels() {
    let rules = rules();
    // Every screen declaration for `selector`, from each rule that names it.
    let styles = |selector: &str| -> String {
        rules
            .iter()
            .filter(|r| r.at_rule.is_none() && selectors(r).any(|s| s == selector))
            .map(|r| r.body.as_str())
            .collect()
    };
    let shared = |a: &str, b: &str| {
        rules
            .iter()
            .any(|r| r.at_rule.is_none() && selectors(r).eq([a, b]))
    };
    assert!(shared(".xref-summary-list", ".footnote-list"));
    assert!(shared(".xref-summary-row", ".footnote-row"));
    for (list, row, label, content) in [
        (
            ".xref-summary-list",
            ".xref-summary-row",
            ".xref-summary-verse",
            ".xref-summary-chips",
        ),
        (
            ".footnote-list",
            ".footnote-row",
            ".footnote-ref",
            ".footnote-text",
        ),
    ] {
        let list_styles = styles(list);
        for declaration in [
            "display: grid",
            "grid-template-columns: minmax(3.6rem, max-content) minmax(0, 1fr)",
        ] {
            assert!(list_styles.contains(declaration), "{list}: {declaration}");
        }
        let row_styles = styles(row);
        for declaration in [
            "display: grid",
            "grid-column: 1 / -1",
            "grid-template-columns: subgrid",
        ] {
            assert!(row_styles.contains(declaration), "{row}: {declaration}");
        }
        // Nothing may wrap a row's content onto a line of its own again.
        assert!(!row_styles.contains("flex"), "{row} isn't a flex row");
        assert!(!styles(label).contains("grid-column"), "{label}");
        assert!(styles(content).contains("min-width: 0"), "{content}");
    }
    let print = block_after(HTML, "@media print");
    for selector in [".xref-summary-list", ".xref-summary-row", ".footnote-list"] {
        assert!(
            !print.contains(selector),
            "print keeps `{selector}`'s layout"
        );
    }
}

/// Verse numbers, the bookmark and note marks after them, and KJV footnote
/// markers sit straight on the page, as plain text with no box behind them
/// in any theme. On Vaporwave's gradient no one color can be AA over every
/// stop (white peaks at 3.15:1 on its orange foot), so there they're a
/// brighter aqua with a crisp 1px outline in the theme's near-black purple
/// on all eight sides of each glyph, and the aqua is AA on that outline
/// wherever the page shows through. In the faded neighboring chapters the
/// outline is lighter, so their numbers don't look heavier than their
/// text. Every other theme keeps exactly its accent, with no outline.
#[test]
fn verse_numbers_and_footnote_markers_are_legible_on_every_theme() {
    let rules = rules();
    for selector in [".verse-num", ".fn-marker"] {
        let body = &screen_rule(&rules, selector).body;
        for declaration in [
            "color: var(--verse-num-color)",
            "text-shadow: var(--verse-num-outline)",
        ] {
            assert!(body.contains(declaration), "{selector}: {declaration}");
        }
        for boxed in ["background", "box-shadow", "border-radius", "padding"] {
            assert!(!body.contains(boxed), "{selector} has no box: {boxed}");
        }
        assert!(!body.contains("var(--accent)"), "{selector}");
        assert!(body.contains("font-weight: normal"), "{selector} is quiet");
    }
    assert!(
        screen_rule(&rules, ".chapter-side .verse-num")
            .body
            .contains("text-shadow: var(--verse-num-outline-faded)")
    );
    for mark in [
        ".chapter-main .verse.is-bookmarked .verse-num::after",
        ".chapter-main .verse.has-notes .verse-num::after",
    ] {
        let body = &screen_rule(&rules, mark).body;
        assert!(
            body.contains("color: var(--verse-num-color)"),
            "{mark} matches its verse number"
        );
        // It inherits the number's outline.
        assert!(!body.contains("text-shadow"), "{mark}");
    }

    for (theme, props) in theme_palettes(&rules) {
        let color = property(&props, "--verse-num-color");
        let outline = shadows(property(&props, "--verse-num-outline"));
        let faded = shadows(property(&props, "--verse-num-outline-faded"));
        if theme != "vaporwave" {
            assert_eq!(color, "var(--accent-2)", "theme `{theme}`");
            assert_eq!(outline, ["none"], "theme `{theme}`");
            assert_eq!(faded, ["none"], "theme `{theme}`");
            continue;
        }
        // Still the theme's aqua family, so markers read as accent.
        assert_eq!(color, "#5ce1e6");
        // A crisp 1px outline on all eight sides, no blur, in the bar's
        // near-black purple.
        let ink = property(&props, "--bar-bg");
        assert_eq!(ink, "#14082c");
        let mut sides: Vec<String> = Vec::new();
        for shadow in &outline {
            let parts: Vec<&str> = shadow.split_whitespace().collect();
            let [x, y, blur, shade] = parts[..] else {
                panic!("vaporwave: expected `x y blur color`, got `{shadow}`")
            };
            assert_eq!(blur, "0", "vaporwave: `{shadow}` is crisp");
            assert_eq!(shade, ink, "vaporwave: `{shadow}`");
            sides.push(format!("{x} {y}"));
        }
        sides.sort();
        let mut every_side: Vec<String> = ["-1px", "0", "1px"]
            .iter()
            .flat_map(|x| ["-1px", "0", "1px"].map(|y| format!("{x} {y}")))
            .filter(|side| side != "0 0")
            .collect();
        every_side.sort();
        assert_eq!(sides, every_side);
        let ratio = contrast_ratio(color, ink);
        assert!(
            ratio >= 4.5,
            "vaporwave: verse numbers on their outline are {ratio:.2}:1, below 4.5:1"
        );
        // The faded neighbors' outline: the same purple, see-through, on
        // four sides.
        assert_eq!(faded.len(), 4);
        for shadow in &faded {
            let (offsets, shade) = shadow
                .split_once("rgba(")
                .unwrap_or_else(|| panic!("vaporwave: `{shadow}` is see-through"));
            assert!(offsets.trim().ends_with(" 0"), "`{shadow}` is crisp");
            let shade = format!("rgba({shade}");
            assert!(shade.starts_with("rgba(20, 8, 44, "), "{shade}");
            assert_eq!(
                composite(&shade, ink),
                ink,
                "{shade} is the outline's purple"
            );
            assert_ne!(composite(&shade, "#ffffff"), ink, "{shade} is see-through");
        }
    }

    // Printed in black, with no outline, like the other accents.
    let print = block_after(HTML, "@media print");
    for declaration in [
        "--verse-num-color: #000000 !important",
        "--verse-num-outline: none !important",
        "--verse-num-outline-faded: none !important",
    ] {
        assert!(print.contains(declaration), "{declaration}");
    }
}

/// A `text-shadow` value's shadows, split on its top-level commas, with
/// runs of whitespace collapsed.
fn shadows(value: &str) -> Vec<String> {
    let mut list = vec![String::new()];
    let mut depth = 0;
    for c in value.chars() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                list.push(String::new());
                continue;
            }
            _ => {}
        }
        list.last_mut().unwrap().push(c);
    }
    list.iter()
        .map(|s| s.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect()
}

// ---- The Tier 1 look: typefaces, brand themes, quieter chrome ---------

/// The display colors a palette's background tokens resolve to: a flat
/// `--bg` itself, or every stop of a gradient one.
fn backgrounds(props: &[(String, String)]) -> Vec<String> {
    property(props, "--bg")
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '#')
        .filter(|word| word.starts_with('#'))
        .map(str::to_owned)
        .collect()
}

/// New installs, and everyone who never chose a theme, get Chi Rho: it's
/// the preferences script's fallback, so with nothing saved no
/// `data-theme` is set, and its palette is the bare `:root` block. Anyone
/// with a saved theme keeps it, Vaporwave included, which now has a
/// `data-theme` block of its own. The picker lists Chi Rho, Vellum and
/// System first, then the other six as before.
#[test]
fn chi_rho_is_the_default_theme() {
    assert_eq!(default_theme(), "chi-rho");
    assert_eq!(
        select_options("theme-select"),
        [
            ("chi-rho", "Chi Rho"),
            ("vellum", "Vellum"),
            ("system", "System"),
            ("vaporwave", "Vaporwave"),
            ("classic-dark", "Classic Dark"),
            ("classic-light", "Classic Light"),
            ("matrix", "Matrix"),
            ("beast-slayer", "Beast Slayer"),
            ("hot-pink", "Hot Pink"),
        ]
        .map(|(v, l)| (v.to_owned(), l.to_owned()))
    );
    let rules = rules();
    let root = theme_rule(&rules, "chi-rho", "chi-rho");
    assert!(selectors(root).any(|s| s == ":root"));
    assert!(selectors(root).any(|s| s == r#"[data-theme-preview="chi-rho"]"#));
    theme_rule(&rules, "vaporwave", "chi-rho");
    assert!(HTML.contains("{ 'monster-hunter': 'beast-slayer' }"));
}

/// Chi Rho and Vellum are the website's palette (site/styles.css) and its
/// light twin: flat grounds, no gradient, and every text color, both
/// accents included, AA on the page, the bar, the popups and the panels.
#[test]
fn chi_rho_and_vellum_use_the_brand_palette() {
    let rules = rules();
    let palettes = theme_palettes(&rules);
    let palette = |theme: &str| &palettes.iter().find(|(t, _)| t == theme).unwrap().1;
    for (theme, expected) in [
        (
            "chi-rho",
            [
                ("--bg", "#0b0d0c"),
                ("--bar-bg", "#101613"),
                ("--option-bg", "#131c18"),
                ("--text", "#ede7d8"),
                ("--text-dim", "#bdb6a5"),
                ("--accent", "#f2c14e"),
                ("--accent-2", "#86dccd"),
            ],
        ),
        (
            "vellum",
            [
                ("--bg", "#f5efe3"),
                ("--bar-bg", "#fbf7ef"),
                ("--option-bg", "#ffffff"),
                ("--text", "#1b1a17"),
                ("--text-dim", "#6a655a"),
                ("--accent", "#16785a"),
                ("--accent-2", "#1f6f63"),
            ],
        ),
    ] {
        let props = palette(theme);
        for (name, value) in expected {
            assert_eq!(property(props, name), value, "{theme} {name}");
        }
        assert_eq!(property(props, "--glow"), "none", "{theme}");
        let bg = property(props, "--bg");
        let bar = property(props, "--bar-bg");
        let option = property(props, "--option-bg");
        let panel = property(props, "--panel-bg");
        for fg in ["--text", "--text-dim", "--accent", "--accent-2"] {
            for (on, behind) in [
                ("the page", bg.to_owned()),
                ("the bar", bar.to_owned()),
                ("a popup", option.to_owned()),
                ("a panel on the page", composite(panel, bg)),
                ("a panel in a popup", composite(panel, option)),
            ] {
                let ratio = contrast_ratio(property(props, fg), &behind);
                assert!(
                    ratio >= 4.5,
                    "{theme}: {fg} on {on} is {ratio:.2}:1, below 4.5:1"
                );
            }
        }
    }
}

/// System is Chi Rho in a dark OS and Vellum in a light one, through
/// `prefers-color-scheme`, so it follows the OS live, with no script: its
/// dark palette is Chi Rho's own block, and its light one an exact copy
/// of Vellum's. Its swatch shows both grounds.
#[test]
fn system_follows_the_os_between_chi_rho_and_vellum() {
    let rules = rules();
    let chi_rho = theme_rule(&rules, "chi-rho", "chi-rho");
    for selector in [
        r#":root[data-theme="system"]"#,
        r#"[data-theme-preview="system"]"#,
    ] {
        assert!(selectors(chi_rho).any(|s| s == selector), "{selector}");
    }
    let vellum = theme_rule(&rules, "vellum", "chi-rho");
    let light = system_light_rule(&rules);
    assert_eq!(
        custom_properties(&light.body),
        custom_properties(&vellum.body)
    );
    assert!(light.body.contains("color-scheme: light"));
    let swatch = screen_rule(
        &rules,
        r#".theme-swatch-preview[data-theme-preview="system"]"#,
    );
    let grounds = [
        property(&custom_properties(&chi_rho.body), "--bg").to_owned(),
        property(&custom_properties(&vellum.body), "--bg").to_owned(),
    ];
    assert!(
        swatch
            .body
            .contains(&format!("{} 50%, {} 50%", grounds[0], grounds[1])),
        "{}",
        swatch.body
    );
}

/// The quiet accent: verse numbers, rules, links, footnote markers and
/// cross-references use `--accent-2`, AA wherever they're drawn; on the
/// six older themes it starts as their `--accent`.
#[test]
fn the_quiet_accent_carries_the_quiet_jobs() {
    let rules = rules();
    for selector in [
        ".xref-citation",
        ".about-links a",
        ".xref-credit a",
        ".note-link",
        ".footnote-ref",
        ".footnote-label",
        ".xref-summary-verse",
    ] {
        assert!(
            screen_rule(&rules, selector)
                .body
                .contains("color: var(--accent-2)"),
            "{selector}"
        );
    }
    let root = custom_properties(&screen_rule(&rules, ":root").body);
    assert_eq!(
        property(&root, "--rule"),
        "color-mix(in srgb, var(--accent-2) 50%, transparent)"
    );
    for selector in [".chapter-heading", ".chapter-heading-link"] {
        assert!(
            screen_rule(&rules, selector)
                .body
                .contains("border-bottom: 1px solid var(--rule)"),
            "{selector}'s rule is a hairline in --accent-2"
        );
    }
    for (theme, props) in theme_palettes(&rules) {
        let accent_2 = property(&props, "--accent-2");
        if [
            "vaporwave",
            "classic-dark",
            "classic-light",
            "matrix",
            "beast-slayer",
            "hot-pink",
        ]
        .contains(&theme.as_str())
        {
            assert_eq!(accent_2, property(&props, "--accent"), "{theme}");
        }
        let option = property(&props, "--option-bg");
        let bar = property(&props, "--bar-bg");
        let mut behind = vec![
            option.to_owned(),
            bar.to_owned(),
            composite(property(&props, "--panel-bg"), option),
        ];
        let page = backgrounds(&props);
        if page.len() == 1 {
            behind.push(page[0].clone());
        }
        for bg in behind {
            let ratio = contrast_ratio(accent_2, &bg);
            assert!(
                ratio >= 4.5,
                "theme `{theme}`: --accent-2 on {bg} is {ratio:.2}:1, below 4.5:1"
            );
        }
        // On a flat page the verse numbers are --accent-2 on it.
        if page.len() == 1 && theme != "vaporwave" {
            let ratio = contrast_ratio(accent_2, &page[0]);
            assert!(ratio >= 4.5, "theme `{theme}`: verse numbers {ratio:.2}:1");
        }
    }
}

/// Neon only where it's the point: `--glow` is none except on Vaporwave,
/// Matrix and Hot Pink, and it's the only glow there is, on the chapter
/// headings (the masthead it also lit is gone, and the picker marks the
/// chosen book and chapter without it).
#[test]
fn glow_is_only_on_the_neon_themes() {
    let rules = rules();
    for (theme, props) in theme_palettes(&rules) {
        let glow = property(&props, "--glow");
        if ["vaporwave", "matrix", "hot-pink"].contains(&theme.as_str()) {
            assert!(glow.starts_with("0 0 14px rgba("), "{theme}: {glow}");
            let accent_rgb = property(&props, "--accent-rgb");
            assert!(glow.contains(accent_rgb), "{theme}: {glow}");
        } else {
            assert_eq!(glow, "none", "{theme}");
        }
    }
    assert!(
        screen_rule(&rules, ".chapter-heading")
            .body
            .contains("text-shadow: var(--glow)")
    );
    let lit: Vec<&str> = rules
        .iter()
        .filter(|r| r.at_rule.as_deref() != Some("@media print"))
        .filter(|r| {
            r.body
                .split(';')
                .any(|d| !d.trim().starts_with("--") && d.contains("var(--glow)"))
        })
        .map(|r| r.selector.as_str())
        .collect();
    assert_eq!(lit, [".chapter-heading"], "only chapter headings glow");
    for rule in rules
        .iter()
        .filter(|r| r.at_rule.as_deref() != Some("@media print"))
    {
        for decl in rule.body.split(';').map(str::trim) {
            if decl.starts_with("text-shadow:") {
                assert!(
                    decl == "text-shadow: var(--glow)" || decl.contains("var(--verse-num-outline"),
                    "`{}` has a glow of its own: {decl}",
                    rule.selector
                );
            }
        }
    }
}

/// The side chapters are faded, not blurred: nothing is blurred, and they
/// sit at 0.55, lifting to 0.95 under the pointer. Vaporwave's faded
/// outline for their verse numbers still applies.
#[test]
fn side_chapters_fade_without_blur() {
    let rules = rules();
    assert!(!stylesheet().contains("blur("));
    let side = screen_rule(&rules, ".chapter-side");
    assert!(side.body.contains("opacity: 0.55"));
    assert!(!side.body.contains("filter"));
    assert!(
        screen_rule(&rules, ".chapter-side:hover")
            .body
            .contains("opacity: 0.95")
    );
    assert!(
        screen_rule(&rules, ".chapter-side .verse-num")
            .body
            .contains("text-shadow: var(--verse-num-outline-faded)")
    );
}

/// Verse numbers are findable, not read: regular weight, smaller than
/// before (0.7rem), tabular figures, still a superscript.
#[test]
fn verse_numbers_are_quiet() {
    let rules = rules();
    let num = &screen_rule(&rules, ".verse-num").body;
    for declaration in [
        "font-weight: normal",
        "font-variant-numeric: tabular-nums",
        "vertical-align: super",
        "font-size: calc(0.64rem * var(--reading-scale))",
    ] {
        assert!(num.contains(declaration), "{declaration}");
    }
    let marker = &screen_rule(&rules, ".fn-marker").body;
    assert!(marker.contains("font-weight: normal"));
    assert!(marker.contains("vertical-align: super"));
}

/// On Vaporwave's gradient no plain color reaches AA over every stop, so
/// its verse numbers keep their outline (see
/// `verse_numbers_and_footnote_markers_are_legible_on_every_theme`). This
/// checks every luminance a color can have, not just a few candidates.
#[test]
fn no_plain_color_is_aa_over_all_of_vaporwave() {
    let rules = rules();
    let props = custom_properties(&theme_rule(&rules, "vaporwave", "chi-rho").body);
    let stops: Vec<f64> = backgrounds(&props)
        .iter()
        .map(|s| relative_luminance(s))
        .collect();
    assert_eq!(stops.len(), 5);
    let best = (0..=10_000)
        .map(|i| f64::from(i) / 10_000.0)
        .map(|l| {
            stops
                .iter()
                .map(|&s| (l.max(s) + 0.05) / (l.min(s) + 0.05))
                .fold(f64::INFINITY, f64::min)
        })
        .fold(0.0, f64::max);
    assert!(
        best < 4.5,
        "a plain color reaches {best:.2}:1 over every stop; the outline could go"
    );
    assert!((3.1..3.2).contains(&best), "{best:.3}");
    assert_ne!(property(&props, "--verse-num-outline"), "none");
}

/// One radius scale: small, medium and large, 4, 8 and 12px, and every
/// corner in the stylesheet uses it (round things excepted).
#[test]
fn one_radius_scale() {
    let rules = rules();
    let root = custom_properties(&screen_rule(&rules, ":root").body);
    assert_eq!(property(&root, "--radius-s"), "4px");
    assert_eq!(property(&root, "--radius-m"), "8px");
    assert_eq!(property(&root, "--radius-l"), "12px");
    for rule in &rules {
        for value in rule
            .body
            .split(';')
            .filter_map(|d| d.trim().strip_prefix("border-radius:"))
            .map(str::trim)
        {
            let ok = ["50%", "0"].contains(&value)
                || value
                    .split_whitespace()
                    .all(|v| v == "0" || v.starts_with("var(--radius-"));
            assert!(ok, "`{}`: border-radius: {value}", rule.selector);
        }
    }
}

/// The bar's buttons are ghost icons at least 36px square, and keep their
/// focus ring.
#[test]
fn bar_buttons_are_ghost_icons_with_room_to_click() {
    let rules = rules();
    let body = &screen_rule(&rules, ".bar-btn").body;
    let rem = |name: &str| -> f64 {
        body.split(';')
            .find_map(|d| d.trim().strip_prefix(name))
            .and_then(|v| v.trim().strip_suffix("rem"))
            .and_then(|v| v.parse().ok())
            .unwrap_or_else(|| panic!("a {name} in rem"))
    };
    assert!(rem("width:") * 16.0 >= 36.0);
    assert!(rem("height:") * 16.0 >= 36.0);
    let focus = rules
        .iter()
        .find(|r| r.at_rule.is_none() && selectors(r).any(|s| s == ".bar-btn:focus-visible"))
        .expect("a focus ring on the bar buttons");
    assert!(focus.body.contains("outline: 2px solid var(--accent)"));
}

/// Literata is bundled in ui/fonts with its licence: regular, true italic
/// and bold, as WOFF2, and the `@font-face` rules point at exactly those
/// files. Scripture and chapter headings use it; everything else uses the
/// system UI sans (the website's `--sans`).
#[test]
fn literata_is_bundled_for_scripture_and_the_ui_uses_the_sans() {
    let fonts = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../ui/fonts");
    let licence = std::fs::read_to_string(fonts.join("OFL.txt")).expect("ui/fonts/OFL.txt");
    assert!(licence.contains("SIL Open Font License, Version 1.1"));
    assert!(licence.contains("The Literata Project Authors"));

    let css = stylesheet();
    let faces: Vec<&str> = css
        .match_indices("@font-face")
        .map(|(at, _)| {
            let open = at + css[at..].find('{').unwrap();
            &css[open..matching_brace_end(&css, open)]
        })
        .collect();
    assert_eq!(faces.len(), 3);
    let mut seen = Vec::new();
    for face in faces {
        assert!(face.contains("font-family: 'Literata'"), "{face}");
        assert!(face.contains("font-display: block"), "{face}");
        let file = face
            .split_once("url('fonts/")
            .and_then(|(_, rest)| rest.split_once("')"))
            .map(|(file, _)| file)
            .expect("a src in ui/fonts");
        assert!(face.contains("format('woff2')"));
        let bytes = std::fs::read(fonts.join(file)).unwrap_or_else(|_| panic!("ui/fonts/{file}"));
        assert_eq!(&bytes[..4], b"wOF2", "{file} is WOFF2");
        let style = if face.contains("font-style: italic") {
            "italic"
        } else {
            "normal"
        };
        let weight = face
            .split_once("font-weight: ")
            .map(|(_, rest)| &rest[..3])
            .unwrap();
        seen.push(format!("{file} {style} {weight}"));
    }
    seen.sort();
    assert_eq!(
        seen,
        [
            "literata-bold.woff2 normal 700",
            "literata-italic.woff2 italic 400",
            "literata-regular.woff2 normal 400",
        ]
    );

    let rules = rules();
    let root = custom_properties(&screen_rule(&rules, ":root").body);
    assert!(property(&root, "--font-scripture").starts_with("'Literata', "));
    assert!(property(&root, "--font-ui").starts_with("system-ui, "));
    assert!(
        screen_rule(&rules, "body")
            .body
            .contains("font-family: var(--font-ui)")
    );
    for selector in [
        ".verse",
        ".verse-num",
        ".chapter-heading",
        ".chapter-heading-link",
        ".note-passage",
        ".theme-swatch-preview",
    ] {
        assert!(
            screen_rule(&rules, selector)
                .body
                .contains("font-family: var(--font-scripture)"),
            "{selector} is scripture"
        );
    }
    // The selection card, its actions and notes sit over the verses, but
    // they're controls.
    for selector in [".verse-actions", ".verse-notes", ".selection-card"] {
        assert!(
            screen_rule(&rules, selector)
                .body
                .contains("font-family: var(--font-ui)"),
            "{selector} is UI"
        );
    }
    // No other face is named anywhere.
    let named: Vec<&str> = rules
        .iter()
        .flat_map(|r| r.body.split(';'))
        .filter_map(|d| d.trim().strip_prefix("font-family:"))
        .map(str::trim)
        .filter(|v| !["inherit", "var(--font-ui)", "var(--font-scripture)"].contains(v))
        .collect();
    assert_eq!(named, Vec::<&str>::new());
    // The supplied words are the real italic, never a synthesized one.
    assert!(
        screen_rule(&rules, ".supplied")
            .body
            .contains("font-style: italic")
    );
    assert!(!css.contains("font-synthesis"));
}

// ---- Tier 1 structure: text first, the picker, the reference in the bar

/// The masthead is gone: no `<header>` with the name, "The Holy Bible" and
/// Random chapter, and none of their styles. The page's only `<h1>` names
/// the app for screen readers; the chapter heading is the first thing on
/// screen under the bar.
#[test]
fn there_is_no_masthead() {
    let rules = rules();
    assert!(!HTML.contains("<header"), "the masthead is gone");
    assert!(!HTML.contains("The Holy Bible"));
    for gone in [
        "header",
        "h1",
        ".subtitle",
        ".random-chapter-btn",
        ".book-list",
        "#chapter-list",
    ] {
        assert!(
            !rules.iter().any(|r| selectors(r).any(|s| s == gone)),
            "`{gone}` is still styled"
        );
    }
    let h1 = HTML.find("<h1").expect("an <h1> for screen readers");
    let tag = &HTML[h1..h1 + HTML[h1..].find('>').unwrap()];
    assert!(tag.contains(r#"class="visually-hidden""#), "{tag}");
    assert_eq!(HTML.matches("<h1").count(), 1);
    // Between the top of the page and the chapter: only that heading and
    // the notice line (hidden unless there's something to say).
    let main = HTML.find(r#"<main class="container">"#).expect("<main>");
    let pane = HTML.find(r#"<div id="reading-pane">"#).unwrap();
    let before: Vec<&str> = HTML[main..pane]
        .lines()
        .skip(1)
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    assert_eq!(before.len(), 2, "{before:?}");
    assert!(before[0].starts_with(r#"<h1 class="visually-hidden">"#));
    assert!(before[1].starts_with(r#"<p id="reader-notice""#) && before[1].contains(" hidden>"));
}

/// The reference button sits at the left of the bar (after only the
/// sidebar's own button), in the UI sans, and opens the picker. Its
/// accessible name says where the reader is, in full, once a chapter is
/// read; `G` is its shortcut.
#[test]
fn the_reference_button_says_where_the_reader_is() {
    let tag = tag_of("ref-btn");
    for attr in [
        r#"type="button""#,
        r#"aria-label="Choose a book and chapter""#,
        r#"aria-keyshortcuts="G""#,
        r#"aria-controls="picker""#,
        r#"aria-expanded="false""#,
        r#"title="Choose a book and chapter (G)""#,
    ] {
        assert!(tag.contains(attr), "#ref-btn: {attr}");
    }
    let bar = &HTML[HTML.find(r#"<div id="top-bar""#).unwrap()..];
    let order: Vec<usize> = [
        r#"id="sidebar-toggle""#,
        r#"id="ref-btn""#,
        r#"id="reader-exit""#,
        r#"id="translation-toggle""#,
        r#"id="aa-toggle""#,
    ]
    .iter()
    .map(|id| bar.find(id).unwrap())
    .collect();
    assert!(order.windows(2).all(|w| w[0] < w[1]), "{order:?}");
    let rules = rules();
    let button = &screen_rule(&rules, ".ref-btn").body;
    for declaration in [
        "font-family: var(--font-ui)",
        "margin-right: auto",
        "color: var(--text)",
        "background: none",
    ] {
        assert!(button.contains(declaration), ".ref-btn: {declaration}");
    }
    // What it says is the reader's (ui-leptos/src/reader_text.rs).
    let reference = block_after(piece("reader_text.rs"), "pub fn reference(");
    assert!(reference.contains(r#"let text = format!("{place} · {}", upper.join(" + "));"#));
    assert!(
        reference.contains(r#"label: format!("{place}, {spoken}. Choose a book and chapter"),"#)
    );
    // Updated whenever a chapter is put on screen.
    let render = block_after(piece("reader.rs"), "fn render_current()");
    assert!(render.contains("update_reference(&here, compare.as_deref());"));
    assert!(render.contains("bridge::show_picker_current(here.book_id, here.chapter);"));
    // G opens the picker, in the keyboard dispatcher, which leaves keys
    // typed into a form control alone.
    assert!(
        piece("keyboard_text.rs").contains(r#"if plain && !shift && (key == "g" || key == "G") {"#)
    );
}

/// The picker is one `<nav>` landmark, "Books and chapters": Random
/// chapter, then each testament's books under its heading. As a popover
/// it's a dialog (the surfaces set the role, ui-leptos/src/surfaces.rs),
/// and both the reference button and the sidebar's button control it.
#[test]
fn the_picker_is_a_navigation_landmark_or_a_dialog() {
    let tag = tag_of("picker");
    assert!(tag.starts_with("<nav"), "{tag}");
    assert!(tag.contains(r#"aria-label="Books and chapters""#));
    assert!(tag.contains(" hidden"));
    let nav = &HTML[HTML.find(r#"<nav id="picker""#).unwrap()..];
    let nav = &nav[..nav.find("</nav>").unwrap()];
    for part in [
        r#"<button type="button" id="random-chapter-btn" class="picker-random">"#,
        r#"<h2 id="picker-ot-title" class="picker-testament">Old Testament</h2>"#,
        r#"<ul id="ot-book-list" class="picker-books" aria-labelledby="picker-ot-title"></ul>"#,
        r#"<h2 id="picker-nt-title" class="picker-testament">New Testament</h2>"#,
        r#"<ul id="nt-book-list" class="picker-books" aria-labelledby="picker-nt-title"></ul>"#,
    ] {
        assert!(nav.contains(part), "the picker should have {part}");
    }
    let toggle = tag_of("sidebar-toggle");
    for attr in [
        r#"aria-controls="picker""#,
        r#"aria-expanded="false""#,
        r#"aria-label="Books and chapters""#,
        r#"class="bar-btn sidebar-toggle""#,
    ] {
        assert!(toggle.contains(attr), "#sidebar-toggle: {attr}");
    }
    let head = &HTML[..HTML.find("</head>").unwrap()];
    let surfaces = [piece("surfaces.rs"), piece("surfaces_text.rs")].concat();
    for wiring in [
        r#"picker.set_attribute("role", "dialog");"#,
        r#"picker.remove_attribute("role");"#,
        r#"ref_btn.set_attribute("aria-haspopup", "dialog");"#,
        r#"Surface::Picker => Some("ref-btn"),"#,
        "['gospel-getter-sidebar', 'sidebar', 'open', ['open', 'closed'], {}],",
    ] {
        assert!(
            head.contains(wiring) || surfaces.contains(wiring),
            "the pre-paint script or the surfaces: {wiring}"
        );
    }
    // A book is a disclosure for its chapters, and the chapter and book
    // being read are marked (the picker's piece, ui-leptos/src/picker.rs,
    // draws and marks them).
    let picker = piece("picker.rs");
    assert!(picker.contains(
        "class=\"book-item\"\n                data-book-id=book.id.to_string()\n                data-chapter-count=book.chapter_count.to_string()\n                aria-expanded=\"false\""
    ));
    assert!(
        picker.contains(r#"button.set_attribute("aria-controls", CHAPTERS_ID);"#)
            && picker.contains(r#"const CHAPTERS_ID: &str = "picker-chapters";"#)
    );
    assert!(picker.contains(r#"role="group""#));
    assert!(picker.contains(r#"book.set_attribute("aria-current", "true");"#));
    assert!(picker.contains(r#"aria-current=(current == Some(n)).then_some("page")"#));
}

/// Psalms (and any book over 50 chapters) gets a compact numeric grid, ten
/// to a row; the rest get round pills, six to a row. The chapter and book
/// being read are marked in `--accent`, with no glow.
#[test]
fn psalms_get_a_compact_grid_ten_to_a_row() {
    let rules = rules();
    // The rule is the picker's (ui-leptos/src/picker_text.rs).
    let picker = piece("picker_text.rs");
    assert!(picker.contains("pub const COMPACT_OVER: i64 = 50;"));
    assert!(picker.contains("pub const COMPACT_COLUMNS: usize = 10;"));
    assert!(picker.contains("pub const PILL_COLUMNS: usize = 6;"));
    assert!(picker.contains("    count > COMPACT_OVER\n"));
    let pills = &screen_rule(&rules, ".chapter-grid").body;
    assert!(pills.contains("grid-template-columns: repeat(6, 2rem)"));
    assert!(
        screen_rule(&rules, ".chapter-btn")
            .body
            .contains("border-radius: 50%")
    );
    let compact = &screen_rule(&rules, ".chapter-grid.is-compact").body;
    assert!(compact.contains("grid-template-columns: repeat(10, minmax(0, 1fr))"));
    assert!(compact.contains("gap: 0.15rem 1px"));
    assert!(compact.contains("margin: 0 -0.35rem"));
    // Room for "150" (21px of digits at 0.72rem, measured in WebKitGTK):
    // the picker's width, less its border and side padding, plus the
    // grid's reach into that padding, shared by ten cells a hairline
    // apart. 27px in the sidebar, 30.1px in the popover, and over 24px
    // even with a 17px Windows scrollbar.
    let picker = &screen_rule(&rules, ".picker:not([hidden])").body;
    assert!(picker.contains("padding: 0.5rem 0.6rem 0.9rem"));
    assert!(picker.contains("width: min(20rem, calc(100vw - 1rem))"));
    let html = custom_properties(&screen_rule(&rules, "html").body);
    let sidebar = property(&html, "--sidebar-width")
        .strip_suffix("rem")
        .unwrap()
        .parse::<f64>()
        .unwrap()
        * 16.0;
    let cell = |outer: f64, border: f64, scrollbar: f64| {
        (outer - border - scrollbar - 2.0 * 0.6 * 16.0 + 2.0 * 0.35 * 16.0 - 9.0) / 10.0
    };
    let digits = 21.0;
    for (place, width) in [
        ("sidebar", cell(sidebar, 1.0, 0.0)),
        ("popover", cell(20.0 * 16.0, 2.0, 0.0)),
        ("sidebar, Windows scrollbar", cell(sidebar, 1.0, 17.0)),
        ("popover, Windows scrollbar", cell(20.0 * 16.0, 2.0, 17.0)),
    ] {
        assert!(width >= 24.0, "{place}: a {width:.1}px cell");
        if !place.contains("Windows") {
            assert!(
                (width - digits) / 2.0 >= 3.0,
                "{place}: {:.1}px either side of 150",
                (width - digits) / 2.0
            );
        }
    }
    assert_eq!(cell(sidebar, 1.0, 0.0), 27.0);
    // The focus ring stays inside the cell.
    assert!(
        screen_rule(
            &rules,
            ".chapter-grid.is-compact .chapter-btn:focus-visible"
        )
        .body
        .contains("outline-offset: -2px")
    );
    let cell = &screen_rule(&rules, ".chapter-grid.is-compact .chapter-btn").body;
    assert!(cell.contains("border-radius: var(--radius-s)"));
    assert!(cell.contains("border: none"));
    let current = &screen_rule(&rules, r#".chapter-btn[aria-current="page"]"#).body;
    assert!(current.contains("background: var(--accent)"));
    assert!(current.contains("color: var(--on-accent)"));
    assert!(!current.contains("glow"));
    let book = &screen_rule(&rules, r#".book-item[aria-current="true"]"#).body;
    assert!(book.contains("color: var(--accent)"));
}

/// The sidebar: from 1200px, 18rem down the left below the bar, in the
/// bar's `--bar-bg`, with the page moved over (on screen only); the
/// neighbors give way first, as on any narrower page. Reading mode and
/// print never show it, and its own button only shows where it can.
#[test]
fn the_sidebar_is_for_wide_windows_only() {
    let rules = rules();
    assert!(piece("surfaces.rs").contains("w.match_media(preferences::WIDE_QUERY)"));
    let html = custom_properties(&screen_rule(&rules, "html").body);
    assert_eq!(property(&html, "--sidebar-width"), "18rem");
    let screen = |selector: &str| -> &Rule {
        rules
            .iter()
            .find(|r| r.at_rule.as_deref() == Some("@media screen") && r.selector == selector)
            .unwrap_or_else(|| panic!("expected a screen-only `{selector}` rule"))
    };
    assert!(
        screen(r#":root[data-picker="sidebar"] body"#)
            .body
            .contains("padding-left: var(--sidebar-width)")
    );
    let sidebar = &screen(r#":root[data-picker="sidebar"] .picker:not([hidden])"#).body;
    for declaration in [
        "top: var(--bar-height)",
        "left: 0",
        "bottom: 0",
        "width: var(--sidebar-width)",
        "background: var(--bar-bg)",
    ] {
        assert!(sidebar.contains(declaration), "the sidebar: {declaration}");
    }
    assert!(
        !rules
            .iter()
            .any(|r| r.selector.contains(r#"[data-picker="sidebar"] .chapter-"#)),
        "the column and its neighbors share one set of rules, sidebar or not"
    );
    assert!(
        screen_rule(&rules, ":root:not([data-wide]) .sidebar-toggle")
            .body
            .contains("display: none")
    );
    let popover = &screen_rule(&rules, ".picker:not([hidden])").body;
    for declaration in [
        "background: var(--option-bg)",
        "color: var(--text)",
        "font-family: var(--font-ui)",
        "overflow-y: auto",
        "max-height: calc(100vh - var(--bar-height) - 1rem)",
    ] {
        assert!(popover.contains(declaration), "the popover: {declaration}");
    }
    // Reading mode never shows the sidebar (see sidebar_shown).
    assert!(
        piece("surfaces_text.rs")
            .contains(r#"wide && sidebar != Some("closed") && reader_mode != Some("on")"#)
    );
    // It's a setting for this window, not exported with the display ones.
    assert_eq!(preferences::SIDEBAR.file_name, None);
}

/// The picker's colors are pairs AA in every theme, on the sidebar's
/// `--bar-bg` and the popover's `--option-bg` alike: books and Random
/// chapter in `--text` (on a wash under the pointer), testament headings in
/// `--text-dim`, the book being read in `--accent`, and chapters in
/// `--text` on `--panel-bg` (`--on-accent` on `--accent` for the one being
/// read, tested with the other controls).
#[test]
fn the_picker_uses_tested_theme_colors() {
    let rules = rules();
    let over = |color: &str, under: &str| -> String {
        if color.starts_with("rgba(") {
            composite(color, under)
        } else {
            color.to_owned()
        }
    };
    for (theme, props) in theme_palettes(&rules) {
        let accent_rgb = property(&props, "--accent-rgb");
        for surface in ["--bar-bg", "--option-bg"] {
            let bg = property(&props, surface);
            let wash = composite(&format!("rgba({accent_rgb}, 0.12)"), bg);
            let panel = over(property(&props, "--panel-bg"), bg);
            for (what, fg, under) in [
                ("a book", property(&props, "--text"), bg.to_owned()),
                ("a book under the pointer", property(&props, "--text"), wash),
                (
                    "a testament heading",
                    property(&props, "--text-dim"),
                    bg.to_owned(),
                ),
                (
                    "the book being read",
                    property(&props, "--accent"),
                    bg.to_owned(),
                ),
                ("a chapter", property(&props, "--text"), panel),
            ] {
                let ratio = contrast_ratio(fg, &under);
                assert!(
                    ratio >= 4.5,
                    "theme `{theme}` on {surface}: {what} is {ratio:.2}:1, below 4.5:1"
                );
            }
        }
    }
    for (selector, declaration) in [
        (".book-item", "color: var(--text)"),
        (".picker-random", "color: var(--text)"),
        (".picker-testament", "color: var(--text-dim)"),
        (".chapter-btn", "color: var(--text)"),
        (".chapter-btn", "background: var(--panel-bg)"),
    ] {
        assert!(
            screen_rule(&rules, selector).body.contains(declaration),
            "{selector}: {declaration}"
        );
    }
    let hover = rules
        .iter()
        .find(|r| r.at_rule.is_none() && selectors(r).any(|s| s == ".book-item:hover"))
        .expect("a hover wash on the books");
    assert!(
        hover
            .body
            .contains("background: rgba(var(--accent-rgb), 0.12)")
    );
    for control in [".ref-btn", ".book-item", ".chapter-btn", ".picker-random"] {
        let focus = format!("{control}:focus-visible");
        assert!(
            rules.iter().any(|r| r.at_rule.is_none()
                && selectors(r).any(|s| s == focus)
                && r.body.contains("outline: 2px solid var(--accent)")),
            "{focus} has the focus ring"
        );
    }
}

// ---- Tier 2: the reading flow ----------------------------------------

/// The chapter ends with a next step: the backend's previous and next
/// chapters (so books are crossed) as two large buttons in the UI sans,
/// straight after the verses (or the compare grid), in a labelled
/// navigation landmark; only the one there is at Genesis 1 and
/// Revelation 22. The keys' hint is one quiet line under them, inside the
/// chapter.
#[test]
fn the_chapter_ends_with_buttons_to_the_chapters_around_it() {
    // The chapter's markup is the reader's (ui-leptos/src/reader_text.rs).
    let reader = piece("reader_text.rs");
    let end = block_after(reader, "fn chapter_end(");
    assert!(
        end.contains(r#"<nav class=\"chapter-end\" aria-label=\"Previous and next chapter\">"#)
    );
    assert!(
        end.contains("let Some(side) = side else { return String::new();"),
        "one button per neighbour there is"
    );
    assert!(
        end.contains("step(dto.prev.as_ref(), Which::Prev), step(dto.next.as_ref(), Which::Next),")
    );
    assert!(end.contains(r#"class="chapter-step chapter-step-{}""#));
    assert!(end.contains(r#"data-book="{}" data-chapter="{}" aria-label="{label}""#));
    assert!(end.contains(r#"<p class=\"nav-hint\">"#));
    // Its three keys are caps alike; the arrows are drawn, and named.
    for key in [
        r#"hint_key("Left arrow", Some(ARROW_LEFT))"#,
        r#"hint_key("Right arrow", Some(ARROW_RIGHT))"#,
        r#"hint_key("?", None)"#,
    ] {
        assert!(end.contains(key), "{key}");
    }
    let caps = block_after(reader, "fn hint_key(");
    assert!(caps.contains(r#"<kbd><span class="visually-hidden">{name}</span>{drawn}</kbd>"#));
    assert!(
        reader.contains(
            r##"const ARROW_LEFT: &str = r#"<svg viewBox="0 0 16 16" aria-hidden="true""##
        )
    );
    let pane = block_after(reader, "pub fn reading_pane(");
    let body = pane.find("{s28}{body}").expect("the verses");
    let after = pane.find("{s28}{end}").expect("the buttons");
    let lists = pane.find("{s28}{footnotes}").unwrap();
    assert!(
        body < after && after < lists,
        "after the verses, before the lists"
    );
    let rules = rules();
    let step = screen_rule(&rules, ".chapter-step");
    assert!(step.body.contains("font-family: var(--font-ui)"));
    assert!(step.body.contains("min-height: 3rem"), "large");
    assert!(
        rules
            .iter()
            .any(|r| r.selector == ".chapter-step:focus-visible" && r.body.contains("outline")),
        "a visible focus ring"
    );
    // Reading mode keeps them (it hides only the hint), and print drops them.
    let reading_mode_hidden: Vec<&str> = rules
        .iter()
        .filter(|r| {
            r.at_rule.as_deref() == Some("@media screen") && r.body.contains("display: none")
        })
        .flat_map(selectors)
        .collect();
    assert!(reading_mode_hidden.contains(&r#":root[data-reader-mode="on"] .nav-hint"#));
    assert!(
        !reading_mode_hidden
            .iter()
            .any(|s| s.contains(".chapter-end") || s.contains(".chapter-step"))
    );
    assert!(block_after(HTML, "@media print").contains(".chapter-end"));
}

/// The tagline footer is gone; nothing in the page or stylesheet is left
/// for it.
#[test]
fn there_is_no_tagline() {
    assert!(!HTML.contains("<footer"));
    assert!(!HTML.contains("whole counsel of God"));
    assert!(
        !rules()
            .iter()
            .any(|r| selectors(r).any(|s| s == "footer" || s.ends_with(" footer")))
    );
}

/// Footnotes and Cross references are disclosures: a button with
/// aria-expanded controlling its list, closed unless the reader opened
/// that kind before (one remembered state per kind, read for every
/// chapter). Printing opens both.
#[test]
fn footnotes_and_cross_references_are_disclosures_closed_by_default() {
    // The disclosures are the reader's (ui-leptos/src/reader.rs and
    // reader_text.rs).
    let reader = piece("reader_text.rs");
    let keys = block_after(reader, "pub fn key(self)");
    assert!(keys.contains(r#"Self::Footnotes => "gospel-getter-footnotes","#));
    assert!(keys.contains(r#"Self::Xrefs => "gospel-getter-cross-references","#));
    let open = block_after(piece("reader.rs"), "fn opened()");
    assert!(
        open.contains(r#".is_some_and(|value| value == "open")"#),
        "closed unless saved open"
    );
    assert!(
        open.contains(".and_then(|s| s.get_item(kind.key()).ok().flatten())"),
        "closed when storage can't be read"
    );
    let toggle = block_after(reader, "fn disclosure_toggle(");
    assert!(toggle.contains(r#"<button type="button" class="disclosure-toggle""#));
    assert!(toggle.contains(r#"aria-expanded="{open}""#));
    assert!(toggle.contains(r#"aria-controls="{controls}""#));
    for (function, kind, title) in [
        (
            "fn footnote_summary(",
            "Disclosure::Footnotes",
            r#"format!("Footnotes ({count})")"#,
        ),
        (
            "fn xref_summary(",
            "Disclosure::Xrefs",
            r#"&format!("Cross references ({count})")"#,
        ),
    ] {
        let body = block_after(reader, function);
        assert!(
            body.contains("disclosure_toggle(") && body.contains(kind),
            "{function}"
        );
        assert!(body.contains(title), "{function}");
        assert!(
            body.contains(r#"if open { "" } else { " hidden" }"#),
            "{function}: the list is hidden while closed"
        );
    }
    // A closed list is hidden on screen, though the lists are grids.
    assert!(
        rules().iter().any(|r| r.at_rule.is_none()
            && r.selector == ".disclosure-body[hidden]"
            && r.body.contains("display: none")),
        "closed lists must not show"
    );
    let set = block_after(piece("reader.rs"), "fn set_disclosure(");
    assert!(set.contains("storage.set_item(kind.key()"));
    let print = block_after(HTML, "@media print");
    let open_in_print = print
        .find(".disclosure-body[hidden]")
        .expect("print opens them");
    assert!(print[open_in_print..].contains("display: block !important"));
}

/// Everything the chapter's end adds is in tested colors: the buttons are
/// --accent on the opaque --option-bg (Vaporwave's --panel-bg lets its
/// gradient through), the disclosure toggles --text on the page (their
/// chevron, a mark, --accent, at least 3:1), and the hint one quiet line of
/// --text-dim, its keys as caps of --text-dim on their own opaque
/// --option-bg (the arrows drawn, not a fallback font's).
///
/// Over the page itself, --text-dim is AA on every stop of every theme's
/// background but two, which every other --text-dim line on the page shares
/// and the palette test (--option-bg only) doesn't see: Vaporwave's
/// gradient, where no plain color is AA (see
/// `no_plain_color_is_aa_over_all_of_vaporwave`), and the deepest stop of
/// Beast Slayer's, 4.00:1 at the very bottom of the window (the gradient is
/// fixed to it). Pinned here, so any other falls out.
#[test]
fn the_chapter_end_uses_tested_theme_colors() {
    let rules = rules();
    for (selector, declarations) in [
        (
            ".chapter-step",
            &["color: var(--accent)", "background: var(--option-bg)"][..],
        ),
        (
            ".disclosure-toggle",
            &["color: var(--text)", "background: none"][..],
        ),
        (".disclosure-chevron", &["color: var(--accent)"][..]),
        (".nav-hint", &["color: var(--text-dim)"][..]),
        (
            ".nav-hint kbd,\n        .shortcut-keys kbd",
            &["color: var(--text-dim)", "background: var(--option-bg)"][..],
        ),
        (
            ".nav-hint kbd svg,\n        .shortcut-keys kbd svg",
            &["stroke: currentColor"][..],
        ),
    ] {
        let rule = screen_rule(&rules, selector);
        for declaration in declarations {
            assert!(rule.body.contains(declaration), "{selector}: {declaration}");
        }
    }
    let mut hint_below_aa = Vec::new();
    for (theme, props) in theme_palettes(&rules) {
        let option = property(&props, "--option-bg").to_owned();
        let dim = property(&props, "--text-dim");
        let key = contrast_ratio(dim, &option);
        assert!(key >= 4.5, "theme `{theme}`: a hint key is {key:.2}:1");
        let grounds = if theme == "vaporwave" {
            vec![option.clone()]
        } else {
            backgrounds(&props)
        };
        for bg in grounds {
            for (what, fg, under) in [
                (
                    "a chapter button",
                    property(&props, "--accent"),
                    option.as_str(),
                ),
                ("a disclosure", property(&props, "--text"), bg.as_str()),
            ] {
                let ratio = contrast_ratio(fg, under);
                assert!(
                    ratio >= 4.5,
                    "theme `{theme}`: {what} is {ratio:.2}:1 on {under}"
                );
            }
            let chevron = contrast_ratio(property(&props, "--accent"), &bg);
            assert!(
                chevron >= 3.0,
                "theme `{theme}`: a chevron is {chevron:.2}:1 on {bg}"
            );
            let hint = contrast_ratio(dim, &bg);
            if hint < 4.5 {
                hint_below_aa.push(format!("{theme} on {bg}: {hint:.2}"));
            }
        }
    }
    assert_eq!(hint_below_aa, ["beast-slayer on #6b3a1f: 4.00"]);
}

/// Selecting verses opens one card that floats over the text (absolutely
/// placed in the reading column, which is its containing block), so the
/// chapter never reflows: everything that used to sit inline under a
/// selection is in it, in order (the actions and citation, the footnotes,
/// the notes, the cross-references as chips), and the verses carry no
/// popup of their own. It's a labelled group straight after the last
/// selected verse in the DOM (next in Tab order), at most a set height
/// with its own scroll, docked to the bottom of the window only when the
/// selection leaves no room on either side; it isn't printed.
#[test]
fn the_selection_card_floats_over_the_text() {
    // The card is the selection card's (ui-leptos/src/card.rs, its markup
    // and rules in card_text.rs).
    let card_rs = piece("card.rs");
    assert!(card_rs.contains(r#"card.set_id("selection-card");"#));
    assert!(card_rs.contains(r#"let _ = card.set_attribute("role", "group");"#));
    assert!(card_rs.contains("let _ = card.append_with_node_3(&actions, &footnotes, &xrefs);"));
    let render = block_after(card_rs, "fn render_selection()");
    assert!(render.contains(r#".set_attribute("aria-label", &format!("Selection: {citation}"));"#));
    assert!(
        render.contains("let _ = last.after_with_node_1(&els.card);"),
        "after the last selected verse"
    );
    assert!(
        !render.contains("last.append"),
        "nothing goes inside a verse"
    );
    assert!(
        render.contains("dom::focus(&verse);"),
        "focus returns to the verse"
    );
    assert!(
        card_rs.contains("let _ = els.xrefs.before_with_node_1(&els.notes);"),
        "notes before the cross-references"
    );
    assert!(!HTML.contains("renderVersePopup"), "verses carry no popup");
    let chips = block_after(piece("card_text.rs"), "pub fn xrefs_html(");
    assert!(chips.contains(r#"<ul class=\"xref-chips\""#));
    assert!(chips.contains(r#"<li><button type="button" class="xref-citation""#));
    let place = &block_after(piece("card_text.rs"), "pub fn placement(");
    let place_card = &block_after(card_rs, "fn place_card()");
    for (body, rule) in [
        (place, "below < wanted"),
        (place, "above >= wanted"),
        (place_card, r#"card.class_list().add_1("is-above")"#),
        (place_card, r#"card.class_list().add_1("is-docked")"#),
    ] {
        assert!(body.contains(rule), "placeCard: {rule}");
    }

    let rules = rules();
    let main = screen_rule(&rules, ".chapter-main");
    assert!(
        main.body.contains("position: relative"),
        "the card's containing block"
    );
    let card = screen_rule(&rules, ".selection-card");
    for declaration in [
        "position: absolute",
        "max-height: min(24rem, calc(100vh - var(--bar-height) - 2rem))",
        "overflow-y: auto",
        "font-family: var(--font-ui)",
        "color: var(--text)",
        "background: var(--option-bg)",
    ] {
        assert!(
            card.body.contains(declaration),
            ".selection-card: {declaration}"
        );
    }
    let docked = screen_rule(&rules, ".selection-card.is-docked");
    assert!(docked.body.contains("position: fixed") && docked.body.contains("bottom: 0.5rem"));
    assert!(
        screen_rule(&rules, ".xref-chips li + li::before")
            .body
            .contains("color: var(--text-dim)")
    );
    assert!(
        screen_rule(&rules, ".xref-citation")
            .body
            .contains("color: var(--accent-2)")
    );
    assert!(block_after(HTML, "@media print").contains(".selection-card"));
    // On the card's opaque --option-bg, every color it uses is AA; its
    // buttons' --panel-bg over it is covered by notes_use_tested_theme_colors.
    for (theme, props) in theme_palettes(&rules) {
        let bg = property(&props, "--option-bg");
        for fg in ["--text", "--text-dim", "--accent", "--accent-2"] {
            let ratio = contrast_ratio(property(&props, fg), bg);
            assert!(
                ratio >= 4.5,
                "theme `{theme}`: {fg} in the card is {ratio:.2}:1"
            );
        }
    }
}

/// Aa's Width row: Narrow, Normal and Wide, as a labelled group of buttons
/// over a hidden <select> like line spacing, Normal by default (no
/// attribute, so nothing changes for anyone who never chose). Normal keeps
/// the widths the columns always had, 492px in three columns (the
/// container's 1100px) and 732px in reading mode (its 780px); Narrow and
/// Wide set both. It's a display setting the backup carries (`width`).
#[test]
fn width_is_a_display_setting_in_aa() {
    assert_eq!(
        select_options("width-select"),
        [
            ("narrow".to_owned(), "Narrow".to_owned()),
            ("normal".to_owned(), "Normal".to_owned()),
            ("wide".to_owned(), "Wide".to_owned()),
        ]
    );
    assert_eq!(script_preset("width-select").1, "normal");
    assert!(HTML.contains(
        r#"<div id="width-choices" class="segmented" role="group" aria-labelledby="width-label"></div>"#
    ));
    assert!(HTML.contains(r#"<span class="section-label" id="width-label">Width</span>"#));
    // Between line spacing and the theme.
    let spacing = HTML.find(r#"id="line-spacing-label""#).unwrap();
    let width = HTML.find(r#"id="width-label""#).unwrap();
    let theme = HTML.find(r#"id="theme-label""#).unwrap();
    assert!(spacing < width && width < theme);
    assert_eq!(preferences::WIDTH.file_name, Some("width"));

    let rules = rules();
    let html = custom_properties(&screen_rule(&rules, "html").body);
    let px = |value: &str| -> f64 {
        value
            .strip_suffix("rem")
            .map(|r| r.parse::<f64>().unwrap() * 16.0)
            .or_else(|| value.strip_suffix("px").map(|p| p.parse().unwrap()))
            .unwrap_or_else(|| panic!("a length, got `{value}`"))
    };
    let column = px(property(&html, "--column"));
    let reading = px(property(&html, "--reader-column"));
    let side = px(property(&html, "--side-width"));
    let gap = px(property(&html, "--row-gap"));
    assert_eq!(
        column, 492.0,
        "Normal is the three-column width it always had"
    );
    assert_eq!(column + 2.0 * side + 2.0 * gap + 48.0, 1100.0);
    assert_eq!(reading + 48.0, 780.0, "and reading mode's");
    for (width, expected_column, expected_reading) in
        [("narrow", 416.0, 592.0), ("wide", 592.0, 880.0)]
    {
        let rule = screen_rule(&rules, &format!(r#":root[data-width="{width}"]"#));
        let props = custom_properties(&rule.body);
        assert_eq!(px(property(&props, "--column")), expected_column, "{width}");
        assert_eq!(
            px(property(&props, "--reader-column")),
            expected_reading,
            "{width}"
        );
    }
    assert!(screen_rule(&rules, ".container").body.contains(
        "max-width: calc(var(--column) + 2 * var(--side-width) + 2 * var(--row-gap) + 3rem)"
    ));
    let reading_mode = rules
        .iter()
        .find(|r| {
            r.at_rule.as_deref() == Some("@media screen")
                && r.selector == r#":root[data-reader-mode="on"] .container"#
        })
        .expect("reading mode's column");
    assert!(
        reading_mode
            .body
            .contains("max-width: calc(var(--reader-column) + 3rem)")
    );
}

/// `#rrggbb` with `rgb` ("242, 193, 78") laid over it at `alpha`.
fn blend(under: &str, rgb: &str, alpha: f64) -> String {
    let under = under.trim_start_matches('#');
    let over: Vec<f64> = rgb.split(',').map(|c| c.trim().parse().unwrap()).collect();
    (0..3)
        .map(|i| {
            let below = f64::from(u8::from_str_radix(&under[2 * i..2 * i + 2], 16).unwrap());
            format!(
                "{:02x}",
                (alpha * over[i] + (1.0 - alpha) * below).round() as u8
            )
        })
        .fold("#".to_owned(), |hex, c| hex + &c)
}

/// The command palette and Copy as… are AA in every theme: the options'
/// `--text`, and the group headings', reasons' and keys' `--text-dim`, on
/// the opaque `--option-bg`; and on the active option's tint (`--accent`
/// at 12% over it), `--text` only, so a disabled command's name and
/// reason turn `--text` there. Copy chapter… is the keys' hint's
/// `--text-dim` on the page (AA on every stop, tested with the hint).
#[test]
fn the_palette_and_copy_as_are_aa_in_every_theme() {
    let rules = rules();
    for selector in [
        ".palette-group-label",
        ".palette-reason",
        ".palette-key kbd",
        ".chapter-copy-btn",
    ] {
        assert!(
            screen_rule(&rules, selector)
                .body
                .contains("color: var(--text-dim)"),
            "{selector}"
        );
    }
    for selector in [".search-command", ".copy-as-item"] {
        assert!(
            screen_rule(&rules, selector)
                .body
                .contains("color: var(--text)"),
            "{selector}"
        );
    }
    assert!(
        screen_rule(&rules, ".copy-as-menu")
            .body
            .contains("background: var(--option-bg)")
    );
    let tint = "rgba(var(--accent-rgb), 0.12)";
    assert!(
        screen_rule(&rules, ".search-command:hover,\n        .search-command.is-active,\n        .search-result.is-active")
            .body
            .contains(tint)
    );
    assert!(
        screen_rule(&rules, ".search-command.is-active .palette-label,\n        .search-command.is-active .palette-reason")
            .body
            .contains("color: var(--text)")
    );
    for (theme, props) in theme_palettes(&rules) {
        let get = |name: &str| property(&props, name).to_owned();
        let panel = get("--option-bg");
        let active = blend(&panel, &get("--accent-rgb"), 0.12);
        for (what, fg, bg) in [
            ("text on the panel", get("--text"), panel.clone()),
            ("dim text on the panel", get("--text-dim"), panel.clone()),
            ("text on the active option", get("--text"), active.clone()),
        ] {
            let ratio = contrast_ratio(&fg, &bg);
            assert!(ratio >= 4.5, "{theme}: {what} is {ratio:.2}:1");
        }
        // Why the active option's dim text turns --text: it isn't AA there
        // in every theme.
        if theme == "matrix" {
            assert!(contrast_ratio(&get("--text-dim"), &active) < 4.5);
        }
    }
}

/// Ctrl+= (and Ctrl++, numpad +) and Ctrl+- (numpad -) step the text size,
/// Ctrl+0 resets it, through Aa's control; their default (the page zoom) is
/// always prevented, and in a field they change nothing. A brief status
/// says the new size, politely.
#[test]
fn the_text_size_keys_step_aa_and_never_zoom_the_page() {
    // Aa's keys are the page's Rust pieces' (`ui-leptos/src/aa.rs`, the
    // rule in `aa_text.rs`).
    let keys = block_after(piece("aa_text.rs"), "pub fn size_key(");
    for key in [
        r#"key == "=""#,
        r#"key == "+""#,
        r#"code == "NumpadAdd""#,
        r#"key == "-""#,
        r#"code == "NumpadSubtract""#,
        r#"code == "Numpad0""#,
    ] {
        assert!(keys.contains(key), "{key}");
    }
    let aa = piece("aa.rs");
    let body = block_after(aa, "fn on_size_key(");
    let prevent = body.find("event.prevent_default();").unwrap();
    let typing = body.find(r#""INPUT" | "TEXTAREA" | "SELECT""#).unwrap();
    assert!(
        prevent < typing,
        "the page zoom is prevented even in a field"
    );
    // The keys' step, which the command palette's Text size commands run
    // too.
    let step = block_after(aa, "pub fn text_size_shortcut(");
    assert!(step.contains(r#"choose(&select, "medium");"#));
    assert!(step.contains("step_text_size(i32::from(step));"));
    assert!(step.contains("show_text_size();"));
    assert!(piece("search.rs").contains("Run::TextSize(step) => aa::text_size_shortcut(*step),"));
    assert!(HTML.contains(r#"<p id="size-status" class="toast size-status is-hidden" role="status" aria-live="polite"></p>"#));
}

/// The faded neighbors never squeeze the chapter being read below 50
/// characters a line at Medium (400px, measured in WebKitGTK at 7.96px a
/// character): they shrink first (flex-shrink 50 against the column's 1),
/// down to 180px, and the column only then, to its floor. When even that
/// doesn't fit, the previous chapter hides, then the next. The breakpoints
/// follow from those numbers and the page's padding: both need
/// 400 + 2 × 180 + 2 × 20 + 48 = 848px, the next alone 400 + 180 + 20 + 48 =
/// 648px.
/// The sidebar (288px) only shows from 1200px, leaving 864px, so it never
/// hides a neighbor; and the floor is the same at every Width. Alone, below
/// 648px, the column fills the window again.
#[test]
fn side_chapters_never_squeeze_the_column_below_50_characters() {
    let rules = rules();
    let html = custom_properties(&screen_rule(&rules, "html").body);
    let px = |name: &str| -> f64 {
        let value = property(&html, name);
        value
            .strip_suffix("rem")
            .map(|r| r.parse::<f64>().unwrap() * 16.0)
            .or_else(|| value.strip_suffix("px").map(|p| p.parse().unwrap()))
            .unwrap_or_else(|| panic!("{name}: a length, got `{value}`"))
    };
    let (floor, side_min, gap, sidebar) = (
        px("--column-min"),
        px("--side-min"),
        px("--row-gap"),
        px("--sidebar-width"),
    );
    assert_eq!(floor, 400.0, "50 characters at 7.96px");
    let padding = 48.0;
    let both = floor + 2.0 * side_min + 2.0 * gap + padding;
    let next = floor + side_min + gap + padding;
    assert_eq!((both, next), (848.0, 648.0));
    assert!(HTML.contains(&format!("@media (max-width: {}px)", both - 1.0)));
    assert!(HTML.contains(&format!("@media (max-width: {}px)", next - 1.0)));
    assert!(
        1200.0 - sidebar - padding >= both - padding,
        "the sidebar never hides a neighbor"
    );

    let main = screen_rule(&rules, ".chapter-main");
    for declaration in [
        "flex: 1 1 var(--column)",
        "max-width: var(--column)",
        "min-width: min(var(--column-min), 100%)",
    ] {
        assert!(
            main.body.contains(declaration),
            ".chapter-main: {declaration}"
        );
    }
    let side = screen_rule(&rules, ".chapter-side");
    for declaration in [
        "flex: 0 50 var(--side-width)",
        "min-width: var(--side-min)",
        "max-width: var(--side-width)",
    ] {
        assert!(
            side.body.contains(declaration),
            ".chapter-side: {declaration}"
        );
    }
    assert!(
        screen_rule(&rules, ".reading-row")
            .body
            .contains("justify-content: center")
    );
    assert!(
        screen_rule(&rules, ".reading-row.is-comparing .chapter-main")
            .body
            .contains("max-width: none")
    );
    let alone = rules
        .iter()
        .find(|r| {
            r.at_rule.as_deref() == Some("@media (max-width: 647px)")
                && r.selector == ".chapter-main"
        })
        .expect("alone, the column fills the window");
    assert!(alone.body.contains("max-width: none"));
}

/// Before Genesis 1 and after Revelation 22, the missing neighbor's place
/// holds the backend's edge card (so the chapter being read doesn't move
/// between Genesis 1 and 2), in the neighbors' faded style with no new
/// colors: a heading like theirs in --accent over --rule, and a line in
/// --text. It has no verses and nothing to click.
#[test]
fn the_bible_s_edges_have_a_quiet_card_in_the_neighbor_s_place() {
    // The chapter's markup is the reader's (ui-leptos/src/reader_text.rs).
    let reader = piece("reader_text.rs");
    let pane = block_after(reader, "pub fn reading_pane(");
    assert!(pane.contains("Some(s) => side_chapter(s), None => edge_card(edge),"));
    assert!(pane.contains("side(dto.prev.as_ref(), dto.prev_edge.as_ref(), Which::Prev)"));
    assert!(pane.contains("side(dto.next.as_ref(), dto.next_edge.as_ref(), Which::Next)"));
    let card = block_after(reader, "fn edge_card(");
    assert!(card.contains("escape_html(&edge.title)") && card.contains("escape_html(d)"));
    assert!(!card.contains("<button") && !card.contains("class=\"verse"));
    let rules = rules();
    let title = screen_rule(&rules, ".edge-card-title");
    for declaration in [
        "font-family: var(--font-scripture)",
        "color: var(--accent)",
        "border-bottom: 1px solid var(--rule)",
    ] {
        assert!(
            title.body.contains(declaration),
            ".edge-card-title: {declaration}"
        );
    }
    let detail = screen_rule(&rules, ".edge-card-detail");
    assert!(detail.body.contains("color: var(--text)"));
    for rule in [title, detail] {
        assert!(
            !rule.body.contains('#') && !rule.body.contains("rgb"),
            "no new colors"
        );
    }
}

/// Layout is a display setting in Aa, after Width: Verses (the default, a
/// verse to a line as always) or Flowing. It's exported with the others.
/// While comparing, its buttons are disabled and a note says why.
#[test]
fn layout_is_a_display_setting_in_aa() {
    assert_eq!(
        select_options("layout-select"),
        [
            ("verses".to_owned(), "Verses".to_owned()),
            ("flowing".to_owned(), "Flowing".to_owned()),
        ]
    );
    assert_eq!(script_preset("layout-select").1, "verses");
    assert!(HTML.contains(r#"<span class="section-label" id="layout-label">Layout</span>"#));
    assert!(HTML.contains(
        r#"<div id="layout-choices" class="segmented" role="group" aria-labelledby="layout-label" aria-describedby="layout-note"></div>"#
    ));
    assert!(HTML.contains(
        r#"<p id="layout-note" class="section-note layout-note" hidden>Compare always shows a verse to a row.</p>"#
    ));
    let width = HTML.find(r#"id="width-label""#).unwrap();
    let layout = HTML.find(r#"id="layout-label""#).unwrap();
    let theme = HTML.find(r#"id="theme-label""#).unwrap();
    assert!(width < layout && layout < theme);
    assert_eq!(preferences::LAYOUT.file_name, Some("layout"));
    let aa = piece("aa.rs");
    assert!(aa.contains("btn.set_disabled(comparing);"));
    assert!(aa.contains("note.set_hidden(!comparing);"));

    // A disabled segment is quiet but still AA: --text-dim on the
    // segments' --panel-bg, over the popover's --option-bg.
    let rules = rules();
    let disabled = screen_rule(&rules, ".segment:disabled");
    assert!(disabled.body.contains("color: var(--text-dim)"));
    for (theme, props) in theme_palettes(&rules) {
        let option_bg = property(&props, "--option-bg");
        let panel = property(&props, "--panel-bg");
        let behind = if panel.starts_with("rgba(") {
            composite(panel, option_bg)
        } else {
            panel.to_owned()
        };
        let ratio = contrast_ratio(property(&props, "--text-dim"), &behind);
        assert!(
            ratio >= 4.5,
            "theme `{theme}`: a disabled segment is {ratio:.2}:1"
        );
    }
}

/// Flowing runs each chapter's verses inline in one paragraph, in the
/// chapter being read and the faded neighbors (and so reading mode and
/// print, which show them). It keys off `.chapter-text`, which compare's
/// grid doesn't have, so compare stays a verse to a row. The verse number
/// is inline with no line height, so it can't open up its line.
#[test]
fn flowing_runs_the_verses_on_as_prose_but_not_in_compare() {
    let rules = rules();
    let verse = screen_rule(
        &rules,
        r#":root[data-layout="flowing"] .chapter-text > .verse"#,
    );
    assert!(verse.body.contains("display: inline"));
    assert!(verse.body.contains("margin: 0"));
    let number = screen_rule(
        &rules,
        r#":root[data-layout="flowing"] .chapter-text > .verse > .verse-num"#,
    );
    for decl in ["display: inline", "min-width: 0", "line-height: 0"] {
        assert!(number.body.contains(decl), "{decl}");
    }
    let text = screen_rule(&rules, r#":root[data-layout="flowing"] .chapter-text"#);
    for decl in [
        "font-family: var(--font-scripture)",
        "font-size: calc(1.05rem * var(--reading-scale))",
        "line-height: var(--reading-leading)",
        "color: var(--text)",
    ] {
        assert!(text.body.contains(decl), "{decl}");
    }
    // The same size as a verse, in the center and the neighbors.
    assert!(
        screen_rule(&rules, ".verse")
            .body
            .contains("font-size: calc(1.05rem * var(--reading-scale))")
    );
    let side = screen_rule(
        &rules,
        r#":root[data-layout="flowing"] .chapter-side .chapter-text"#,
    );
    assert!(
        side.body
            .contains("font-size: calc(0.9rem * var(--reading-scale))")
    );
    // Only .chapter-text flows, and only the reading pane and the side
    // chapters have one; compare's grid doesn't.
    for rule in rules
        .iter()
        .filter(|r| r.selector.contains(r#"data-layout="flowing""#))
    {
        for selector in selectors(rule) {
            assert!(selector.contains(".chapter-text"), "{selector}");
            assert!(!selector.contains("compare"), "{selector}");
        }
    }
    // The chapter's markup is the reader's (ui-leptos/src/reader_text.rs).
    let reader = piece("reader_text.rs");
    let pane = block_after(reader, "pub fn reading_pane(");
    assert!(pane.contains("Some(compare) => compare_grid(dto, compare),"));
    assert!(pane.contains(r##"r#"<div class="chapter-text">{}</div>"#,"##));
    assert!(!block_after(reader, "fn compare_grid(").contains("chapter-text"));
    assert!(
        block_after(reader, "fn side_chapter(")
            .contains(r#"<div class=\"chapter-text\">{verses}</div>"#)
    );
    // Nothing in print touches the verses' display: printed, Flowing
    // flows (and Verses stays a verse to a line).
    for rule in rules
        .iter()
        .filter(|r| r.at_rule.as_deref() == Some("@media print"))
    {
        let names_a_verse = |selector: &str| {
            selector
                .split(|c: char| c.is_whitespace() || c == '>')
                .any(|part| {
                    part.starts_with(".chapter-text")
                        || part.strip_prefix(".verse").is_some_and(|rest| {
                            !rest.starts_with(|c: char| c.is_alphanumeric() || c == '-')
                        })
                })
        };
        let verses = selectors(rule).any(names_a_verse);
        assert!(
            !(verses && rule.body.contains("display")),
            "print sets a verse's display: {}",
            rule.selector
        );
    }
}

/// In Flowing, a verse shows where it starts and ends without boxes
/// breaking up the text: a faint wash under the pointer and the selection
/// tint, each on the verse's own lines. Over both, --text stays AA on every
/// theme's background (each stop of a gradient one), and never less legible
/// than on the bare page: Vaporwave's two lowest stops are under AA for its
/// text already (3.93:1 on #bf5349), and the wash raises that.
#[test]
fn flowing_verses_wash_and_select_at_aa_in_every_theme() {
    let rules = rules();
    let hover = screen_rule(
        &rules,
        r#":root[data-layout="flowing"] .chapter-main .chapter-text > .verse:where([data-translation]):hover"#,
    );
    assert!(
        hover
            .body
            .contains("background: rgba(var(--accent-rgb), 0.06)")
    );
    let selected = screen_rule(
        &rules,
        r#":root[data-layout="flowing"] .chapter-main .chapter-text > .verse.verse-selected"#,
    );
    assert!(
        selected
            .body
            .contains("background: rgba(var(--accent-rgb), 0.12)")
    );
    assert!(selected.body.contains("box-shadow: none"));
    let runs = screen_rule(
        &rules,
        r#":root[data-layout="flowing"] .chapter-main .chapter-text > .verse"#,
    );
    assert!(runs.body.contains("box-decoration-break: clone"));
    for (theme, props) in theme_palettes(&rules) {
        let accent_rgb = property(&props, "--accent-rgb");
        for bg in backgrounds(&props) {
            for alpha in ["0.06", "0.12"] {
                let under = composite(&format!("rgba({accent_rgb}, {alpha})"), &bg);
                let text = property(&props, "--text");
                let ratio = contrast_ratio(text, &under);
                let bare = contrast_ratio(text, &bg);
                assert!(
                    ratio >= 4.5_f64.min(bare),
                    "theme `{theme}`: verse text over a {alpha} wash on {bg} is {ratio:.2}:1 \
                     (on the bare page, {bare:.2}:1)"
                );
            }
        }
    }
}

/// Only verses that can be selected look clickable: the pointer (every
/// layout) and Flowing's wash select `.verse[data-translation]`, the
/// attribute the reader's verses carry (SELECTABLE). The plain-text
/// fallback's verses (step 7 F2) carry none, so they get neither. The
/// wash's `:where()` keeps it below the selection tint, which comes after
/// it at the same specificity.
#[test]
fn only_selectable_verses_look_clickable() {
    let rules = rules();
    let pointer = screen_rule(
        &rules,
        ".chapter-main .chapter-text > .verse[data-translation]",
    );
    assert!(pointer.body.contains("cursor: pointer"));
    let wash = screen_rule(
        &rules,
        r#":root[data-layout="flowing"] .chapter-main .chapter-text > .verse:where([data-translation]):hover"#,
    );
    assert!(wash.body.contains("background"));
    let at = |selector: &str| {
        rules
            .iter()
            .position(|r| r.at_rule.is_none() && r.selector == selector)
            .unwrap_or_else(|| panic!("expected a `{selector}` rule"))
    };
    assert!(
        at(&wash.selector)
            < at(
                r#":root[data-layout="flowing"] .chapter-main .chapter-text > .verse.verse-selected"#
            ),
        "the selection tint comes after the wash"
    );
    // No other rule gives a bare verse the pointer or a hover.
    for rule in rules.iter().filter(|r| r.at_rule.is_none()) {
        for selector in selectors(rule) {
            assert!(
                !selector.ends_with(".verse:hover"),
                "`{selector}` washes every verse"
            );
            assert!(
                !(selector.ends_with("> .verse") && rule.body.contains("cursor")),
                "`{selector}` gives every verse a cursor"
            );
        }
    }
    // The reader's verses carry the attribute; the fallback's don't.
    assert!(
        piece("keyboard.rs")
            .contains(r#"pub const SELECTABLE: &str = ".chapter-main .verse[data-translation]";"#)
    );
    assert!(piece("reader_text.rs").contains(r#"data-translation="{code}""#));
    let fallback = block_after(
        HTML,
        "document.addEventListener('gospel-getter:pieces-failed'",
    );
    assert!(fallback.contains("var verse = make('p', 'verse');"));
    for attribute in ["data-translation", "dataset", "setAttribute"] {
        assert!(!fallback.contains(attribute), "{attribute}");
    }
}

/// The card's place in Flowing: under the last selected verse's last line
/// and above the first one's first line (their line boxes, not the whole
/// run's box), from the column's left edge. A new layout re-places it.
#[test]
fn the_selection_card_anchors_to_line_boxes_in_flowing() {
    // The card is the selection card's (ui-leptos/src/card.rs, its rules in
    // card_text.rs).
    let place = block_after(piece("card.rs"), "fn place_card()");
    assert!(place.contains("let first_rect = line_rect(&first, true);"));
    assert!(place.contains("let last_rect = line_rect(&card_verse, false);"));
    assert!(
        block_after(piece("card_text.rs"), "pub fn card_box(")
            .contains("let left = if flowing { 0.0")
    );
    let line = block_after(piece("card.rs"), "fn line_rect(");
    assert!(line.contains("verse.get_client_rects()"));
    assert!(
        piece("card.rs")
            .contains("dom::on_document_event(crate::aa::LAYOUT_CHANGE_EVENT, |_| place_card());")
    );
    // A new layout keeps the reader's place: Aa asks the surfaces'
    // keep_reading_place, for the place found then.
    assert!(piece("aa.rs").contains("surfaces::keep_reading_place(change, None);"));
    assert!(
        piece("surfaces.rs")
            .contains("pub fn keep_reading_place(change: impl FnOnce(), place: Option<Place>) {")
    );
}

/// The welcome is one card: a modal dialog labelled by its title and
/// described by its one sentence, focusable itself (focus goes there on
/// open), with × and Skip, "Pick a look" as a radio group of two real theme
/// previews, and "Where to start" as a list the backend fills. No pages,
/// no Next or Back, and no key lists.
#[test]
fn the_welcome_is_one_accessible_card() {
    // The card is the welcome piece's (ui-leptos/src/welcome.rs).
    let card = piece("welcome.rs");
    let panel = tag_of("welcome-panel");
    for attr in [
        r#"class="welcome-panel modal-panel""#,
        r#"role="dialog""#,
        r#"aria-modal="true""#,
        r#"aria-labelledby="welcome-title""#,
        r#"aria-describedby="welcome-intro""#,
        r#"tabindex="-1""#,
    ] {
        assert!(panel.contains(attr), "{attr}");
    }
    assert!(card.contains(
        r#"<h2 id="welcome-title" class="welcome-title">"Welcome to Gospel Getter"</h2>"#
    ));
    assert!(tag_of("welcome-intro").contains(r#"class="welcome-intro""#));
    assert!(card.contains(
        r#""A quiet place to read the Bible on your computer, offline and with no account.""#
    ));
    let close = tag_of("welcome-close");
    for attr in [
        r#"type="button""#,
        r#"class="panel-close""#,
        r#"aria-label="Close""#,
        r#"title="Close (Esc)""#,
    ] {
        assert!(close.contains(attr), "{attr}");
    }
    let skip = tag_of("welcome-skip");
    assert!(skip.contains(r#"type="button""#) && skip.contains(r#"class="reader-mode-btn""#));
    assert!(card.contains(r#""Skip""#));
    let looks = tag_of("welcome-looks");
    for attr in [
        r#"class="welcome-looks""#,
        r#"role="radiogroup""#,
        r#"aria-labelledby="welcome-look-label""#,
    ] {
        assert!(looks.contains(attr), "{attr}");
    }
    assert!(
        card.contains(r#"<h3 class="section-label" id="welcome-look-label">"Pick a look"</h3>"#)
    );
    assert!(card.contains(r#"<p class="welcome-hint">"More themes in Aa."</p>"#));
    assert!(tag_of("welcome-start-label").contains(r#"class="section-label""#));
    assert!(card.contains(r#""Where to start""#));
    let start = tag_of("welcome-start");
    assert!(start.contains(r#"class="welcome-start""#));
    assert!(start.contains(r#"aria-labelledby="welcome-start-label""#));
    // The two looks: radios named by theme (as the theme list names them),
    // each a preview of its own theme, hidden from assistive tech: one
    // radio's markup, made for each of the two.
    let themes = select_options("theme-select");
    for (value, name) in [("chi-rho", "Chi Rho"), ("vellum", "Vellum")] {
        assert!(themes.contains(&(value.to_owned(), name.to_owned())));
        assert!(
            card.contains(&format!(r#"("{value}", "{name}")"#)),
            "{value}"
        );
    }
    assert!(card.contains(r#"const LOOKS: [(&str, &str); 2] ="#));
    assert_eq!(card.matches(r#"role="radio""#).count(), 1);
    for markup in [
        r#"class="welcome-look""#,
        "data-value=*value",
        r#"<span class="welcome-look-preview" data-theme-preview=*value aria-hidden="true">"#,
        r#"<span class="welcome-look-name">{*name}</span>"#,
    ] {
        assert!(card.contains(markup), "{markup}");
    }
    for gone in [
        "welcome-page",
        "welcome-next",
        "welcome-back",
        "welcome-progress",
        "<kbd>",
    ] {
        assert!(!card.contains(gone), "the card has no {gone}");
    }
    // Its own keyboard, ahead of the page's (capturing, on window): Escape,
    // the radios' arrows, and Tab kept inside; focus goes to the card.
    assert!(card.contains("dom::on_window_keydown("));
    assert!(card.contains("event.stop_propagation();"));
    assert!(card.contains(r#"if key == "Escape""#));
    assert!(card.contains(r#""ArrowLeft" | "ArrowRight" | "ArrowUp" | "ArrowDown""#));
    assert!(card.contains(r#"key == "Tab""#));
    assert!(card.contains("panel.focus()"));
    // The places to start are the backend's, kept from boot (`page`) and
    // shown as text (no markup is made from them).
    assert!(
        piece("page.rs").contains("let start_here = if js_sys::Array::is_array(&start_here) {")
    );
    assert!(card.contains(r#"<span class="welcome-start-title">{title}</span>"#));
    assert!(card.contains(r#"<span class="welcome-start-line">{line}</span>"#));
    assert!(!card.contains("inner_html"));
    // And the seen flag is the old tour's.
    assert!(
        piece("bridge.rs").contains(r#"pub const WELCOME_KEY: &str = "gospel-getter-welcome";"#)
    );
}

/// The card adds no colors of its own: --text, --text-dim and --accent on
/// the modal's opaque --option-bg, all AA in every theme; a place to start
/// under the pointer gets a 0.06 --accent wash, with its title (--accent)
/// and line (--text-dim) still AA; a look's border and its checked ring are
/// --accent-dim and --accent, and the checked ring is at least 3:1. Each
/// look previews its theme's own --bg and --accent.
#[test]
fn the_welcome_uses_tested_theme_colors() {
    let rules = rules();
    for (selector, color) in [
        (".welcome-title", "--accent"),
        (".welcome-look", "--text"),
        (".welcome-hint", "--text-dim"),
        (".welcome-start-row", "--text"),
        (".welcome-start-title", "--accent"),
        (".welcome-start-line", "--text-dim"),
        (".welcome-look-sample", "--accent"),
    ] {
        assert!(
            screen_rule(&rules, selector)
                .body
                .contains(&format!("color: var({color})")),
            "{selector} should be {color}"
        );
    }
    assert!(
        screen_rule(&rules, ".welcome-start-row:hover")
            .body
            .contains("background: rgba(var(--accent-rgb), 0.06)")
    );
    let checked = screen_rule(&rules, r#".welcome-look[aria-checked="true"]"#);
    assert!(checked.body.contains("border-color: var(--accent)"));
    assert!(checked.body.contains("box-shadow: 0 0 0 1px var(--accent)"));
    assert!(
        screen_rule(&rules, ".welcome-look-preview")
            .body
            .contains("background: var(--bg)")
    );
    for (theme, props) in theme_palettes(&rules) {
        let bg = property(&props, "--option-bg");
        let wash = composite(
            &format!("rgba({}, 0.06)", property(&props, "--accent-rgb")),
            bg,
        );
        for (what, fg, under) in [
            ("the title", "--accent", bg.to_owned()),
            ("a look's name", "--text", bg.to_owned()),
            ("the hint", "--text-dim", bg.to_owned()),
            (
                "a place's title under the pointer",
                "--accent",
                wash.clone(),
            ),
            (
                "a place's line under the pointer",
                "--text-dim",
                wash.clone(),
            ),
        ] {
            let ratio = contrast_ratio(property(&props, fg), &under);
            assert!(ratio >= 4.5, "theme `{theme}`: {what} is {ratio:.2}:1");
        }
        let ring = contrast_ratio(property(&props, "--accent"), bg);
        assert!(
            ring >= 3.0,
            "theme `{theme}`: the checked ring is {ring:.2}:1"
        );
    }
    // The previews: each theme's large "Aa" on its own page background.
    for theme in ["chi-rho", "vellum"] {
        let props = theme_palettes(&rules)
            .into_iter()
            .find(|(name, _)| name == theme)
            .unwrap()
            .1;
        let ratio = contrast_ratio(property(&props, "--accent"), property(&props, "--bg"));
        assert!(ratio >= 3.0, "{theme}'s preview is {ratio:.2}:1");
    }
}

/// The four-page tour is gone entirely: no markup, style or script of its
/// own is left (its pages, Next and Back, the progress line, the icon and
/// the key caps it styled), and nothing in the app refers to a tour.
#[test]
fn nothing_is_left_of_the_old_tour() {
    for gone in [
        "welcome-page",
        "welcome-pages",
        "welcome-next",
        "welcome-back",
        "welcome-progress",
        "welcome-icon",
        "welcome-text-",
        "welcome-title-",
        "showWelcomePage",
        "Start reading",
    ] {
        assert!(!HTML.contains(gone), "`{gone}` is left of the tour");
        for (file, source) in PIECES {
            assert!(
                !source.contains(gone),
                "`{gone}` is left of the tour in {file}"
            );
        }
    }
    // Every welcome class the stylesheet styles is used outside it, in the
    // markup or the script, the page's or the welcome's (ui-leptos).
    let outside = format!(
        "{}{}",
        &HTML[HTML.find("</style>").unwrap()..],
        piece("welcome.rs")
    );
    let rules = rules();
    for rule in rules.iter().filter(|r| r.selector.contains("welcome")) {
        for class in rule
            .selector
            .split(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
            .filter(|word| word.starts_with("welcome"))
        {
            assert!(
                outside.contains(class),
                "`{class}` (in `{}`) styles nothing",
                rule.selector
            );
        }
    }
}

/// A cross-reference previewed in the card: the chips control one preview
/// region, labelled by its title, that nothing navigates to until its Go to
/// button; and its colors are the card's tested ones, AA over its faint
/// wash in every theme: the title and any notice in --text-dim, the verses
/// in --text, their numbers and Go to in --accent-2. The pressed chip keeps
/// its --accent-2 text, AA over the same wash, ringed at 3:1.
#[test]
fn the_cross_reference_preview_is_accessible_and_uses_tested_theme_colors() {
    // The preview is the selection card's (ui-leptos/src/card.rs, its markup
    // in card_text.rs).
    let render = block_after(piece("card_text.rs"), "pub fn xrefs_html(");
    assert!(render.contains(r#"aria-expanded="false" aria-controls="xref-preview""#));
    assert!(piece("card.rs").contains(r#"let _ = preview.set_attribute("role", "region");"#));
    assert!(
        piece("card.rs")
            .contains(r#"let _ = preview.set_attribute("aria-labelledby", "xref-preview-title");"#)
    );
    let toggle = block_after(piece("card.rs"), "fn toggle_preview(");
    assert!(toggle.contains(
        "\"get_passage\", &PassageTextArgs { translation_code: &code, book_id, chapter, start, end,"
    ));
    assert!(
        !toggle.contains("reader::"),
        "a chip previews; only Go to opens"
    );
    // Its verses' markup is the reader's (reader_text::verse_html), without
    // note markers.
    assert!(
        block_after(piece("card_text.rs"), "pub fn preview_html(").contains("verse_html(v, false)")
    );
    // Through the page's own `invoke` (ui-leptos/src/ipc.rs).
    assert!(block_after(piece("card.rs"), "fn send(").contains("ipc::send(cmd, args)"));

    let rules = rules();
    for control in [".xref-citation", ".xref-preview-go"] {
        let focus = format!("{control}:focus-visible");
        assert!(
            rules.iter().any(|r| r.at_rule.is_none()
                && selectors(r).any(|s| s == focus)
                && r.body.contains("outline: 2px solid var(--accent)")),
            "{control} has the shared focus ring"
        );
    }
    let wash = "background: rgba(var(--accent-rgb), 0.05)";
    for selector in [".xref-preview", r#".xref-citation[aria-expanded="true"]"#] {
        assert!(
            screen_rule(&rules, selector).body.contains(wash),
            "{selector}"
        );
    }
    assert!(
        screen_rule(&rules, r#".xref-citation[aria-expanded="true"]"#)
            .body
            .contains("box-shadow: 0 0 0 1px var(--accent-2)")
    );
    for (selector, color) in [
        (".xref-preview-title", "--text-dim"),
        (".xref-preview-notice", "--text-dim"),
        (".xref-preview-text", "--text"),
        (".xref-preview-num", "--accent-2"),
        (".xref-preview-go", "--accent-2"),
    ] {
        assert!(
            screen_rule(&rules, selector)
                .body
                .contains(&format!("color: var({color})")),
            "{selector}"
        );
    }
    assert!(
        screen_rule(&rules, ".xref-preview-text")
            .body
            .contains("font-family: var(--font-scripture)")
    );
    for (theme, props) in theme_palettes(&rules) {
        let card = property(&props, "--option-bg");
        let under = composite(
            &format!("rgba({}, 0.05)", property(&props, "--accent-rgb")),
            card,
        );
        for fg in ["--text", "--text-dim", "--accent-2"] {
            let ratio = contrast_ratio(property(&props, fg), &under);
            assert!(
                ratio >= 4.5,
                "theme `{theme}`: {fg} on the preview's wash is {ratio:.2}:1"
            );
        }
        let ring = contrast_ratio(property(&props, "--accent-2"), card);
        assert!(
            ring >= 3.0,
            "theme `{theme}`: the pressed ring is {ring:.2}:1"
        );
    }
}

/// The chapter's Cross references are buttons that open their passage:
/// --accent-2 like the rest of the quiet links, underlined on hover and
/// focus, with the shared focus ring; the commas between them are outside
/// the buttons. AA, and the ring 3:1, on every flat page; on a gradient
/// they're the color of the verse labels beside them (see
/// `the_quiet_accent_carries_the_quiet_jobs`). Printing doesn't style them.
#[test]
fn chapter_end_cross_references_are_quiet_links() {
    // The chapter's markup is the reader's (ui-leptos/src/reader_text.rs).
    let summary = block_after(piece("reader_text.rs"), "fn xref_summary(");
    assert!(summary.contains(r#"<button type="button" class="xref-summary-chip" data-book="{}""#));
    let rules = rules();
    let chip = screen_rule(&rules, ".xref-summary-chip");
    for declaration in [
        "color: var(--accent-2)",
        "background: none",
        "border: none",
        "cursor: pointer",
    ] {
        assert!(chip.body.contains(declaration), "{declaration}");
    }
    assert!(!chip.body.contains("text-decoration"), "quiet at rest");
    // Rules naming `selector` among others.
    let naming = |selector: &str| -> Vec<&Rule> {
        rules
            .iter()
            .filter(|r| r.at_rule.is_none() && selectors(r).any(|s| s == selector))
            .collect()
    };
    for state in [
        ".xref-summary-chip:hover",
        ".xref-summary-chip:focus-visible",
    ] {
        assert!(
            naming(state)
                .iter()
                .any(|r| r.body.contains("text-decoration: underline")),
            "{state} is underlined"
        );
    }
    assert!(
        naming(".xref-summary-chip:focus-visible")
            .iter()
            .any(|r| r.body.contains("outline: 2px solid var(--accent)")),
        "the shared focus ring"
    );
    assert!(summary.contains(r#"<span class="xref-summary-item"><button"#));
    screen_rule(&rules, ".xref-summary-item:not(:last-child)::after");
    assert!(!block_after(HTML, "@media print").contains(".xref-summary"));
    for (theme, props) in theme_palettes(&rules) {
        let page = backgrounds(&props);
        // Gradient pages (Vaporwave, Beast Slayer, Hot Pink) are held to
        // the verse labels beside the citations, already --accent-2.
        let [bg] = &page[..] else { continue };
        let ratio = contrast_ratio(property(&props, "--accent-2"), bg);
        assert!(
            ratio >= 4.5,
            "theme `{theme}`: a citation on {bg} is {ratio:.2}:1"
        );
        let focus = contrast_ratio(property(&props, "--accent"), bg);
        assert!(
            focus >= 3.0,
            "theme `{theme}`: the focus ring on {bg} is {focus:.2}:1"
        );
    }
}

/// The translation button's translations are a radio group, one line
/// each ("KJV · King James Version · 1611/1769 · public domain"), and
/// compare's partner a row of buttons beside Compare. The words are --text, the code --accent and the
/// rest --text-dim, on --panel-bg; the chosen one all --on-accent on
/// --accent. Each AA in every theme, the focus ring 3:1. About's credits
/// are --text-dim on the panel, AA too.
#[test]
fn the_translation_chooser_and_credits_are_accessible_and_use_tested_theme_colors() {
    assert!(HTML.contains(
        r#"<div id="translation-choices" class="translation-list" role="radiogroup" aria-labelledby="translation-label"></div>"#
    ));
    assert!(HTML.contains(
        r#"<div id="compare-with-choices" class="segmented" role="group" aria-label="Compare with"></div>"#
    ));
    // Aa's rows (`ui-leptos/src/aa.rs`).
    let render = block_after(piece("aa.rs"), "fn translation_view(");
    assert!(render.contains(r#"class="translation-choice" role="radio""#));
    assert!(render.contains(r#"<span class="translation-dot" aria-hidden="true">"#));
    // About is the Rust pieces' (`ui-leptos/src/about.rs`).
    assert!(piece("about.rs").contains(r#"<ul id="about-credits" class="about-credits-list">"#));

    let rules = rules();
    // The screen rule naming `selector`, alone or in a list.
    let naming = |selector: &str| -> &Rule {
        rules
            .iter()
            .find(|r| r.at_rule.is_none() && selectors(r).any(|s| s == selector))
            .unwrap_or_else(|| panic!("expected a rule naming `{selector}`"))
    };
    let choice = naming(".translation-choice");
    for declaration in ["color: var(--text)", "background: var(--panel-bg)"] {
        assert!(choice.body.contains(declaration), "{declaration}");
    }
    for (selector, color) in [
        (".translation-code", "--accent"),
        (".translation-detail", "--text-dim"),
        (".translation-dot", "--text-dim"),
        (".about-credits-list", "--text-dim"),
    ] {
        assert!(
            naming(selector)
                .body
                .contains(&format!("color: var({color})")),
            "{selector}"
        );
    }
    let checked = naming(r#".translation-choice[aria-checked="true"] .translation-detail"#);
    assert!(checked.body.contains("background: var(--accent)"));
    assert!(checked.body.contains("color: var(--on-accent)"));
    assert!(
        naming(".translation-choice:focus-visible")
            .body
            .contains("outline: 2px solid var(--accent)")
    );
    for (theme, props) in theme_palettes(&rules) {
        // --panel-bg is a translucent tint over the popover's opaque
        // --option-bg.
        let panel = &composite(
            property(&props, "--panel-bg"),
            property(&props, "--option-bg"),
        );
        for fg in ["--text", "--text-dim", "--accent"] {
            let ratio = contrast_ratio(property(&props, fg), panel);
            assert!(
                ratio >= 4.5,
                "theme `{theme}`: {fg} on a translation is {ratio:.2}:1"
            );
        }
        let chosen = contrast_ratio(
            property(&props, "--on-accent"),
            property(&props, "--accent"),
        );
        assert!(
            chosen >= 4.5,
            "theme `{theme}`: the chosen translation is {chosen:.2}:1"
        );
        let ring = contrast_ratio(property(&props, "--accent"), panel);
        assert!(
            ring >= 3.0,
            "theme `{theme}`: the focus ring is {ring:.2}:1"
        );
        let credits = contrast_ratio(
            property(&props, "--text-dim"),
            property(&props, "--option-bg"),
        );
        assert!(
            credits >= 4.5,
            "theme `{theme}`: About's credits are {credits:.2}:1"
        );
    }
}

/// A USFM footnote's verse number (the BSB's "²¹But this kind") is small
/// and raised without opening up its line.
#[test]
fn a_footnotes_verse_number_is_set_small() {
    let rules = rules();
    let rule = screen_rule(&rules, ".footnote-verse");
    for declaration in [
        "font-size: 0.75em",
        "line-height: 0",
        "vertical-align: super",
    ] {
        assert!(rule.body.contains(declaration), "{declaration}");
    }
    // A footnote's markup is the reader's (ui-leptos/src/reader_text.rs).
    let footnote = block_after(piece("reader_text.rs"), "pub fn footnote_html(");
    assert!(footnote.contains(r#"<em class="footnote-alt">"#));
    assert!(footnote.contains(r#"<sup class="footnote-verse">"#));
}

/// The key values a handler compares against: `event.key === 'j'`,
/// `key === '='`, `code === 'NumpadAdd'`, `toLowerCase() === 'k'`,
/// `event.key !== 'Escape'`; in Rust, `key == "="` and `code ==
/// "NumpadAdd"`.
fn compared_keys(handler: &str) -> Vec<String> {
    let mut keys = Vec::new();
    for (marker, quote) in [
        ("key === '", '\''),
        ("key !== '", '\''),
        ("key() != \"", '"'),
        ("code === '", '\''),
        ("toLowerCase() === '", '\''),
        ("key == \"", '"'),
        ("code == \"", '"'),
    ] {
        let mut rest = handler;
        while let Some(at) = rest.find(marker) {
            let after = &rest[at + marker.len()..];
            let end = after.find(quote).unwrap();
            keys.push(after[..end].to_owned());
            rest = &after[end..];
        }
    }
    keys
}

/// The key values every row of the table of shortcuts says it handles
/// (`SHORTCUTS` in ui-leptos's `shortcut_table.rs`).
fn shortcut_table_handles() -> Vec<String> {
    let source = piece("shortcut_table.rs");
    let start = source
        .find("pub const SHORTCUTS: &[Shortcut] = &[")
        .expect("the shortcuts table");
    let table = &source[start..start + source[start..].find("\n];").unwrap()];
    let mut handles = Vec::new();
    for row in table.split("handles: &[").skip(1) {
        // Quoted values, until the list's closing bracket (outside quotes:
        // `]` is a key itself).
        let mut rest = row.trim_start();
        while let Some(quoted) = rest.strip_prefix('"') {
            let end = quoted.find('"').unwrap();
            handles.push(quoted[..end].to_owned());
            rest = quoted[end + 1..].trim_start_matches([',', ' ']);
        }
        assert!(rest.starts_with(']'), "a handles list ends: {rest}");
    }
    handles
}

/// One table of shortcuts: every key the app's key handlers answer (the
/// main one, Aa's text-size keys and its Escape) is in some row's
/// `handles`, so the shortcuts sheet, which lists the table, lists it.
#[test]
fn every_handled_key_is_in_the_shortcuts_table() {
    let handled: Vec<String> = [
        block_after(piece("keyboard_text.rs"), "pub fn route("),
        block_after(piece("aa_text.rs"), "pub fn size_key("),
        block_after(piece("surfaces.rs"), "fn on_escape("),
    ]
    .iter()
    .flat_map(|handler| compared_keys(handler))
    .collect();
    for key in [
        "?",
        "j",
        "J",
        "k",
        "K",
        "[",
        "]",
        "ArrowLeft",
        "Escape",
        "NumpadAdd",
        "c",
    ] {
        assert!(handled.iter().any(|k| k == key), "the scan finds {key}");
    }
    let listed = shortcut_table_handles();
    for key in &handled {
        assert!(
            listed.contains(key),
            "`{key}` is handled but not in SHORTCUTS"
        );
    }
    // And the sheet is that table, grouped: it reads the table itself (the
    // page's request to open says only where focus goes back to), and
    // lists each group's rows.
    let open = block_after(piece("page.rs"), "pub fn open_shortcuts(");
    assert!(open.contains(r#"&[("returnFocus", &element_or_null(return_focus))],"#));
    let sheet = piece("shortcuts.rs");
    assert!(sheet.contains("shortcut_table::GROUPS"));
    assert!(sheet.contains("shortcut_table::in_group(group)"));
}

/// The shortcuts sheet is a modal dialog named by its heading, reached
/// from ☰ (a menu item that says its key), with a labelled close button
/// and a body that scrolls by keyboard too. Its rows are `--text` and its
/// key caps the chapter-end hint's (`--text-dim` on `--option-bg`), and
/// its group headings `--text-dim`, on the panel's `--option-bg`: all AA
/// in every theme.
#[test]
fn the_shortcuts_sheet_is_a_modal_dialog_aa_in_every_theme() {
    let panel = tag_of("shortcuts-panel");
    for attr in [
        r#"role="dialog""#,
        r#"aria-modal="true""#,
        r#"aria-labelledby="shortcuts-title""#,
        r#"class="modal-panel shortcuts-panel""#,
    ] {
        assert!(panel.contains(attr), "{attr}");
    }
    assert!(tag_of("shortcuts-title").starts_with("<h2"));
    assert!(tag_of("shortcuts-close").contains(r#"aria-label="Close keyboard shortcuts""#));
    let body = tag_of("shortcuts-body");
    assert!(body.contains(r#"tabindex="0""#) && body.contains(r#"role="region""#));
    assert!(tag_of("shortcuts-overlay").contains("modal-overlay"));
    let item = tag_of("shortcuts-btn");
    assert!(item.contains(r#"role="menuitem""#) && item.contains(r#"aria-keyshortcuts="?""#));

    let rules = rules();
    let caps = screen_rule(&rules, ".nav-hint kbd,\n        .shortcut-keys kbd");
    for declaration in [
        "color: var(--text-dim)",
        "background: var(--option-bg)",
        "border: 1px solid var(--accent-dim)",
    ] {
        assert!(caps.body.contains(declaration), "{declaration}");
    }
    assert!(
        screen_rule(&rules, ".shortcut-does")
            .body
            .contains("color: var(--text)")
    );
    assert!(
        screen_rule(&rules, ".shortcut-group-title")
            .body
            .contains("color: var(--text-dim)")
    );
    assert!(
        screen_rule(&rules, ".shortcuts-title")
            .body
            .contains("color: var(--accent)")
    );
    assert!(
        screen_rule(&rules, ".modal-panel")
            .body
            .contains("background: var(--option-bg)")
    );
    for (theme, props) in theme_palettes(&rules) {
        let get = |name: &str| property(&props, name).to_owned();
        let panel = get("--option-bg");
        for (what, fg) in [
            ("a row", get("--text")),
            ("a key cap or heading", get("--text-dim")),
            ("the title", get("--accent")),
        ] {
            let ratio = contrast_ratio(&fg, &panel);
            assert!(ratio >= 4.5, "{theme}: {what} is {ratio:.2}:1");
        }
    }
}

/// The page never opens a web page itself: no anchor opens a new window,
/// and each link out of the app (Release notes, Support Gospel Getter, the
/// cross references' credits, a note's web links) is one the backend
/// opens, through a click handler. A note's references are links in the
/// app (`href="#"`), which go to a passage. The page and its Rust pieces
/// (About, with Release notes and Support Gospel Getter, is `about.rs`; a
/// note's links are drawn by `note_markdown.rs` and followed by
/// `note_links.rs`) together; the pieces' shipped code, not their tests.
#[test]
fn every_link_out_of_the_app_is_opened_by_the_backend() {
    let shipped = |source: &'static str| match source.find("#[cfg(test)]") {
        Some(tests) => &source[..tests],
        None => source,
    };
    let sources: Vec<&str> = std::iter::once(HTML)
        .chain(PIECES.iter().map(|(_, source)| shipped(source)))
        .collect();
    let mut anchors: Vec<&str> = Vec::new();
    for source in &sources {
        assert!(!source.contains("target=\"_blank\""));
        assert!(!source.contains("window.open("));
        anchors.extend(
            source
                .match_indices("<a")
                .filter(|(at, _)| {
                    source[at + 2..]
                        .chars()
                        .next()
                        .is_some_and(char::is_whitespace)
                })
                .map(|(at, _)| &source[at..at + source[at..].find('>').unwrap()]),
        );
    }
    assert_eq!(anchors.len(), 6, "{anchors:#?}");
    for anchor in anchors {
        assert!(
            anchor.contains("id=\"release-notes-link\"")
                || anchor.contains("id=\"support-link\"")
                || anchor.contains("class=\"credit-link\" data-link=")
                || anchor.contains(r#"class=\"note-web-link\" href=\"{address}\""#)
                || anchor.contains(r##"class=\"note-ref-link\" href=\"#\""##),
            "{anchor}"
        );
    }
    // A note's links: a capturing click handler keeps every click on them
    // from the page, has the backend open a web link, and goes to a
    // reference's passage itself.
    let links = piece("note_links.rs");
    assert!(links.contains(r#"const LINKS: &str = "a.note-ref-link, a.note-web-link";"#));
    assert!(links.contains("event.prevent_default();"));
    assert!(links.contains(r#"ipc::send("open_web_link", args)"#));
    assert!(links.contains("crate::reader::open("));
    assert!(piece("about.rs").contains(r#"ipc::invoke_value("open_release_notes""#));
    assert!(piece("about.rs").contains(r#"ipc::invoke_value("open_support_page""#));
    // The credit links are the reader's (ui-leptos/src/reader.rs).
    assert!(piece("reader.rs").contains(r#"ipc::send("open_credit_link", args)"#));
}

/// A Check now that times out says what the backend says when GitHub
/// can't be reached, word for word: the backend gives the check up itself
/// (`updater::CHECK_TIMEOUT`, 20 s) with its own message, so neither the
/// page nor its Rust pieces keep a copy of the message or a timer of their
/// own.
#[test]
fn a_timed_out_update_check_says_the_backends_offline_message() {
    let offline = gospel_getter_lib::updater::messages::OFFLINE;
    assert_eq!(
        gospel_getter_lib::updater::CHECK_TIMEOUT,
        std::time::Duration::from_secs(20)
    );
    for (file, source) in std::iter::once(("index.html", HTML)).chain(PIECES) {
        assert!(!source.contains(offline), "{file} repeats the message");
        assert!(!source.contains("Couldn’t reach GitHub"), "{file}");
        assert!(
            !source.contains("20000") && !source.contains("20_000"),
            "{file}"
        );
    }
    assert!(!HTML.contains("withinUpdateCheckTime"));
}

/// Under the Content Security Policy (see `tauri.conf.json`), only the
/// page's own inline `<script>` and `<style>` run, by their build-time
/// hashes: an inline event handler or a `style` attribute, in the page or
/// in markup a script injects, is blocked without a sound. So the page has
/// none, as far as a scan of its source can tell, and sets styles through
/// the CSSOM (`el.style.setProperty`), which the policy allows.
#[test]
fn nothing_inline_that_the_content_security_policy_would_block() {
    // Attributes, in the HTML or in a script's markup strings:
    // ` onclick="…"`, ` style='…'`.
    let attribute_at = |name_ok: &dyn Fn(&str) -> bool| -> Vec<String> {
        let bytes = HTML.as_bytes();
        let mut found = Vec::new();
        for (i, _) in HTML.match_indices('=') {
            if !matches!(bytes.get(i + 1), Some(b'"' | b'\'')) {
                continue;
            }
            let start = HTML[..i]
                .rfind(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
                .map_or(0, |p| p + 1);
            let name = &HTML[start..i];
            let before = HTML[..start].chars().next_back();
            if before.is_some_and(char::is_whitespace) && name_ok(name) {
                let line = HTML[..i].lines().count();
                found.push(format!("line {line}: {name}="));
            }
        }
        found
    };
    let handlers = attribute_at(&|name: &str| {
        name.len() > 2
            && name.starts_with("on")
            && name[2..].chars().all(|c| c.is_ascii_lowercase())
    });
    assert!(handlers.is_empty(), "inline event handlers: {handlers:?}");
    let styles = attribute_at(&|name: &str| name.eq_ignore_ascii_case("style"));
    assert!(styles.is_empty(), "style attributes: {styles:?}");

    for blocked in [
        "setAttribute('style'",
        "setAttribute(\"style\"",
        ".cssText",
        "javascript:",
        "eval(",
        "new Function",
        "<script src",
        "<link rel=\"stylesheet\"",
        "@import",
    ] {
        assert!(!HTML.contains(blocked), "{blocked}");
    }
    // The three inline scripts (the pre-paint script and boot's prefetch in
    // the head, and the reader's plain-text fallback at the end of the
    // body; the main script is gone, step 7c) and the one stylesheet Tauri
    // hashes.
    assert_eq!(HTML.matches("<script>").count(), 3);
    assert_eq!(HTML.matches("<script").count(), 3);
    assert_eq!(HTML.matches("<style>").count(), 1);
    assert_eq!(HTML.matches("<style").count(), 1);
    // The one style the markup used to carry, the compare grid's column
    // count, is set through the CSSOM, by the reader, whose markup carries
    // no style attribute either.
    let reader = piece("reader_text.rs");
    assert!(reader.contains(r#"<div class="compare-grid" data-columns="{}">"#));
    assert!(
        piece("reader.rs").contains(r#"grid.style().set_property("--compare-columns", &count)"#)
    );
    assert!(!reader.contains(" style="));
}
