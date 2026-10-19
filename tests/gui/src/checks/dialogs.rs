//! Checks 5 to 8: the dialogs the Rust pieces (ui-leptos) took over from
//! the page's JavaScript, each through the real UI: the welcome on a fresh
//! install, the note editor, the keyboard shortcuts sheet, and the import
//! preview after Export data… and Import data… through the native file
//! dialogs (on Windows, those stop at "the dialog appeared").

use std::path::Path;
use std::time::Duration;

use anyhow::{Result, bail};
use serde_json::json;
use thirtyfour::prelude::*;

use super::{DESCRIBE_JS, IMPORT_SHOT, Run, SHOTS, VISIBLE_JS};
use crate::native;

/// The note text check 6 saves over check 3's.
const EDITED_TEXT: &str = "gui-check smoke note, edited: Let there be light";

/// The native dialogs' titles (`commands::export_reader_data`,
/// `commands::choose_import_file`).
const EXPORT_DATA_TITLE: &str = "Export bookmarks, highlights, notes and settings";
const IMPORT_DATA_TITLE: &str = "Import bookmarks, highlights, notes and settings";

/// Where check 8's export goes, in the run's own folder.
const DATA_FILE: &str = "gui-check-reader-data.json";

/// Whether the element with id `arguments[0]` has focus, and what does.
const FOCUS_ON_JS: &str = "const want = arguments[0];
    const el = document.activeElement;
    return [!!el && el.id === want, el ? (el.id ? '#' + el.id : el.tagName.toLowerCase()) : null];";

