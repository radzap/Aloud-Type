//! The controller: one `step()` per ~20 ms drives the Dictation lifecycle,
//! audio, Streaming insertion, the Listening indicator and faults.
use aloud_type_core::lifecycle::{DictationEngine, Effect, LifecycleConfig, Phase};
use aloud_type_core::listening::{Action, FaultReason, Indicator, KeyOutcome, ListeningMonitor, Tray};
use aloud_type_core::power::{on_power, PowerAction, PowerEvent};
use aloud_type_core::streaming::{StreamingConfig, StreamingInsertion, TextSink};
use aloud_type_core::supervisor::WorkerFault;
use crate::residency::Residency;
use aloud_type_core::{Clock, Keyboard, Recognizer, VoiceGate};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// Which microphone to open.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Mic {
    #[default]
    SystemDefault,
    Specific(String),
}

/// Settings read when a Dictation starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DictationSettings {
    pub language: String,
    pub microphone: Mic,
}

impl Default for DictationSettings {
    fn default() -> Self {
        Self { language: "auto".into(), microphone: Mic::SystemDefault }
    }
}

/// Platform seam: microphone capture for one Dictation at a time.
pub trait Capture {
    /// Open the stream. `Ok(true)` means the chosen microphone was missing and
    /// the system default is used (a brief note is shown).
    fn start(&mut self, mic: &Mic) -> Result<bool, FaultReason>;
    fn stop(&mut self);
    /// A fault raised by the stream mid-Dictation.
    fn take_fault(&mut self) -> Option<FaultReason>;
    /// 16 kHz mono samples captured since the last call.
    fn read(&mut self) -> Vec<f32>;
}

/// Platform seam: the check run when the Dictation key goes down. A message
/// means "do not start; show this amber notice" (administrator window).
pub trait StartGate {
    fn check(&mut self) -> Option<String>;
}

/// Host seam: first-run setup. While it is incomplete a Dictation key press
/// opens Settings instead of dictating.
pub trait SetupGate {
    fn complete(&mut self) -> bool;
    fn open_settings(&mut self);
}

/// Platform seam: the typing fault recorded by the text sink, if any.
pub trait TypingFaults {
    fn take(&mut self) -> Option<String>;
}

#[derive(Debug, Clone, Copy)]
pub struct ControllerConfig {
    /// Audio is handed to Streaming insertion in chunks of this length.
    pub chunk_ms: u64,
    /// Chunks this soon after speech still count as speech (no holes inside a sentence).
    pub hangover_ms: u64,
}

impl Default for ControllerConfig {
    fn default() -> Self {
        Self { chunk_ms: 600, hangover_ms: 1000 }
    }
}

pub struct Parts<K, C> {
    pub keyboard: K,
    pub clock: C,
    pub lifecycle: LifecycleConfig,
    pub capture: Box<dyn Capture>,
    pub recognizer: Box<dyn Recognizer>,
    pub voice_gate: Box<dyn VoiceGate>,
    pub sink: Box<dyn TextSink>,
    pub start_gate: Box<dyn StartGate>,
    pub typing_faults: Box<dyn TypingFaults>,
    pub settings: Box<dyn FnMut() -> DictationSettings>,
    pub config: ControllerConfig,
}

// Single-threaded sharing of the long-lived recognizer, gate and sink across
// the per-Dictation `StreamingInsertion`s.
struct SharedRec(Rc<RefCell<Box<dyn Recognizer>>>);
impl Recognizer for SharedRec {
    fn recognize(&mut self, a: &[f32], l: &str, p: Option<&str>) -> Vec<aloud_type_core::Segment> {
        self.0.borrow_mut().recognize(a, l, p)
    }
}
/// The voice gate with a hangover: chunks shortly after speech still count as
/// speech, so a quiet moment inside a sentence is not cut out of the audio.
struct SharedGate {
    inner: Rc<RefCell<Box<dyn VoiceGate>>>,
    raw: Rc<Cell<Option<bool>>>,
    hangover_samples: usize,
    quiet_samples: usize,
    heard: bool,
}
impl VoiceGate for SharedGate {
    fn is_speech(&mut self, a: &[f32]) -> bool {
        let r = self.inner.borrow_mut().is_speech(a);
        self.raw.set(Some(r));
        if r {
            self.heard = true;
            self.quiet_samples = 0;
        } else {
            self.quiet_samples += a.len();
        }
        r || (self.heard && self.quiet_samples <= self.hangover_samples)
    }
}
struct SharedSink(Rc<RefCell<Box<dyn TextSink>>>);
impl TextSink for SharedSink {
    fn type_text(&mut self, t: &str) {
        self.0.borrow_mut().type_text(t)
    }
}

type Insertion<C> = StreamingInsertion<SharedRec, SharedGate, SharedSink, C>;

struct Session<C: Clock> {
    insertion: Insertion<C>,
    /// Audio read but not yet handed to the insertion (less than one chunk).
    pending: Vec<f32>,
    /// A typing fault already ended this session: do not decode or type more.
    failed: bool,
}

