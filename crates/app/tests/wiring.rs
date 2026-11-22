//! Settings, Settings-window commands and state publishing, through fakes.
use aloud_type_app::wiring::*;
use aloud_type_config::*;
use aloud_type_core::ipc::Command;
use aloud_type_core::lifecycle::LifecycleConfig;
use aloud_type_core::listening::{FaultReason, Indicator, Tray};
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone, Default)]
struct Log(Rc<RefCell<Vec<String>>>);
impl Log {
    fn take(&self) -> Vec<String> {
        std::mem::take(&mut *self.0.borrow_mut())
    }
    fn add(&self, s: String) {
        self.0.borrow_mut().push(s);
    }
}
impl Targets for Log {
    fn set_keys(&mut self, v: &[u32]) {
        self.add(format!("keys {v:?}"));
    }
    fn set_suspended(&mut self, s: bool) {
        self.add(format!("suspended {s}"));
    }
    fn set_lifecycle(&mut self, c: LifecycleConfig) {
        self.add(format!("life {} {}", c.tap_threshold_ms, c.modifier_key));
    }
    fn set_style(&mut self, s: IndicatorStyle) {
        self.add(format!("style {}", s.as_str()));
    }
    fn set_placement(&mut self, v: VerticalPosition, h: HorizontalPosition) {
        self.add(format!("place {} {}", v.as_str(), h.as_str()));
    }
    fn set_memory(&mut self, m: ModelMemory) {
        self.add(format!("memory {}", m.as_str()));
    }
    fn request_model(&mut self, id: &str) {
        self.add(format!("model {id}"));
    }
    fn retest_gpu(&mut self) {
        self.add("retest".into());
    }
    fn notify(&mut self, t: &str) {
        self.add(format!("notify {t}"));
    }
}

type Rig = (SettingsHost<Log>, Log, Rc<RefCell<Settings>>, Rc<RefCell<Option<String>>>);

fn host(initial: Settings) -> Rig {
    let log = Log::default();
    let cur = Rc::new(RefCell::new(initial));
    let notice = Rc::new(RefCell::new(None));
    let (c, n) = (cur.clone(), notice.clone());
    let source = Box::new(move || Loaded { settings: c.borrow().clone(), notice: n.borrow_mut().take() });
    (SettingsHost::new(log.clone(), source), log, cur, notice)
}

#[test]
fn first_reload_applies_everything_including_the_configured_key_not_a_fixed_one() {
    let s = Settings { dictation_key: vec!["F9".into()], ..Settings::default() };
    let (mut h, log, _, _) = host(s);
    h.reload();
    let l = log.take();
    assert!(l.contains(&"keys [120]".to_string()), "{l:?}"); // F9 = 0x78
    assert!(l.contains(&"life 250 false".to_string()), "F9 is not a modifier: no chord rule");
    assert!(l.contains(&"style trace-strip".to_string()));
    assert!(l.contains(&"place bottom center".to_string()));
    assert!(l.contains(&"model base-q5_1".to_string()));
}

#[test]
fn right_ctrl_is_a_modifier_key() {
    let (mut h, log, _, _) = host(Settings::default());
    h.reload();
    let l = log.take();
    assert!(l.contains(&"keys [163]".to_string()));
    assert!(l.contains(&"life 250 true".to_string()));
}

#[test]
fn reload_applies_only_what_changed_without_restart() {
    let (mut h, log, cur, _) = host(Settings::default());
    h.reload();
    log.take();
    h.reload();
    assert!(log.take().is_empty(), "nothing changed, nothing touched");
    {
        let mut s = cur.borrow_mut();
        s.indicator_style = IndicatorStyle::Orb;
        s.indicator_horizontal = HorizontalPosition::Left;
        s.model_memory = ModelMemory::UnloadWhenIdle;
        s.microphone = Some("Headset".into());
        s.language = Language::Auto;
    }
    h.on_command(Command::ReloadSettings);
    assert_eq!(log.take(), vec!["style orb", "place bottom left", "memory unload-when-idle"]);
    let d = h.dictation_settings();
    assert_eq!(d.borrow().language, "auto");
    assert_eq!(d.borrow().microphone, aloud_type_app::Mic::Specific("Headset".into()));
}

