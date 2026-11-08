//! Check 70: an AI-flagged finding (ui-leptos/src/findings.rs), end to end.
//! The reader's switch, "Let AI agents save findings", is turned on through
//! the page's own IPC (as Aa's switch saves it); then gospel-getter-cli's
//! `save-findings`, the one write an agent has, marks John 11:35 for the
//! request "grief" while the app runs, in the checks' own environment, so
//! in the database the app uses. John 11 is then gone to and read in
//! reading mode:
//!
//! - the verse is flagged (`.has-finding`, its dotted underline);
//! - its finding is in the margin collapsed, as the AI badge and the verse
//!   (or, where there's no room for the margin, a marker after the verse);
//! - a click opens it to "Found by Claude for your request “grief”.";
//! - "Keep as mine" opens the note editor with those words in it, all of
//!   them selected, the text box focused; Escape closes it, and the finding
//!   is still there.
//!
//! Then the finding is removed and the switch put back, through the page's
//! IPC, and reading mode left.

use std::process::Command;

use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};

use super::Run;

/// The request the finding is saved for.
const REQUEST: &str = "grief";

/// What the app shows for it.
const SHOWN: &str = "Found by Claude for your request \u{201c}grief\u{201d}.";

/// Starts one IPC call through the page (`arguments[0]`, with
/// `arguments[1]`), keeping how it went in `window.__guiCheckIpc` for
/// [`IPC_DONE_JS`]: never a WebDriver call waiting on the app.
const IPC_JS: &str = "const s = window.__guiCheckIpc = { done: false, value: null, error: null };
    window.__TAURI__.core.invoke(arguments[0], arguments[1])
        .then((v) => { s.value = v; s.done = true; }, (e) => { s.error = String(e); s.done = true; });
    return null;";

/// Whether the IPC call has finished, and how it went.
const IPC_DONE_JS: &str = "const s = window.__guiCheckIpc; return [!!s && s.done, s || null];";

/// The verse's finding, wherever it's drawn: in the margin, or a marker.
const FINDING_JS: &str = "const verse = document.querySelector('.chapter-main .chapter-text > .verse[data-verse=\"35\"]');
    const head = document.querySelector('.chapter-main .margin-notes .margin-finding[data-finding-key=\"35-35\"] .margin-finding-head')
        || (verse && verse.querySelector('.finding-marker'));
    const body = head && document.getElementById(head.getAttribute('aria-controls'));
    const seen = { flagged: !!verse && verse.classList.contains('has-finding'),
        shown: window.gospelGetterMargin ? window.gospelGetterMargin.shown() : null,
        head: head ? head.textContent : null, marker: !!head && head.classList.contains('finding-marker'),
        expanded: head ? head.getAttribute('aria-expanded') : null,
        open: !!body && !body.hidden,
        words: body ? [...body.querySelectorAll('.finding-text')].map((t) => t.textContent) : [] };
    return [seen.flagged && !!seen.head, seen];";

/// Brings John 11:35 to the middle of the window, so its finding has room
/// below it to open, and starts counting two animation frames (the margin
/// is placed at most once a frame), kept in `window.__guiCheckFrames`.
const CENTRE_JS: &str = "const verse = document.querySelector('.chapter-main .chapter-text > .verse[data-verse=\"35\"]');
    if (verse) verse.scrollIntoView({ block: 'center' });
    const s = window.__guiCheckFrames = { done: false };
    requestAnimationFrame(() => requestAnimationFrame(() => { s.done = true; }));
    return null;";

/// Whether those two frames have passed.
const FRAMES_DONE_JS: &str = "const s = window.__guiCheckFrames; return [!!s && s.done, null];";

/// The note editor, as Keep as mine opened it.
const EDITOR_JS: &str = "const overlay = document.getElementById('note-overlay');
    const input = document.getElementById('note-input');
    const seen = { open: !!overlay && !overlay.hidden, value: input ? input.value : null,
        start: input ? input.selectionStart : null, end: input ? input.selectionEnd : null,
        focused: document.activeElement === input,
        title: (document.getElementById('note-title') || {}).textContent || null };
    return [seen.open && seen.focused, seen];";