pub struct Controller<K: Keyboard, C: Clock + Clone> {
    engine: DictationEngine<K, C>,
    monitor: ListeningMonitor<C>,
    clock: C,
    capture: Box<dyn Capture>,
    rec: Rc<RefCell<Box<dyn Recognizer>>>,
    gate: Rc<RefCell<Box<dyn VoiceGate>>>,
    sink: Rc<RefCell<Box<dyn TextSink>>>,
    last_gate: Rc<Cell<Option<bool>>>,
    start_gate: Box<dyn StartGate>,
    typing_faults: Box<dyn TypingFaults>,
    settings: Box<dyn FnMut() -> DictationSettings>,
    config: ControllerConfig,
    session: Option<Session<C>>,
    latched: bool,
    residency: Option<Residency>,
    setup: Option<Box<dyn SetupGate>>,
}

impl<K: Keyboard, C: Clock + Clone> Controller<K, C> {
    pub fn new(p: Parts<K, C>) -> Self {
        Self {
            engine: DictationEngine::new(p.keyboard, p.clock.clone(), p.lifecycle),
            monitor: ListeningMonitor::new(p.clock.clone()),
            clock: p.clock,
            capture: p.capture,
            rec: Rc::new(RefCell::new(p.recognizer)),
            gate: Rc::new(RefCell::new(p.voice_gate)),
            sink: Rc::new(RefCell::new(p.sink)),
            last_gate: Rc::new(Cell::new(None)),
            start_gate: p.start_gate,
            typing_faults: p.typing_faults,
            settings: p.settings,
            config: p.config,
            session: None,
            latched: false,
            residency: None,
            setup: None,
        }
    }

    /// Tell the model's residency about Dictation start/end and idle ticks.
    pub fn set_residency(&mut self, r: Residency) {
        self.residency = Some(r);
    }

    /// Install the first-run check (absent means setup is never in the way).
    pub fn set_setup_gate(&mut self, g: Box<dyn SetupGate>) {
        self.setup = Some(g);
    }

    /// Apply a changed tap threshold or key kind (takes effect from the next press).
    pub fn set_lifecycle(&mut self, cfg: LifecycleConfig) {
        self.engine.set_config(cfg);
    }

    pub fn indicator(&self) -> Indicator {
        self.monitor.indicator()
    }
    pub fn tray(&self) -> Tray {
        self.monitor.tray()
    }
    pub fn blocked_fault_standing(&self) -> bool {
        self.monitor.blocked_fault_standing()
    }
    pub fn dictating(&self) -> bool {
        self.session.is_some()
    }

    /// Advance the whole app by one tick.
    pub fn step(&mut self) {
        for effect in self.engine.poll() {
            match effect {
                Effect::StartDictation => self.on_key_down(),
                Effect::EndDictation => self.on_end(),
                Effect::CancelDictation => self.cancel(),
                // The platform replays the chord itself.
                Effect::ReplayModifierChord => {}
            }
            self.sync_latch();
        }
        self.sync_latch();
        self.pump_audio();
        if let Some(r) = &self.residency {
            r.tick();
        }
        self.monitor.tick();
        self.run_actions();
        // A fault that ended a hands-free Dictation left the engine latched;
        // once nothing is showing any more, the next press must start afresh.
        if self.engine.phase() == Phase::Latched && self.session.is_none() && self.monitor.indicator() == Indicator::Hidden {
            self.engine.reset();
            self.latched = false;
        }
    }

    /// Sleep ends the Dictation without flushing; the actions the host must
    /// perform itself (hook re-install, microphone re-enumeration) are returned.
    pub fn power(&mut self, e: PowerEvent) -> Vec<PowerAction> {
        let mut host = Vec::new();
        for a in on_power(e) {
            match a {
                PowerAction::EndDictation => self.cancel(),
                PowerAction::CloseAudio => self.capture.stop(),
                other => host.push(other),
            }
        }
        host
    }

    /// A worker died for good (restart budget spent): show its fault.
    pub fn worker_fault(&mut self, f: WorkerFault) {
        // A worker can die while idle: the fault must still show (and the tray
        // keep it), so open the monitor's window for it first.
        if self.session.is_none() && self.monitor.indicator() == Indicator::Hidden {
            self.monitor.start(false);
        }
        match f {
            WorkerFault::Listening(r) => self.monitor.platform_fault(r),
            WorkerFault::Typing(m) => self.typing_failed(m.to_string()),
        }
        self.run_actions();
    }

    /// A worker panicked and is being restarted: drop the broken Dictation.
    pub fn recover(&mut self) {
        self.cancel();
    }

    fn run_actions(&mut self) {
        for a in self.monitor.take_actions() {
            match a {
                Action::EndDictationFlushAudio => self.finish(true),
            }
        }
    }

    fn sync_latch(&mut self) {
        let latched = self.engine.phase() == Phase::Latched;
        if latched && !self.latched {
            self.monitor.latch();
        }
        self.latched = latched;
    }

