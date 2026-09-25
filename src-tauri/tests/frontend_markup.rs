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
/// chrome (settings, header, book list, chapter chooser, neighboring
/// chapter panes, nav hint, footer, cross-reference popups) but must
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
        ".settings-toggle",
        ".settings-menu",
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
