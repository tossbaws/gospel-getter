//! The one thing WebDriver can't reach: the native save dialog, a GTK
//! window on Linux and a Win32 one on Windows, outside the webview.
//!
//! Linux: xdotool, only ever on the run's private Xvfb. Before every call
//! [`Xvfb::confirm`] checks that the display is the Xvfb the script
//! started (its lock file names that pid, the pid is Xvfb, and every
//! socket listening for the display belongs to it), and xdotool runs with
//! an emptied environment holding nothing but that `DISPLAY` and `PATH`.
//!
//! Windows: UI Automation, through PowerShell, finds the dialog and closes
//! it; the check stops at "the save dialog appeared" there.

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use tokio::process::Command;

/// The run's private X display, as the script started it.
#[derive(Debug, Clone)]
pub struct Xvfb {
    display: String,
    number: u32,
    pid: u32,
}

impl Xvfb {
    /// The display and pid the script passes (`GUI_XVFB_DISPLAY`, e.g.
    /// ":99", and `GUI_XVFB_PID`), confirmed.
    pub fn new(display: &str, pid: &str) -> Result<Self> {
        let number = display
            .strip_prefix(':')
            .and_then(|n| n.parse().ok())
            .with_context(|| format!("not a local X display: {display:?}"))?;
        let pid = pid.parse().with_context(|| format!("not a pid: {pid:?}"))?;
        let xvfb = Self {
            display: display.to_owned(),
            number,
            pid,
        };
        xvfb.confirm()?;
        Ok(xvfb)
    }

    pub fn display(&self) -> &str {
        &self.display
    }

    /// Refuses unless the display is the private Xvfb: its lock file names
    /// our pid, that pid is Xvfb, and every socket listening for the
    /// display (on disk and abstract) is that process's.
    pub fn confirm(&self) -> Result<()> {
        let n = self.number;
        let lock = std::fs::read_to_string(format!("/tmp/.X{n}-lock"))
            .with_context(|| format!("refusing: no lock file for {}", self.display))?;
        let lock_pid: u32 = lock.trim().parse().unwrap_or(0);
        if lock_pid != self.pid {
            bail!(
                "refusing: {}'s lock file names pid {lock_pid}, not the private Xvfb ({})",
                self.display,
                self.pid
            );
        }
        let comm = std::fs::read_to_string(format!("/proc/{}/comm", self.pid)).unwrap_or_default();
        if comm.trim() != "Xvfb" {
            bail!("refusing: pid {} is {:?}, not Xvfb", self.pid, comm.trim());
        }
        let listening = listening_inodes(n)?;
        if listening.is_empty() {
            bail!("refusing: nothing is listening for {}", self.display);
        }
        let owned = socket_inodes(self.pid)?;
        if let Some(stranger) = listening.iter().find(|inode| !owned.contains(inode)) {
            bail!(
                "refusing: a socket for {} (inode {stranger}) is not the private Xvfb's",
                self.display
            );
        }
        Ok(())
    }

    /// Runs xdotool on this display, and only this display.
    pub async fn xdotool(&self, args: &[&str]) -> Result<String> {
        self.confirm()?;
        let output = Command::new("xdotool")
            .args(args)
            .env_clear()
            .env("DISPLAY", &self.display)
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .stdin(Stdio::null())
            .kill_on_drop(true)
            .output()
            .await
            .context("running xdotool")?;
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        if !output.status.success() {
            bail!(
                "xdotool {} failed ({}): {}",
                args.join(" "),
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        Ok(stdout)
    }

    /// The visible windows whose name contains `fragment`, as
    /// `(id, name)`; none is not an error.
    pub async fn windows_named(&self, fragment: &str) -> Result<Vec<(String, String)>> {
        self.confirm()?;
        let output = Command::new("xdotool")
            .args(["search", "--onlyvisible", "--name", &regex_escape(fragment)])
            .env_clear()
            .env("DISPLAY", &self.display)
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .stdin(Stdio::null())
            .kill_on_drop(true)
            .output()
            .await
            .context("running xdotool")?;
        // xdotool search exits 1 when nothing matches.
        let ids: Vec<String> = String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::to_owned)
            .collect();
        let mut windows = Vec::new();
        for id in ids {
            let name = self.xdotool(&["getwindowname", &id]).await?;
            windows.push((id, name.trim_end_matches('\n').to_owned()));
        }
        Ok(windows)
    }

    /// Saves through the GTK save dialog `window`: focuses it, replaces the
    /// suggested name with the full `path` and presses Enter.
    pub async fn save_as(&self, window: &str, path: &Path) -> Result<()> {
        let path = path.to_str().context("the export path is not UTF-8")?;
        self.xdotool(&["windowfocus", "--sync", window]).await?;
        self.xdotool(&["key", "--clearmodifiers", "ctrl+a"]).await?;
        self.xdotool(&["type", "--delay", "5", "--clearmodifiers", path])
            .await?;
        self.xdotool(&["key", "--clearmodifiers", "Return"]).await?;
        Ok(())
    }
}

/// The inodes of the Unix sockets listening for X display `n`.
fn listening_inodes(n: u32) -> Result<Vec<u64>> {
    let table = std::fs::read_to_string("/proc/net/unix").context("reading /proc/net/unix")?;
    let names = [
        format!("/tmp/.X11-unix/X{n}"),
        format!("@/tmp/.X11-unix/X{n}"),
    ];
    // Num RefCount Protocol Flags Type St Inode Path; a listening socket
    // has the __SO_ACCEPTCON flag (00010000).
    Ok(table
        .lines()
        .skip(1)
        .filter_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            let path = fields.get(7)?;
            (fields.get(3) == Some(&"00010000") && names.iter().any(|name| name == path))
                .then(|| fields.get(6)?.parse().ok())
                .flatten()
        })
        .collect())
}

