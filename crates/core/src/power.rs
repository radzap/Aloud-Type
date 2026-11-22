//! Sleep and resume policy: suspend ends any Dictation and closes audio;
//! resume re-installs the hook and re-enumerates microphones.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerEvent {
    Suspend,
    Resume,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerAction {
    /// End the Dictation in progress (flush nothing; the PC is going to sleep).
    EndDictation,
    CloseAudio,
    ReinstallHook,
    ReenumerateMicrophones,
}

pub fn on_power(event: PowerEvent) -> Vec<PowerAction> {
    match event {
        PowerEvent::Suspend => vec![PowerAction::EndDictation, PowerAction::CloseAudio],
        PowerEvent::Resume => vec![
            PowerAction::ReinstallHook,
            PowerAction::ReenumerateMicrophones,
        ],
    }
}

/// Windows reports a resume twice (automatic and user-triggered); this drops
/// repeats so each suspend/resume pair is handled once.
#[derive(Debug, Default)]
pub struct PowerTracker {
    last: Option<PowerEvent>,
}

impl PowerTracker {
    pub fn observe(&mut self, e: PowerEvent) -> Option<PowerEvent> {
        if self.last == Some(e) {
            return None;
        }
        self.last = Some(e);
        Some(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_resume_notifications_are_handled_once() {
        let mut t = PowerTracker::default();
        assert_eq!(t.observe(PowerEvent::Suspend), Some(PowerEvent::Suspend));
        assert_eq!(t.observe(PowerEvent::Resume), Some(PowerEvent::Resume));
        assert_eq!(t.observe(PowerEvent::Resume), None);
        assert_eq!(t.observe(PowerEvent::Suspend), Some(PowerEvent::Suspend));
    }

    #[test]
    fn suspend_ends_dictation_then_closes_audio() {
        assert_eq!(
            on_power(PowerEvent::Suspend),
            vec![PowerAction::EndDictation, PowerAction::CloseAudio]
        );
    }

    #[test]
    fn resume_reinstalls_hook_and_reenumerates_microphones() {
        assert_eq!(
            on_power(PowerEvent::Resume),
            vec![
                PowerAction::ReinstallHook,
                PowerAction::ReenumerateMicrophones
            ]
        );
    }
}
