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
/// the current chapter, not scroll past the full previous one first. This
/// pins down the `@media (max-width: 800px)` rule that hides
/// `.chapter-side` (the prev/next chapter columns) rather than just
/// re-stacking them under `.chapter-main`.
#[test]
fn mobile_breakpoint_hides_chapter_side_neighbors() {
    let block = block_after(HTML, "@media (max-width: 800px)");

    assert!(
        block.contains(".chapter-side"),
        "mobile breakpoint should target .chapter-side"
    );
    assert!(
        block.contains("display: none"),
        "mobile breakpoint should hide .chapter-side instead of just \
         restacking it, so only the current chapter is visible"
    );
}

/// Regression guard for GH-6: printing a chapter should hide all page
/// chrome (the top bar and its popovers, the Library, the dialogs and the
/// toast, header, book list, chapter chooser, neighboring chapter panes,
/// nav hint, footer, cross-reference popups) but must
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
        "header",
        ".book-list",
        "#chapter-list",
        ".chapter-side",
        ".nav-hint",
        "footer",
        ".xref-popup",
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
    for command in ["get_home", "get_chapters", "get_reading", "get_xref_text"] {
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
/// Aa (`[data-theme-preview="..."]`), so the block's selector list is
/// exactly the two.
fn theme_rule<'a>(rules: &'a [Rule], theme: &str, default_theme: &str) -> &'a Rule {
    let page = if theme == default_theme {
        ":root".to_owned()
    } else {
        format!(r#":root[data-theme="{theme}"]"#)
    };
    let preview = format!(r#"[data-theme-preview="{theme}"]"#);
    rules
        .iter()
        .find(|r| r.at_rule.is_none() && selectors(r).eq([page.as_str(), preview.as_str()]))
        .unwrap_or_else(|| panic!("expected a `{page}, {preview}` rule"))
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

/// Palette for each theme the picker offers (see `theme_rule`).
fn theme_palettes(rules: &[Rule]) -> Vec<(String, Vec<(String, String)>)> {
    let default_theme = default_theme();
    select_options("theme-select")
        .into_iter()
        .map(|(value, _)| {
            let props = custom_properties(&theme_rule(rules, &value, &default_theme).body);
            (value, props)
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
        .filter(|name| !name.starts_with("--reading-"))
        .collect();

    for (theme, props) in theme_palettes(&rules) {
        for name in &color_props {
            assert!(
                props.iter().any(|(n, _)| n == name),
                "theme `{theme}` should define `{name}`"
            );
        }
    }
    for (theme, _) in theme_palettes(&rules) {
        let rule = theme_rule(&rules, &theme, &default_theme());
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
    for id in ["theme-select", "text-size-select", "line-spacing-select"] {
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
    let root = custom_properties(&theme_rule(&rules, &default_theme(), &default_theme()).body);
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
/// numbers, chapter headings and compare mode's "not numbered" cells
/// (which sit in a row beside a verse) — never to the rest of the UI (so
/// the settings menu, book list etc. don't grow with it).
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
                    ".verse-num",
                    ".chapter-heading",
                    ".compare-gap"
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
        "header",
        ".book-list",
        "#chapter-list",
        ".chapter-side",
        ".nav-hint",
        "footer",
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
    assert!(library.contains(r#"aria-label="Library: bookmarks and highlights""#));
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
/// buttons' text and their open state AA on it in every theme; and they
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
                accent.to_owned(),
                composite(property(&props, "--panel-bg"), bar_bg),
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
