//! The header surfaces' rules, free of the DOM so they are tested natively
//! (`cargo test`): the seven surfaces, their elements, buttons and first
//! focus; when the book and chapter picker is the sidebar; and the ☰
//! menu's keys. They are the head script's (`ui/index.html` at 49736df:
//! `SURFACES`, `sidebarShown`, `focusInto` and the menu's keydown), rule
//! for rule.

/// One of the header's popovers and panels. Only one is open at a time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Surface {
    /// The translation button's popover.
    Translation,
    /// Aa (text and display).
    Aa,
    /// The Library panel.
    Library,
    /// The ☰ menu.
    Menu,
    /// About Gospel Getter, opened from ☰ (or the palette).
    About,
    /// The book and chapter picker as a popover. As the sidebar it isn't a
    /// surface: it stays while the reader reads, beside any of these.
    Picker,
    /// Recent places, the menu from ▾ between Back and Forward (`nav`).
    Recent,
}

impl Surface {
    /// Every surface.
    #[cfg(test)]
    pub const ALL: [Surface; 7] = [
        Surface::Translation,
        Surface::Aa,
        Surface::Library,
        Surface::Menu,
        Surface::About,
        Surface::Picker,
        Surface::Recent,
    ];

    /// The page's name for it, as `gospel-getter:surface-open` and
    /// `-close` say it and `gospelGetterSurfaces.current()` answers.
    pub fn name(self) -> &'static str {
        match self {
            Surface::Translation => "translation",
            Surface::Aa => "aa",
            Surface::Library => "library",
            Surface::Menu => "menu",
            Surface::About => "about",
            Surface::Picker => "picker",
            Surface::Recent => "recent",
        }
    }

    /// Its element's id. About's is this crate's (`about`), made when the
    /// module loads.
    pub fn element_id(self) -> &'static str {
        match self {
            Surface::Translation => "translation-popover",
            Surface::Aa => "aa-popover",
            Surface::Library => "library-panel",
            Surface::Menu => "app-menu",
            Surface::About => "about-overlay",
            Surface::Picker => "picker",
            Surface::Recent => "recent-places-menu",
        }
    }

    /// Its own button, which opens and closes it and says whether it's open
    /// (`aria-expanded`); About has none (it opens from ☰).
    pub fn toggle_id(self) -> Option<&'static str> {
        match self {
            Surface::Translation => Some("translation-toggle"),
            Surface::Aa => Some("aa-toggle"),
            Surface::Library => Some("library-toggle"),
            Surface::Menu => Some("menu-toggle"),
            Surface::About => None,
            Surface::Picker => Some("ref-btn"),
            Surface::Recent => Some("recent-places-toggle"),
        }
    }

    /// A menu (☰, Recent places): ↑ and ↓ move between its items, Tab
    /// closes it, and focus moves without scrolling the page.
    pub fn menu(self) -> bool {
        matches!(self, Surface::Menu | Surface::Recent)
    }

    /// A popover, which a click outside it (and its button) closes. The
    /// Library stays open while the reader reads, and About is modal.
    pub fn popover(self) -> bool {
        !matches!(self, Surface::Library | Surface::About)
    }

    /// Where focus goes as it opens: the first of these, inside it, that
    /// is there. The translation that's chosen (or the first button); Aa's
    /// first enabled button; the Library's chosen tab; the menu's first
    /// enabled item; the chapter being read in the picker, or its book, or
    /// Random chapter; About's ×.
    pub fn first_focus(self) -> &'static [&'static str] {
        match self {
            Surface::Translation => &[r#".translation-choice[aria-checked="true"]"#, "button"],
            Surface::Aa => &["button:not(:disabled)"],
            Surface::Library => &[r#"[role="tab"][aria-selected="true"]"#],
            Surface::Menu | Surface::Recent => &[MENU_ITEMS],
            Surface::About => &["#about-close"],
            Surface::Picker => &[
                r#".chapter-btn[aria-current="page"]"#,
                r#".book-item[aria-current="true"]"#,
                "#random-chapter-btn",
            ],
        }
    }
}

/// The ☰ menu's items that can be chosen.
pub const MENU_ITEMS: &str = r#"[role="menuitem"]:not(:disabled)"#;

/// Whether the picker is the sidebar: on a wide window, unless the reader
/// hid it (`html[data-sidebar]` is `closed`); never in reading mode
/// (`html[data-reader-mode]` is `on`).
pub fn sidebar_shown(wide: bool, sidebar: Option<&str>, reader_mode: Option<&str>) -> bool {
    wide && sidebar != Some("closed") && reader_mode != Some("on")
}

/// What a key does in the ☰ menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuKey {
    /// Focus the item at this index (↓ and ↑ wrap; Home, End). The key is
    /// the menu's: no default, and no other handler sees it.
    Move(usize),
    /// One of the menu's moving keys with nothing to move to (no items):
    /// still the menu's.
    Nowhere,
    /// Tab: the menu closes, focus staying put.
    Close,
    /// ← and →: not a chapter turn while in the menu, but otherwise as
    /// they are (no other handler sees them).
    Keep,
    /// Anything else, as it is.
    Pass,
}

