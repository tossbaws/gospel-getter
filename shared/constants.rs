//! Values the backend and the page must agree on, each defined once. This
//! one file is compiled into both crates, as their `constants` module
//! (`#[path]`): the backend's in core (`src-tauri/core`, which the app
//! re-exports), and the page's Rust pieces (`ui-leptos`). Plain values
//! only, so it compiles anywhere.

/// The highlighter colors, in the order the selection card's swatches show
/// them. The backend accepts exactly these (`db::HighlightColor`).
pub const HIGHLIGHT_COLORS: [&str; 4] = ["yellow", "green", "blue", "pink"];

/// The page About's "Support Gospel Getter" link opens, and shows as its
/// address: the page never sends a URL, and the backend's
/// `open_support_page` only ever opens this one.
pub const SUPPORT_PAGE: &str = "https://ko-fi.com/crcllc";
