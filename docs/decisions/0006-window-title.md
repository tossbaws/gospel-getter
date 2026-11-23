# 0006. The window title is always "Gospel Getter"

- **Status:** accepted, 2026-11-19 (D24; shipped in 2.33.0)

## Decision

The window's title is "Gospel Getter" on every platform, for the whole
session. It doesn't change with the chapter, the verse or the translation.
The reference button in the app says where the reader is.

## Why

A title that named the place changed the title bar, the taskbar entry and
Alt-Tab on every move; a fixed one keeps them still while the reader moves.
It reverses an earlier decision to name the place in the title.

## Consequences

- `src-tauri/tauri.conf.json` sets the title; `src-tauri/src/window.rs`'s
  `//!` gives the rule. Nothing sets a title after, so the webview needs no
  window permission.
- `src-tauri/tests/packaging_config.rs`
  (`the_window_title_is_the_apps_name_and_nothing_changes_it`) and the GUI
  checks in `tests/gui/` hold it.