/// What `key` does among `count` items, focus on the one at `index` (or on
/// none of them). As the head script's: ↑ from outside the items lands one
/// before the last.
pub fn menu_key(key: &str, index: Option<usize>, count: usize) -> MenuKey {
    // The head script's `items.indexOf(activeElement)`: -1 when outside.
    let at = index.map_or(-1, |i| i as i64);
    let len = count as i64;
    let to = |i: i64| {
        usize::try_from(i)
            .ok()
            .filter(|&i| i < count)
            .map_or(MenuKey::Nowhere, MenuKey::Move)
    };
    match key {
        "ArrowDown" if len > 0 => to((at + 1) % len),
        "ArrowUp" if len > 0 => to((at - 1 + len) % len),
        "Home" => to(0),
        "End" => to(len - 1),
        "ArrowDown" | "ArrowUp" => MenuKey::Nowhere,
        "Tab" => MenuKey::Close,
        "ArrowLeft" | "ArrowRight" => MenuKey::Keep,
        _ => MenuKey::Pass,
    }
}

/// The first of the marks (each mark's bottom edge, in order) that reaches
/// below the bar's bottom: the one whose place a reflow keeps.
pub fn first_below(bar_bottom: f64, bottoms: impl IntoIterator<Item = f64>) -> Option<usize> {
    bottoms.into_iter().position(|bottom| bottom > bar_bottom)
}

/// The reader's place in a painted layout scrolled to `y`, as
/// [`first_below`] would find it on screen there: of the `marks` (each
/// mark's top and bottom in the page, which scrolling doesn't change, in
/// order), the first that reaches below the bar's bottom (`bar_bottom`, on
/// screen), and its top on screen.
pub fn place_at(bar_bottom: f64, marks: &[(f64, f64)], y: f64) -> Option<(usize, f64)> {
    let index = first_below(bar_bottom, marks.iter().map(|&(_, bottom)| bottom - y))?;
    Some((index, marks[index].0 - y))
}

/// The scroll position to read the painted layout at, as the window
/// changes shape: `now`, unless the browser scrolls to keep a place of its
/// own on a resize (`anchors`), and so may have since the resize began:
/// then the one the page saw before it (`before_resize`), if known.
pub fn scroll_read_at(anchors: bool, before_resize: Option<f64>, now: f64) -> f64 {
    match before_resize {
        Some(y) if anchors => y,
        _ => now,
    }
}

