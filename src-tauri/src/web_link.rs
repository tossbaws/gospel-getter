//! The web links in the reader's notes (`https://…`, `[text](https://…)`),
//! opened in the system browser.
//!
//! Unlike the credit links and About's Support link, whose pages are fixed
//! here, a note's link is whatever the reader typed, so the page names it.
//! Only a web address opens: `http` or `https`, with a host, and no user
//! name or password in it. Anything else (`file:`, `javascript:`, `data:`,
//! another program's own scheme) is refused with a sentence, and nothing is
//! opened. The page draws only such links (`ui-leptos/src/note_markdown.rs`),
//! and never opens one itself.

/// The messages the reader sees.
pub mod messages {
    pub const NOT_A_WEB_LINK: &str =
        "Only web links (starting with http:// or https://) open from a note.";
}

/// `address` as the web address the browser is given, if it's one the app
/// opens: `http` or `https`, a host, no user name or password. Otherwise
/// why not, for the reader.
pub fn web_address(address: &str) -> Result<tauri::Url, &'static str> {
    let url = tauri::Url::parse(address.trim()).map_err(|_| messages::NOT_A_WEB_LINK)?;
    let web = matches!(url.scheme(), "http" | "https")
        && url.host_str().is_some_and(|host| !host.is_empty())
        && url.username().is_empty()
        && url.password().is_none();
    if web {
        Ok(url)
    } else {
        Err(messages::NOT_A_WEB_LINK)
    }
}

#[cfg(desktop)]
pub use desktop::*;

#[cfg(desktop)]
mod desktop {
    use super::web_address;
    use crate::credit_links::messages::OPEN_FAILED;

    /// Open a note's web link `url` in the system browser. Anything but an
    /// `http` or `https` address is refused.
    #[tauri::command]
    pub fn open_web_link(url: String) -> Result<(), String> {
        let address = web_address(&url).map_err(|message| {
            tracing::error!("Refused to open a note's link that isn't a web address: {url:?}");
            message.to_string()
        })?;
        open::that_detached(address.as_str()).map_err(|e| {
            tracing::error!("Failed to open {address}: {e}");
            OPEN_FAILED.to_string()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_addresses_open() {
        for (typed, opened) in [
            ("https://example.com", "https://example.com/"),
            (
                "http://example.com/a/b?c=d#e",
                "http://example.com/a/b?c=d#e",
            ),
            (
                "https://www.biblegateway.com/passage/?search=Rom+5:8",
                "https://www.biblegateway.com/passage/?search=Rom+5:8",
            ),
            ("HTTPS://Example.COM/Path", "https://example.com/Path"),
            ("  https://example.com/  ", "https://example.com/"),
            ("https://192.168.1.1:8080/", "https://192.168.1.1:8080/"),
        ] {
            assert_eq!(
                web_address(typed).map(|url| url.to_string()),
                Ok(opened.to_string()),
                "{typed:?}"
            );
        }
    }

    #[test]
    fn anything_else_is_refused_with_a_sentence() {
        for typed in [
            "",
            "   ",
            "javascript:alert(1)",
            "JavaScript:alert(document.cookie)",
            "file:///etc/passwd",
            "file://C:/Windows/System32",
            "data:text/html,<script>alert(1)</script>",
            "mailto:someone@example.com",
            "ftp://example.com/file",
            "tauri://localhost",
            "gospel-getter://open",
            "vbscript:msgbox",
            "about:blank",
            "https://",
            "https://user:secret@example.com/",
            "https://user@example.com/",
            "example.com",
            "//example.com",
            "/etc/passwd",
            "Rom 5:8",
        ] {
            assert_eq!(
                web_address(typed).map(|url| url.to_string()),
                Err(messages::NOT_A_WEB_LINK),
                "{typed:?}"
            );
        }
        assert!(messages::NOT_A_WEB_LINK.ends_with('.'));
    }

    /// The page draws a note's web links with the class the command's
    /// caller looks for, and sends their addresses to this command.
    #[test]
    fn the_page_sends_its_note_links_here() {
        let markdown = include_str!("../../ui-leptos/src/note_markdown.rs");
        assert!(markdown.contains(r#"class=\"note-web-link\""#));
        let links = include_str!("../../ui-leptos/src/note_links.rs");
        assert!(links.contains(r#""open_web_link""#));
    }
}
