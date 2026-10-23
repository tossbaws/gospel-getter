fn main() {
    // On Windows (MSVC) the linker embeds the app manifest (Common Controls
    // v6, which the dialogs need) into every executable this crate links,
    // its test binaries included, instead of tauri-build embedding it into
    // the app's alone: a test binary without it doesn't start
    // (STATUS_ENTRYPOINT_NOT_FOUND). It's tauri-build's own default.
    let mut windows = tauri_build::WindowsAttributes::new();
    let target = |key: &str| std::env::var(key).unwrap_or_default();
    if target("CARGO_CFG_TARGET_OS") == "windows" && target("CARGO_CFG_TARGET_ENV") == "msvc" {
        windows = tauri_build::WindowsAttributes::new_without_app_manifest();
        let manifest =
            std::path::Path::new(&target("CARGO_MANIFEST_DIR")).join("windows-app-manifest.xml");
        println!("cargo:rerun-if-changed={}", manifest.display());
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
    }
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))
        .expect("failed to run tauri-build");
}