    fn on_key_down(&mut self) {
        if self.monitor.key_down() == KeyOutcome::Dismissed {
            // The press only dismissed a standing fault: it is not a Dictation.
            self.engine.reset();
            self.latched = false;
            return;
        }
        if let Some(g) = self.setup.as_mut() {
            if !g.complete() {
                g.open_settings();
                self.engine.reset();
                self.latched = false;
                return;
            }
        }
        if let Some(msg) = self.start_gate.check() {
            self.monitor.refuse_start(msg);
            return;
        }
        let settings = (self.settings)();
        // The model reloads (if unloaded) in parallel with opening the microphone.
        if let Some(r) = &self.residency {
            r.dictation_started();
        }
        match self.capture.start(&settings.microphone) {
            Err(reason) => {
                if let Some(r) = &self.residency {
                    r.dictation_ended();
                }
                self.monitor.start(false);
                self.monitor.platform_fault(reason);
                self.monitor.take_actions();
            }
            Ok(note) => {
                self.monitor.start(note);
                self.last_gate.set(None);
                let insertion = StreamingInsertion::new(
                    SharedRec(self.rec.clone()),
                    SharedGate {
                        inner: self.gate.clone(),
                        raw: self.last_gate.clone(),
                        hangover_samples: (self.config.hangover_ms * 16) as usize,
                        quiet_samples: 0,
                        heard: false,
                    },
                    SharedSink(self.sink.clone()),
                    self.clock.clone(),
                    StreamingConfig::default(),
                    &settings.language,
                );
                self.session = Some(Session { insertion, pending: Vec::new(), failed: false });
            }
        }
    }

    /// The Dictation key ended the Dictation: release in hold mode, or the
    /// next press in hands-free mode.
    fn on_end(&mut self) {
        if self.latched {
            // Hands-free: this was a key-down.
            self.latched = false;
            if self.monitor.key_down() == KeyOutcome::Dismissed {
                return;
            }
            self.finish(false);
        } else {
            self.finish(false);
            self.monitor.release();
        }
    }

    fn cancel(&mut self) {
        self.capture.stop();
        if self.session.take().is_some() {
            self.monitor.end();
            self.dictation_over();
        }
    }

    fn pump_audio(&mut self) {
        if self.session.is_none() {
            return;
        }
        if let Some(reason) = self.capture.take_fault() {
            self.monitor.platform_fault(reason);
            return; // the monitor's flush action ends the session
        }
        let samples = self.capture.read();
        if samples.is_empty() {
            return;
        }
        self.monitor.audio(&samples, false);
        if let Some(s) = self.session.as_mut() {
            s.pending.extend_from_slice(&samples);
        }
        self.pump_pending();
    }

    /// Hand any further whole chunks that accumulated to the insertion.
    fn pump_pending(&mut self) {
        let chunk = (self.config.chunk_ms * 16) as usize;
        loop {
            let Some(s) = self.session.as_mut() else { return };
            if s.failed || s.pending.len() < chunk {
                return;
            }
            let part: Vec<f32> = s.pending.drain(..chunk).collect();
            self.last_gate.set(None);
            s.insertion.feed(&part);
            if let Some(v) = self.last_gate.get() {
                self.monitor.audio(&part, v);
            }
            if self.check_typing_faults() {
                return;
            }
        }
    }

    /// Report a typing fault (sink rejection or model too slow). Returns true if one was raised.
    fn check_typing_faults(&mut self) -> bool {
        let slot = self.typing_faults.take();
        let model = self.session.as_ref().and_then(|s| s.insertion.fault()).map(|f| f.message().to_string());
        match slot.or(model) {
            Some(m) => {
                self.typing_failed(m);
                true
            }
            None => false,
        }
    }

    fn typing_failed(&mut self, message: String) {
        if let Some(s) = self.session.as_mut() {
            s.failed = true;
        }
        self.monitor.typing_fault(message);
    }

    /// Stop capture and run the audio captured so far through recognition,
    /// typing it. `from_monitor` is true when the monitor already knows the
    /// Dictation ended (a fault); otherwise it is told here.
    fn finish(&mut self, from_monitor: bool) {
        let had_session = self.session.is_some();
        self.finish_session(from_monitor);
        if had_session {
            self.dictation_over();
        }
    }

    fn dictation_over(&self) {
        if let Some(r) = &self.residency {
            r.dictation_ended();
        }
    }

    fn finish_session(&mut self, from_monitor: bool) {
        let Some(mut s) = self.session.take() else { return };
        // Whatever the stream still holds (it keeps its samples after a fault).
        let rest = self.capture.read();
        self.capture.stop();
        if !s.failed {
            s.pending.extend_from_slice(&rest);
            if !s.pending.is_empty() {
                s.insertion.feed(&std::mem::take(&mut s.pending));
            }
            s.insertion.end();
            let fault = self.typing_faults.take().or_else(|| s.insertion.fault().map(|f| f.message().to_string()));
            if let Some(m) = fault {
                if !from_monitor {
                    self.monitor.typing_fault(m);
                    self.monitor.take_actions();
                    return;
                }
            }
        }
        if !from_monitor {
            self.monitor.end();
        }
    }
}