impl Run<'_> {
    /// One IPC call through the page, and its answer.
    async fn ipc(&self, what: &str, cmd: &str, args: Value) -> Result<Value> {
        self.ui.js(IPC_JS, vec![json!(cmd), args]).await?;
        let seen = self.ui.wait(what, IPC_DONE_JS).await?;
        if let Some(error) = seen["error"].as_str() {
            bail!("{what} failed: {error}");
        }
        Ok(seen["value"].clone())
    }

    /// Check 70 (see the module's docs).
    pub(super) async fn ai_findings(&self) -> Result<String> {
        let Some(cli) = self.config.cli.clone() else {
            bail!(
                "not run: no gospel-getter-cli beside the app (set GUI_CHECK_CLI to one), so no \
                 agent could save a finding"
            );
        };
        self.ipc(
            "the reader's switch to be turned on",
            "set_ai_findings_enabled",
            json!({ "enabled": true }),
        )
        .await?;
        let output = self
            .ui
            .step(
                "gospel-getter-cli save-findings, beside the running app",
                async {
                    Command::new(&cli)
                        .args([
                            "save-findings",
                            "--request",
                            REQUEST,
                            "--agent",
                            "Claude",
                            "John 11:35",
                        ])
                        .output()
                        .with_context(|| format!("couldn't run {}", cli.display()))
                },
            )
            .await?;
        ensure!(
            output.status.success(),
            "save-findings failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
        let saved: Value =
            serde_json::from_slice(&output.stdout).context("save-findings printed no JSON")?;
        ensure!(
            saved["saved"].as_u64().unwrap_or(0) + saved["already_saved"].as_u64().unwrap_or(0)
                == 1,
            "save-findings saved {saved}"
        );

        // The chapter, shown afresh, reads the finding the CLI saved.
        self.go_to("John 11", "John 11").await?;
        self.reading_mode_on().await?;
        let found = self
            .ui
            .wait("John 11:35 flagged, its finding collapsed", FINDING_JS)
            .await?;
        ensure!(
            found["expanded"] == "false" && found["open"] == false,
            "it should open collapsed: {found}"
        );
        let badge = if found["marker"] == true {
            "AI"
        } else {
            "AI John 11:35"
        };
        ensure!(
            found["head"] == badge,
            "its badge should read {badge:?}: {found}"
        );
        let head = if found["marker"] == true {
            ".chapter-main .verse[data-verse=\"35\"] .finding-marker"
        } else {
            ".chapter-main .margin-notes .margin-finding[data-finding-key=\"35-35\"] .margin-finding-head"
        };
        self.ui.js(CENTRE_JS, Vec::new()).await?;
        self.ui
            .wait("John 11:35 in the middle of the window", FRAMES_DONE_JS)
            .await?;
        self.ui.click("the finding's badge", head).await?;
        let opened = self
            .ui
            .wait(
                "the finding open: who found it and for what",
                &format!(
                    "const r = (() => {{ {} }})(); return [r[1].open && r[1].expanded === 'true', r[1]];",
                    FINDING_JS.replace("return [seen.flagged && !!seen.head, seen];", "return [true, seen];")
                ),
            )
            .await?;
        ensure!(opened["words"] == json!([SHOWN]), "its words: {opened}");

        let keep = if found["marker"] == true {
            ".chapter-main .verse[data-verse=\"35\"] .finding-inline .finding-keep"
        } else {
            ".chapter-main .margin-notes .margin-finding[data-finding-key=\"35-35\"] .finding-keep"
        };
        self.ui.click("Keep as mine", keep).await?;
        let editor = self
            .ui
            .wait("the note editor, the finding's words in it", EDITOR_JS)
            .await?;
        let length = SHOWN.encode_utf16().count();
        ensure!(
            editor["value"] == SHOWN
                && editor["start"] == 0
                && editor["end"] == json!(length)
                && editor["title"] == "John 11:35",
            "the editor should hold the finding's words, all of them selected: {editor}"
        );
        self.ui
            .press(
                "Escape (nothing changed: it closes)",
                thirtyfour::Key::Escape,
            )
            .await?;
        self.ui
            .wait(
                "the editor closed",
                "const o = document.getElementById('note-overlay'); return [!o || o.hidden, null];",
            )
            .await?;
        let left = self
            .ipc(
                "the chapter's findings",
                "list_findings",
                json!({ "bookId": 43, "chapter": 11 }),
            )
            .await?;
        ensure!(
            left.as_array().map(Vec::len) == Some(1),
            "cancelled, the finding should stay: {left}"
        );
        Ok(format!(
            "saved by gospel-getter-cli, shown {} as {badge:?}, opened to {SHOWN:?}; Keep as \
             mine opened the editor with all {length} characters selected",
            if found["marker"] == true {
                "as a marker"
            } else {
                "in the margin"
            }
        ))
    }

    /// Removes the check's finding, turns the switch off and leaves reading
    /// mode, whatever happened.
    pub(super) async fn ai_findings_clean_up(&self) -> Result<()> {
        self.ipc(
            "the check's findings to be removed",
            "remove_finding_request",
            json!({ "request": REQUEST }),
        )
        .await?;
        self.ipc(
            "the reader's switch to be turned off",
            "set_ai_findings_enabled",
            json!({ "enabled": false }),
        )
        .await?;
        let reading = self
            .ui
            .js(
                "return document.documentElement.dataset.readerMode === 'on';",
                Vec::new(),
            )
            .await?;
        if reading == true {
            self.ui.click("Exit reading mode", "#reader-exit").await?;
            self.reading_mode_off("reading mode left").await?;
        }
        Ok(())
    }
}
