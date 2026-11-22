//! The host controller at its seam: key events in, typed text and indicator out.
use aloud_type_app::testing::*;
use aloud_type_app::*;
use aloud_type_core::lifecycle::LifecycleConfig;
use aloud_type_core::listening::{FaultReason, Indicator, Tray};
use aloud_type_core::power::{PowerAction, PowerEvent};
use aloud_type_core::supervisor::{Worker, WorkerFault};
use aloud_type_core::testing::{FakeClock, FakeKeyboard, FakeRecognizer};
use aloud_type_core::{KeyEvent, Segment};

const CHUNK_MS: u64 = 100;

fn seg(t: &str) -> Vec<Segment> {
    vec![Segment {
        text: t.into(),
        words: vec![],
        no_speech_prob: 0.0,
    }]
}

struct Rig {
    c: Controller<FakeKeyboard, FakeClock>,
    kb: FakeKeyboard,
    clock: FakeClock,
    cap: FakeCapture,
    typed: FakeSink,
    gate: FakeStartGate,
}

fn rig(script: Vec<&str>) -> Rig {
    rig_with(
        script,
        ControllerConfig {
            chunk_ms: CHUNK_MS,
            hangover_ms: 1000,
        },
    )
}

fn rig_with(script: Vec<&str>, config: ControllerConfig) -> Rig {
    let kb = FakeKeyboard::new();
    let clock = FakeClock::new();
    let cap = FakeCapture::default();
    let typed = FakeSink::default();
    let gate = FakeStartGate::default();
    let mut rec = FakeRecognizer::new();
    for s in script {
        rec = rec.then(seg(s));
    }
    let c = Controller::new(Parts {
        keyboard: kb.clone(),
        clock: clock.clone(),
        lifecycle: LifecycleConfig {
            modifier_key: true,
            ..Default::default()
        },
        capture: Box::new(cap.clone()),
        recognizer: Box::new(rec),
        voice_gate: Box::new(aloud_type_core::testing::FakeVoiceGate(0.1)),
        sink: Box::new(typed.clone()),
        start_gate: Box::new(gate.clone()),
        typing_faults: Box::new(typed.clone()),
        settings: Box::new(DictationSettings::default),
        config,
    });
    Rig {
        c,
        kb,
        clock,
        cap,
        typed,
        gate,
    }
}

impl Rig {
    fn press(&mut self) {
        self.kb.press(KeyEvent::DictationKeyDown);
        self.c.step();
    }
    fn release(&mut self) {
        self.kb.press(KeyEvent::DictationKeyUp);
        self.c.step();
    }
    /// Voiced audio for `ms`, delivered in 100 ms packets with a step each.
    fn speak(&mut self, ms: u64) {
        for _ in 0..ms / CHUNK_MS {
            self.clock.advance(CHUNK_MS);
            self.cap.push(&vec![0.5; (CHUNK_MS * 16) as usize]);
            self.c.step();
        }
    }
    fn wait(&mut self, ms: u64) {
        self.clock.advance(ms);
        self.c.step();
    }
}

#[test]
fn holding_types_confirmed_words_while_speaking_then_flushes_on_release() {
    let mut r = rig(vec!["hello", "hello world", "hello world today"]);
    r.press();
    assert!(r.cap.is_open());
    r.clock.advance(300); // held well past the tap threshold
    r.speak(300);
    assert_eq!(
        r.typed.text(),
        "hello",
        "confirmed words appear while still held"
    );
    r.release();
    assert_eq!(
        r.typed.text(),
        "hello world today",
        "release flushes the rest"
    );
    assert!(!r.cap.is_open());
    assert_eq!(r.c.indicator(), Indicator::Hidden);
    assert_eq!(r.c.tray(), Tray::Idle);
}

#[test]
fn with_the_default_config_words_appear_within_two_seconds_of_speech_starting() {
    // Each decode of the growing utterance extends the hypothesis by about a second of speech.
    let mut r = rig_with(
        vec![
            "one",
            "one two",
            "one two three",
            "one two three four",
            "one two three four five",
        ],
        ControllerConfig::default(),
    );
    r.press();
    r.clock.advance(300);
    r.speak(1900);
    assert!(
        !r.typed.text().is_empty(),
        "nothing typed 1.9 s into speech: {:?}",
        r.typed.text()
    );
}