/// The scroll position the page had before Chromium's scroll anchoring
/// scrolled it for a resize, found from where its anchor is now. Chromium
/// keeps the first heading or verse of the reading row that reaches below
/// the bar (`bar_bottom`, its scroll padding) where it was on screen: the
/// faded chapter beside first, as it comes first in the page. Of the
/// row's marks (`painted`: each one's top and bottom in the painted page,
/// in order; `now`: each one's top and bottom on screen now, none if it's
/// gone), it's the first on screen now (above `height`) that, scrolled
/// back to where it was, is the first to reach below the bar. Chromium
/// scrolls by whole pixels, so it can be half a pixel off where it was.
/// None if no mark is.
pub fn scroll_before_anchoring(
    bar_bottom: f64,
    height: f64,
    painted: &[(f64, f64)],
    now: &[Option<(f64, f64)>],
) -> Option<f64> {
    let first_at = |y: f64| first_below(bar_bottom, painted.iter().map(|&(_, bottom)| bottom - y));
    painted
        .iter()
        .zip(now)
        .enumerate()
        .find_map(|(index, (&(page_top, _), now))| {
            let &(top, bottom) = now.as_ref()?;
            if bottom <= bar_bottom || top >= height {
                return None;
            }
            let y = page_top - top;
            [y, y - 0.5, y + 0.5]
                .into_iter()
                .any(|at| first_at(at) == Some(index))
                .then_some(y)
        })
}

/// Where to say a mark was, to put it back `wanted` px down the screen
/// from `now`: the nearest top a whole-pixel scroll reaches. WebKitGTK
/// scrolls by whole pixels and drops a fraction, so a scroll of 889.998px
/// would be 889px, a pixel short.
pub fn whole_pixel_top(now: f64, wanted: f64) -> f64 {
    now - (now - wanted).round()
}

