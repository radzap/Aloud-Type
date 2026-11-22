//! Embeds the app icon into the Settings executable.
fn main() {
    println!("cargo:rerun-if-changed=settings-ui.rc");
    println!("cargo:rerun-if-changed=../../assets/icons/app.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile("settings-ui.rc", embed_resource::NONE)
            .manifest_optional()
            .unwrap();
    }
}
