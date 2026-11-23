# 0009. The page's logic is Rust

- **Status:** accepted, 2026-10-26 (2.19.0, when the interface moved from JavaScript to Rust)

## Decision

The page's logic is Rust, in `ui-leptos/`, compiled to WebAssembly.
`ui/index.html` keeps only the page's markup, its stylesheet and its three
small inline scripts: the pre-paint script (generated from
`shared/preferences.rs`), boot's prefetch, and the plain-text fallback.
`ui/leptos-loader.js` loads the WebAssembly. No other JavaScript is added.

## Why

One language for the page and the backend lets them share types and rules
(`shared/`), keeps the logic under Rust's compiler, Clippy and tests, and
keeps the Content Security Policy to hashes of a few small scripts.

## Consequences

- `src-tauri/tests/frontend_markup.rs`
  (`nothing_inline_that_the_content_security_policy_would_block`) counts the
  inline scripts, and checks that the pre-paint script matches the
  preferences table.
- [`ui-leptos/README.md`](../../ui-leptos/README.md) is the module map.
