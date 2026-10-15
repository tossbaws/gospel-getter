//! Lightweight regression checks on `ui/index.html`'s CSS and script,
//! without a browser or JS test framework — plain substring/brace-matching
//! assertions against the bundled static asset, the same file Tauri loads
//! into the webview at `frontendDist`.

const HTML: &str = include_str!("../../ui/index.html");

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

fn block_after<'a>(src: &'a str, needle: &str) -> &'a str {
    let start = src
        .find(needle)
        .unwrap_or_else(|| panic!("expected to find `{needle}` in ui/index.html"));
    let open = src[start..]
        .find('{')
        .map(|i| start + i)
        .unwrap_or_else(|| panic!("expected `{{` after `{needle}`"));
    let close = matching_brace_end(src, open);
    &src[open..close]
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
            block[rule..].starts_with(&format!("{side} {{\n                display: none;")),
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

/// Regression guard for GH-5: the "Random chapter" feature picks a chapter
/// entirely client-side, so every rendered `.book-item` button must carry
/// its `chapter_count` as a data attribute, or the client has no valid
/// upper bound to pick from. Book buttons are now built from one shared
/// `renderBookButton` function (used for both the Old and New Testament
/// lists), rather than two server-rendered loops, so this checks that
/// function's template literal and that both lists are built from it.
#[test]
fn book_item_buttons_expose_chapter_count() {
    assert!(
        HTML.contains(r#"data-book-id="${book.id}" data-chapter-count="${book.chapterCount}""#),
        "book-item buttons must expose chapterCount as a data attribute for \
         client-side random selection"
    );
    assert!(
        HTML.contains("home.otBooks.map((b) => renderBookButton(b"),
        "Old Testament books should render through the shared book-button \
         renderer"
    );
    assert!(
        HTML.contains("home.ntBooks.map((b) => renderBookButton(b"),
        "New Testament books should render through the shared book-button \
         renderer"
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

/// Regression guard for GH-5: the random-pick logic must be seeded from
/// `.book-item` buttons (uniform choice of book, then 1..=chapter_count
/// for that book) and must bail out rather than call `loadReading` when
/// that data is missing or non-numeric — it must never fabricate a
/// request to an invalid chapter.
#[test]
fn random_chapter_script_guards_against_malformed_data() {
    assert!(
        HTML.contains("document.querySelectorAll('.book-item')"),
        "random pick should choose uniformly from the rendered book buttons"
    );
    assert!(
        HTML.contains("!Number.isInteger(chapterCount) || chapterCount < 1"),
        "random pick should guard against a missing/non-numeric/non-positive \
         chapter count instead of issuing an invalid request"
    );
    assert!(
        HTML.contains("Math.floor(Math.random() * chapterCount) + 1"),
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
    // `get_home`, so it doesn't call `get_chapters`; cross-references open
    // their passage, so it doesn't call `get_xref_text`.
    for command in ["get_home", "get_reading", "list_notes"] {
        assert!(
            HTML.contains(&format!("invoke('{command}'")),
            "expected a call to the `{command}` Tauri command"
        );
    }
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
/// (e.g. `@media print`) it's nested in, if any.
struct Rule {
    at_rule: Option<String>,
    selector: String,
    body: String,
}

fn rules_in(css: &str, at_rule: Option<&str>, out: &mut Vec<Rule>) {
    let mut cursor = 0;
    while let Some(i) = css[cursor..].find('{') {
        let open = cursor + i;
        let close = matching_brace_end(css, open);
        let selector = css[cursor..open].trim().to_owned();
        let body = &css[open + 1..close];
        if selector.starts_with('@') {
            rules_in(body, Some(&selector), out);
        } else {
            out.push(Rule {
                at_rule: at_rule.map(str::to_owned),
                selector,
                body: body.to_owned(),
            });
        }
        cursor = close + 1;
    }
}

fn rules() -> Vec<Rule> {
    let mut out = Vec::new();
    rules_in(&stylesheet(), None, &mut out);
    out
}

fn screen_rule<'a>(rules: &'a [Rule], selector: &str) -> &'a Rule {
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
    body.split(';')
        .filter_map(|decl| decl.trim().split_once(':'))
        .filter(|(name, _)| name.trim().starts_with("--"))
        .map(|(name, value)| (name.trim().to_owned(), value.trim().to_owned()))
        .collect()
}

fn property<'a>(props: &'a [(String, String)], name: &str) -> &'a str {
    props
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, v)| v.as_str())
        .unwrap_or_else(|| panic!("expected `{name}` to be declared"))
}

