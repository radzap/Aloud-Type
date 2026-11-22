//! Key names stored in settings.toml (see ticket 06): "RightCtrl", "A", "F5"...
//! Shared by the Settings process (capture) and the resident hook (binding).

/// Name for a Windows virtual-key code, or None for keys we cannot bind
/// (mouse buttons, unknown codes). Generic (side-less) modifier codes are not
/// named: the hook reports the left/right variants.
pub fn name_for_vk(vk: u32) -> Option<String> {
    let s = match vk {
        0xA0 => "LeftShift",
        0xA1 => "RightShift",
        0xA2 => "LeftCtrl",
        0xA3 => "RightCtrl",
        0xA4 => "LeftAlt",
        0xA5 => "RightAlt",
        0x5B => "LeftWin",
        0x5C => "RightWin",
        0x08 => "Backspace",
        0x09 => "Tab",
        0x0D => "Enter",
        0x1B => "Escape",
        0x20 => "Space",
        0x21 => "PageUp",
        0x22 => "PageDown",
        0x23 => "End",
        0x24 => "Home",
        0x25 => "Left",
        0x26 => "Up",
        0x27 => "Right",
        0x28 => "Down",
        0x2D => "Insert",
        0x2E => "Delete",
        0x14 => "CapsLock",
        0x90 => "NumLock",
        0x91 => "ScrollLock",
        0x13 => "Pause",
        0x2C => "PrintScreen",
        0x5D => "Menu",
        0xBA => "Semicolon",
        0xBB => "Equals",
        0xBC => "Comma",
        0xBD => "Minus",
        0xBE => "Period",
        0xBF => "Slash",
        0xC0 => "Backtick",
        0xDB => "BracketLeft",
        0xDC => "Backslash",
        0xDD => "BracketRight",
        0xDE => "Quote",
        0x30..=0x39 | 0x41..=0x5A => return Some((vk as u8 as char).to_string()),
        0x70..=0x87 => return Some(format!("F{}", vk - 0x6F)),
        _ => return None,
    };
    Some(s.to_string())
}

/// Inverse of [`name_for_vk`].
pub fn vk_for_name(name: &str) -> Option<u32> {
    (0u32..256).find(|vk| name_for_vk(*vk).as_deref() == Some(name))
}

const MODIFIER_ORDER: [&str; 8] = [
    "LeftCtrl",
    "RightCtrl",
    "LeftShift",
    "RightShift",
    "LeftAlt",
    "RightAlt",
    "LeftWin",
    "RightWin",
];

/// Modifiers first in a fixed order, then the other key: a stable stored form.
pub fn canonical(keys: &[String]) -> Vec<String> {
    let mut out: Vec<String> = MODIFIER_ORDER
        .iter()
        .filter(|m| keys.iter().any(|k| k == *m))
        .map(|m| m.to_string())
        .collect();
    out.extend(
        keys.iter()
            .filter(|k| !MODIFIER_ORDER.contains(&k.as_str()))
            .cloned(),
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_match_the_settings_file_convention() {
        assert_eq!(name_for_vk(0xA3).as_deref(), Some("RightCtrl"));
        assert_eq!(name_for_vk(0x41).as_deref(), Some("A"));
        assert_eq!(name_for_vk(0x74).as_deref(), Some("F5"));
        assert_eq!(name_for_vk(0x01), None);
    }
    #[test]
    fn names_map_back_to_key_codes() {
        assert_eq!(vk_for_name("RightCtrl"), Some(0xA3));
        assert_eq!(vk_for_name("F5"), Some(0x74));
        assert_eq!(vk_for_name("Q"), Some(0x51));
        assert_eq!(vk_for_name("Nope"), None);
    }
    #[test]
    fn canonical_puts_modifiers_first_in_fixed_order() {
        let k = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            canonical(&k(&["Q", "LeftShift", "LeftCtrl"])),
            k(&["LeftCtrl", "LeftShift", "Q"])
        );
    }
}
