//! The main window: its title, and its size and place from the last run.
//!
//! `tauri.conf.json` describes the window, with `create: false` and
//! `visible: false`; [`create_main_window`] builds it from that, restores
//! its size, position and maximized state (through the official
//! `tauri-plugin-window-state`), makes sure it's on a screen that's there
//! and at least its minimum size, and only then shows it, so it never
//! flashes at the default size first.
//!
//! The title is the page's: the frontend sets `document.title` ("John 3 ·
//! KJV — Gospel Getter") with the reference button, and the webview's own
//! title-changed event (WebKitGTK's `notify::title`, WebView2's
//! `DocumentTitleChanged`) copies it to the native window. So the webview
//! needs no window permission.

/// The window's label in `tauri.conf.json` and the capability file.
pub const MAIN: &str = "main";

/// The window state file, in the app's data directory beside the database.
pub const STATE_FILE: &str = "window-state.json";

/// The part of a window's top edge that must be on a screen for the reader
/// to reach it: its title bar, to drag it back. In physical pixels.
const TITLE_STRIP: u32 = 40;

/// How much of that strip, across, must be on one screen.
const GRAB_WIDTH: u32 = 120;

/// A rectangle on the desktop, in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    /// The overlap of `self` and `other`, as (width, height).
    fn overlap(&self, other: &Rect) -> (u32, u32) {
        let span = |a: i32, a_len: u32, b: i32, b_len: u32| {
            let start = i64::from(a.max(b));
            let end = (i64::from(a) + i64::from(a_len)).min(i64::from(b) + i64::from(b_len));
            u32::try_from((end - start).max(0)).unwrap_or(u32::MAX)
        };
        (
            span(self.x, self.width, other.x, other.width),
            span(self.y, self.height, other.y, other.height),
        )
    }
}

/// Whether the reader can reach `window` on one of `screens`: its whole
/// title strip is on one screen, top to bottom, for at least
/// 120px across (or the window's width, if it's narrower).
pub fn reachable(window: Rect, screens: &[Rect]) -> bool {
    let strip = Rect {
        height: TITLE_STRIP.min(window.height),
        ..window
    };
    screens.iter().any(|screen| {
        let (across, down) = strip.overlap(screen);
        across >= GRAB_WIDTH.min(window.width) && down == strip.height
    })
}

/// Where a window that isn't reachable goes: centered on `screen`, no
/// larger than it, and no smaller than `min` (which wins if the screen is
/// smaller still).
pub fn centered_on(window: Rect, screen: Rect, min: (u32, u32)) -> Rect {
    let width = window.width.min(screen.width).max(min.0);
    let height = window.height.min(screen.height).max(min.1);
    let offset = |outer: u32, inner: u32| {
        i32::try_from((i64::from(outer) - i64::from(inner)) / 2).unwrap_or(0)
    };
    Rect {
        x: screen.x + offset(screen.width, width),
        y: screen.y + offset(screen.height, height),
        width,
        height,
    }
}

/// `size` (physical pixels at `scale`), grown if needed to the window's
/// minimum (`min`, in logical pixels). `None` if it's already big enough.
/// A size saved on a screen with a different scale can come back smaller.
pub fn at_least(size: (u32, u32), scale: f64, min: (f64, f64)) -> Option<(f64, f64)> {
    let logical = (f64::from(size.0) / scale, f64::from(size.1) / scale);
    // Rounding to whole physical pixels can leave a hair under the minimum.
    let short = |have: f64, need: f64| have + 0.5 / scale < need;
    (short(logical.0, min.0) || short(logical.1, min.1))
        .then(|| (logical.0.max(min.0), logical.1.max(min.1)))
}

#[cfg(desktop)]
pub use desktop::*;

#[cfg(desktop)]
mod desktop {
    use anyhow::Context;
    use tauri::{LogicalSize, PhysicalPosition, PhysicalSize, WebviewWindowBuilder};
    use tauri_plugin_window_state::{StateFlags, WindowExt};

    use super::{MAIN, Rect, STATE_FILE, at_least, centered_on, reachable};

