//! Pure classification of raw low-level keyboard events. No Win32 calls, so
//! it is unit-testable; the hook callback only calls `Classifier::process`.
use aloud_type_core::KeyEvent;

pub const VK_LCONTROL: u32 = 0xA2;
pub const VK_RCONTROL: u32 = 0xA3;
pub const VK_LMENU: u32 = 0xA4;
pub const VK_RMENU: u32 = 0xA5;
pub const VK_LWIN: u32 = 0x5B;
pub const VK_RWIN: u32 = 0x5C;

/// Magic `dwExtraInfo` tag on every event the app injects ("ALOD").
pub const INJECT_TAG: usize = 0x414C_4F44;

pub fn is_modifier(vk: u32) -> bool {
    matches!(vk, 0x10 | 0x11 | 0x12 | 0xA0..=0xA5 | VK_LWIN | VK_RWIN)
}

/// Alt and Win activate a menu / Start when tapped alone.
pub fn needs_mask(vk: u32) -> bool {
    matches!(vk, 0x12 | VK_LMENU | VK_RMENU | VK_LWIN | VK_RWIN)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawKey {
    pub vk: u32,
    pub up: bool,
    pub injected: bool,
    pub extra_info: usize,
}

/// A key event the platform must inject (tagged with `INJECT_TAG`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Inject {
    Down(u32),
    Up(u32),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Decision {
    pub swallow: bool,
    pub post: Option<KeyEvent>,
    pub inject: Vec<Inject>,
}

impl Decision {
    fn pass() -> Self {
        Self::default()
    }
}

#[derive(Debug)]
pub struct Classifier {
    key: u32,
    modifier: bool,
    mask_key: u32,
    held: bool,
    replayed: bool,
    /// Extra keys of a combination binding (empty for a single key).
    combo: Vec<u32>,
    combo_down: Vec<u32>,
    /// Trigger vk whose down was swallowed and whose up must be too.
    swallowed_down: Option<u32>,
    suspended: bool,
}

impl Classifier {
    pub fn new(dictation_vk: u32) -> Self {
        Self {
            key: dictation_vk,
            modifier: is_modifier(dictation_vk),
            mask_key: VK_LCONTROL,
            held: false,
            replayed: false,
            combo: vec![],
            combo_down: vec![],
            swallowed_down: None,
            suspended: false,
        }
    }

    /// A single key (chord rule, mask key) or a combination (modifiers plus one
    /// key). In a combination the modifiers pass through to the app; only the
    /// non-modifier key is swallowed. The chord rule does not apply to combos.
    pub fn with_keys(keys: &[u32]) -> Self {
        let mut c = Self::new(keys.first().copied().unwrap_or(VK_RCONTROL));
        c.set_keys(keys);
        c
    }

    /// Live rebinding: replaces the binding and forgets any held state.
    pub fn set_keys(&mut self, keys: &[u32]) {
        let mut fresh = Self::new(keys.first().copied().unwrap_or(VK_RCONTROL));
        if keys.len() > 1 {
            fresh.combo = keys.to_vec();
        }
        fresh.suspended = self.suspended;
        *self = fresh;
    }

    /// While suspended (key capture in Settings) every event passes through.
    pub fn set_suspended(&mut self, suspended: bool) {
        self.suspended = suspended;
        if suspended {
            let keys = if self.combo.is_empty() { vec![self.key] } else { self.combo.clone() };
            self.set_keys(&keys);
        }
    }

    fn process_combo(&mut self, k: RawKey) -> Decision {
        if !self.combo.contains(&k.vk) {
            return Decision::pass();
        }
        let swallow_it = !is_modifier(k.vk);
        if k.up {
            self.combo_down.retain(|v| *v != k.vk);
            let swallow = self.swallowed_down == Some(k.vk);
            if swallow {
                self.swallowed_down = None;
            }
            let post = if self.held {
                self.held = false;
                Some(KeyEvent::DictationKeyUp)
            } else {
                None
            };
            return Decision { swallow, post, inject: vec![] };
        }
        if self.combo_down.contains(&k.vk) {
            // auto-repeat
            return Decision { swallow: self.held && swallow_it, ..Decision::pass() };
        }
        self.combo_down.push(k.vk);
        if self.combo_down.len() == self.combo.len() {
            self.held = true;
            if swallow_it {
                self.swallowed_down = Some(k.vk);
            }
            return Decision { swallow: swallow_it, post: Some(KeyEvent::DictationKeyDown), inject: vec![] };
        }
        Decision::pass()
    }

    pub fn is_modifier_key(&self) -> bool {
        self.modifier
    }

    /// True while the Dictation key is physically held (watchdog skips then).
    pub fn held(&self) -> bool {
        self.held
    }

    pub fn process(&mut self, k: RawKey) -> Decision {
        if self.suspended || (k.injected && k.extra_info == INJECT_TAG) {
            return Decision::pass();
        }
        if !self.combo.is_empty() {
            return self.process_combo(k);
        }
        if k.vk == self.key {
            return if k.up { self.dictation_up() } else { self.dictation_down() };
        }
        if !k.up && self.held && self.modifier && !self.replayed {
            self.replayed = true;
            return Decision {
                swallow: true,
                post: Some(KeyEvent::OtherKeyDown),
                inject: vec![Inject::Down(self.key), Inject::Down(k.vk)],
            };
        }
        Decision::pass()
    }

    fn dictation_down(&mut self) -> Decision {
        if self.held {
            // Auto-repeat: swallow so it neither types nor re-triggers.
            return Decision { swallow: true, ..Decision::pass() };
        }
        self.held = true;
        self.replayed = false;
        Decision { swallow: true, post: Some(KeyEvent::DictationKeyDown), inject: vec![] }
    }

    fn dictation_up(&mut self) -> Decision {
        if !self.held {
            return Decision::pass();
        }
        self.held = false;
        if self.replayed {
            // The app saw an injected down, so let the real up through.
            self.replayed = false;
            return Decision::pass();
        }
        let inject = if needs_mask(self.key) {
            vec![Inject::Down(self.mask_key), Inject::Up(self.mask_key)]
        } else {
            vec![]
        };
        Decision { swallow: true, post: Some(KeyEvent::DictationKeyUp), inject }
    }
}

/// Watchdog policy: Windows removes a timed-out hook silently, so re-install
/// periodically, but never while the Dictation key is held.
pub fn should_reinstall(now_ms: u64, last_install_ms: u64, interval_ms: u64, key_held: bool) -> bool {
    !key_held && now_ms.saturating_sub(last_install_ms) >= interval_ms
}

#[cfg(test)]
mod tests {
    use super::*;

    fn down(vk: u32) -> RawKey {
        RawKey { vk, up: false, injected: false, extra_info: 0 }
    }
    fn up(vk: u32) -> RawKey {
        RawKey { vk, up: true, ..down(vk) }
    }

    #[test]
    fn dictation_key_is_swallowed_and_posted_on_down_and_up() {
        let mut c = Classifier::new(VK_RCONTROL);
        let d = c.process(down(VK_RCONTROL));
        assert_eq!((d.swallow, d.post), (true, Some(KeyEvent::DictationKeyDown)));
        let u = c.process(up(VK_RCONTROL));
        assert_eq!((u.swallow, u.post), (true, Some(KeyEvent::DictationKeyUp)));
        assert!(u.inject.is_empty());
    }

    #[test]
    fn auto_repeat_is_swallowed_without_new_events() {
        let mut c = Classifier::new(VK_RCONTROL);
        c.process(down(VK_RCONTROL));
        let r = c.process(down(VK_RCONTROL));
        assert_eq!((r.swallow, r.post), (true, None));
    }

    #[test]
    fn unrelated_keys_pass_through() {
        let mut c = Classifier::new(VK_RCONTROL);
        assert_eq!(c.process(down(0x41)), Decision::pass());
        assert_eq!(c.process(up(0x41)), Decision::pass());
        assert_eq!(c.process(up(VK_RCONTROL)), Decision::pass()); // stray up
    }

    #[test]
    fn chord_swallows_other_key_and_replays_modifier_then_key() {
        let mut c = Classifier::new(VK_RCONTROL);
        c.process(down(VK_RCONTROL));
        let d = c.process(down(0x43)); // C
        assert!(d.swallow);
        assert_eq!(d.post, Some(KeyEvent::OtherKeyDown));
        assert_eq!(d.inject, vec![Inject::Down(VK_RCONTROL), Inject::Down(0x43)]);
        // Later keys pass; the real modifier release passes without an event.
        assert_eq!(c.process(down(0x56)), Decision::pass());
        assert_eq!(c.process(up(VK_RCONTROL)), Decision::pass());
        // Next press starts fresh.
        assert_eq!(c.process(down(VK_RCONTROL)).post, Some(KeyEvent::DictationKeyDown));
    }

    #[test]
    fn non_modifier_dictation_key_does_not_apply_chord_rule() {
        let mut c = Classifier::new(0x78); // F9
        c.process(down(0x78));
        assert_eq!(c.process(down(0x41)), Decision::pass());
    }

    #[test]
    fn lone_alt_or_win_tap_injects_mask_key() {
        for vk in [VK_RMENU, VK_LWIN] {
            let mut c = Classifier::new(vk);
            c.process(down(vk));
            let u = c.process(up(vk));
            assert_eq!(u.inject, vec![Inject::Down(VK_LCONTROL), Inject::Up(VK_LCONTROL)]);
        }
    }

    #[test]
    fn own_tagged_injected_events_are_ignored_but_foreign_injected_are_not() {
        let mut c = Classifier::new(VK_RCONTROL);
        let own = RawKey { injected: true, extra_info: INJECT_TAG, ..down(VK_RCONTROL) };
        assert_eq!(c.process(own), Decision::pass());
        assert!(!c.held());
        let foreign = RawKey { injected: true, extra_info: 7, ..down(VK_RCONTROL) };
        assert_eq!(c.process(foreign).post, Some(KeyEvent::DictationKeyDown));
    }

    #[test]
    fn watchdog_reinstalls_after_interval_unless_key_held() {
        assert!(!should_reinstall(9_999, 0, 10_000, false));
        assert!(should_reinstall(10_000, 0, 10_000, false));
        assert!(!should_reinstall(60_000, 0, 10_000, true));
    }

    const Q: u32 = 0x51;

    #[test]
    fn combo_starts_when_the_last_key_goes_down_and_swallows_only_the_trigger() {
        let mut c = Classifier::with_keys(&[VK_LCONTROL, Q]);
        assert_eq!(c.process(down(VK_LCONTROL)), Decision::pass());
        let d = c.process(down(Q));
        assert!(d.swallow);
        assert_eq!(d.post, Some(KeyEvent::DictationKeyDown));
        assert!(c.held());
        // auto-repeat of the trigger is swallowed and posts nothing
        assert_eq!(c.process(down(Q)), Decision { swallow: true, ..Decision::default() });
        let u = c.process(up(Q));
        assert!(u.swallow);
        assert_eq!(u.post, Some(KeyEvent::DictationKeyUp));
        assert!(!c.held());
        assert_eq!(c.process(up(VK_LCONTROL)), Decision::pass());
    }

    #[test]
    fn combo_ends_when_a_modifier_is_released_first() {
        let mut c = Classifier::with_keys(&[VK_LCONTROL, Q]);
        c.process(down(VK_LCONTROL));
        c.process(down(Q));
        let u = c.process(up(VK_LCONTROL));
        assert!(!u.swallow);
        assert_eq!(u.post, Some(KeyEvent::DictationKeyUp));
        // trigger up afterwards was swallowed on down, so swallow its up too
        assert_eq!(c.process(up(Q)), Decision { swallow: true, ..Decision::default() });
    }

    #[test]
    fn modifier_only_combo_passes_the_modifiers_through() {
        let mut c = Classifier::with_keys(&[VK_LCONTROL, 0xA0]);
        c.process(down(VK_LCONTROL));
        let d = c.process(down(0xA0));
        assert!(!d.swallow);
        assert_eq!(d.post, Some(KeyEvent::DictationKeyDown));
        assert_eq!(c.process(up(0xA0)).post, Some(KeyEvent::DictationKeyUp));
    }

    #[test]
    fn other_key_with_a_partial_combo_is_untouched() {
        let mut c = Classifier::with_keys(&[VK_LCONTROL, Q]);
        c.process(down(VK_LCONTROL));
        assert_eq!(c.process(down(0x43)), Decision::pass());
        assert!(!c.held());
    }

    #[test]
    fn suspended_classifier_passes_everything_and_forgets_held_state() {
        let mut c = Classifier::new(VK_RCONTROL);
        c.process(down(VK_RCONTROL));
        c.set_suspended(true);
        assert!(!c.held());
        assert_eq!(c.process(down(VK_RCONTROL)), Decision::pass());
        c.set_suspended(false);
        assert_eq!(c.process(up(VK_RCONTROL)), Decision::pass());
        assert_eq!(c.process(down(VK_RCONTROL)).post, Some(KeyEvent::DictationKeyDown));
    }

    #[test]
    fn set_keys_rebinds_live() {
        let mut c = Classifier::new(VK_RCONTROL);
        c.set_keys(&[0x78]);
        assert_eq!(c.process(down(VK_RCONTROL)), Decision::pass());
        assert_eq!(c.process(down(0x78)).post, Some(KeyEvent::DictationKeyDown));
    }
}
