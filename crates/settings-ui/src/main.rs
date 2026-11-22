//! Stand-alone entry point for the Settings process (the host app's `--settings`
//! flag runs this executable). With `--background-models` it runs the downloads
//! the resident process asked for, without a window.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() -> iced::Result {
    if std::env::args().any(|a| a == aloud_type_settings_ui::background::FLAG) {
        std::process::exit(aloud_type_settings_ui::background::run_default());
    }
    aloud_type_settings_ui::run()
}