/// The `value` attributes of a `<select id="...">`'s options, in order.
fn select_options(id: &str) -> Vec<(String, String)> {
    let start = HTML
        .find(&format!(r#"<select id="{id}""#))
        .unwrap_or_else(|| panic!("expected a <select id=\"{id}\">"));
    let end = HTML[start..].find("</select>").unwrap() + start;
    HTML[start..end]
        .split("<option value=\"")
        .skip(1)
        .map(|chunk| {
            let (value, rest) = chunk.split_once('"').unwrap();
            let label = &rest[rest.find('>').unwrap() + 1..rest.find("</option>").unwrap()];
            (value.to_owned(), label.trim().to_owned())
        })
        .collect()
}

/// The `values: [...]` list and `fallback` the preferences script
/// validates a saved choice for the `<select id>` against.
fn script_preset(select_id: &str) -> (Vec<String>, String) {
    let at = HTML
        .find(&format!("select: '{select_id}',"))
        .unwrap_or_else(|| panic!("the preferences script should manage #{select_id}"));
    let entry = &HTML[at..];
    let quoted = |field: &str| {
        let from = entry.find(field).unwrap() + field.len();
        entry[from..entry[from..].find('\n').unwrap() + from].to_owned()
    };
    let fallback = quoted("fallback: ")
        .trim_matches([' ', '\'', ','])
        .to_owned();
    let values = quoted("values: [")
        .trim_end_matches("],")
        .split(',')
        .map(|v| v.trim().trim_matches('\'').to_owned())
        .collect();
    (values, fallback)
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

    assert!(
        HTML.contains("renamed: { 'monster-hunter': 'beast-slayer' }"),
        "a saved `monster-hunter` theme should be migrated to `beast-slayer`"
    );
    let (values, _) = script_preset("theme-select");
    assert!(
        !values.iter().any(|v| v == "monster-hunter"),
        "the old key should be migrated, not accepted as-is"
    );
}

/// Each preference `<select>` must offer exactly the values the script
/// accepts from storage (otherwise a saved choice could be un-selectable,
/// or a selectable one silently reset on restart), and its default must be
/// one of them. The `<select>`s are hidden records of each choice; Aa's
/// buttons, labelled groups built from their options, are what the reader
/// uses.
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
            HTML.contains(&format!(r#"<select id="{id}" hidden>"#)),
            "#{id} should be a hidden record behind Aa's buttons"
        );
    }
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
/// Escape must also leave reading mode, and all of it has to be wired up
/// in the `<head>` preferences script — which never touches the Tauri
/// bridge — so a restored reading mode can't strand the reader even if
/// the main script fails.
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

    let head_end = HTML.find("</head>").unwrap();
    let head_script = &HTML[HTML.find("<script>").unwrap()..head_end];
    assert!(
        !head_script.contains("__TAURI__.core"),
        "the preferences script must not depend on the Tauri bridge"
    );
    for wiring in [
        "byId('reader-exit')",
        "byId('aa-toggle')",
        "event.key !== 'Escape'",
        "gospel-getter-reader-mode",
    ] {
        assert!(
            head_script.contains(wiring),
            "preferences script should contain `{wiring}`"
        );
    }
}

/// The opening tag of the element with `id`.
fn tag_of(id: &str) -> &'static str {
    let at = HTML
        .find(&format!(r#"id="{id}""#))
        .unwrap_or_else(|| panic!("expected an element #{id}"));
    let tag = &HTML[HTML[..at].rfind('<').unwrap()..];
    &tag[..tag.find('>').unwrap()]
}

/// Each header button announces what it opens and whether it's open, and
/// what it opens says what it is.
#[test]
fn header_buttons_announce_what_they_open() {
    for (toggle, surface, role, label) in [
        (
            "aa-toggle",
            "aa-popover",
            r#"role="dialog""#,
            "Text and display",
        ),
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
    assert!(HTML.contains("preview.dataset.themePreview = option.value;"));
    assert!(HTML.contains("optionsOf(themeSelect).forEach("));
}

/// About's Release notes link shows the same URL the backend opens.
#[test]
fn release_notes_link_matches_the_backend() {
    let prefix = gospel_getter_lib::updater::RELEASE_NOTES_PREFIX;
    assert!(HTML.contains(&format!("`{prefix}${{version}}`")));
    assert!(HTML.contains("invoke('open_release_notes')"));
}

/// Arrow keys pressed on a focused settings `<select>` change that
/// select, and must not also turn the page to another chapter.
#[test]
fn arrow_key_navigation_ignores_focused_form_controls() {
    assert!(HTML.contains("/^(INPUT|TEXTAREA|SELECT)$/.test(event.target.tagName)"));
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
        assert!(HTML.contains(&format!(r#"class="{class}""#)), "{class}");
    }
    assert!(HTML.contains(r#"<div id="note-overlay" class="modal-overlay" hidden>"#));
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
    assert!(HTML.contains("renamed: { 'monster-hunter': 'beast-slayer' }"));
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
    assert!(HTML.contains(
        "refText.textContent = `${place} · ${codes.map((c) => c.toUpperCase()).join(' + ')}`;"
    ));
    assert!(HTML.contains(
        "refBtn.setAttribute('aria-label', `${place}, ${spoken}. Choose a book and chapter`);"
    ));
    // Updated whenever a chapter is put on screen.
    let render = block_after(HTML, "function renderCurrent()");
    assert!(render.contains("updateReference(compare);"));
    assert!(render.contains("markPickerCurrent();"));
    // G opens the picker, in the main keyboard handler, which leaves keys
    // typed into a form control alone.
    assert!(
        HTML.contains(
            "if (plain && !event.shiftKey && (event.key === 'g' || event.key === 'G')) {"
        )
    );
}

/// The picker is one `<nav>` landmark, "Books and chapters": Random
/// chapter, then each testament's books under its heading. As a popover
/// it's a dialog (the preferences script sets the role), and both the
/// reference button and the sidebar's button control it.
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
    for wiring in [
        "picker.setAttribute('role', 'dialog');",
        "picker.removeAttribute('role');",
        "refBtn.setAttribute('aria-haspopup', 'dialog');",
        "picker: { el: picker, toggle: refBtn, popover: true },",
        "key: 'gospel-getter-sidebar',",
    ] {
        assert!(head.contains(wiring), "the preferences script: {wiring}");
    }
    // A book is a disclosure for its chapters, and the chapter and book
    // being read are marked.
    assert!(HTML.contains(r#"class="book-item" data-book-id="${book.id}" data-chapter-count="${book.chapterCount}" aria-expanded="false""#));
    assert!(HTML.contains("button.setAttribute('aria-controls', 'picker-chapters');"));
    assert!(HTML.contains("grid.setAttribute('role', 'group');"));
    assert!(HTML.contains("book.setAttribute('aria-current', 'true');"));
    assert!(HTML.contains(r#"${n === current ? ' aria-current="page"' : ''}"#));
}

/// Psalms (and any book over 50 chapters) gets a compact numeric grid, ten
/// to a row; the rest get round pills, six to a row. The chapter and book
/// being read are marked in `--accent`, with no glow.
#[test]
fn psalms_get_a_compact_grid_ten_to_a_row() {
    let rules = rules();
    assert!(HTML.contains("const COMPACT_OVER = 50;"));
    assert!(HTML.contains("const COMPACT_COLUMNS = 10;"));
    assert!(HTML.contains("const PILL_COLUMNS = 6;"));
    assert!(HTML.contains("const compact = count > COMPACT_OVER;"));
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
    assert!(HTML.contains("var WIDE = '(min-width: 1200px)';"));
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
    // Reading mode never shows the sidebar (see sidebarShown).
    assert!(
        HTML.contains("return isWide() && data.sidebar !== 'closed' && data.readerMode !== 'on';")
    );
    // It's a setting for this window, not exported with the display ones.
    let names = block_after(HTML, "var FILE_NAMES =");
    assert!(!names.contains("sidebar"));
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

/// The script's source from `needle` to the end of that function's body.
fn function_body(name: &str) -> &'static str {
    block_after(HTML, &format!("function {name}("))
}

/// The chapter ends with a next step: the backend's previous and next
/// chapters (so books are crossed) as two large buttons in the UI sans,
/// straight after the verses (or the compare grid), in a labelled
/// navigation landmark; only the one there is at Genesis 1 and
/// Revelation 22. The keys' hint is one quiet line under them, inside the
/// chapter.
#[test]
fn the_chapter_ends_with_buttons_to_the_chapters_around_it() {
    let end = function_body("renderChapterEnd");
    assert!(end.contains(r#"<nav class="chapter-end" aria-label="Previous and next chapter">"#));
    assert!(
        end.contains("if (!side) return '';"),
        "one button per neighbour there is"
    );
    assert!(end.contains("step(dto.prev, 'prev')}${step(dto.next, 'next')"));
    assert!(end.contains(r#"class="chapter-step chapter-step-${which}""#));
    assert!(end.contains(r#"data-book="${side.bookId}" data-chapter="${side.chapter}""#));
    assert!(end.contains(r#"<p class="nav-hint">"#));
    // Its four keys are caps alike; the arrows are drawn, and named.
    for key in [
        "${hintKey('Left arrow', ARROW_LEFT)}",
        "${hintKey('Right arrow', ARROW_RIGHT)}",
        "${hintKey('/')}",
        "${hintKey('C')}",
    ] {
        assert!(end.contains(key), "{key}");
    }
    let caps = function_body("hintKey");
    assert!(caps.contains(r#"<kbd><span class="visually-hidden">${name}</span>${drawn}</kbd>"#));
    assert!(HTML.contains(r#"const ARROW_LEFT = '<svg viewBox="0 0 16 16" aria-hidden="true""#));
    let pane = function_body("renderReadingPane");
    let body = pane.find("${body}").expect("the verses");
    let after = pane.find("${renderChapterEnd(dto)}").expect("the buttons");
    let lists = pane.find("${renderFootnoteSummary(dto, compare)}").unwrap();
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
    let keys = block_after(HTML, "const DISCLOSURE_KEYS =");
    assert!(keys.contains("footnotes: 'gospel-getter-footnotes'"));
    assert!(keys.contains("xrefs: 'gospel-getter-cross-references'"));
    let open = function_body("disclosureOpen");
    assert!(open.contains("=== 'open'"), "closed unless saved open");
    assert!(
        open.contains("return false;"),
        "closed when storage can't be read"
    );
    let toggle = function_body("disclosureToggle");
    assert!(toggle.contains(r#"<button type="button" class="disclosure-toggle""#));
    assert!(toggle.contains(r#"aria-expanded="${disclosureOpen(kind)}""#));
    assert!(toggle.contains(r#"aria-controls="${controls}""#));
    for (function, kind, title) in [
        (
            "renderFootnoteSummary",
            "footnotes",
            "`Footnotes (${count})`",
        ),
        (
            "renderXrefSummary",
            "xrefs",
            "`Cross references (${count})`",
        ),
    ] {
        let body = function_body(function);
        assert!(
            body.contains(&format!("disclosureToggle('{kind}'")),
            "{function}"
        );
        assert!(body.contains(title), "{function}");
        assert!(
            body.contains(&format!("disclosureOpen('{kind}') ? '' : ' hidden'")),
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
    let set = function_body("setDisclosure");
    assert!(set.contains("localStorage.setItem(DISCLOSURE_KEYS[kind]"));
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
/// --text-dim, its four keys as caps of --text-dim on their own opaque
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
            ".nav-hint kbd",
            &["color: var(--text-dim)", "background: var(--option-bg)"][..],
        ),
        (".nav-hint kbd svg", &["stroke: currentColor"][..]),
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
    assert!(HTML.contains("selectionCard.id = 'selection-card';"));
    assert!(HTML.contains("selectionCard.setAttribute('role', 'group');"));
    assert!(HTML.contains("selectionCard.append(verseActions, cardFootnotes, cardXrefs);"));
    let render = function_body("renderSelection");
    assert!(render.contains("selectionCard.setAttribute('aria-label', `Selection: ${citation}`);"));
    assert!(
        render.contains("last.after(selectionCard);"),
        "after the last selected verse"
    );
    assert!(
        !render.contains("last.append("),
        "nothing goes inside a verse"
    );
    assert!(
        render.contains("verse.focus({ preventScroll: true });"),
        "focus returns to the verse"
    );
    assert!(
        HTML.contains("cardXrefs.before(notesSection);"),
        "notes before the cross-references"
    );
    assert!(!HTML.contains("renderVersePopup"), "verses carry no popup");
    let chips = function_body("renderCardReferences");
    assert!(chips.contains(r#"<ul class="xref-chips""#));
    assert!(chips.contains(r#"<li><button type="button" class="xref-citation""#));
    let place = function_body("placeCard");
    for rule in [
        "below < wanted",
        "above >= wanted",
        "classList.add('is-above')",
        "classList.add('is-docked')",
    ] {
        assert!(place.contains(rule), "placeCard: {rule}");
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
    assert!(block_after(HTML, "var FILE_NAMES =").contains("width: 'width',"));

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

/// Ctrl+= (and Ctrl++, numpad +) and Ctrl+- (numpad -) step the text size,
/// Ctrl+0 resets it, through Aa's control; their default (the page zoom) is
/// always prevented, and in a field they change nothing. A brief status
/// says the new size, politely.
#[test]
fn the_text_size_keys_step_aa_and_never_zoom_the_page() {
    let keys = function_body("sizeKey");
    for key in [
        "key === '='",
        "key === '+'",
        "code === 'NumpadAdd'",
        "key === '-'",
        "code === 'NumpadSubtract'",
        "code === 'Numpad0'",
    ] {
        assert!(keys.contains(key), "{key}");
    }
    let at = HTML.find("var step = sizeKey(event);").unwrap();
    let body = &HTML[at..at + HTML[at..].find("showTextSize();").unwrap()];
    let prevent = body.find("event.preventDefault();").unwrap();
    let typing = body.find("INPUT|TEXTAREA|SELECT").unwrap();
    assert!(
        prevent < typing,
        "the page zoom is prevented even in a field"
    );
    assert!(body.contains("choose(textSizeSelect, 'medium');"));
    assert!(body.contains("stepTextSize(step);"));
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
    let pane = function_body("renderReadingPane");
    assert!(pane.contains("${s ? renderSideChapter(s) : renderEdgeCard(edge)}"));
    assert!(pane.contains("side(dto.prev, dto.prevEdge, 'prev')"));
    assert!(pane.contains("side(dto.next, dto.nextEdge, 'next')"));
    let card = function_body("renderEdgeCard");
    assert!(card.contains("escapeHtml(edge.title)") && card.contains("escapeHtml(edge.detail)"));
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
    assert!(block_after(HTML, "var FILE_NAMES =").contains("layout: 'layout',"));
    assert!(HTML.contains("btn.disabled = comparing;"));
    assert!(HTML.contains("layoutNote.hidden = !comparing;"));

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
    let pane = function_body("renderReadingPane");
    assert!(pane.contains("? renderCompareGrid(dto, compare)"));
    assert!(pane.contains(r#": `<div class="chapter-text">${dto.currentVerses.map("#));
    assert!(!function_body("renderCompareGrid").contains("chapter-text"));
    assert!(
        function_body("renderSideChapter").contains(r#"<div class="chapter-text">${verses}</div>"#)
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
        r#":root[data-layout="flowing"] .chapter-main .chapter-text > .verse:hover"#,
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

/// The card's place in Flowing: under the last selected verse's last line
/// and above the first one's first line (their line boxes, not the whole
/// run's box), from the column's left edge. A new layout re-places it.
#[test]
fn the_selection_card_anchors_to_line_boxes_in_flowing() {
    let place = function_body("placeCard");
    assert!(place.contains("const firstRect = lineRect(first, 'first');"));
    assert!(place.contains("const lastRect = lineRect(cardVerse, 'last');"));
    assert!(place.contains("const left = flowing ? 0 :"));
    let line = function_body("lineRect");
    assert!(line.contains("verse.getClientRects()"));
    assert!(HTML.contains("document.addEventListener('gospel-getter:layout-change', placeCard);"));
    assert!(HTML.contains("keepReadingPlace(change);"));
}
