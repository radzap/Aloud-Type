//! Aloud Type, walking skeleton: hold Right Ctrl, speak, and the words are typed
//! into the focused window as they are confirmed. A console reports state; no
//! tray, overlay or settings window yet (later tickets).
//!
//! The Speech model is installed by hand: `ggml-base.bin` and
//! `ggml-silero-v5.1.2.bin` in `%LOCALAPPDATA%\AloudType\models`, or the folder
//! named by `ALOUDTYPE_MODEL_DIR`, or `%LOCALAPPDATA%\AloudTypeDev\models`.
#[cfg(not(windows))]
fn main() {
    eprintln!("Aloud Type runs on Windows only.");
}

#[cfg(windows)]
fn main() {
    host::run();
}

#[cfg(windows)]
mod host {
    use aloud_type_app::*;
    use aloud_type_config::{verbose_requested, AppPaths, ConfigFile, Event, Language, Logger, Settings, SettingsNotice, StdFs};
    use aloud_type_core::lifecycle::LifecycleConfig;
    use aloud_type_core::listening::{FaultReason, Indicator, Tray};
    use aloud_type_core::power::{PowerAction, PowerTracker};
    use aloud_type_core::supervisor::{run_supervised, Report, Supervisor, Worker};
    use aloud_type_core::{Clock, KeyEvent, Keyboard, Millis};
    use aloud_type_platform_windows::audio::cpal_backend::CpalBackend;
    use aloud_type_platform_windows::audio::device::MicChoice;
    use aloud_type_platform_windows::audio::privacy::WinRegistry;
    use aloud_type_platform_windows::audio::session::AudioCapture;
    use aloud_type_platform_windows::hook::{HookConfig, KeyboardHook};
    use aloud_type_platform_windows::inject::{check_can_start, FaultSlot, InjectingSink};
    use aloud_type_platform_windows::inject_win::{ForegroundElevation, SendInputBackend};
    use aloud_type_platform_windows::power_win::PowerWatcher;
    use aloud_type_app::residency::{Loader, Model, Residency};
    use aloud_type_recognizer::{pins, SileroVoiceGate, WhisperRecognizer};
    use std::sync::Arc;
    use std::cell::RefCell;
    use std::path::PathBuf;
    use std::rc::Rc;
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    #[derive(Clone, Copy)]
    struct SystemClock(Instant);
    impl Clock for SystemClock {
        fn now(&self) -> Millis {
            self.0.elapsed().as_millis() as Millis
        }
    }

    /// The hook is shared so the host can also ask it for health and re-install.
    struct SharedHook(Rc<KeyboardHook>);
    impl Keyboard for SharedHook {
        fn next_event(&mut self) -> Option<KeyEvent> {
            self.0.try_next()
        }
    }

    struct MicCapture(AudioCapture<CpalBackend>);
    impl Capture for MicCapture {
        fn start(&mut self, mic: &Mic) -> Result<bool, FaultReason> {
            let choice = match mic {
                Mic::SystemDefault => MicChoice::SystemDefault,
                Mic::Specific(id) => MicChoice::Specific(id.clone()),
            };
            self.0.start(&choice)
        }
        fn stop(&mut self) {
            self.0.stop()
        }
        fn take_fault(&mut self) -> Option<FaultReason> {
            self.0.take_fault()
        }
        fn read(&mut self) -> Vec<f32> {
            aloud_type_core::AudioSource::read(&mut self.0)
        }
    }

    struct ElevatedGate;
    impl StartGate for ElevatedGate {
        fn check(&mut self) -> Option<String> {
            check_can_start(&mut ForegroundElevation).err().map(|f| f.message().to_string())
        }
    }

    struct SlotFaults(FaultSlot);
    impl TypingFaults for SlotFaults {
        fn take(&mut self) -> Option<String> {
            self.0.lock().unwrap().take().map(|f| f.message().to_string())
        }
    }

    fn find_model(paths: &AppPaths, name: &str) -> Option<PathBuf> {
        let mut dirs = Vec::new();
        if let Some(d) = std::env::var_os("ALOUDTYPE_MODEL_DIR") {
            dirs.push(PathBuf::from(d));
        }
        dirs.push(paths.models_dir());
        if let Some(l) = std::env::var_os("LOCALAPPDATA") {
            dirs.push(PathBuf::from(l).join("AloudTypeDev").join("models"));
        }
        dirs.into_iter().map(|d| d.join(name)).find(|p| p.exists())
    }

    fn describe(c: &Controller<SharedHook, SystemClock>, key_unavailable: bool) -> String {
        let ind = match c.indicator() {
            Indicator::Hidden => "idle".to_string(),
            Indicator::Listening { note: Some(n), .. } => format!("listening ({n})"),
            Indicator::Listening { .. } => "listening".to_string(),
            Indicator::ListeningFault { message, .. } => format!("RED fault: {message}"),
            Indicator::TypingFault { message } => format!("AMBER fault: {message}"),
        };
        let tray = match c.tray() {
            Tray::Fault(r) => format!(" [tray fault: {r}]"),
            _ => String::new(),
        };
        let key = if key_unavailable { " [Dictation key unavailable]" } else { "" };
        format!("{ind}{tray}{key}")
    }

