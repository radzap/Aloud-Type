//! Maps winit/iced physical key code names (the `Debug` text of
//! `keyboard::key::Code`, e.g. "KeyA", "ControlRight") to Windows virtual-key
//! codes, so the Settings window can feed the same `Capture` the hook rules use.

pub fn vk_for_code_name(name: &str) -> Option<u32> {
    if let Some(l) = name.strip_prefix("Key") {
        let c = l.chars().next()?;
        if l.len() == 1 && c.is_ascii_uppercase() {
            return Some(c as u32);
        }
    }
    if let Some(d) = name.strip_prefix("Digit") {
        let c = d.chars().next()?;
        if d.len() == 1 && c.is_ascii_digit() {
            return Some(c as u32);
        }
    }
    if let Some(n) = name.strip_prefix('F').and_then(|n| n.parse::<u32>().ok()) {
        if (1..=24).contains(&n) {
            return Some(0x6F + n);
        }
    }
    Some(match name {
        "ControlLeft" => 0xA2,
        "ControlRight" => 0xA3,
        "ShiftLeft" => 0xA0,
        "ShiftRight" => 0xA1,
        "AltLeft" => 0xA4,
        "AltRight" => 0xA5,
        "SuperLeft" => 0x5B,
        "SuperRight" => 0x5C,
        "Escape" => 0x1B,
        "Space" => 0x20,
        "Enter" | "NumpadEnter" => 0x0D,
        "Tab" => 0x09,
        "Backspace" => 0x08,
        "Delete" => 0x2E,
        "Insert" => 0x2D,
        "Home" => 0x24,
        "End" => 0x23,
        "PageUp" => 0x21,
        "PageDown" => 0x22,
        "ArrowLeft" => 0x25,
        "ArrowUp" => 0x26,
        "ArrowRight" => 0x27,
        "ArrowDown" => 0x28,
        "CapsLock" => 0x14,
        "ScrollLock" => 0x91,
        "Pause" => 0x13,
        "PrintScreen" => 0x2C,
        "ContextMenu" => 0x5D,
        "Comma" => 0xBC,
        "Period" => 0xBE,
        "Semicolon" => 0xBA,
        "Quote" => 0xDE,
        "Slash" => 0xBF,
        "Backslash" => 0xDC,
        "Minus" => 0xBD,
        "Equal" => 0xBB,
        "BracketLeft" => 0xDB,
        "BracketRight" => 0xDD,
        "Backquote" => 0xC0,
        _ => return None,
    })
}

/// "RightCtrl" -> "Right Ctrl"; a combination reads "Left Ctrl + Q".
pub fn display_binding(keys: &[String]) -> String {
    keys.iter().map(|k| split_camel(k)).collect::<Vec<_>>().join(" + ")
}

fn split_camel(s: &str) -> String {
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && c.is_ascii_uppercase() && !s.chars().nth(i - 1).is_some_and(|p| p.is_ascii_uppercase()) {
            out.push(' ');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_names_map_to_virtual_keys() {
        assert_eq!(vk_for_code_name("ControlRight"), Some(0xA3));
        assert_eq!(vk_for_code_name("KeyQ"), Some(0x51));
        assert_eq!(vk_for_code_name("Digit7"), Some(0x37));
        assert_eq!(vk_for_code_name("F9"), Some(0x78));
        assert_eq!(vk_for_code_name("Escape"), Some(0x1B));
        assert_eq!(vk_for_code_name("Unidentified"), None);
    }

    #[test]
    fn bindings_read_naturally() {
        let k = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(display_binding(&k(&["RightCtrl"])), "Right Ctrl");
        assert_eq!(display_binding(&k(&["LeftCtrl", "Q"])), "Left Ctrl + Q");
        assert_eq!(display_binding(&k(&["F9"])), "F9");
    }
}
