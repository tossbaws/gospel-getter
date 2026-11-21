//! What the GUI checks (`gui-check`) and their Windows launcher
//! (`gui-launch`) share: the record of the apps gui-launch starts, and what
//! it looks for in the WebView2 data folder it keeps; and, on Linux, the
//! WebKit helpers an app leaves behind for a moment.

pub mod helpers;
pub mod launched;
pub mod webview2;
