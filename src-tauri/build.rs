const WINDOWS_APP_MANIFEST_FILE: &str = "windows-app-manifest.xml";

fn main() {
    let windows = tauri_build::WindowsAttributes::new_without_app_manifest();
    let attributes = tauri_build::Attributes::new().windows_attributes(windows);
    embed_windows_app_manifest_in_every_target();
    tauri_build::try_build(attributes).expect("failed to run tauri build script");
}

fn targets_msvc() -> bool {
    std::env::var("CARGO_CFG_TARGET_ENV").is_ok_and(|target_env| target_env == "msvc")
}

// tauri-build embeds its manifest into bins only; test binaries need Common Controls v6 too.
fn embed_windows_app_manifest_in_every_target() {
    if !targets_msvc() {
        return;
    }
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(WINDOWS_APP_MANIFEST_FILE);
    println!("cargo:rerun-if-changed={}", manifest.display());
    println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
    println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
}
