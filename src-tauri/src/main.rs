//! The app's executable. Everything is in the library
//! (`gospel_getter_lib`), so the frontend tests can drive it; this only
//! starts it.

// On Windows, this is a windowed desktop app with no console UI of its
// own — suppress the console window that would otherwise pop up when
// launched from a shortcut or at login. No effect on other platforms.
#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() {
    gospel_getter_lib::run();
}