/// The inodes of the sockets process `pid` holds.
fn socket_inodes(pid: u32) -> Result<Vec<u64>> {
    let dir = format!("/proc/{pid}/fd");
    let entries = std::fs::read_dir(&dir).with_context(|| format!("reading {dir}"))?;
    Ok(entries
        .filter_map(|entry| {
            let target = std::fs::read_link(entry.ok()?.path()).ok()?;
            let target = target.to_str()?;
            target
                .strip_prefix("socket:[")?
                .strip_suffix(']')?
                .parse()
                .ok()
        })
        .collect())
}

/// `text` as a POSIX extended regular expression matching it literally.
fn regex_escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for c in text.chars() {
        if "\\^$.|?*+()[]{}".contains(c) {
            escaped.push('\\');
        }
        escaped.push(c);
    }
    escaped
}

/// Windows: waits up to `within` for a window named `title` (the save
/// dialog) through UI Automation, then closes it. Returns its class name.
/// The PowerShell script is written to `work_dir` and run with `-File`, so
/// the title arrives as a parameter, never spliced into code.
pub async fn windows_find_and_close(
    title: &str,
    within: Duration,
    work_dir: &Path,
) -> Result<String> {
    const SCRIPT: &str = r#"
param([Parameter(Mandatory)][string]$Title, [Parameter(Mandatory)][int]$Millis)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
$A = [System.Windows.Automation.AutomationElement]
$condition = New-Object System.Windows.Automation.AndCondition(
    (New-Object System.Windows.Automation.PropertyCondition($A::NameProperty, $Title)),
    (New-Object System.Windows.Automation.PropertyCondition($A::ControlTypeProperty, [System.Windows.Automation.ControlType]::Window)))
$deadline = [DateTime]::UtcNow.AddMilliseconds($Millis)
do {
    $window = $A::RootElement.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition)
    if ($window) { break }
    Start-Sleep -Milliseconds 100
} while ([DateTime]::UtcNow -lt $deadline)
if (-not $window) { Write-Error "no window named '$Title' appeared"; exit 1 }
$class = $window.Current.ClassName
$window.GetCurrentPattern([System.Windows.Automation.WindowPattern]::Pattern).Close()
Write-Output $class
"#;
    let script = work_dir.join("find-save-dialog.ps1");
    tokio::fs::write(&script, SCRIPT)
        .await
        .with_context(|| format!("writing {}", script.display()))?;
    let script = script.to_str().context("the script path is not UTF-8")?;
    let millis = within.as_millis().to_string();
    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
            script,
            "-Title",
            title,
            "-Millis",
            &millis,
        ])
        .stdin(Stdio::null())
        .kill_on_drop(true)
        .output()
        .await
        .context("running PowerShell")?;
    if !output.status.success() {
        bail!(
            "UI Automation found no save dialog: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regex_escape_matches_literally() {
        assert_eq!(
            regex_escape("Genesis 1 · KJV — Gospel Getter"),
            "Genesis 1 · KJV — Gospel Getter"
        );
        assert_eq!(regex_escape("a.b(c)"), r"a\.b\(c\)");
    }
}