    /// What comes back on launch: size, position and maximized state. Not
    /// visibility (the window is always shown) or decorations (always the
    /// native title bar).
    pub fn state_flags() -> StateFlags {
        StateFlags::SIZE | StateFlags::POSITION | StateFlags::MAXIMIZED
    }

    /// The window state plugin, keeping its file in `data_dir`. It saves
    /// when the app exits. It doesn't restore the main window by itself:
    /// [`create_main_window`] does, before showing it.
    pub fn state_plugin<R: tauri::Runtime>(
        data_dir: &std::path::Path,
    ) -> anyhow::Result<tauri::plugin::TauriPlugin<R>> {
        // The plugin joins its file name to the config directory; an
        // absolute path replaces that, so the file sits with the database.
        let file = data_dir.join(STATE_FILE);
        let file = file
            .to_str()
            .context("The app data directory isn't valid UTF-8")?;
        Ok(tauri_plugin_window_state::Builder::new()
            .with_state_flags(state_flags())
            .with_filename(file)
            .skip_initial_state(MAIN)
            .build())
    }

    fn rect(position: PhysicalPosition<i32>, size: PhysicalSize<u32>) -> Rect {
        Rect {
            x: position.x,
            y: position.y,
            width: size.width,
            height: size.height,
        }
    }

    /// Build the main window from `tauri.conf.json`, hidden; give it last
    /// run's size and place; then show it.
    pub fn create_main_window(app: &tauri::App) -> anyhow::Result<()> {
        let config = app
            .config()
            .app
            .windows
            .iter()
            .find(|w| w.label == MAIN)
            .context("tauri.conf.json has no main window")?
            .clone();
        let window = WebviewWindowBuilder::from_config(app.handle(), &config)?
            .on_document_title_changed(|window, title| {
                if let Err(e) = window.set_title(&title) {
                    tracing::warn!("Failed to set the window title: {e}");
                }
            })
            .build()
            .context("Failed to create the main window")?;

        // Best-effort: a window at the default size is better than none.
        if let Err(e) = window.restore_state(state_flags()) {
            tracing::warn!("Failed to restore the window's size and position: {e}");
        }
        let min = (
            config.min_width.unwrap_or(config.width),
            config.min_height.unwrap_or(config.height),
        );
        if let Err(e) = keep_on_screen(&window, min) {
            tracing::warn!("Failed to check the window is on screen: {e}");
        }
        window.show()?;
        window.set_focus()?;
        Ok(())
    }

