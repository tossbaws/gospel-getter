# 0003. The UI is a Leptos and hand-built DOM hybrid

- **Status:** accepted, 2026-11-19 (D23)

## Decision

The page stays a hybrid: Leptos components for dialogs, hand-built DOM
(through `web-sys`) for the reader, the panes and the card. Neither half is
rewritten into the other as a matter of course. Before the next big UI
feature, measure the release `.wasm` size and Leptos's share of it again and
compare with the numbers below.

## Why

The reading half is the earlier JavaScript page carried into Rust: it drives
the DOM through `web-sys` and builds markup itself, and the jsdom harness
and the GUI checks pin its behaviour closely, including DOM writes that
happen at once rather than on Leptos's deferred updates. Rewriting it in
Leptos would put that behaviour at risk for nothing a reader would see.
Dialogs are where Leptos's views pay for themselves. Carrying Leptos costs
binary size, so that's what gets watched.

## Measured

At commit `0f7bbb4773f32b5436c5f8008124793e7c67803c` (v2.37.0), with the
CI-pinned Trunk 0.21.14 (wasm-bindgen 0.2.129, wasm-opt version_123),
`trunk build --release`:

- `dist/ui-leptos_bg.wasm`, the shipped file: **1,518,791 bytes**
  (604,520 gzipped with `gzip -9`).
- Leptos's share, from `ui_leptos.wasm` before wasm-opt (cargo's output,
  which keeps the name section), function-body bytes attributed to the crate
  at the root of each v0-mangled symbol: 1,900,337 bytes of code in all;
  `reactive_graph` 13.4% and `tachys` 13.0% (**26.5%** together; 27.3% with
  every Leptos crate); this crate's own code 22.5%.
- 169 of the 173 crates in `cargo tree --target wasm32-unknown-unknown -e
  normal` (in `ui-leptos/`) are `leptos` or come in through it.

To measure again: `trunk build --release`, then the size of
`dist/ui-leptos_bg.wasm`; for the share, read the code section and the
`name` section of `target/wasm32-unknown-unknown/release/ui_leptos.wasm`
(any wasm parser, or `twiggy top` on that file) and sum each function's
body size by its crate.

## Consequences

- [`ui-leptos/README.md`](../../ui-leptos/README.md)'s module map says which
  modules are which; it stays the one list.
- A new dialog is a Leptos component; a change to the reader, the panes or
  the card follows the hand-built DOM code already there.