impl Run<'_> {
    /// Waits for focus on `#id`.
    async fn focus_on(&self, what: &str, id: &str) -> Result<()> {
        self.ui
            .wait_with(what, FOCUS_ON_JS, vec![json!(id)])
            .await?;
        Ok(())
    }

    /// Takes screenshot `name`, noting it for check 4.
    async fn shot(&mut self, what: &str, name: &'static str) {
        let shot = self.ui.screenshot(what, &self.shot_path(name)).await;
        self.shots.note(name, shot);
    }

    /// Linux: waits for the native dialog `title` on the private display
    /// and returns its X window id.
    async fn native_window(&self, title: &'static str) -> Result<String> {
        let Some(xvfb) = &self.config.xvfb else {
            bail!("no private display");
        };
        self.ui
            .step(
                &format!("the GTK dialog \"{title}\" to appear on the private display"),
                async {
                    loop {
                        let windows = xvfb.windows_named(title).await?;
                        if let Some((id, _)) = windows.iter().find(|(_, name)| name == title) {
                            return Ok(id.clone());
                        }
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                },
            )
            .await
    }

    /// Linux: waits until no window named `title` is left.
    async fn native_window_gone(&self, title: &'static str) -> Result<()> {
        let Some(xvfb) = &self.config.xvfb else {
            return Ok(());
        };
        self.ui
            .step(&format!("the dialog \"{title}\" to be gone"), async {
                while !xvfb.windows_named(title).await?.is_empty() {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                Ok(())
            })
            .await
    }

    /// Windows: the native dialog `title` appears (UI Automation), and is
    /// closed. Returns its window class.
    async fn native_appeared_and_closed(&self, title: &'static str) -> Result<String> {
        let limit = self.config.step_timeout;
        self.ui
            .step(
                &format!("the dialog \"{title}\" to appear (UI Automation), then close it"),
                async {
                    native::windows_find_and_close(title, limit, &self.config.export_dir).await
                },
            )
            .await
    }

    /// Waits for the toast to say something starting with `start`.
    async fn toast_starting(&self, what: &str, start: &str) -> Result<String> {
        let seen = self
            .ui
            .wait_with(
                what,
                "const toast = document.getElementById('data-status');
                 return [toast.textContent.startsWith(arguments[0]) && !toast.classList.contains('is-hidden'),
                     toast.textContent];",
                vec![json!(start)],
            )
            .await?;
        Ok(seen.as_str().unwrap_or_default().to_owned())
    }

    /// Opens ☰ and waits for it.
    async fn open_menu(&self) -> Result<()> {
        self.ui.click("open ☰", "#menu-toggle").await?;
        self.ui
            .wait(
                "the ☰ menu to show",
                &format!(
                    "{VISIBLE_JS} return [visible(document.getElementById('app-menu')), null];"
                ),
            )
            .await?;
        Ok(())
    }

    /// Check 5: on a fresh install the welcome is the Rust pieces' one; a
    /// look picked there applies (Vellum, then Chi Rho again, so the other
    /// checks start from the default); Escape closes it and gives the
    /// keyboard back to the page, and it's recorded as seen.
    pub(super) async fn welcome(&mut self) -> Result<String> {
        let ui = &self.ui;
        let seen = ui
            .wait(
                "the welcome open, mounted by the Rust pieces, focus on its card",
                &format!(
                    "{VISIBLE_JS} {DESCRIBE_JS}
                     const overlay = document.getElementById('welcome-overlay');
                     const scripts = [...document.body.querySelectorAll(':scope > script')];
                     const last = scripts[scripts.length - 1];
                     // ui-leptos mounts its dialogs at the end of <body>,
                     // after the page's own markup and scripts.
                     const mounted = !!overlay && !!last
                         && !!(last.compareDocumentPosition(overlay) & Node.DOCUMENT_POSITION_FOLLOWING);
                     const focus = document.activeElement;
                     return [mounted && visible(overlay) && !!focus && focus.id === 'welcome-panel',
                         {{ mounted, focus: describe(focus),
                            looks: [...document.querySelectorAll('#welcome-looks [role=\"radio\"]')].map((r) => r.dataset.value),
                            places: document.querySelectorAll('#welcome-start .welcome-start-row').length }}];"
                ),
            )
            .await?;

        ui.click(
            "pick the Vellum look",
            "#welcome-looks [role=\"radio\"][data-value=\"vellum\"]",
        )
        .await?;
        ui.wait(
            "Vellum to apply, as Aa applies a theme",
            "const radio = document.querySelector('#welcome-looks [data-value=\"vellum\"]');
             const select = document.getElementById('theme-select').value;
             const attr = document.documentElement.dataset.theme || null;
             return [select === 'vellum' && attr === 'vellum' && radio.getAttribute('aria-checked') === 'true'
                 && document.activeElement === radio, { select, attr, checked: radio.getAttribute('aria-checked') }];",
        )
        .await?;
        self.shot("screenshot: the welcome, Vellum", SHOTS[4]).await;

        let ui = &self.ui;
        ui.press("press ← (the other look, Chi Rho)", Key::Left)
            .await?;
        ui.wait(
            "Chi Rho to apply again",
            "const radio = document.querySelector('#welcome-looks [data-value=\"chi-rho\"]');
             const select = document.getElementById('theme-select').value;
             const attr = document.documentElement.dataset.theme || null;
             return [select === 'chi-rho' && attr === null && radio.getAttribute('aria-checked') === 'true',
                 { select, attr }];",
        )
        .await?;
        ui.press("press Escape (close the welcome)", Key::Escape)
            .await?;
        let after = ui
            .wait(
                "the welcome to close, the keyboard back with the page, seen recorded",
                &format!(
                    "{DESCRIBE_JS}
                     const overlay = document.getElementById('welcome-overlay');
                     const focus = document.activeElement;
                     const seen = localStorage.getItem('gospel-getter-welcome');
                     return [overlay.hidden && (!focus || focus === document.body) && seen === 'seen',
                         {{ hidden: overlay.hidden, focus: !focus || focus === document.body ? 'body' : describe(focus), seen }}];"
                ),
            )
            .await?;
        Ok(format!(
            "the Rust pieces' welcome opened on the fresh install (looks {}, {} places to start), focus on its card; \
             Vellum applied (#theme-select, html[data-theme]); ← back to Chi Rho; Escape closed it, focus back with \
             the page ({}), seen recorded",
            seen["looks"],
            seen["places"],
            after["focus"].as_str().unwrap_or("body")
        ))
    }

    /// Check 6: the note check 3 wrote, edited from the Library and saved
    /// (its Library row and the card follow, the verse keeps its marker);
    /// then, with unsaved text, Escape asks before discarding.
    pub(super) async fn note_editor(&mut self) -> Result<String> {
        self.open_library_notes().await?;
        let ui = &self.ui;
        ui.click("the Library row's Edit", "#note-list .note-list-edit")
            .await?;
        let opened = ui
            .wait_with(
                "the note editor to open on the note, focus in its text",
                &format!(
                    "{VISIBLE_JS} {DESCRIBE_JS}
                     const input = document.getElementById('note-input');
                     return [visible(document.getElementById('note-panel')) && document.activeElement === input
                         && input.value === arguments[0] && visible(document.getElementById('note-delete')),
                         {{ title: document.getElementById('note-title').textContent, value: input.value,
                            focus: describe(document.activeElement) }}];"
                ),
                vec![json!(super::NOTE_TEXT)],
            )
            .await?;
        self.shot("screenshot: the note editor, Chi Rho", SHOTS[5])
            .await;

        let ui = &self.ui;
        ui.step("replace the note's text", async {
            let input = ui.driver().find(By::Id("note-input")).await?;
            input.clear().await?;
            input.send_keys(EDITED_TEXT).await?;
            Ok(())
        })
        .await?;
        ui.wait(
            "Save to be enabled",
            "return [!document.getElementById('note-save').disabled, null];",
        )
        .await?;
        ui.click("click Save", "#note-save").await?;
        let saved = ui
            .wait_with(
                "the editor to close, the Library row and the card to show the edit, the marker to stay",
                "const want = arguments[0];
                 const row = document.querySelector('#note-list .note-preview');
                 const card = document.querySelector('.verse-notes .note-card-body');
                 const marked = !!document.querySelector('.chapter-main .verse.has-notes[data-verse=\"1\"]');
                 const closed = document.getElementById('note-overlay').hidden;
                 return [closed && !!row && row.textContent === want && (!card || card.textContent === want) && marked,
                     { closed, row: row && row.textContent, card: card && card.textContent, marked }];",
                vec![json!(EDITED_TEXT)],
            )
            .await?;

        // Unsaved text, then Escape: it asks first.
        ui.click("the card's Edit", ".verse-notes .note-edit")
            .await?;
        self.focus_on("the editor to open again, focus in its text", "note-input")
            .await?;
        let ui = &self.ui;
        ui.type_into("type more, unsaved", "#note-input", " (not to keep)")
            .await?;
        ui.press("press Escape with unsaved text", Key::Escape)
            .await?;
        ui.wait(
            "the discard prompt, focus on Keep editing",
            &format!(
                "{VISIBLE_JS}
                 return [visible(document.getElementById('note-discard'))
                     && document.getElementById('note-buttons').hidden
                     && document.activeElement && document.activeElement.id === 'note-keep',
                     document.getElementById('note-discard-question').textContent];"
            ),
        )
        .await?;
        ui.click("click Discard", "#note-discard-yes").await?;
        ui.wait_with(
            "the editor to close with nothing saved",
            "const row = document.querySelector('#note-list .note-preview');
             return [document.getElementById('note-overlay').hidden && !!row && row.textContent === arguments[0],
                 row && row.textContent];",
            vec![json!(EDITED_TEXT)],
        )
        .await?;

        Ok(format!(
            "opened from the Library row's Edit on \"{}\" (focus in its text); saved an edit: the Library row \
             and the card show it, verse 1 keeps its marker ({}); unsaved text then Escape asked \
             \"Discard this note?\" with focus on Keep editing; Discard closed it, nothing saved",
            opened["title"].as_str().unwrap_or_default(),
            if saved["card"].is_null() {
                "no card shown"
            } else {
                "card updated"
            }
        ))
    }

    /// Check 7: `?` opens the shortcuts sheet (the Rust pieces' one), Tab
    /// stays inside it, Escape closes it, and focus goes back where it was.
    pub(super) async fn shortcuts(&mut self) -> Result<String> {
        // Somewhere known to come back to: ☰, after its menu closes.
        self.open_menu().await?;
        let ui = &self.ui;
        ui.press("press Escape (close ☰)", Key::Escape).await?;
        self.focus_on("focus back on ☰", "menu-toggle").await?;

        let ui = &self.ui;
        ui.press("press ?", "?").await?;
        let opened = ui
            .wait(
                "the sheet to open, every shortcut listed, focus on its close button",
                &format!(
                    "{VISIBLE_JS}
                     const rows = document.querySelectorAll('#shortcuts-body .shortcut-row').length;
                     const table = (window.gospelGetterShortcuts || []).length;
                     return [visible(document.getElementById('shortcuts-panel')) && rows > 0 && rows === table
                         && document.activeElement && document.activeElement.id === 'shortcuts-close',
                         {{ rows, table, groups: document.querySelectorAll('#shortcuts-body .shortcut-group').length }}];"
                ),
            )
            .await?;
        self.shot("screenshot: the shortcuts sheet, Chi Rho", SHOTS[6])
            .await;

        let ui = &self.ui;
        ui.press("Tab (to the list)", Key::Tab).await?;
        self.focus_on("focus on the list", "shortcuts-body").await?;
        let ui = &self.ui;
        ui.press("Tab (round to the close button)", Key::Tab)
            .await?;
        self.focus_on("focus back on the close button", "shortcuts-close")
            .await?;
        let ui = &self.ui;
        ui.press("press Escape (close the sheet)", Key::Escape)
            .await?;
        ui.wait(
            "the sheet to close, focus back on ☰",
            "const focus = document.activeElement;
             return [document.getElementById('shortcuts-overlay').hidden && !!focus && focus.id === 'menu-toggle',
                 focus ? (focus.id || focus.tagName) : null];",
        )
        .await?;
        Ok(format!(
            "? opened the Rust pieces' sheet ({} shortcuts in {} groups, the page's one table), focus on its close \
             button; Tab went to the list and round again; Escape closed it, focus back on ☰",
            opened["rows"], opened["groups"]
        ))
    }

    /// Check 8: Export data… writes a file through the native save dialog,
    /// Import data… opens it through the native open dialog, the preview
    /// shows what it holds, and Cancel closes it with nothing changed. On
    /// Windows each native dialog is only seen to appear (and is closed).
    pub(super) async fn import_preview(&mut self) -> Result<String> {
        let counts = "return [true, { bookmarks: document.getElementById('bookmark-count').textContent,
            notes: document.getElementById('note-count').textContent,
            noteText: (document.querySelector('#note-list .note-preview') || {}).textContent || null }];";
        let before = self.ui.wait("what the reader has now", counts).await?;

        self.open_menu().await?;
        self.ui
            .click("click Export data…", "#export-data-btn")
            .await?;
        let mut said = Vec::new();
        let path = self.config.export_dir.join(DATA_FILE);
        match &self.config.xvfb {
            Some(xvfb) => {
                let window = self.native_window(EXPORT_DATA_TITLE).await?;
                self.ui
                    .step(
                        "type the file's path into the save dialog and press Enter (xdotool)",
                        async { xvfb.save_as(&window, &path).await },
                    )
                    .await?;
                let bytes = self
                    .ui
                    .step("the exported file", async {
                        loop {
                            if let Ok(text) = tokio::fs::read_to_string(&path).await
                                && text.contains("\"gospel-getter-reader-data\"")
                            {
                                return Ok(text.len());
                            }
                            tokio::time::sleep(Duration::from_millis(100)).await;
                        }
                    })
                    .await?;
                let toast = self
                    .toast_starting("the toast to say it was saved", "Saved ")
                    .await?;
                self.native_window_gone(EXPORT_DATA_TITLE).await?;
                said.push(format!(
                    "Export data… wrote {DATA_FILE} through the GTK save dialog ({bytes} bytes; toast \"{toast}\")"
                ));
            }
            None => {
                let class = self.native_appeared_and_closed(EXPORT_DATA_TITLE).await?;
                self.toast_starting("the toast to say nothing was saved", "Export cancelled.")
                    .await?;
                said.push(format!(
                    "Export data…'s save dialog appeared (window class {class}) and was closed"
                ));
            }
        }

        self.open_menu().await?;
        self.ui
            .click("click Import data…", "#import-data-btn")
            .await?;
        match &self.config.xvfb {
            Some(xvfb) => {
                let window = self.native_window(IMPORT_DATA_TITLE).await?;
                self.ui
                    .step(
                        "type the file's path into the open dialog's location (xdotool)",
                        async { xvfb.type_location(&window, Path::new(&path)).await },
                    )
                    .await?;
                // GTK's location entry reads the typed folder in the
                // background, and an Enter that comes before it has is
                // dropped; nothing outside GTK can see when it's done. So:
                // Enter, then up to a second for the dialog to go, and Enter
                // again only if it hasn't; each one counted and reported.
                let enters = self
                    .ui
                    .step("press Enter until the open dialog takes the file", async {
                        let mut enters = 0u32;
                        loop {
                            xvfb.press_enter(&window).await?;
                            enters += 1;
                            if enters > 1 {
                                self.ui.log(&format!(
                                    "  Enter {enters}: the open dialog was still up a second after the last"
                                ));
                            }
                            let gone = tokio::time::timeout(Duration::from_secs(1), async {
                                while !xvfb.windows_named(IMPORT_DATA_TITLE).await?.is_empty() {
                                    tokio::time::sleep(Duration::from_millis(50)).await;
                                }
                                anyhow::Ok(())
                            })
                            .await;
                            match gone {
                                Ok(result) => {
                                    result?;
                                    return Ok(enters);
                                }
                                Err(_) => continue,
                            }
                        }
                    })
                    .await?;
                let preview = self
                    .ui
                    .wait_with(
                        "the import preview, focus on Import",
                        &format!(
                            "{VISIBLE_JS}
                             const file = document.getElementById('import-file').textContent;
                             const summary = [...document.querySelectorAll('#import-summary li')].map((li) => li.textContent);
                             return [visible(document.getElementById('import-panel')) && file.startsWith(arguments[0])
                                 && summary.length > 0 && document.activeElement
                                 && document.activeElement.id === 'import-confirm',
                                 {{ file, summary, title: document.getElementById('import-title').textContent }}];"
                        ),
                        vec![json!(format!("{DATA_FILE} — exported"))],
                    )
                    .await?;
                self.native_window_gone(IMPORT_DATA_TITLE).await?;
                self.shot("screenshot: the import preview, Chi Rho", IMPORT_SHOT)
                    .await;
                self.ui.click("click Cancel", "#import-cancel").await?;
                self.ui
                    .wait(
                        "the preview to close, focus back on ☰",
                        "const focus = document.activeElement;
                         return [document.getElementById('import-overlay').hidden && !!focus && focus.id === 'menu-toggle',
                             focus ? (focus.id || focus.tagName) : null];",
                    )
                    .await?;
                self.toast_starting(
                    "the toast to say nothing was changed",
                    "Import cancelled. Nothing was changed.",
                )
                .await?;
                let summary: Vec<String> = preview["summary"]
                    .as_array()
                    .map(|lines| {
                        lines
                            .iter()
                            .map(|l| l.as_str().unwrap_or_default().to_owned())
                            .collect()
                    })
                    .unwrap_or_default();
                said.push(format!(
                    "Import data… opened it through the GTK open dialog (Enter pressed {enters}×); the preview \
                     (\"{}\") said: {}; Cancel closed it, focus back on ☰, toast \"Import cancelled. Nothing was \
                     changed.\"",
                    preview["title"].as_str().unwrap_or_default(),
                    summary.join(" / ")
                ));
            }
            None => {
                let class = self.native_appeared_and_closed(IMPORT_DATA_TITLE).await?;
                self.toast_starting(
                    "the toast to say nothing was changed",
                    "Import cancelled. Nothing was changed.",
                )
                .await?;
                said.push(format!(
                    "Import data…'s open dialog appeared (window class {class}) and was closed; on Windows \
                     the check stops there"
                ));
            }
        }

        let after = self.ui.wait("what the reader has after", counts).await?;
        if after != before {
            bail!("the reader's data changed: before {before}, after {after}");
        }
        said.push(format!("nothing changed ({after})"));
        Ok(said.join("; "))
    }
}
