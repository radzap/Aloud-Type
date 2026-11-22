//! Pure tray logic: maps the engine's reported state to what the tray shows,
//! builds the menu, names the Dictation key for the Startup notice, and
//! de-duplicates updates so an idle resident process does no work.
use aloud_type_core::listening::Tray;

/// Tooltip shown when the keyboard hook cannot be restored.
pub const KEY_UNAVAILABLE: &str = "Dictation key unavailable";
const APP_NAME: &str = "Aloud Type";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconKind {
    Idle,
    Listening,
    Fault,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayView {
    pub icon: IconKind,
    pub tooltip: String,
}

/// What the tray should show, given the engine's tray state and whether the
/// keyboard hook is gone for good. A dead hook outranks everything: nothing
/// else can happen while the Dictation key does nothing.
pub fn view(engine: &Tray, key_unavailable: bool) -> TrayView {
    if key_unavailable {
        return TrayView {
            icon: IconKind::Fault,
            tooltip: format!("{APP_NAME}: {KEY_UNAVAILABLE}"),
        };
    }
    match engine {
        Tray::Idle => TrayView {
            icon: IconKind::Idle,
            tooltip: APP_NAME.into(),
        },
        Tray::Listening => TrayView {
            icon: IconKind::Listening,
            tooltip: format!("{APP_NAME}: listening"),
        },
        Tray::Fault(reason) => TrayView {
            icon: IconKind::Fault,
            tooltip: format!("{APP_NAME}: {reason}"),
        },
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    OpenSettings,
    OpenMicrophonePrivacySettings,
    Exit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuItem {
    pub label: &'static str,
    pub command: Command,
}

/// The context menu. The privacy entry exists only while the
/// microphone-blocked fault stands.
pub fn menu(blocked_fault_standing: bool) -> Vec<MenuItem> {
    let mut items = vec![MenuItem {
        label: "Settings",
        command: Command::OpenSettings,
    }];
    if blocked_fault_standing {
        items.push(MenuItem {
            label: "Open microphone privacy settings",
            command: Command::OpenMicrophonePrivacySettings,
        });
    }
    items.push(MenuItem {
        label: "Exit",
        command: Command::Exit,
    });
    items
}

/// Double-click opens Settings.
pub fn double_click_command() -> Command {
    Command::OpenSettings
}

/// "RightCtrl" -> "Right Ctrl". Splits CamelCase words; digits stay attached
/// (F9).
pub fn key_display_name(name: &str) -> String {
    let mut out = String::new();
    let mut prev: Option<char> = None;
    for c in name.chars() {
        if let Some(p) = prev {
            if c.is_uppercase() && (p.is_lowercase()) {
                out.push(' ');
            }
        }
        out.push(c);
        prev = Some(c);
    }
    out
}

/// "Right Ctrl", or "Ctrl + Space" for a combination.
pub fn dictation_key_label(keys: &[String]) -> String {
    keys.iter()
        .map(|k| key_display_name(k))
        .collect::<Vec<_>>()
        .join(" + ")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartupAction {
    /// Show the brief notice (about 3 seconds, never takes focus).
    Notice(String),
    /// Setup is incomplete: open Settings instead of the notice.
    OpenSettings,
}

pub const NOTICE_DURATION_MS: u32 = 3000;

pub fn startup_action(first_run_complete: bool, dictation_key: &[String]) -> StartupAction {
    if !first_run_complete {
        return StartupAction::OpenSettings;
    }
    StartupAction::Notice(format!(
        "{APP_NAME} is running. Hold {} to dictate.",
        dictation_key_label(dictation_key)
    ))
}

/// Remembers what the tray currently shows so the host only touches the
/// shell when something really changed (no timers, no polling).
#[derive(Default)]
pub struct TrayState {
    shown: Option<TrayView>,
}

impl TrayState {
    pub fn current(&self) -> Option<&TrayView> {
        self.shown.as_ref()
    }

    /// Returns the view to apply, or `None` when it is already on show.
    pub fn update(&mut self, next: TrayView) -> Option<TrayView> {
        if self.shown.as_ref() == Some(&next) {
            return None;
        }
        self.shown = Some(next.clone());
        Some(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(k: &[&str]) -> Vec<String> {
        k.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn tray_follows_engine_state_with_fault_reason_in_tooltip() {
        assert_eq!(view(&Tray::Idle, false).icon, IconKind::Idle);
        assert_eq!(view(&Tray::Listening, false).icon, IconKind::Listening);
        let v = view(&Tray::Fault("Microphone disconnected"), false);
        assert_eq!(v.icon, IconKind::Fault);
        assert!(v.tooltip.contains("Microphone disconnected"));
    }

    #[test]
    fn dead_hook_shows_fault_with_key_unavailable_tooltip() {
        for engine in [Tray::Idle, Tray::Listening] {
            let v = view(&engine, true);
            assert_eq!(v.icon, IconKind::Fault);
            assert!(v.tooltip.contains("Dictation key unavailable"));
        }
    }

    #[test]
    fn menu_has_settings_and_exit_and_privacy_entry_only_while_blocked() {
        let plain: Vec<_> = menu(false).iter().map(|m| m.command).collect();
        assert_eq!(plain, [Command::OpenSettings, Command::Exit]);
        let blocked: Vec<_> = menu(true).iter().map(|m| m.command).collect();
        assert_eq!(
            blocked,
            [
                Command::OpenSettings,
                Command::OpenMicrophonePrivacySettings,
                Command::Exit
            ]
        );
    }

    #[test]
    fn double_click_opens_settings() {
        assert_eq!(double_click_command(), Command::OpenSettings);
    }

    #[test]
    fn startup_notice_names_the_real_dictation_key() {
        assert_eq!(
            startup_action(true, &keys(&["RightCtrl"])),
            StartupAction::Notice("Aloud Type is running. Hold Right Ctrl to dictate.".into())
        );
        assert_eq!(
            startup_action(true, &keys(&["CapsLock"])),
            StartupAction::Notice("Aloud Type is running. Hold Caps Lock to dictate.".into())
        );
        assert_eq!(dictation_key_label(&keys(&["F9"])), "F9");
        assert_eq!(
            dictation_key_label(&keys(&["LeftCtrl", "Space"])),
            "Left Ctrl + Space"
        );
    }

    #[test]
    fn incomplete_setup_opens_settings_instead_of_notice() {
        assert_eq!(
            startup_action(false, &keys(&["RightCtrl"])),
            StartupAction::OpenSettings
        );
    }

    #[test]
    fn unchanged_state_causes_no_shell_update() {
        let mut s = TrayState::default();
        assert!(s.update(view(&Tray::Idle, false)).is_some());
        assert!(s.update(view(&Tray::Idle, false)).is_none());
        assert!(s.update(view(&Tray::Listening, false)).is_some());
        assert!(s.update(view(&Tray::Listening, false)).is_none());
    }
}

/// The tray icon file for a state. Windows does not recolour tray icons, so
/// each state ships twice: `light_taskbar` picks the dark glyph.
pub fn tray_ico(kind: IconKind, light_taskbar: bool) -> &'static [u8] {
    macro_rules! ico {
        ($n:literal) => {
            include_bytes!(concat!("../../../assets/icons/tray/tray-", $n, ".ico")).as_slice()
        };
    }
    match (kind, light_taskbar) {
        (IconKind::Idle, false) => ico!("idle-dark"),
        (IconKind::Idle, true) => ico!("idle-light"),
        (IconKind::Listening, false) => ico!("listening-dark"),
        (IconKind::Listening, true) => ico!("listening-light"),
        (IconKind::Fault, false) => ico!("fault-dark"),
        (IconKind::Fault, true) => ico!("fault-light"),
    }
}

#[cfg(test)]
mod icon_tests {
    use super::*;
    use crate::icon::ico_image;

    #[test]
    fn every_tray_icon_has_a_small_and_a_large_image() {
        for kind in [IconKind::Idle, IconKind::Listening, IconKind::Fault] {
            for light in [false, true] {
                let ico = tray_ico(kind, light);
                assert_eq!(ico_image(ico, 16).unwrap().1, 16);
                assert_eq!(ico_image(ico, 24).unwrap().1, 24);
                // 150% scaling asks for 24, 125% for 20; odd sizes round up.
                assert_eq!(ico_image(ico, 30).unwrap().1, 32);
                assert_eq!(ico_image(ico, 512).unwrap().1, 48);
            }
        }
        assert_ne!(
            tray_ico(IconKind::Idle, false),
            tray_ico(IconKind::Idle, true)
        );
    }
}