#[test]
fn tap_goes_hands_free_and_the_next_press_ends_and_flushes() {
    let mut r = rig(vec!["one", "one two"]);
    r.press();
    r.clock.advance(100);
    r.release();
    assert!(r.cap.is_open(), "still listening after a tap");
    r.speak(200);
    r.press();
    assert_eq!(r.typed.text(), "one two");
    assert!(!r.cap.is_open());
    r.release(); // its release is ignored
    assert!(!r.cap.is_open());
}

#[test]
fn elevated_window_shows_an_amber_notice_and_starts_nothing() {
    let mut r = rig(vec![]);
    r.gate.set(Some("Can't type into an administrator window"));
    r.press();
    assert!(!r.cap.is_open());
    assert_eq!(
        r.c.indicator(),
        Indicator::TypingFault {
            message: "Can't type into an administrator window".into()
        }
    );
}

#[test]
fn elevated_notice_on_a_tap_still_goes_after_about_2_seconds() {
    let mut r = rig(vec![]);
    r.gate.set(Some("Can't type into an administrator window"));
    r.press();
    r.clock.advance(100);
    r.release(); // a tap
    r.wait(1800);
    assert!(matches!(r.c.indicator(), Indicator::TypingFault { .. }));
    r.wait(200);
    assert_eq!(r.c.indicator(), Indicator::Hidden);
    r.gate.set(None);
    r.press(); // the engine is not stuck hands-free: this starts a Dictation
    assert!(r.cap.is_open());
}

#[test]
fn missing_microphone_is_a_red_fault_that_lingers_after_release() {
    let mut r = rig(vec![]);
    r.cap.fail_start(FaultReason::NoMicrophone);
    r.press();
    assert!(matches!(
        r.c.indicator(),
        Indicator::ListeningFault {
            reason: FaultReason::NoMicrophone,
            ..
        }
    ));
    r.clock.advance(300);
    r.release();
    r.wait(1900);
    assert!(matches!(r.c.indicator(), Indicator::ListeningFault { .. }));
    r.wait(200);
    assert_eq!(r.c.indicator(), Indicator::Hidden);
}

#[test]
fn hands_free_fault_flushes_audio_and_the_next_dictation_starts_normally() {
    let mut r = rig(vec!["a", "a b", "a b c", "a b c", "x"]);
    r.press();
    r.clock.advance(100);
    r.release();
    r.speak(300);
    r.cap.raise_fault(FaultReason::Disconnected);
    r.wait(50);
    assert!(!r.cap.is_open());
    assert_eq!(
        r.typed.text(),
        "a b c",
        "audio captured before the fault is flushed"
    );
    assert!(matches!(
        r.c.indicator(),
        Indicator::ListeningFault {
            reason: FaultReason::Disconnected,
            ..
        }
    ));
    assert_eq!(r.c.tray(), Tray::Fault("Microphone disconnected"));
    r.wait(5100); // auto-dismiss
    assert_eq!(r.c.indicator(), Indicator::Hidden);
    r.press(); // engine was reset: this starts, not ends
    assert!(r.cap.is_open());
}

#[test]
fn a_tap_on_a_standing_hands_free_fault_only_dismisses_it() {
    let mut r = rig(vec![]);
    r.press();
    r.clock.advance(100);
    r.release();
    r.cap.raise_fault(FaultReason::InUse);
    r.wait(50);
    r.press();
    r.release();
    assert_eq!(r.c.indicator(), Indicator::Hidden);
    assert!(!r.cap.is_open());
    r.press();
    assert!(r.cap.is_open(), "the dismissing tap left the engine idle");
}