    /// A window restored to a screen that's gone (or mostly off one) moves
    /// to the middle of the primary screen, and one restored smaller than
    /// its minimum grows to it. A maximized window is the window manager's
    /// to place. (The plugin already leaves the position alone when no
    /// screen overlaps the saved window at all; this also catches one
    /// with only a corner on a screen, or its title bar off the top.)
    fn keep_on_screen<R: tauri::Runtime>(
        window: &tauri::WebviewWindow<R>,
        min: (f64, f64),
    ) -> tauri::Result<()> {
        let scale = window.scale_factor()?;
        if let Some((width, height)) = at_least(window.inner_size()?.into(), scale, min) {
            window.set_size(LogicalSize { width, height })?;
        }
        if window.is_maximized()? {
            return Ok(());
        }
        let screens: Vec<Rect> = window
            .available_monitors()?
            .iter()
            .map(|m| rect(m.work_area().position, m.work_area().size))
            .collect();
        let here = rect(window.outer_position()?, window.outer_size()?);
        // No screens reported (some Wayland sessions), or a hidden window
        // GTK hasn't placed yet (it reads as 0x0 at 0,0; GTK and the
        // window manager keep a title bar on screen there): nothing to
        // check. On Windows a hidden window has its real place.
        if screens.is_empty() || here.width == 0 || here.height == 0 || reachable(here, &screens) {
            return Ok(());
        }
        let Some(primary) = window
            .primary_monitor()?
            .or(window.available_monitors()?.pop())
        else {
            return Ok(());
        };
        let area = primary.work_area();
        let min_physical = (
            (min.0 * primary.scale_factor()).ceil() as u32,
            (min.1 * primary.scale_factor()).ceil() as u32,
        );
        let to = centered_on(here, rect(area.position, area.size), min_physical);
        tracing::info!("The window was off screen at {here:?}; moving it to {to:?}");
        window.set_size(PhysicalSize::new(to.width, to.height))?;
        window.set_position(PhysicalPosition::new(to.x, to.y))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LAPTOP: Rect = Rect {
        x: 0,
        y: 0,
        width: 1920,
        height: 1080,
    };
    const RIGHT: Rect = Rect {
        x: 1920,
        y: 0,
        width: 2560,
        height: 1440,
    };

    fn window(x: i32, y: i32) -> Rect {
        Rect {
            x,
            y,
            width: 1280,
            height: 900,
        }
    }

    #[test]
    fn a_window_on_a_screen_stays_where_it_was() {
        assert!(reachable(window(100, 50), &[LAPTOP]));
        // Partly off the side, with plenty of title bar left to drag.
        assert!(reachable(window(1500, 50), &[LAPTOP]));
        assert!(reachable(window(-1000, 0), &[LAPTOP]));
        // Across two screens.
        assert!(reachable(window(1800, 0), &[LAPTOP, RIGHT]));
        // Its bottom off the screen is fine; its top is what's dragged.
        assert!(reachable(window(100, 900), &[LAPTOP]));
    }

    #[test]
    fn a_window_on_a_screen_that_is_gone_is_not_reachable() {
        // It was on the right-hand screen, now unplugged.
        assert!(!reachable(window(2200, 100), &[LAPTOP]));
        assert!(reachable(window(2200, 100), &[LAPTOP, RIGHT]));
        // Only a sliver of its title bar on screen.
        assert!(!reachable(window(1850, 50), &[LAPTOP]));
        // Its title bar above the top of the screen.
        assert!(!reachable(window(100, -20), &[LAPTOP]));
        // Nowhere near.
        assert!(!reachable(window(-5000, -5000), &[LAPTOP]));
        assert!(!reachable(window(100, 100), &[]));
    }

    #[test]
    fn an_unreachable_window_is_centered_on_the_screen_and_fits_it() {
        assert_eq!(
            centered_on(window(2200, 100), LAPTOP, (640, 480)),
            Rect {
                x: 320,
                y: 90,
                width: 1280,
                height: 900
            }
        );
        // Too big for the screen that's left: it shrinks to fit.
        let small = Rect {
            x: 0,
            y: 30,
            width: 1024,
            height: 738,
        };
        assert_eq!(
            centered_on(window(2200, 100), small, (640, 480)),
            Rect {
                x: 0,
                y: 30,
                width: 1024,
                height: 738
            }
        );
        // Never below the minimum, even on a tiny screen.
        let tiny = Rect {
            x: 0,
            y: 0,
            width: 600,
            height: 400,
        };
        let placed = centered_on(window(2200, 100), tiny, (640, 480));
        assert_eq!((placed.width, placed.height), (640, 480));
    }

    #[test]
    fn a_window_comes_back_at_least_640_by_480() {
        let min = (640.0, 480.0);
        assert_eq!(at_least((1280, 900), 1.0, min), None);
        assert_eq!(at_least((640, 480), 1.0, min), None);
        // Saved at 1x, restored on a 2x screen: half the size.
        assert_eq!(at_least((1280, 900), 2.0, min), Some((640.0, 480.0)));
        assert_eq!(at_least((1000, 1200), 2.0, min), Some((640.0, 600.0)));
        // 1.25x rounds 640 to 800 physical pixels; that's enough.
        assert_eq!(at_least((800, 600), 1.25, min), None);
    }

    #[cfg(desktop)]
    #[test]
    fn size_position_and_maximized_come_back() {
        use tauri_plugin_window_state::StateFlags;
        let flags = state_flags();
        assert_eq!(
            flags.bits(),
            (StateFlags::SIZE | StateFlags::POSITION | StateFlags::MAXIMIZED).bits()
        );
        // Not visibility (always shown) or decorations (always native).
        assert!(!flags.intersects(StateFlags::VISIBLE | StateFlags::DECORATIONS));
    }
}
