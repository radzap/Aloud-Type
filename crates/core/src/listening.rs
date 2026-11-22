//! Listening indicator and tray state: loudness, Listening faults (red),
//! Typing faults (amber), fault dismissal timing and recovery.
//!
//! Driven by a `Clock`; the host feeds it audio packets, platform fault
//! reports and key events, calls `tick()` regularly, and reads `indicator()`,
//! `tray()` and `take_actions()`.
use crate::seams::*;

pub const NO_SOUND_MS: Millis = 4000;
pub const STALL_MS: Millis = 1000;
pub const DIGITAL_SILENCE_MS: Millis = 4000;
pub const HOLD_LINGER_MS: Millis = 2000;
pub const HANDS_FREE_DISMISS_MS: Millis = 5000;
pub const FALLBACK_NOTE_MS: Millis = 2000;
pub const FALLBACK_NOTE: &str = "Using default microphone";
/// A Dictation refused at the key press (administrator window) shows its notice this long.
pub const REFUSAL_NOTICE_MS: Millis = 2000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaultReason {
    NoMicrophone,
    Disconnected,
    InUse,
    Blocked,
    MutedOrSilent,
    /// Recoverable: clears once a voice is heard.
    NoSound,
    MicrophoneError,
}

impl FaultReason {
    pub fn message(self) -> &'static str {
        match self {
            Self::NoMicrophone => "No microphone found",
            Self::Disconnected => "Microphone disconnected",
            Self::InUse => "Microphone in use by another app",
            Self::Blocked => "Windows is blocking the microphone",
            Self::MutedOrSilent => "Microphone is muted or silent",
            Self::NoSound => "No sound detected",
            Self::MicrophoneError => "Microphone error",
        }
    }
    pub fn is_recoverable(self) -> bool {
        self == Self::NoSound
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Indicator {
    Hidden,
    /// Normal listening (not red). `note` is the brief missing-microphone note.
    Listening {
        level: f32,
        note: Option<&'static str>,
    },
    /// Red, grown, with the reason.
    ListeningFault {
        reason: FaultReason,
        message: &'static str,
        recoverable: bool,
    },
    /// Amber, with the reason.
    TypingFault {
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tray {
    Idle,
    Listening,
    /// Reason shown in the tooltip.
    Fault(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Stop capture, flush audio captured so far through recognition and type
    /// it, then end the Dictation. Text already typed stays.
    EndDictationFlushAudio,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Hold,
    HandsFree,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyOutcome {
    /// Nothing to do here.
    Ignored,
    /// A standing fault was dismissed; do not start a Dictation (hands-free tap).
    Dismissed,
    /// A lingering fault was closed; start a new Dictation (call `start`).
    DismissedStartNew,
}

#[derive(Debug, Clone, PartialEq)]
enum Standing {
    Listening(FaultReason),
    Typing(String),
}

struct Active {
    started: Millis,
    last_packet: Millis,
    last_nonzero: Millis,
    voice_heard: bool,
    no_sound_shown: bool,
    level: f32,
    note_until: Option<Millis>,
}

enum Phase {
    Idle,
    Active(Active),
    /// `fixed`: the dismissal time does not depend on the key (a refused start).
    Fault {
        what: Standing,
        dismiss_at: Option<Millis>,
        fixed: bool,
    },
}

pub struct ListeningMonitor<C: Clock> {
    clock: C,
    phase: Phase,
    mode: Mode,
    key_held: bool,
    tray_fault: Option<FaultReason>,
    actions: Vec<Action>,
}

impl<C: Clock> ListeningMonitor<C> {
    pub fn new(clock: C) -> Self {
        Self {
            clock,
            phase: Phase::Idle,
            mode: Mode::Hold,
            key_held: false,
            tray_fault: None,
            actions: vec![],
        }
    }

    /// A Dictation starts with the key held. `fallback_note` is set when the
    /// chosen microphone was missing and the system default is used.
    pub fn start(&mut self, fallback_note: bool) {
        let now = self.clock.now();
        self.mode = Mode::Hold;
        self.key_held = true;
        self.phase = Phase::Active(Active {
            started: now,
            last_packet: now,
            last_nonzero: now,
            voice_heard: false,
            no_sound_shown: false,
            level: 0.0,
            note_until: fallback_note.then_some(now + FALLBACK_NOTE_MS),
        });
    }

    /// The press became hands-free (tap latched).
    pub fn latch(&mut self) {
        self.mode = Mode::HandsFree;
        self.key_held = false;
        let now = self.clock.now();
        if let Phase::Fault {
            dismiss_at,
            fixed: false,
            ..
        } = &mut self.phase
        {
            *dismiss_at = Some(now + HANDS_FREE_DISMISS_MS);
        }
    }

    /// The Dictation key was released in hold mode.
    pub fn release(&mut self) {
        self.key_held = false;
        let now = self.clock.now();
        if let (
            Phase::Fault {
                dismiss_at,
                fixed: false,
                ..
            },
            Mode::Hold,
        ) = (&mut self.phase, self.mode)
        {
            *dismiss_at = Some(now + HOLD_LINGER_MS);
        }
    }

    /// The Dictation ended normally (no fault).
    pub fn end(&mut self) {
        if let Phase::Active(_) = self.phase {
            self.phase = Phase::Idle;
        }
    }

    /// Dictation key pressed while this monitor may show a fault.
    pub fn key_down(&mut self) -> KeyOutcome {
        match (&self.phase, self.mode) {
            (Phase::Fault { .. }, Mode::HandsFree) => {
                self.phase = Phase::Idle;
                KeyOutcome::Dismissed
            }
            (Phase::Fault { .. }, Mode::Hold) if !self.key_held => {
                self.phase = Phase::Idle;
                KeyOutcome::DismissedStartNew
            }
            _ => KeyOutcome::Ignored,
        }
    }

    /// One audio packet (non-empty) and whether the voice gate heard speech.
    pub fn audio(&mut self, samples: &[f32], voice: bool) {
        let now = self.clock.now();
        let Phase::Active(a) = &mut self.phase else {
            return;
        };
        if samples.is_empty() {
            return;
        }
        a.last_packet = now;
        if samples.iter().any(|s| *s != 0.0) {
            a.last_nonzero = now;
        }
        let rms = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt();
        a.level = rms.min(1.0);
        if voice {
            a.voice_heard = true;
            a.no_sound_shown = false;
            self.tray_fault = None;
        }
    }

    /// A fault reported by the platform. Recoverable reasons are not platform reports.
    pub fn platform_fault(&mut self, reason: FaultReason) {
        if reason.is_recoverable() {
            return;
        }
        self.end_with(reason);
    }

    /// The Dictation key went down but no Dictation may start (an administrator
    /// window has focus): an amber notice for a fixed ~2 s, whatever the key does.
    pub fn refuse_start(&mut self, message: impl Into<String>) {
        let now = self.clock.now();
        self.mode = Mode::Hold;
        self.key_held = true;
        self.phase = Phase::Fault {
            what: Standing::Typing(message.into()),
            dismiss_at: Some(now + REFUSAL_NOTICE_MS),
            fixed: true,
        };
    }

    /// Typing could not be delivered: ends the Dictation with an amber message.
    pub fn typing_fault(&mut self, message: impl Into<String>) {
        if !matches!(self.phase, Phase::Active(_)) {
            return;
        }
        self.enter_fault(Standing::Typing(message.into()));
    }

    /// Evaluate the timing rules.
    pub fn tick(&mut self) {
        let now = self.clock.now();
        let mut ending = None;
        match &mut self.phase {
            Phase::Active(a) => {
                if a.note_until.is_some_and(|t| now >= t) {
                    a.note_until = None;
                }
                if now - a.last_packet > STALL_MS {
                    ending = Some(FaultReason::Disconnected);
                } else if now - a.last_nonzero >= DIGITAL_SILENCE_MS {
                    ending = Some(FaultReason::MutedOrSilent);
                } else if !a.voice_heard && now - a.started >= NO_SOUND_MS {
                    a.no_sound_shown = true;
                }
            }
            Phase::Fault {
                dismiss_at: Some(t),
                ..
            } if now >= *t => self.phase = Phase::Idle,
            _ => {}
        }
        if let Some(r) = ending {
            self.end_with(r);
        }
    }

    fn end_with(&mut self, reason: FaultReason) {
        if !matches!(self.phase, Phase::Active(_)) {
            return;
        }
        self.tray_fault = Some(reason);
        self.enter_fault(Standing::Listening(reason));
    }

    fn enter_fault(&mut self, what: Standing) {
        let now = self.clock.now();
        let dismiss_at = (self.mode == Mode::HandsFree).then_some(now + HANDS_FREE_DISMISS_MS);
        self.actions.push(Action::EndDictationFlushAudio);
        self.phase = Phase::Fault {
            what,
            dismiss_at,
            fixed: false,
        };
    }

    pub fn take_actions(&mut self) -> Vec<Action> {
        std::mem::take(&mut self.actions)
    }

    pub fn indicator(&self) -> Indicator {
        match &self.phase {
            Phase::Idle => Indicator::Hidden,
            Phase::Active(a) if a.no_sound_shown => Indicator::ListeningFault {
                reason: FaultReason::NoSound,
                message: FaultReason::NoSound.message(),
                recoverable: true,
            },
            Phase::Active(a) => Indicator::Listening {
                level: a.level,
                note: a.note_until.map(|_| FALLBACK_NOTE),
            },
            Phase::Fault {
                what: Standing::Listening(r),
                ..
            } => Indicator::ListeningFault {
                reason: *r,
                message: r.message(),
                recoverable: false,
            },
            Phase::Fault {
                what: Standing::Typing(m),
                ..
            } => Indicator::TypingFault { message: m.clone() },
        }
    }

    /// Tray state. The recoverable "No sound detected" never affects it.
    pub fn tray(&self) -> Tray {
        match (&self.phase, self.tray_fault) {
            (Phase::Active(_), _) => Tray::Listening,
            (_, Some(r)) => Tray::Fault(r.message()),
            _ => Tray::Idle,
        }
    }

    /// True while the microphone-blocked fault stands (tray menu entry).
    pub fn blocked_fault_standing(&self) -> bool {
        self.tray_fault == Some(FaultReason::Blocked)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::FakeClock;

    const VOICE: [f32; 4] = [0.5, -0.5, 0.5, -0.5];
    const QUIET: [f32; 4] = [0.001, -0.001, 0.001, 0.0];
    const ZEROS: [f32; 4] = [0.0; 4];

    fn rig() -> (ListeningMonitor<FakeClock>, FakeClock) {
        let c = FakeClock::new();
        (ListeningMonitor::new(c.clone()), c)
    }

    /// Feed `packet` every 100 ms for `ms`, ticking each time.
    fn feed(
        m: &mut ListeningMonitor<FakeClock>,
        c: &FakeClock,
        ms: u64,
        packet: &[f32],
        voice: bool,
    ) {
        for _ in 0..ms / 100 {
            c.advance(100);
            m.audio(packet, voice);
            m.tick();
        }
    }

    fn no_sound(m: &ListeningMonitor<FakeClock>) -> bool {
        matches!(
            m.indicator(),
            Indicator::ListeningFault {
                reason: FaultReason::NoSound,
                ..
            }
        )
    }

    #[test]
    fn hidden_until_a_dictation_starts_and_level_follows_loudness() {
        let (mut m, c) = rig();
        assert_eq!(m.indicator(), Indicator::Hidden);
        assert_eq!(m.tray(), Tray::Idle);
        m.start(false);
        feed(&mut m, &c, 100, &VOICE, true);
        assert_eq!(
            m.indicator(),
            Indicator::Listening {
                level: 0.5,
                note: None
            }
        );
        assert_eq!(m.tray(), Tray::Listening);
        feed(&mut m, &c, 100, &QUIET, false);
        let Indicator::Listening { level, .. } = m.indicator() else {
            panic!()
        };
        assert!(level < 0.01);
    }

    #[test]
    fn no_sound_appears_at_4s_without_voice_and_never_touches_the_tray() {
        let (mut m, c) = rig();
        m.start(false);
        feed(&mut m, &c, 3900, &QUIET, false);
        assert!(!no_sound(&m));
        feed(&mut m, &c, 100, &QUIET, false);
        assert!(no_sound(&m));
        assert_eq!(
            m.indicator(),
            Indicator::ListeningFault {
                reason: FaultReason::NoSound,
                message: "No sound detected",
                recoverable: true
            }
        );
        assert_eq!(m.tray(), Tray::Listening);
        assert!(m.take_actions().is_empty());
    }

    #[test]
    fn no_sound_clears_when_a_voice_is_heard_and_does_not_return() {
        let (mut m, c) = rig();
        m.start(false);
        feed(&mut m, &c, 5000, &QUIET, false);
        assert!(no_sound(&m));
        feed(&mut m, &c, 100, &VOICE, true);
        assert!(matches!(m.indicator(), Indicator::Listening { .. }));
        feed(&mut m, &c, 10_000, &QUIET, false);
        assert!(
            matches!(m.indicator(), Indicator::Listening { .. }),
            "silence after a voice never faults"
        );
    }

    #[test]
    fn voice_inside_the_first_4s_prevents_no_sound() {
        let (mut m, c) = rig();
        m.start(false);
        feed(&mut m, &c, 3000, &QUIET, false);
        feed(&mut m, &c, 100, &VOICE, true);
        feed(&mut m, &c, 3000, &QUIET, false);
        assert!(!no_sound(&m));
    }

    #[test]
    fn stall_over_1s_ends_with_a_flush_and_a_tray_fault() {
        let (mut m, c) = rig();
        m.start(false);
        feed(&mut m, &c, 500, &VOICE, true);
        c.advance(1000);
        m.tick();
        assert!(
            matches!(m.indicator(), Indicator::Listening { .. }),
            "exactly 1 s is not over 1 s"
        );
        c.advance(1);
        m.tick();
        assert_eq!(
            m.indicator(),
            Indicator::ListeningFault {
                reason: FaultReason::Disconnected,
                message: "Microphone disconnected",
                recoverable: false
            }
        );
        assert_eq!(m.take_actions(), vec![Action::EndDictationFlushAudio]);
        assert_eq!(m.tray(), Tray::Fault("Microphone disconnected"));
    }

    #[test]
    fn digital_silence_for_4s_ends_even_though_packets_flow() {
        let (mut m, c) = rig();
        m.start(false);
        feed(&mut m, &c, 3900, &ZEROS, false);
        assert!(matches!(m.indicator(), Indicator::Listening { .. }));
        feed(&mut m, &c, 100, &ZEROS, false);
        assert!(matches!(
            m.indicator(),
            Indicator::ListeningFault {
                reason: FaultReason::MutedOrSilent,
                recoverable: false,
                ..
            }
        ));
        assert_eq!(m.take_actions(), vec![Action::EndDictationFlushAudio]);
    }

    #[test]
    fn any_nonzero_sample_resets_the_digital_silence_run() {
        let (mut m, c) = rig();
        m.start(false);
        feed(&mut m, &c, 3000, &ZEROS, false);
        feed(&mut m, &c, 100, &QUIET, true);
        feed(&mut m, &c, 3000, &ZEROS, false);
        assert!(matches!(m.indicator(), Indicator::Listening { .. }));
    }

    #[test]
    fn every_platform_reason_has_its_message() {
        let cases = [
            (FaultReason::NoMicrophone, "No microphone found"),
            (FaultReason::Disconnected, "Microphone disconnected"),
            (FaultReason::InUse, "Microphone in use by another app"),
            (FaultReason::Blocked, "Windows is blocking the microphone"),
            (FaultReason::MutedOrSilent, "Microphone is muted or silent"),
            (FaultReason::MicrophoneError, "Microphone error"),
        ];
        for (r, msg) in cases {
            let (mut m, _) = rig();
            m.start(false);
            m.platform_fault(r);
            assert_eq!(
                m.indicator(),
                Indicator::ListeningFault {
                    reason: r,
                    message: msg,
                    recoverable: false
                }
            );
            assert_eq!(m.tray(), Tray::Fault(msg));
            assert_eq!(m.take_actions(), vec![Action::EndDictationFlushAudio]);
        }
        assert_eq!(FaultReason::NoSound.message(), "No sound detected");
    }

    #[test]
    fn platform_cannot_report_the_recoverable_reason_as_ending() {
        let (mut m, _) = rig();
        m.start(false);
        m.platform_fault(FaultReason::NoSound);
        assert!(matches!(m.indicator(), Indicator::Listening { .. }));
    }

    #[test]
    fn hold_mode_fault_stays_while_held_then_lingers_2s_after_release() {
        let (mut m, c) = rig();
        m.start(false);
        m.platform_fault(FaultReason::NoMicrophone);
        c.advance(30_000);
        m.tick();
        assert!(
            matches!(m.indicator(), Indicator::ListeningFault { .. }),
            "red while the key is held"
        );
        m.release();
        c.advance(1999);
        m.tick();
        assert!(matches!(m.indicator(), Indicator::ListeningFault { .. }));
        c.advance(1);
        m.tick();
        assert_eq!(m.indicator(), Indicator::Hidden);
        assert_eq!(
            m.tray(),
            Tray::Fault("No microphone found"),
            "tray keeps the reason"
        );
    }

    #[test]
    fn key_press_during_lingering_fault_closes_it_and_starts_a_new_dictation() {
        let (mut m, c) = rig();
        m.start(false);
        m.platform_fault(FaultReason::InUse);
        m.release();
        c.advance(500);
        assert_eq!(m.key_down(), KeyOutcome::DismissedStartNew);
        assert_eq!(m.indicator(), Indicator::Hidden);
        m.start(false);
        assert_eq!(m.tray(), Tray::Listening);
    }

    #[test]
    fn hands_free_fault_auto_dismisses_after_5s() {
        let (mut m, c) = rig();
        m.start(false);
        m.latch();
        feed(&mut m, &c, 500, &VOICE, true);
        m.platform_fault(FaultReason::Disconnected);
        c.advance(4999);
        m.tick();
        assert!(matches!(m.indicator(), Indicator::ListeningFault { .. }));
        c.advance(1);
        m.tick();
        assert_eq!(m.indicator(), Indicator::Hidden);
    }

    #[test]
    fn hands_free_next_tap_only_dismisses() {
        let (mut m, c) = rig();
        m.start(false);
        m.latch();
        m.platform_fault(FaultReason::Disconnected);
        c.advance(1000);
        assert_eq!(m.key_down(), KeyOutcome::Dismissed);
        assert_eq!(m.indicator(), Indicator::Hidden);
        assert_eq!(m.tray(), Tray::Fault("Microphone disconnected"));
    }

    #[test]
    fn fault_before_latch_gets_the_hands_free_timeout_from_the_latch() {
        let (mut m, c) = rig();
        m.start(false);
        m.platform_fault(FaultReason::NoMicrophone);
        c.advance(100);
        m.latch();
        c.advance(4999);
        m.tick();
        assert!(matches!(m.indicator(), Indicator::ListeningFault { .. }));
        c.advance(1);
        m.tick();
        assert_eq!(m.indicator(), Indicator::Hidden);
    }

    #[test]
    fn tray_fault_clears_on_the_next_successful_dictation_and_blocked_entry_follows() {
        let (mut m, c) = rig();
        m.start(false);
        m.platform_fault(FaultReason::Blocked);
        assert!(m.blocked_fault_standing());
        m.release();
        c.advance(2000);
        m.tick();
        assert!(m.blocked_fault_standing());
        m.start(false);
        assert!(
            m.blocked_fault_standing(),
            "still standing until a voice is heard"
        );
        feed(&mut m, &c, 100, &VOICE, true);
        assert!(!m.blocked_fault_standing());
        m.end();
        assert_eq!(m.tray(), Tray::Idle);
    }

    #[test]
    fn other_faults_do_not_enable_the_privacy_menu_entry() {
        let (mut m, _) = rig();
        m.start(false);
        m.platform_fault(FaultReason::InUse);
        assert!(!m.blocked_fault_standing());
    }

    #[test]
    fn missing_microphone_fallback_shows_a_brief_note_then_clears() {
        let (mut m, c) = rig();
        m.start(true);
        feed(&mut m, &c, 100, &VOICE, true);
        assert_eq!(
            m.indicator(),
            Indicator::Listening {
                level: 0.5,
                note: Some(FALLBACK_NOTE)
            }
        );
        feed(&mut m, &c, 1900, &VOICE, true);
        assert_eq!(
            m.indicator(),
            Indicator::Listening {
                level: 0.5,
                note: None
            }
        );
    }

    #[test]
    fn fallback_note_reads_using_default_microphone_for_about_2s() {
        let (mut m, c) = rig();
        m.start(true);
        assert_eq!(
            m.indicator(),
            Indicator::Listening {
                level: 0.0,
                note: Some("Using default microphone")
            }
        );
        feed(&mut m, &c, 1900, &VOICE, true);
        assert!(matches!(
            m.indicator(),
            Indicator::Listening { note: Some(_), .. }
        ));
        feed(&mut m, &c, 100, &VOICE, true);
        assert!(matches!(
            m.indicator(),
            Indicator::Listening { note: None, .. }
        ));
    }

    #[test]
    fn start_refusal_notice_lasts_2s_even_when_the_press_was_a_tap() {
        let (mut m, c) = rig();
        m.refuse_start("Can't type into an administrator window");
        assert_eq!(
            m.indicator(),
            Indicator::TypingFault {
                message: "Can't type into an administrator window".into()
            }
        );
        assert!(m.take_actions().is_empty());
        c.advance(50);
        m.release();
        m.latch(); // the engine saw a tap and went hands-free
        c.advance(1949);
        m.tick();
        assert!(
            matches!(m.indicator(), Indicator::TypingFault { .. }),
            "still up just before 2 s"
        );
        c.advance(1);
        m.tick();
        assert_eq!(m.indicator(), Indicator::Hidden);
    }

    #[test]
    fn start_refusal_notice_lasts_2s_when_the_key_stays_held() {
        let (mut m, c) = rig();
        m.refuse_start("x");
        c.advance(1999);
        m.tick();
        assert!(matches!(m.indicator(), Indicator::TypingFault { .. }));
        c.advance(1);
        m.tick();
        assert_eq!(m.indicator(), Indicator::Hidden);
    }

    #[test]
    fn typing_fault_is_amber_and_distinct_from_listening_fault() {
        let (mut m, _) = rig();
        m.start(false);
        m.typing_fault("Cannot type into an administrator window");
        let amber = m.indicator();
        assert_eq!(
            amber,
            Indicator::TypingFault {
                message: "Cannot type into an administrator window".into()
            }
        );
        assert_eq!(m.take_actions(), vec![Action::EndDictationFlushAudio]);
        m.platform_fault(FaultReason::NoMicrophone);
        assert_eq!(m.indicator(), amber, "a standing fault is not replaced");
        let (mut r, _) = rig();
        r.start(false);
        r.platform_fault(FaultReason::NoMicrophone);
        assert_ne!(
            std::mem::discriminant(&r.indicator()),
            std::mem::discriminant(&amber)
        );
    }

    #[test]
    fn faults_end_the_dictation_only_once() {
        let (mut m, c) = rig();
        m.start(false);
        c.advance(2000);
        m.tick();
        m.tick();
        assert_eq!(m.take_actions(), vec![Action::EndDictationFlushAudio]);
        c.advance(10);
        m.tick();
        assert!(m.take_actions().is_empty());
    }
}
