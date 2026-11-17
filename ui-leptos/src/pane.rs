//! The reading pane: its elements, and what scrolls it. Every read or write
//! of the reading view's scroll position, and of the band of it in view,
//! goes through a pane's [`Scroller`], and every reading-view query runs on
//! the pane's root ([`PaneId::root`]), not on the document. This is the one
//! place that knows where the reading view scrolls and where its elements
//! are, so a pane that scrolls in an element of its own changes this module,
//! not its callers. `src-tauri/tests/frontend_markup.rs` keeps the window's
//! scroll calls here (`reading_view_scroll_stays_in_pane`).
//!
//! The selection card, the picker, search, the dialogs and the card's own
//! scrolling aren't the reading view's: they scroll themselves.
//!
//! What each pane shows is its own (its chapter, its loads, its history and
//! place, the card's and the findings' facts about its chapter): the pieces
//! keep it in a [`PerPane`], reached by the pane's [`PaneId`]. What acts on
//! a pane without being in one (the keys, the palette, the header) acts on
//! the [`active`] one; what follows every pane goes to each [`shown`].

use std::ops::{Index, IndexMut};

use wasm_bindgen::{JsCast, JsValue};
use web_sys::{Element, EventTarget, HtmlElement};

use crate::dom::{self, by_id};

/// A reading pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PaneId {
    /// The page's reading pane: the one the reader opens, whose place the
    /// backend saves.
    First,
}

impl PaneId {
    /// The element the reader draws the pane's chapter into.
    pub fn root(self) -> Option<HtmlElement> {
        match self {
            Self::First => by_id("reading-pane"),
        }
    }

    /// The pane's `#reading-pane-inner`, which says which chapter is on
    /// screen and which are around it (`data-current-*`, `data-prev-*`,
    /// `data-next-*`).
    pub fn inner(self) -> Option<Element> {
        self.root()?
            .query_selector("#reading-pane-inner")
            .ok()
            .flatten()
    }

    /// The notice above the pane's chapter (a chapter that couldn't be read,
    /// a verse the translation doesn't number).
    pub fn notice(self) -> Option<HtmlElement> {
        match self {
            Self::First => by_id("reader-notice"),
        }
    }

    /// The first element in the pane matching `selector`.
    pub fn query(self, selector: &str) -> Option<HtmlElement> {
        self.root()?
            .query_selector(selector)
            .ok()??
            .dyn_into::<HtmlElement>()
            .ok()
    }

    /// Every element in the pane matching `selector`, in page order.
    pub fn query_all(self, selector: &str) -> Vec<HtmlElement> {
        self.root()
            .and_then(|root| root.query_selector_all(selector).ok())
            .map(|found| dom::elements(&found))
            .unwrap_or_default()
    }

    /// What scrolls the pane.
    pub fn scroller(self) -> Scroller {
        match self {
            Self::First => Scroller::Window,
        }
    }
}

/// The pane the keys, the palette, the header and the selection card act
/// on.
pub fn active() -> PaneId {
    PaneId::First
}

/// Every pane on screen, in order.
pub fn shown() -> impl Iterator<Item = PaneId> {
    [PaneId::First].into_iter()
}

/// The pane `el` is in, if it's in one.
pub fn of(el: &Element) -> Option<PaneId> {
    shown().find(|pane| pane.root().is_some_and(|root| root.contains(Some(el))))
}

/// One `T` for each pane, reached by its [`PaneId`].
#[derive(Debug, Default)]
pub struct PerPane<T> {
    first: T,
}

impl<T> PerPane<T> {
    /// Each pane's `T`, made by `make`.
    pub fn new(make: impl Fn(PaneId) -> T) -> Self {
        Self {
            first: make(PaneId::First),
        }
    }
}

impl<T> Index<PaneId> for PerPane<T> {
    type Output = T;

    fn index(&self, pane: PaneId) -> &T {
        match pane {
            PaneId::First => &self.first,
        }
    }
}

impl<T> IndexMut<PaneId> for PerPane<T> {
    fn index_mut(&mut self, pane: PaneId) -> &mut T {
        match pane {
            PaneId::First => &mut self.first,
        }
    }
}

/// What scrolls a reading pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scroller {
    /// The window: the page scrolls, under the fixed top bar.
    Window,
}

/// The band of a scroller in view, in client coordinates: from `top` (the
/// foot of the bar over it) down to `bottom`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct View {
    pub top: f64,
    pub bottom: f64,
}

impl Scroller {
    /// How far it's scrolled down.
    pub fn y(&self) -> Option<f64> {
        match self {
            Self::Window => web_sys::window()?.scroll_y().ok(),
        }
    }

    /// Scrolls it to `y`.
    pub fn scroll_to(&self, y: f64) {
        match self {
            Self::Window => {
                if let Some(window) = web_sys::window() {
                    window.scroll_to_with_x_and_y(0.0, y);
                }
            }
        }
    }

    /// Scrolls it by `dy`.
    pub fn scroll_by(&self, dy: f64) {
        match self {
            Self::Window => {
                if let Some(window) = web_sys::window() {
                    window.scroll_by_with_x_and_y(0.0, dy);
                }
            }
        }
    }

    /// The band of it in view: the window's, below the top bar, which is
    /// fixed over the page's top.
    pub fn view(&self) -> Option<View> {
        match self {
            Self::Window => {
                let top = by_id("top-bar")?.get_bounding_client_rect().bottom();
                let bottom = web_sys::window()?.inner_height().ok()?.as_f64()?;
                Some(View { top, bottom })
            }
        }
    }

    /// What its `scroll` and `scrollend` events are dispatched to.
    pub fn target(&self) -> Option<EventTarget> {
        match self {
            Self::Window => web_sys::window().map(Into::into),
        }
    }

    /// Whether the browser scrolls it to keep a place of its own when the
    /// window is resized: Chromium's scroll anchoring does the window's
    /// (WebView2, on Windows); WebKitGTK's doesn't. Both say they support
    /// `overflow-anchor`, so it's told by Chromium's own `window.chrome`.
    /// Its place needn't be the reader's: it can be a line of the faded
    /// chapter beside, which comes first in the page (`surfaces`'
    /// `on_shape_change` says what's done about it).
    pub fn anchors_on_resize(&self) -> bool {
        match self {
            Self::Window => web_sys::window()
                .and_then(|window| js_sys::Reflect::get(&window, &JsValue::from_str("chrome")).ok())
                .is_some_and(|chrome| chrome.is_truthy()),
        }
    }
}