/// How far to scroll to put back a mark that was `before` px down the
/// screen and now is `after`: nothing if it hasn't moved (or a box gave no
/// number).
pub fn scroll_shift(before: f64, after: f64) -> Option<f64> {
    let shift = after - before;
    (shift != 0.0 && !shift.is_nan()).then_some(shift)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_surfaces_elements_and_buttons() {
        let table: Vec<_> = Surface::ALL
            .iter()
            .map(|s| (s.name(), s.element_id(), s.toggle_id(), s.popover()))
            .collect();
        assert_eq!(
            table,
            [
                (
                    "translation",
                    "translation-popover",
                    Some("translation-toggle"),
                    true
                ),
                ("aa", "aa-popover", Some("aa-toggle"), true),
                ("library", "library-panel", Some("library-toggle"), false),
                ("menu", "app-menu", Some("menu-toggle"), true),
                ("about", "about-overlay", None, false),
                ("picker", "picker", Some("ref-btn"), true),
                (
                    "recent",
                    "recent-places-menu",
                    Some("recent-places-toggle"),
                    true
                ),
            ]
        );
    }

    #[test]
    fn where_focus_goes_first() {
        assert_eq!(
            Surface::Translation.first_focus(),
            [r#".translation-choice[aria-checked="true"]"#, "button"]
        );
        assert_eq!(Surface::Menu.first_focus(), [MENU_ITEMS]);
        assert_eq!(Surface::Recent.first_focus(), [MENU_ITEMS]);
        let menus: Vec<_> = Surface::ALL.iter().filter(|s| s.menu()).collect();
        assert_eq!(menus, [&Surface::Menu, &Surface::Recent]);
        assert_eq!(Surface::About.first_focus(), ["#about-close"]);
        assert_eq!(
            Surface::Picker.first_focus().last(),
            Some(&"#random-chapter-btn")
        );
    }

    #[test]
    fn the_sidebar_shows_on_a_wide_window_unless_hidden_or_reading() {
        assert!(sidebar_shown(true, None, None));
        assert!(sidebar_shown(true, Some("open"), Some("off")));
        assert!(!sidebar_shown(false, None, None));
        assert!(!sidebar_shown(true, Some("closed"), None));
        assert!(!sidebar_shown(true, None, Some("on")));
    }

    #[test]
    fn the_menu_arrows_wrap() {
        assert_eq!(menu_key("ArrowDown", Some(0), 5), MenuKey::Move(1));
        assert_eq!(menu_key("ArrowDown", Some(4), 5), MenuKey::Move(0));
        assert_eq!(menu_key("ArrowUp", Some(0), 5), MenuKey::Move(4));
        assert_eq!(menu_key("ArrowUp", Some(3), 5), MenuKey::Move(2));
        assert_eq!(menu_key("Home", Some(3), 5), MenuKey::Move(0));
        assert_eq!(menu_key("End", Some(1), 5), MenuKey::Move(4));
    }

    #[test]
    fn the_menu_arrows_from_outside_its_items() {
        assert_eq!(menu_key("ArrowDown", None, 5), MenuKey::Move(0));
        // (-1 - 1 + 5) % 5, as the head script had it.
        assert_eq!(menu_key("ArrowUp", None, 5), MenuKey::Move(3));
        assert_eq!(menu_key("ArrowUp", None, 1), MenuKey::Move(0));
    }

    #[test]
    fn the_menu_with_no_items() {
        for key in ["ArrowDown", "ArrowUp", "Home", "End"] {
            assert_eq!(menu_key(key, None, 0), MenuKey::Nowhere, "{key}");
        }
    }

    #[test]
    fn the_menu_other_keys() {
        assert_eq!(menu_key("Tab", Some(1), 5), MenuKey::Close);
        assert_eq!(menu_key("ArrowLeft", Some(1), 5), MenuKey::Keep);
        assert_eq!(menu_key("ArrowRight", None, 5), MenuKey::Keep);
        assert_eq!(menu_key("Enter", Some(1), 5), MenuKey::Pass);
        assert_eq!(menu_key("Escape", Some(1), 5), MenuKey::Pass);
        assert_eq!(menu_key("a", Some(1), 5), MenuKey::Pass);
    }

    #[test]
    fn the_mark_kept_in_place() {
        assert_eq!(first_below(56.0, [10.0, 56.0, 57.0, 200.0]), Some(2));
        assert_eq!(first_below(56.0, [100.0]), Some(0));
        assert_eq!(first_below(56.0, [0.0, 0.0]), None);
        assert_eq!(first_below(56.0, []), None);
    }

    #[test]
    fn the_place_in_a_painted_layout_at_a_scroll_position() {
        // A heading and three verses, by their place in the page; the bar
        // ends 54px down the screen.
        let marks = [
            (0.0, 40.0),
            (30.0, 70.0),
            (2980.0, 3020.0),
            (3030.0, 3070.0),
        ];
        assert_eq!(place_at(54.0, &marks, 0.0), Some((1, 30.0)));
        // Scrolled 3000px after the layout was painted: the verse at 3030
        // is 30px down, as on screen; the one at 2980 ends above the bar.
        assert_eq!(place_at(54.0, &marks, 3000.0), Some((3, 30.0)));
        assert_eq!(place_at(54.0, &marks, 2960.0), Some((2, 20.0)));
        // Nothing below the bar, or nothing painted.
        assert_eq!(place_at(54.0, &marks, 5000.0), None);
        assert_eq!(place_at(54.0, &[], 0.0), None);
    }

    #[test]
    fn the_place_before_chromium_scrolled_for_the_resize() {
        // Psalms 119:30 to 35 by their place in the page, a 1280px window
        // with the sidebar (WebView2, a line 28.56px); the bar ends 54.4px
        // down the screen. At scrollY 2000, verse 33 is 45.3px down.
        let marks = [
            (1850.0, 1907.1),
            (1915.1, 1972.2),
            (1980.2, 2037.3),
            (2045.3, 2102.4),
            (2110.4, 2167.5),
            (2175.5, 2232.6),
        ];
        // Narrowed to 1000px, Chromium has scrolled to 2115 for a place of
        // its own (a line of Psalms 118, beside): the place is still
        // verse 33's, from the scroll seen before the resize.
        let y = scroll_read_at(true, Some(2000.0), 2115.0);
        assert_eq!(y, 2000.0);
        let (index, top) = place_at(54.4, &marks, y).unwrap();
        assert_eq!(index, 3);
        assert!((top - 45.3).abs() < 1e-9, "{top}");
        // Read at 2115, it would be verse 35.
        assert_eq!(place_at(54.4, &marks, 2115.0).map(|(i, _)| i), Some(5));
        // WebKitGTK leaves the scroll position alone: now is the reader's,
        // even if the page was scrolled after the resize's frame began.
        assert_eq!(scroll_read_at(false, Some(2000.0), 2115.0), 2115.0);
        // No resize seen: now.
        assert_eq!(scroll_read_at(true, None, 2115.0), 2115.0);
    }

    #[test]
    fn the_scroll_before_chromium_anchored_after_a_scroll_it_hadnt_seen() {
        // The reading row by page position, as painted at 1280px with the
        // sidebar: Psalms 118:22 (faded, beside: it comes first), then
        // Psalms 119:31 to 33; the bar ends 54.4px down the screen.
        let painted = [
            (2024.1, 2093.3),
            (1915.1, 1972.2),
            (1980.2, 2037.3),
            (2045.3, 2102.4),
        ];
        // Scrolled to 2000 with no frame before the window narrowed to
        // 1000px: Chromium kept 118:22 24.3px down and scrolled to 2115.
        // Laid out narrower, 119:33 is now 215px down.
        let now = [
            Some((24.3, 93.5)),
            Some((-180.0, -120.0)),
            Some((-110.0, -50.0)),
            Some((215.0, 280.0)),
        ];
        let y = scroll_before_anchoring(54.4, 681.0, &painted, &now).unwrap();
        assert!((y - 1999.8).abs() < 1e-9, "{y}");
        // There, the reader's place is 119:33, 45.5px down.
        let main = &painted[1..];
        let (index, top) = place_at(54.4, main, y).unwrap();
        assert_eq!(index, 2);
        assert!((top - 45.5).abs() < 1e-9, "{top}");

        // A mark on screen now that wasn't the first below the bar where
        // it says the page was isn't the anchor: 119:33 alone says 1830.3,
        // where 118:22 was first.
        let alone = [None, None, None, Some((215.0, 280.0))];
        assert_eq!(scroll_before_anchoring(54.4, 681.0, &painted, &alone), None);
        // Chromium scrolls by whole pixels: a verse that ended 0.2px below
        // the bar, kept 0.3px higher and taller now, still says 2000.3.
        let edge = [(1990.0, 2054.6)];
        let y = scroll_before_anchoring(54.4, 681.0, &edge, &[Some((-10.3, 80.0))]).unwrap();
        assert!((y - 2000.3).abs() < 1e-9, "{y}");
        assert_eq!(first_below(54.4, [edge[0].1 - y]), None);
        // Below the window, or nothing painted: none.
        let below = [Some((700.0, 760.0)), None, None, None];
        assert_eq!(scroll_before_anchoring(54.4, 681.0, &painted, &below), None);
        assert_eq!(scroll_before_anchoring(54.4, 681.0, &[], &[]), None);
    }

    #[test]
    fn the_scroll_that_keeps_it() {
        assert_eq!(scroll_shift(30.0, 287.0), Some(257.0));
        assert_eq!(scroll_shift(55.0, -202.0), Some(-257.0));
        assert_eq!(scroll_shift(29.7, 29.7), None);
        assert_eq!(scroll_shift(f64::NAN, 10.0), None);
        assert_eq!(scroll_shift(10.0, f64::NAN), None);
    }

    #[test]
    fn a_place_is_put_back_by_a_whole_pixel_scroll() {
        // 943.53 down, wanted 53.53 above a 54px bar's bottom (0.47): a
        // scroll of 943.06 would be 943 in WebKitGTK; 943 is asked for.
        let top = whole_pixel_top(943.53, 0.47);
        assert_eq!(scroll_shift(top, 943.53), Some(943.0));
        assert!((top - 0.53).abs() < 1e-9);
        // Already whole: unchanged.
        assert_eq!(whole_pixel_top(100.0, 40.0), 40.0);
        assert_eq!(scroll_shift(whole_pixel_top(40.0, 40.0), 40.0), None);
    }
}
