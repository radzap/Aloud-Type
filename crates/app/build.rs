//! Embeds the per-monitor DPI manifest (MSVC linker) and the app icon into the executable.
fn main() {
    println!("cargo:rerun-if-changed=aloud-type.manifest");
    println!("cargo:rerun-if-changed=aloud-type.rc");
    println!("cargo:rerun-if-changed=../../assets/icons/app.ico");
    let target = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if target == "windows" && env == "msvc" {
        let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg-bins=/MANIFESTINPUT:{dir}/aloud-type.manifest");
        println!("cargo:rustc-link-arg-bins=/MANIFESTUAC:NO");
        // The Vulkan GPU backend imports vulkan-1.dll, which only exists where a GPU driver
        // installed it. Delay-load it so the app still starts without it; the hooks in
        // `src/vulkan_guard.rs` then make Vulkan initialisation fail cleanly (CPU only).
        if std::env::var_os("CARGO_FEATURE_VULKAN").is_some() {
            println!("cargo:rustc-link-arg-bins=/DELAYLOAD:vulkan-1.dll");
            println!("cargo:rustc-link-arg-bins=delayimp.lib");
        }
        embed_resource::compile("aloud-type.rc", embed_resource::NONE)
            .manifest_optional()
            .unwrap();
    }
}