#[test]
fn changed_key_and_tap_threshold_are_applied_live() {
    let (mut h, log, cur, _) = host(Settings::default());
    h.reload();
    log.take();
    {
        let mut s = cur.borrow_mut();
        s.dictation_key = vec!["LeftCtrl".into(), "Space".into()];
        s.tap_threshold_ms = 400;
    }
    h.reload();
    assert_eq!(log.take(), vec!["keys [162, 32]", "life 400 false"]);
}

#[test]
fn an_unknown_key_name_keeps_the_current_binding() {
    let (mut h, log, cur, _) = host(Settings::default());
    h.reload();
    log.take();
    cur.borrow_mut().dictation_key = vec!["NoSuchKey".into()];
    h.reload();
    assert!(log.take().iter().all(|l| !l.starts_with("keys")));
}

#[test]
fn suspend_resume_and_retest_commands_reach_their_targets() {
    let (mut h, log, _, _) = host(Settings::default());
    h.on_command(Command::SuspendKeys);
    h.on_command(Command::ResumeKeys);
    h.on_command(Command::RetestGpu);
    assert_eq!(log.take(), vec!["suspended true", "suspended false", "retest"]);
}

#[test]
fn an_invalid_settings_file_notice_goes_to_the_tray() {
    let (mut h, log, _, notice) = host(Settings::default());
    h.reload();
    log.take();
    *notice.borrow_mut() = Some("Settings file is invalid.".into());
    h.reload();
    assert_eq!(log.take(), vec!["notify Settings file is invalid."]);
}

#[derive(Default)]
struct Shown {
    indicators: usize,
    trays: Vec<(Tray, bool, bool)>,
}
impl Surfaces for Shown {
    fn indicator(&mut self, _: &Indicator) {
        self.indicators += 1;
    }
    fn tray(&mut self, t: &Tray, k: bool, b: bool) {
        self.trays.push((t.clone(), k, b));
    }
}

#[test]
fn the_overlay_gets_every_tick_but_the_tray_only_changes() {
    let (mut p, mut s) = (Publisher::default(), Shown::default());
    for _ in 0..3 {
        p.publish(&mut s, &Indicator::Hidden, &Tray::Idle, false, false);
    }
    let fault = Tray::Fault(FaultReason::MicrophoneError.message());
    p.publish(&mut s, &Indicator::Hidden, &fault, false, false);
    p.publish(&mut s, &Indicator::Hidden, &fault, true, false);
    assert_eq!(s.indicators, 5);
    assert_eq!(s.trays.len(), 3);
    assert!(s.trays[2].1, "dead hook is reported");
}

#[test]
fn a_spent_restart_budget_shows_the_fault_and_a_restart_only_drops_the_dictation() {
    use aloud_type_app::testing::*;
    use aloud_type_app::*;
    use aloud_type_core::supervisor::{Report, Worker, WorkerFault};
    use aloud_type_core::testing::{FakeClock, FakeKeyboard, FakeRecognizer, FakeVoiceGate};
    let mut c = Controller::new(Parts {
        keyboard: FakeKeyboard::new(),
        clock: FakeClock::new(),
        lifecycle: LifecycleConfig::default(),
        capture: Box::new(FakeCapture::default()),
        recognizer: Box::new(FakeRecognizer::new()),
        voice_gate: Box::new(FakeVoiceGate(0.1)),
        sink: Box::new(FakeSink::default()),
        start_gate: Box::new(FakeStartGate::default()),
        typing_faults: Box::new(FakeSink::default()),
        settings: Box::new(DictationSettings::default),
        config: ControllerConfig::default(),
    });
    assert!(!handle_report(&mut c, Report::Restarted(Worker::Recognizer)));
    assert_eq!(c.tray(), Tray::Idle);
    let f = WorkerFault::for_worker(Worker::Audio);
    assert!(handle_report(&mut c, Report::Faulted(Worker::Audio, f)));
    assert_eq!(c.tray(), Tray::Fault("Microphone error"));
    assert!(matches!(c.indicator(), Indicator::ListeningFault { .. }));
}

#[test]
fn the_settings_window_is_the_sibling_executable() {
    let p = settings_exe_beside(std::path::Path::new("C:/Apps/AloudType/aloud-type.exe"));
    assert_eq!(p, std::path::PathBuf::from("C:/Apps/AloudType/aloud-type-settings-ui.exe"));
}
