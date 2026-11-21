//! Windows: what gui-launch looks for in the WebView2 data folder it keeps
//! for the whole run (see src/bin/gui-launch.rs).

/// Where WebView2 writes the debugging port it was given (0: one it
/// picked) and its browser's DevTools path, relative to its data folder:
/// in `EBWebView`, the browser's own folder inside it; the folder itself
/// is looked in too, in case a WebView2 release moves it.
pub const PORT_FILES: [&str; 2] = [r"EBWebView\DevToolsActivePort", "DevToolsActivePort"];

/// The file a running WebView2 holds open in its data folder, letting
/// nothing else open it exclusively, and deletes as it exits.
pub const LOCK_FILE: &str = r"EBWebView\lockfile";

/// Whether `text` is a whole port file: a port, then the browser's DevTools
/// path, each on a line. WebView2 writes it in pieces; one read halfway
/// isn't passed on.
pub fn port_file_complete(text: &str) -> bool {
    let mut lines = text.lines();
    let port = lines
        .next()
        .and_then(|port| port.trim().parse::<u16>().ok());
    let path = lines.next().map(str::trim);
    matches!(port, Some(port) if port > 0)
        && path.is_some_and(|path| path.starts_with("/devtools/"))
}

#[cfg(test)]
mod tests {
    use super::port_file_complete;

    #[test]
    fn only_a_whole_port_file_is_passed_on() {
        assert!(port_file_complete(
            "64778\n/devtools/browser/0b7c0f2e-6d3b-4c1e-9a41-3f1c2b7d9e10"
        ));
        assert!(port_file_complete("64778\r\n/devtools/browser/x\r\n"));
        for partial in [
            "",
            "6",
            "64778",
            "64778\n",
            "64778\n/dev",
            "0\n/devtools/browser/x",
            "x\n/devtools/browser/x",
        ] {
            assert!(!port_file_complete(partial), "{partial:?}");
        }
    }
}
