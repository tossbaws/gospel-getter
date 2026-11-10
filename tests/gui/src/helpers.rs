//! WebKit's helper processes the run's app leaves behind for a moment on
//! Linux, so the checks can wait for them before starting the app again.
//!
//! WebKitWebDriver ends the app at the end of a session, but WebKitGTK's
//! helpers (its network, web and GPU processes) are processes of their own
//! and go a moment later. The network process holds the page's local
//! storage, and it writes what the page stored in its last moments (check
//! 65's note draft, kept as typing paused just before the app quit) to disk
//! only as it shuts down. An app started again before then reads local
//! storage without it.
//!
//! A helper is the run's when its environment has the run's own
//! `XDG_RUNTIME_DIR` (scripts/gui-check.sh gives everything it starts a
//! private one), so nothing outside the run is ever counted. The checks
//! only wait for these; they never stop them.

/// Whether a process named `comm` (`/proc/<pid>/comm`, at most 15 bytes)
/// with environment `environ` (`/proc/<pid>/environ`: `NAME=value` entries,
/// each ended by a NUL) is one of WebKit's helpers in the run whose
/// `XDG_RUNTIME_DIR` is `runtime_dir`. WebKitWebDriver, the run's driver,
/// isn't one.
pub fn is_run_helper(comm: &str, environ: &[u8], runtime_dir: &str) -> bool {
    let helper = comm.starts_with("WebKit") && comm != "WebKitWebDriver";
    let ours = format!("XDG_RUNTIME_DIR={runtime_dir}");
    helper && environ.split(|b| *b == 0).any(|var| var == ours.as_bytes())
}

/// The run's WebKit helpers running now, as `(pid, comm)`. Processes that
/// end (or can't be read) while they're looked through are left out.
#[cfg(target_os = "linux")]
pub fn running(runtime_dir: &str) -> std::io::Result<Vec<(u32, String)>> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir("/proc")? {
        let entry = entry?;
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<u32>().ok())
        else {
            continue;
        };
        let Ok(comm) = std::fs::read_to_string(format!("/proc/{pid}/comm")) else {
            continue;
        };
        let comm = comm.trim_end_matches('\n');
        if !comm.starts_with("WebKit") {
            continue;
        }
        let Ok(environ) = std::fs::read(format!("/proc/{pid}/environ")) else {
            continue;
        };
        if is_run_helper(comm, &environ, runtime_dir) {
            found.push((pid, comm.to_owned()));
        }
    }
    Ok(found)
}

/// None elsewhere: there's no `/proc` to look in, and on Windows the
/// checks wait for the app gui-launch recorded instead (`launched`).
#[cfg(not(target_os = "linux"))]
pub fn running(_runtime_dir: &str) -> std::io::Result<Vec<(u32, String)>> {
    Ok(Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    const RUN: &str = "/home/u/.cache/x/gui-check.AbC123/runtime";

    fn environ(vars: &[&str]) -> Vec<u8> {
        vars.iter()
            .flat_map(|v| [v.as_bytes(), b"\0"].concat())
            .collect()
    }

    #[test]
    fn webkit_s_helpers_in_the_run_are_counted() {
        let env = environ(&["HOME=/x", &format!("XDG_RUNTIME_DIR={RUN}"), "DISPLAY=:99"]);
        for comm in ["WebKitNetworkPr", "WebKitWebProces", "WebKitGPUProces"] {
            assert!(is_run_helper(comm, &env, RUN), "{comm}");
        }
    }

    #[test]
    fn the_driver_the_app_and_others_runs_are_not() {
        let env = environ(&[&format!("XDG_RUNTIME_DIR={RUN}")]);
        assert!(
            !is_run_helper("WebKitWebDriver", &env, RUN),
            "the run's driver"
        );
        assert!(
            !is_run_helper("gospel_getter", &env, RUN),
            "the app is WebKitWebDriver's"
        );
        assert!(!is_run_helper("Xvfb", &env, RUN));
        // Another run's helper, or the desktop's own app's.
        let other = environ(&["XDG_RUNTIME_DIR=/run/user/1000"]);
        assert!(!is_run_helper("WebKitNetworkPr", &other, RUN));
        // Only the whole value counts, not a longer path that starts with it.
        let longer = environ(&[&format!("XDG_RUNTIME_DIR={RUN}/more")]);
        assert!(!is_run_helper("WebKitNetworkPr", &longer, RUN));
        let named = environ(&[&format!("NOT_XDG_RUNTIME_DIR={RUN}")]);
        assert!(!is_run_helper("WebKitNetworkPr", &named, RUN));
        assert!(!is_run_helper("WebKitNetworkPr", b"", RUN));
    }
}
