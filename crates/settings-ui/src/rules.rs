//! Which bindings may be used as the Dictation key.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Accept,
    /// Allowed, but the user is told why it may misbehave.
    Warn(String),
    Reject(String),
}

pub fn is_modifier(name: &str) -> bool {
    matches!(
        name,
        "LeftCtrl"
            | "RightCtrl"
            | "LeftShift"
            | "RightShift"
            | "LeftAlt"
            | "RightAlt"
            | "LeftWin"
            | "RightWin"
    )
}

fn is_ctrl(n: &str) -> bool {
    n.ends_with("Ctrl")
}
fn is_alt(n: &str) -> bool {
    n.ends_with("Alt")
}
fn is_win(n: &str) -> bool {
    n.ends_with("Win")
}
fn is_shift(n: &str) -> bool {
    n.ends_with("Shift")
}

/// Keys that produce text or edit it; unsafe to use alone.
fn is_typing_key(name: &str) -> bool {
    let one = name.len() == 1 && name.chars().all(|c| c.is_ascii_alphanumeric());
    one || matches!(
        name,
        "Space"
            | "Enter"
            | "Tab"
            | "Backspace"
            | "Escape"
            | "Comma"
            | "Period"
            | "Semicolon"
            | "Quote"
            | "Slash"
            | "Backslash"
            | "Minus"
            | "Equals"
            | "BracketLeft"
            | "BracketRight"
            | "Backtick"
    )
}

pub fn validate(keys: &[String]) -> Verdict {
    let mods: Vec<&str> = keys
        .iter()
        .map(|s| s.as_str())
        .filter(|k| is_modifier(k))
        .collect();
    let others: Vec<&str> = keys
        .iter()
        .map(|s| s.as_str())
        .filter(|k| !is_modifier(k))
        .collect();
    if keys.is_empty() {
        return Verdict::Reject("No key was pressed.".into());
    }
    if others.len() > 1 {
        return Verdict::Reject(
            "Use at most one key besides modifier keys (Ctrl, Shift, Alt, Win).".into(),
        );
    }
    let ctrl = mods.iter().any(|m| is_ctrl(m));
    let alt = mods.iter().any(|m| is_alt(m));
    let win = mods.iter().any(|m| is_win(m));
    let shift_only = mods.iter().all(|m| is_shift(m));
    if let Some(&key) = others.first() {
        if key == "Delete" && ctrl && alt {
            return Verdict::Reject(
                "Ctrl+Alt+Delete is reserved by Windows and cannot be used.".into(),
            );
        }
        if key == "L" && win {
            return Verdict::Reject("Win+L locks the computer and cannot be used.".into());
        }
        if is_typing_key(key) && shift_only {
            return Verdict::Reject(format!(
                "{key} is needed for typing. Combine it with Ctrl, Alt or Win, or pick another key."
            ));
        }
        let ctrl_only = mods.iter().all(|m| is_ctrl(m)) && ctrl;
        let known = (alt && key == "Tab")
            || (alt && key == "F4")
            || (alt && key == "Escape")
            || (win && key.len() == 1 && key.chars().all(|c| c.is_ascii_alphabetic()))
            || (ctrl_only && key == "Space")
            || (ctrl && key == "Escape");
        if known {
            return Verdict::Warn(
                "This combination is a Windows or app shortcut and may do something else too."
                    .into(),
            );
        }
        return Verdict::Accept;
    }
    if mods == ["RightAlt"] {
        return Verdict::Warn(
            "Right Alt is AltGr on many keyboard layouts and may interfere with typing characters."
                .into(),
        );
    }
    if mods.len() == 1 && (is_alt(mods[0]) || is_win(mods[0])) {
        return Verdict::Warn("A lone Alt or Win key also opens menus or Start; Aloud Type masks this, but some apps may still react.".into());
    }
    Verdict::Accept
}

#[cfg(test)]
mod tests {
    use super::*;
    fn v(k: &[&str]) -> Verdict {
        validate(&k.iter().map(|s| s.to_string()).collect::<Vec<_>>())
    }
    fn rejected(k: &[&str]) -> bool {
        matches!(v(k), Verdict::Reject(_))
    }
    fn warned(k: &[&str]) -> bool {
        matches!(v(k), Verdict::Warn(_))
    }

    #[test]
    fn lone_typing_keys_are_rejected() {
        for k in [
            "A",
            "7",
            "Space",
            "Enter",
            "Tab",
            "Backspace",
            "Escape",
            "Comma",
            "Semicolon",
        ] {
            assert!(rejected(&[k]), "{k}");
        }
    }
    #[test]
    fn shift_plus_typing_key_is_still_typing() {
        assert!(rejected(&["LeftShift", "A"]));
    }
    #[test]
    fn single_modifiers_and_function_keys_are_accepted() {
        assert_eq!(v(&["RightCtrl"]), Verdict::Accept);
        assert_eq!(v(&["LeftCtrl"]), Verdict::Accept);
        assert_eq!(v(&["F9"]), Verdict::Accept);
        assert_eq!(v(&["LeftCtrl", "LeftShift"]), Verdict::Accept);
        assert_eq!(v(&["LeftCtrl", "Q"]), Verdict::Accept);
    }
    #[test]
    fn win_l_and_ctrl_alt_del_are_rejected() {
        assert!(rejected(&["LeftWin", "L"]));
        assert!(rejected(&["RightWin", "L"]));
        assert!(rejected(&["LeftCtrl", "LeftAlt", "Delete"]));
        assert!(rejected(&["RightCtrl", "RightAlt", "Delete"]));
    }
    #[test]
    fn two_non_modifier_keys_are_rejected() {
        assert!(rejected(&["A", "B"]));
        assert!(rejected(&[]));
    }
    #[test]
    fn known_system_shortcuts_warn() {
        assert!(warned(&["LeftAlt", "Tab"]));
        assert!(warned(&["LeftWin", "E"]));
        assert!(warned(&["LeftCtrl", "Space"]));
        assert!(warned(&["LeftAlt", "F4"]));
    }
    #[test]
    fn altgr_and_lone_alt_or_win_warn() {
        assert!(warned(&["RightAlt"]));
        assert!(warned(&["LeftAlt"]));
        assert!(warned(&["LeftWin"]));
    }
}