    pub fn run() {
        let verbose = verbose_requested(std::env::args());
        let Some(paths) = AppPaths::from_env() else {
            eprintln!("LOCALAPPDATA is not set.");
            std::process::exit(1);
        };
        let clock = SystemClock(Instant::now());
        let logger = Logger::new(StdFs, &paths.app_dir, verbose);
        logger.log(clock.now(), Event::new("start"));

        let models_dir = std::env::var_os("ALOUDTYPE_MODEL_DIR").map(PathBuf::from).unwrap_or_else(|| paths.models_dir());
        let Some(vad) = find_model(&paths, pins::VAD_MODEL_FILE) else {
            eprintln!("Voice gate model not found. Put {} in {}.", pins::VAD_MODEL_FILE, models_dir.display());
            std::process::exit(1);
        };
        let gate = SileroVoiceGate::load(&vad).unwrap_or_else(|e| {
            eprintln!("Could not load the voice gate: {e}");
            std::process::exit(1)
        });

        let mut settings_file = ConfigFile::<Settings>::open(paths.settings_file());
        // The Speech model loads on a worker thread; only verified files are loaded.
        let loader: Loader = {
            let (dir, legacy) = (models_dir.clone(), find_model(&paths, pins::BASE_MODEL_FILE));
            Arc::new(move |id: &str| {
                let info = aloud_type_models::find(id).ok_or_else(|| format!("unknown model {id}"))?;
                let path = match aloud_type_models::verified_path(&dir, info) {
                    Ok(p) => p,
                    // Hand-installed development model (no checksum known for it).
                    Err(_) if legacy.is_some() && std::env::var_os("ALOUDTYPE_MODEL_DIR").is_some() => legacy.clone().unwrap(),
                    Err(e) => return Err(format!("{e:?}")),
                };
                let m = WhisperRecognizer::load(&path).map_err(|e| e.to_string())?;
                Ok(Box::new(m) as Model)
            })
        };
        let started = Instant::now();
        let residency = Residency::start(
            loader,
            Arc::new(move || started.elapsed().as_millis() as Millis),
            &settings_file.value().speech_model,
            settings_file.value().model_memory,
        );
        let tap_ms = settings_file.value().tap_threshold_ms as Millis;

        let hook = Rc::new(KeyboardHook::start(HookConfig::default()).unwrap_or_else(|e| {
            eprintln!("Dictation key unavailable: {e}");
            std::process::exit(1)
        }));
        let (sink, fault_slot) = InjectingSink::new(SendInputBackend);
        let power = PowerWatcher::start().ok();

        let mut notice = SettingsNotice::new();
        let res_for_settings = residency.clone();
        let settings = Box::new(move || {
            // Settings are re-read at every Dictation start (no file watcher yet).
            let status = settings_file.reload().clone();
            if let Some(text) = notice.observe(&status) {
                println!("notice: {text}");
            }
            let s = settings_file.value();
            res_for_settings.set_memory(s.model_memory);
            res_for_settings.request_model(&s.speech_model);
            DictationSettings {
                language: match &s.language {
                    Language::Auto => "auto".into(),
                    Language::Fixed(c) => c.clone(),
                },
                microphone: s.microphone.clone().map_or(Mic::SystemDefault, Mic::Specific),
            }
        });

        let controller = RefCell::new(Controller::new(Parts {
            keyboard: SharedHook(hook.clone()),
            clock,
            lifecycle: LifecycleConfig { tap_threshold_ms: tap_ms, modifier_key: true },
            capture: Box::new(MicCapture(AudioCapture::new(CpalBackend, Box::new(WinRegistry)))),
            recognizer: Box::new(residency.recognizer()),
            voice_gate: Box::new(gate),
            sink: Box::new(sink),
            start_gate: Box::new(ElevatedGate),
            typing_faults: Box::new(SlotFaults(fault_slot)),
            settings,
            config: ControllerConfig::default(),
        }));

        controller.borrow_mut().set_residency(residency.clone());
        println!("Aloud Type is running. Hold Right Ctrl to dictate (tap to go hands-free). Ctrl+C quits.");
        let supervisor = Mutex::new(Supervisor::new());
        let tracker = RefCell::new(PowerTracker::default());
        let last = RefCell::new(String::new());
        let body = || loop {
            {
                let mut c = controller.borrow_mut();
                c.step();
                for n in residency.take_notices() {
                    println!("notice: {n}");
                }
                if let Some(p) = &power {
                    while let Ok(e) = p.events.try_recv() {
                        if let Some(e) = tracker.borrow_mut().observe(e) {
                            for a in c.power(e) {
                                if a == PowerAction::ReinstallHook {
                                    hook.reinstall_now();
                                }
                            }
                        }
                    }
                }
                let now = describe(&c, hook.key_unavailable());
                let mut l = last.borrow_mut();
                if *l != now {
                    println!("{now}");
                    logger.log(clock.now(), Event::new("state_changed"));
                    *l = now;
                }
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        let report = |r: Report| match r {
            Report::Restarted(_) => {
                logger.log(clock.now(), Event::new("worker_restarted"));
                controller.borrow_mut().recover();
                println!("worker restarted after an internal error");
            }
            Report::Faulted(_, f) => {
                controller.borrow_mut().worker_fault(f);
                println!("fault: {}", f.message());
                logger.log(clock.now(), Event::new("worker_faulted"));
            }
        };
        run_supervised(&supervisor, Worker::Recognizer, body, report);
        std::process::exit(1);
    }
}
