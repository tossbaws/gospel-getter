//! About's "Support Gospel Getter" link, opened in the system browser.
//!
//! The page never names a URL: `open_support_page` takes nothing and only
//! ever opens [`SUPPORT_PAGE`], the same way `updater`'s Release notes and
//! releases page only ever open their own URLs. The Ko-fi page is used
//! rather than the website's, since an installed app's link can't change
//! until the next release and the Ko-fi address won't move.

/// The page About's Support Gospel Getter link opens.
pub const SUPPORT_PAGE: &str = "https://ko-fi.com/crcllc";

#[cfg(desktop)]
pub use desktop::*;

#[cfg(desktop)]
mod desktop {
    use super::SUPPORT_PAGE;
    use crate::credit_links::messages;

    /// Open [`SUPPORT_PAGE`] in the system browser (About's Support Gospel
    /// Getter link).
    #[tauri::command]
    pub fn open_support_page() -> Result<(), String> {
        open::that_detached(SUPPORT_PAGE).map_err(|e| {
            tracing::error!("Failed to open {SUPPORT_PAGE}: {e}");
            messages::OPEN_FAILED.to_string()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_support_page_is_the_ko_fi_page() {
        assert_eq!(SUPPORT_PAGE, "https://ko-fi.com/crcllc");
    }

    /// About's link shows this URL as its `href`, so what the link reads as
    /// is what it opens.
    #[test]
    fn about_shows_the_url_it_opens() {
        let about = include_str!("../../ui-leptos/src/about.rs");
        assert!(about.contains(&format!("href=\"{SUPPORT_PAGE}\"")));
        assert!(about.contains(r#"ipc::invoke_value("open_support_page""#));
    }
}
