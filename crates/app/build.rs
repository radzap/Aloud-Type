//! Embeds the per-monitor DPI manifest into the executable (MSVC linker).
fn main() {
    println!("cargo:rerun-if-changed=aloud-type.manifest");
    let target = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if target == "windows" && env == "msvc" {
        let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg-bins=/MANIFESTINPUT:{dir}/aloud-type.manifest");
        println!("cargo:rustc-link-arg-bins=/MANIFESTUAC:NO");
    }
}