#[test]
fn rejected_typing_ends_the_dictation_with_an_amber_fault() {
    let mut r = rig(vec!["hello", "hello world", "hello world there"]);
    r.press();
    r.clock.advance(300);
    r.speak(200);
    r.typed.reject_next("Couldn't type into this window");
    r.speak(100);
    assert!(!r.cap.is_open());
    assert_eq!(
        r.c.indicator(),
        Indicator::TypingFault {
            message: "Couldn't type into this window".into()
        }
    );
    assert_eq!(
        r.c.tray(),
        Tray::Idle,
        "typing faults do not touch the tray"
    );
}

#[test]
fn chord_cancels_the_dictation_without_flushing() {
    let mut r = rig(vec!["hello", "hello world", "never typed"]);
    r.press();
    r.speak(100);
    r.kb.press(KeyEvent::OtherKeyDown);
    r.c.step();
    assert!(!r.cap.is_open());
    assert_eq!(r.typed.text(), "");
}

#[test]
fn sleep_ends_the_dictation_without_flushing_and_resume_asks_the_host_to_reinstall() {
    let mut r = rig(vec!["hello", "hello world"]);
    r.press();
    r.clock.advance(300);
    r.speak(200);
    assert!(r.c.power(PowerEvent::Suspend).is_empty());
    assert!(!r.cap.is_open());
    assert_eq!(r.typed.text(), "");
    assert_eq!(
        r.c.power(PowerEvent::Resume),
        vec![
            PowerAction::ReinstallHook,
            PowerAction::ReenumerateMicrophones
        ]
    );
}

#[test]
fn a_dead_recognizer_worker_shows_an_amber_fault() {
    let mut r = rig(vec![]);
    r.press();
    r.clock.advance(300);
    r.c.worker_fault(WorkerFault::for_worker(Worker::Recognizer));
    assert_eq!(
        r.c.indicator(),
        Indicator::TypingFault {
            message: "Speech recognition stopped working".into()
        }
    );
    assert!(!r.cap.is_open());
}

#[test]
fn a_dead_audio_worker_shows_a_red_fault() {
    let mut r = rig(vec![]);
    r.press();
    r.clock.advance(300);
    r.c.worker_fault(WorkerFault::for_worker(Worker::Audio));
    assert!(matches!(
        r.c.indicator(),
        Indicator::ListeningFault {
            reason: FaultReason::MicrophoneError,
            ..
        }
    ));
}

/// Recognizer that records the length of every window it is given.
struct Probe(std::rc::Rc<std::cell::RefCell<Vec<usize>>>);
impl aloud_type_core::Recognizer for Probe {
    fn recognize(&mut self, a: &[f32], _: &str, _: Option<&str>) -> Vec<Segment> {
        self.0.borrow_mut().push(a.len());
        vec![]
    }
}
/// Voice gate that follows a script of per-chunk verdicts.
struct Script(std::collections::VecDeque<bool>);
impl aloud_type_core::VoiceGate for Script {
    fn is_speech(&mut self, _: &[f32]) -> bool {
        self.0.pop_front().unwrap_or(false)
    }
}

#[test]
fn a_brief_non_speech_verdict_inside_speech_does_not_cut_a_hole_in_the_audio() {
    let lens = std::rc::Rc::new(std::cell::RefCell::new(vec![]));
    let (kb, clock, cap, sink) = (
        FakeKeyboard::new(),
        FakeClock::new(),
        FakeCapture::default(),
        FakeSink::default(),
    );
    let mut c = Controller::new(Parts {
        keyboard: kb.clone(),
        clock: clock.clone(),
        lifecycle: LifecycleConfig::default(),
        capture: Box::new(cap.clone()),
        recognizer: Box::new(Probe(lens.clone())),
        voice_gate: Box::new(Script([true, false, true].into())),
        sink: Box::new(sink.clone()),
        start_gate: Box::new(FakeStartGate::default()),
        typing_faults: Box::new(sink),
        settings: Box::new(DictationSettings::default),
        config: ControllerConfig {
            chunk_ms: CHUNK_MS,
            hangover_ms: 1000,
        },
    });
    kb.press(KeyEvent::DictationKeyDown);
    c.step();
    for _ in 0..3 {
        clock.advance(CHUNK_MS);
        cap.push(&vec![0.5; 1600]);
        c.step();
    }
    assert_eq!(*lens.borrow(), vec![1600, 3200, 4800]);
}

