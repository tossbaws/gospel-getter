//! The credit links at the end of each chapter's cross references, opened in
//! the system browser.
//!
//! The page names a link by a fixed key, never by a URL, and only the keys
//! in [`CREDIT_LINKS`] open anything, the same way `updater`'s Release notes
//! and releases page only ever open their own URLs.

/// Each credit link the page shows: its key, and the page it opens.
pub const CREDIT_LINKS: &[(&str, &str)] = &[
    (
        "openbible",
        "https://www.openbible.info/labs/cross-references/",
    ),
    ("cc-by", "https://creativecommons.org/licenses/by/4.0/"),
];

/// The messages the reader sees.
pub mod messages {
    pub const OPEN_FAILED: &str = "Couldn\u{2019}t open the link in your browser.";
    pub const UNKNOWN_LINK: &str = "That isn\u{2019}t a link Gospel Getter opens.";
}

/// The URL for credit link `which`, if it's one of [`CREDIT_LINKS`].
#[must_use]
pub fn credit_link_url(which: &str) -> Option<&'static str> {
    CREDIT_LINKS
        .iter()
        .find(|(key, _)| *key == which)
        .map(|(_, url)| *url)
}

#[cfg(desktop)]
pub use desktop::*;

#[cfg(desktop)]
mod desktop {
    use super::{credit_link_url, messages};

    /// Open credit link `which` ("openbible" or "cc-by") in the system
    /// browser. Any other key is refused.
    #[tauri::command]
    pub fn open_credit_link(which: String) -> Result<(), String> {
        let Some(url) = credit_link_url(&which) else {
            tracing::error!("Refused to open an unknown credit link: {which:?}");
            return Err(messages::UNKNOWN_LINK.to_string());
        };
        open::that_detached(url).map_err(|e| {
            tracing::error!("Failed to open {url}: {e}");
            messages::OPEN_FAILED.to_string()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_credit_links_open_their_fixed_pages() {
        assert_eq!(
            credit_link_url("openbible"),
            Some("https://www.openbible.info/labs/cross-references/")
        );
        assert_eq!(
            credit_link_url("cc-by"),
            Some("https://creativecommons.org/licenses/by/4.0/")
        );
        assert_eq!(CREDIT_LINKS.len(), 2);
    }

    #[test]
    fn anything_else_is_refused() {
        for which in [
            "",
            "OpenBible",
            "cc-by ",
            "https://www.openbible.info/labs/cross-references/",
            "javascript:alert(1)",
            "releases",
        ] {
            assert_eq!(credit_link_url(which), None, "{which:?}");
        }
    }

    /// The page's two anchors name these keys, and show these URLs as their
    /// `href`s, so what a link reads as is what it opens.
    #[test]
    fn the_page_shows_the_urls_it_opens() {
        let page = include_str!("../../ui/index.html");
        for (key, url) in CREDIT_LINKS {
            let anchor = format!("<a class=\"credit-link\" data-link=\"{key}\" href=\"{url}\">");
            assert!(page.contains(&anchor), "{anchor}");
        }
    }
}
