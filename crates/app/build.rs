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
        embed_resource::compile("aloud-type.rc", embed_resource::NONE)
            .manifest_optional()
            .unwrap();
    }
}
