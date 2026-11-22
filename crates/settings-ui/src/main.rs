//! Stand-alone entry point for the Settings process (the host app's `--settings`
//! flag calls the same `aloud_type_settings_ui::run`).
fn main() -> iced::Result {
    aloud_type_settings_ui::run()
}