mod residency {
    use super::*;
    use aloud_type_app::residency::{Model, Residency, IDLE_UNLOAD_MS};
    use aloud_type_config::ModelMemory;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;

    #[test]
    fn dictations_reload_an_unloaded_model_and_restart_the_idle_clock() {
        let clock = Arc::new(AtomicU64::new(0));
        let c2 = clock.clone();
        let loader: aloud_type_app::residency::Loader =
            Arc::new(|_| Ok(Box::new(FakeRecognizer::new()) as Model));
        let res = Residency::start(
            loader,
            Arc::new(move || c2.load(Ordering::SeqCst)),
            "base",
            ModelMemory::UnloadWhenIdle,
        );
        let mut r = rig(vec![]);
        r.c.set_residency(res.clone());
        let t = std::time::Instant::now();
        while res.loaded_model().is_none() {
            assert!(t.elapsed().as_secs() < 5);
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        clock.store(IDLE_UNLOAD_MS, Ordering::SeqCst);
        r.c.step(); // idle long enough: unloaded
        assert!(res.loaded_model().is_none());
        r.press(); // the Dictation key reloads in parallel with capture
        while res.loaded_model().is_none() {
            assert!(t.elapsed().as_secs() < 5);
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        r.clock.advance(300);
        r.release();
        clock.store(IDLE_UNLOAD_MS * 2 - 1, Ordering::SeqCst);
        r.c.step();
        assert!(
            res.loaded_model().is_some(),
            "idle clock restarted at the end of the Dictation"
        );
        clock.store(IDLE_UNLOAD_MS * 3, Ordering::SeqCst);
        r.c.step();
        assert!(res.loaded_model().is_none());
    }
}

#[test]
fn a_key_press_before_setup_opens_settings_and_starts_nothing() {
    let mut r = rig(vec![]);
    let setup = FakeSetup::default();
    setup.set_complete(false);
    r.c.set_setup_gate(Box::new(setup.clone()));
    r.press();
    assert_eq!(setup.opened(), 1);
    assert!(!r.cap.is_open());
    assert_eq!(r.c.indicator(), Indicator::Hidden);
    r.release();
    setup.set_complete(true);
    r.press();
    assert!(r.cap.is_open(), "after setup the same key dictates");
    assert_eq!(setup.opened(), 1);
}

#[test]
fn a_changed_tap_threshold_and_key_kind_apply_to_the_next_press() {
    let mut r = rig(vec![]);
    r.c.set_lifecycle(LifecycleConfig {
        tap_threshold_ms: 1000,
        modifier_key: false,
    });
    r.press();
    r.clock.advance(600);
    r.release();
    assert!(
        r.cap.is_open(),
        "600 ms is a tap under a 1000 ms threshold, so it went hands-free"
    );
}

#[test]
fn an_idle_controller_asks_for_no_polling_and_an_active_one_does() {
    let mut r = rig(vec!["hello"]);
    r.c.step();
    assert_eq!(
        r.c.poll_interval(),
        None,
        "nothing is pending: the worker may sleep until woken"
    );
    r.press();
    assert!(
        r.c.poll_interval().is_some(),
        "key down: timers and audio need servicing"
    );
    r.clock.advance(300);
    r.speak(200);
    assert!(r.c.poll_interval().is_some());
    r.release();
    assert_eq!(
        r.c.poll_interval(),
        None,
        "back to idle once the Dictation is over"
    );
}

#[test]
fn a_lingering_fault_keeps_polling_until_it_is_gone() {
    let mut r = rig(vec![]);
    r.cap.fail_start(FaultReason::NoMicrophone);
    r.press();
    r.clock.advance(300);
    r.release();
    assert!(
        r.c.poll_interval().is_some(),
        "the red overlay is still timing out"
    );
    r.wait(2100);
    assert_eq!(r.c.indicator(), Indicator::Hidden);
    assert_eq!(r.c.poll_interval(), None);
}
