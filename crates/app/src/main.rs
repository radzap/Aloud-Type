//! Aloud Type: the resident host. One executable, three modes:
//! the tray app (default), `--settings` (the Settings window) and
//! `--probe-gpu` (a child that benchmarks the GPU so a driver crash cannot
//! hurt the app).
//!
//! Threads: the main thread runs the tray's message loop; the worker thread
//! runs the controller and Whisper decoding; the keyboard hook, overlay, power
//! watcher and Settings pipe each have their own thread.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(all(
    windows,
    target_env = "msvc",
    target_arch = "x86_64",
    feature = "vulkan"
))]
mod vulkan_guard;

#[cfg(not(windows))]
fn main() {
    eprintln!("Aloud Type runs on Windows only.");
}

#[cfg(windows)]
fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args
        .iter()
        .any(|a| a == aloud_type_recognizer::gpu::PROBE_FLAG)
    {
        std::process::exit(aloud_type_recognizer::gpu::probe_child_main(&args));
    }
    if args.iter().any(|a| a == "--settings") {
        std::process::exit(run_settings_window());
    }
    host::run();
}

/// `--settings`: runs the Settings window (its own executable beside this one).
#[cfg(windows)]
fn run_settings_window() -> i32 {
    let exe = std::env::current_exe().map(|e| aloud_type_app::wiring::settings_exe_beside(&e));
    match exe.and_then(|e| std::process::Command::new(e).status()) {
        Ok(s) => s.code().unwrap_or(1),
        Err(e) => {
            eprintln!("Could not start the Settings window: {e}");
            1
        }
    }
}

#[cfg(windows)]
mod host {
    use aloud_type_app::residency::{Loader, Model, Residency};
    use aloud_type_app::wake::Wake;
    use aloud_type_app::wiring::*;
    use aloud_type_app::*;
    use aloud_type_config::{
        verbose_requested, AppPaths, ConfigFile, Event, HorizontalPosition, IndicatorStyle, Logger,
        ModelMemory, Settings, SettingsNotice, State, StdFs, VerticalPosition,
    };
    use aloud_type_core::ipc::{Command as PipeCommand, PIPE_NAME};
    use aloud_type_core::lifecycle::LifecycleConfig;
    use aloud_type_core::listening::{FaultReason, Indicator, Tray};
    use aloud_type_core::power::{PowerAction, PowerTracker};
    use aloud_type_core::supervisor::{run_supervised, Supervisor, Worker};
    use aloud_type_core::{Clock, KeyEvent, Keyboard, Millis, VoiceGate};
    use aloud_type_platform_windows::audio::cpal_backend::CpalBackend;
    use aloud_type_platform_windows::audio::device::MicChoice;
    use aloud_type_platform_windows::audio::privacy::WinRegistry;
    use aloud_type_platform_windows::audio::session::AudioCapture;
    use aloud_type_platform_windows::file_watch::watch_file;
    use aloud_type_platform_windows::hook::{HookConfig, KeyboardHook};
    use aloud_type_platform_windows::inject::{check_can_start, FaultSlot, InjectingSink};
    use aloud_type_platform_windows::inject_win::{ForegroundElevation, SendInputBackend};
    use aloud_type_platform_windows::instance::{self, Launch};
    use aloud_type_platform_windows::overlay::{self, OverlayHandle};
    use aloud_type_platform_windows::overlay_model::{Placement, Style};
    use aloud_type_platform_windows::pipe_server::PipeServer;
    use aloud_type_platform_windows::power_win::PowerWatcher;
    use aloud_type_platform_windows::settings_launcher::{ExeSpawner, SettingsLauncher};
    use aloud_type_platform_windows::thread_priority;
    use aloud_type_platform_windows::tray::{self, TrayHandle};
    use aloud_type_platform_windows::tray_model::{self, startup_action, Command as TrayCommand};
    use aloud_type_recognizer::SileroVoiceGate;
    use std::cell::{Cell, RefCell};
    use std::path::PathBuf;
    use std::rc::Rc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc::{channel, Receiver, Sender};
    use std::sync::{Arc, Mutex};
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
            check_can_start(&mut ForegroundElevation)
                .err()
                .map(|f| f.message().to_string())
        }
    }

    struct SlotFaults(FaultSlot);
    impl TypingFaults for SlotFaults {
        fn take(&mut self) -> Option<String> {
            self.0
                .lock()
                .unwrap()
                .take()
                .map(|f| f.message().to_string())
        }
    }

    /// First-run: until a Speech model is installed a key press opens Settings.
    struct Setup {
        complete: Box<dyn Fn() -> bool>,
        commands: Sender<TrayCommand>,
    }
    impl SetupGate for Setup {
        fn complete(&mut self) -> bool {
            (self.complete)()
        }
        fn open_settings(&mut self) {
            let _ = self.commands.send(TrayCommand::OpenSettings);
        }
    }

    /// The voice gate model is found on first use, so a missing file never
    /// stops the app from running; until it is found everything counts as speech.
    struct LazyGate {
        find: Box<dyn Fn() -> Option<PathBuf>>,
        gate: Option<SileroVoiceGate>,
    }
    impl VoiceGate for LazyGate {
        fn is_speech(&mut self, a: &[f32]) -> bool {
            if self.gate.is_none() {
                if let Some(p) = (self.find)() {
                    self.gate = SileroVoiceGate::load(&p).ok();
                }
            }
            match self.gate.as_mut() {
                Some(g) => g.is_speech(a),
                None => true,
            }
        }
    }

    fn setup_complete(paths: &AppPaths) -> bool {
        aloud_type_models::setup_complete(&paths.models_dir())
    }

    /// Starts the Settings program's windowless downloader (one at a time).
    #[derive(Default)]
    struct ExeBackground(Option<std::process::Child>);
    impl BackgroundSpawner for ExeBackground {
        fn spawn_background_models(&mut self) {
            use std::os::windows::process::CommandExt;
            if self
                .0
                .as_mut()
                .is_some_and(|c| matches!(c.try_wait(), Ok(None)))
            {
                return;
            }
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            self.0 = std::env::current_exe().ok().and_then(|exe| {
                std::process::Command::new(settings_exe_beside(&exe))
                    .arg("--background-models")
                    .creation_flags(CREATE_NO_WINDOW)
                    .spawn()
                    .ok()
            });
        }
    }

    /// Overlay and tray as the places controller state is shown.
    struct Shown {
        overlay: Option<OverlayHandle>,
        tray: TrayHandle,
    }
    impl Surfaces for Shown {
        fn indicator(&mut self, i: &Indicator) {
            if let Some(o) = &self.overlay {
                o.set_indicator(i.clone());
            }
        }
        fn tray(&mut self, t: &Tray, key_unavailable: bool, blocked: bool) {
            self.tray.set_state(t, key_unavailable, blocked);
        }
    }

    /// Everything Settings can change live.
    struct Live {
        hook: Rc<KeyboardHook>,
        overlay: Option<OverlayHandle>,
        residency: Residency,
        tray: TrayHandle,
        state_path: PathBuf,
        lifecycle: Rc<Cell<Option<LifecycleConfig>>>,
    }
    impl Targets for Live {
        fn set_keys(&mut self, vks: &[u32]) {
            self.hook.set_keys(vks);
        }
        fn set_suspended(&mut self, s: bool) {
            self.hook.set_suspended(s);
        }
        fn set_lifecycle(&mut self, cfg: LifecycleConfig) {
            self.lifecycle.set(Some(cfg));
        }
        fn set_style(&mut self, s: IndicatorStyle) {
            if let (Some(o), Some(st)) = (&self.overlay, Style::from_setting(s.as_str())) {
                o.set_style(st);
            }
        }
        fn set_placement(&mut self, v: VerticalPosition, h: HorizontalPosition) {
            if let (Some(o), Some(p)) = (
                &self.overlay,
                Placement::from_settings(v.as_str(), h.as_str()),
            ) {
                o.set_placement(p);
            }
        }
        fn set_memory(&mut self, m: ModelMemory) {
            self.residency.set_memory(m);
        }
        fn request_model(&mut self, id: &str) {
            self.residency.request_model(id);
        }
        fn retest_gpu(&mut self) {
            aloud_type_recognizer::gpu::retest_on_disk(&self.state_path);
        }
        fn notify(&mut self, text: &str) {
            self.tray.notify(text);
        }
    }

    fn describe(ind: &Indicator, tray: &Tray, key_unavailable: bool) -> String {
        let ind = match ind {
            Indicator::Hidden => "idle".to_string(),
            Indicator::Listening { note: Some(n), .. } => format!("listening ({n})"),
            Indicator::Listening { .. } => "listening".to_string(),
            Indicator::ListeningFault { message, .. } => format!("RED fault: {message}"),
            Indicator::TypingFault { message } => format!("AMBER fault: {message}"),
        };
        let tray = match tray {
            Tray::Fault(r) => format!(" [tray fault: {r}]"),
            _ => String::new(),
        };
        let key = if key_unavailable {
            " [Dictation key unavailable]"
        } else {
            ""
        };
        format!("{ind}{tray}{key}")
    }

    pub fn run() {
        let args: Vec<String> = std::env::args().collect();
        let Some(paths) = AppPaths::from_env() else {
            eprintln!("LOCALAPPDATA is not set.");
            std::process::exit(1);
        };
        // One resident process: a second launch asks the first to open Settings.
        let guard = match instance::acquire("AloudType") {
            Ok(Launch::First(g)) => g,
            Ok(Launch::Second) => std::process::exit(0),
            Err(e) => {
                eprintln!("Could not check for a running Aloud Type: {e}");
                std::process::exit(1);
            }
        };
        let (cmd_tx, cmd_rx) = channel::<TrayCommand>();
        {
            let tx = cmd_tx.clone();
            guard.on_open_settings(move || {
                let _ = tx.send(TrayCommand::OpenSettings);
            });
        }

        let settings0 = ConfigFile::<Settings>::open(paths.settings_file());
        let startup = startup_action(setup_complete(&paths), &settings0.value().dictation_key);
        let tray_handle = TrayHandle::new();

        // Tray menu / second launch / key-before-setup: open Settings or exit.
        {
            let tray = tray_handle.clone();
            std::thread::spawn(move || {
                let mut launcher = SettingsLauncher::new(ExeSpawner);
                for c in cmd_rx {
                    match c {
                        TrayCommand::OpenSettings => {
                            let _ = launcher.open();
                        }
                        TrayCommand::Exit => {
                            tray.quit();
                            return;
                        }
                        TrayCommand::OpenMicrophonePrivacySettings => {}
                    }
                }
            });
        }

        let verbose = verbose_requested(args.iter());
        {
            let (paths, tray, tx) = (paths.clone(), tray_handle.clone(), cmd_tx.clone());
            std::thread::spawn(move || worker(paths, verbose, tray, tx));
        }

        let initial = tray_model::view(&Tray::Idle, false);
        if let Err(e) = tray::run(tray_handle, cmd_tx, startup, initial) {
            eprintln!("Tray failed: {e}");
        }

        std::process::exit(0);
    }

    /// The controller loop: Dictation, recognition and typing. Runs on its own
    /// thread so Whisper decoding never blocks the tray's message loop.
    fn worker(paths: AppPaths, verbose: bool, tray: TrayHandle, commands: Sender<TrayCommand>) {
        // Decoding runs on this thread: keep it below the hook and audio threads.
        thread_priority::set_current_below_normal();
        let wake = Wake::new();
        let clock = SystemClock(Instant::now());
        let logger = Logger::new(StdFs, &paths.app_dir, verbose);
        logger.log(clock.now(), Event::new("start"));

        let settings_file = ConfigFile::<Settings>::open(paths.settings_file());
        let first = settings_file.value().clone();

        // Only checksum-verified models ever load.
        let models_dir = paths.models_dir();
        let loader: Loader = {
            let dir = models_dir.clone();
            let (settings_path, state_path) = (paths.settings_file(), paths.state_file());
            let log_dir = paths.app_dir.clone();
            let clock = SystemClock(clock.0);
            Arc::new(move |id: &str| {
                let info =
                    aloud_type_models::find(id).ok_or_else(|| format!("unknown model {id}"))?;
                let path =
                    aloud_type_models::verified_path(&dir, info).map_err(|e| format!("{e:?}"))?;
                use aloud_type_recognizer::gpu;
                // The probe benchmarks the base q5_1 model (spec); fall back to the model being loaded.
                let probe_model = aloud_type_models::find("base-q5_1")
                    .and_then(|i| aloud_type_models::verified_path(&dir, i).ok())
                    .unwrap_or_else(|| path.clone());
                let runner = gpu::probe_runner(&probe_model).map_err(|e| e.to_string())?;
                let gpu_setting = ConfigFile::<Settings>::open(settings_path.clone())
                    .value()
                    .gpu
                    .clone();
                let m = gpu::open_for_host(
                    Box::new(gpu::WhisperBackend::new(&path)),
                    &gpu_setting,
                    &state_path,
                    &gpu::installed_adapters(),
                    &runner,
                    gpu::PROBE_TIMEOUT,
                );
                Logger::new(StdFs, &log_dir, verbose).log(
                    clock.now(),
                    Event::new(if m.using_gpu() {
                        "speech_model_on_gpu"
                    } else {
                        "speech_model_on_cpu"
                    }),
                );
                Ok(Box::new(m) as Model)
            })
        };
        let started = Instant::now();
        let residency = Residency::start(
            loader,
            Arc::new(move || started.elapsed().as_millis() as Millis),
            &first.speech_model,
            first.model_memory,
        );
        residency.set_waker(wake.notifier());

        let placement = Placement::from_settings(
            first.indicator_vertical.as_str(),
            first.indicator_horizontal.as_str(),
        )
        .unwrap_or_else(|| Placement::from_settings("bottom", "center").unwrap());
        let overlay = overlay::spawn(placement).ok();
        let mut shown = Shown {
            overlay: overlay.clone(),
            tray: tray.clone(),
        };

        let hook = match KeyboardHook::start_notifying(HookConfig::default(), wake.notifier()) {
            Ok(h) => Rc::new(h),
            Err(e) => {
                // Stay resident: the tray says the key is unavailable.
                logger.log(clock.now(), Event::new("hook_install_failed"));
                eprintln!("Dictation key unavailable: {e}");
                tray.set_state(&Tray::Idle, true, false);
                tray.notify(tray_model::KEY_UNAVAILABLE);
                return;
            }
        };
        let (sink, fault_slot) = InjectingSink::new(SendInputBackend);
        let power = PowerWatcher::start_notifying(wake.notifier()).ok();

        // Settings window -> host (suspend/resume keys, reload, GPU re-test).
        let (pipe_tx, pipe_rx): (Sender<PipeCommand>, Receiver<PipeCommand>) = channel();
        let pipe_wake = wake.notifier();
        let pipe = PipeServer::start(PIPE_NAME, move |c| {
            let _ = pipe_tx.send(c);
            pipe_wake();
        });
        if pipe.is_err() {
            logger.log(clock.now(), Event::new("pipe_unavailable"));
        }

        // The settings file is watched by the kernel; a change wakes the worker.
        let settings_dirty = Arc::new(AtomicBool::new(false));
        {
            let (dirty, notify) = (settings_dirty.clone(), wake.notifier());
            let file = paths.settings_file();
            let changed: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
                dirty.store(true, Ordering::SeqCst);
                notify();
            });
            if let (Some(dir), Some(name)) =
                (file.parent(), file.file_name().and_then(|n| n.to_str()))
            {
                if watch_file(dir, name, changed).is_err() {
                    logger.log(clock.now(), Event::new("settings_watch_unavailable"));
                }
            }
        }

        let lifecycle = Rc::new(Cell::new(None));
        let live = Live {
            hook: hook.clone(),
            overlay: overlay.clone(),
            residency: residency.clone(),
            tray: tray.clone(),
            state_path: paths.state_file(),
            lifecycle: lifecycle.clone(),
        };
        let source = {
            let mut file = settings_file;
            let mut notice = SettingsNotice::new();
            Box::new(move || {
                let status = file.reload().clone();
                Loaded {
                    settings: file.value().clone(),
                    notice: notice.observe(&status).map(str::to_string),
                }
            })
        };
        let mut settings_host = SettingsHost::new(live, source);
        settings_host.reload(); // key, style, placement, memory, model from the file
        let dictation = settings_host.dictation_settings();

        let find_vad = {
            let dir = models_dir.clone();
            Box::new(move || {
                aloud_type_models::verified_path(&dir, &aloud_type_models::VAD_MODEL).ok()
            })
        };
        let mut c = Controller::new(Parts {
            keyboard: SharedHook(hook.clone()),
            clock,
            lifecycle: lifecycle_for(&first),
            capture: Box::new(MicCapture(AudioCapture::new(
                CpalBackend,
                Box::new(WinRegistry),
            ))),
            recognizer: Box::new(residency.recognizer()),
            voice_gate: Box::new(LazyGate {
                find: find_vad,
                gate: None,
            }),
            sink: Box::new(sink),
            start_gate: Box::new(ElevatedGate),
            typing_faults: Box::new(SlotFaults(fault_slot)),
            settings: Box::new(move || dictation.borrow().clone()),
            config: ControllerConfig::default(),
        });
        c.set_residency(residency.clone());
        let p = paths.clone();
        c.set_setup_gate(Box::new(Setup {
            complete: Box::new(move || setup_complete(&p)),
            commands,
        }));

        let controller = RefCell::new(c);
        let settings_host = RefCell::new(settings_host);
        let publisher = RefCell::new(Publisher::default());
        let shown = RefCell::new(&mut shown);
        let supervisor = Mutex::new(Supervisor::new());
        let tracker = RefCell::new(PowerTracker::default());
        let last = RefCell::new(String::new());

        let publish = |c: &Controller<SharedHook, SystemClock>| {
            publisher.borrow_mut().publish(
                &mut **shown.borrow_mut(),
                &c.indicator(),
                &c.tray(),
                hook.key_unavailable(),
                c.blocked_fault_standing(),
            );
            let now = describe(&c.indicator(), &c.tray(), hook.key_unavailable());
            let mut l = last.borrow_mut();
            if *l != now {
                if verbose {
                    println!("{now}");
                }
                logger.log(clock.now(), Event::new("state_changed"));
                *l = now;
            }
        };
        let background = RefCell::new((BackgroundModels::default(), ExeBackground::default()));
        let last_loaded = RefCell::new(None::<String>);
        let body = || loop {
            // Anything that can change what is due: a command, a changed file, a finished load.
            let mut recheck_downloads = false;
            while let Ok(cmd) = pipe_rx.try_recv() {
                settings_host.borrow_mut().on_command(cmd);
                logger.log(clock.now(), Event::new("settings_command"));
                recheck_downloads = true;
            }
            if settings_dirty.swap(false, Ordering::SeqCst) {
                settings_host.borrow_mut().reload();
                logger.log(clock.now(), Event::new("settings_file_changed"));
                recheck_downloads = true;
            }
            let loaded = residency.loaded_model();
            if loaded != *last_loaded.borrow() {
                recheck_downloads |= loaded.is_some();
                *last_loaded.borrow_mut() = loaded;
            }
            if recheck_downloads {
                let mut bg = background.borrow_mut();
                bg.0.rearm();
                if let Some(settings) = settings_host.borrow().settings() {
                    let state = ConfigFile::<State>::open(paths.state_file());
                    let (checker, spawner) = &mut *bg;
                    checker.check(settings, state.value(), &models_dir, spawner);
                }
            }
            let mut c = controller.borrow_mut();
            if let Some(l) = lifecycle.take() {
                c.set_lifecycle(l);
            }
            c.step();
            for n in residency.take_notices() {
                tray.notify(&n);
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
            publish(&c);
            let next = c.poll_interval();
            drop(c);
            // Idle: block until a key, command, power event or load wakes us.
            wake.wait(next);
        };
        let report = |r| {
            logger.log(clock.now(), Event::new("worker_report"));
            handle_report(&mut controller.borrow_mut(), r);
        };
        run_supervised(&supervisor, Worker::Recognizer, body, report);

        // The worker is gone for good, but the app stays resident: the tray
        // keeps the fault, the key passes through to other apps, and Settings
        // can still be opened from the tray.
        publish(&controller.borrow());
        hook.set_suspended(true);
        let shown_at = Instant::now();
        let mut hidden = false;
        loop {
            while let Ok(cmd) = pipe_rx.try_recv() {
                if cmd != PipeCommand::SuspendKeys && cmd != PipeCommand::ResumeKeys {
                    settings_host.borrow_mut().on_command(cmd);
                }
            }
            let left = Duration::from_secs(6).saturating_sub(shown_at.elapsed());
            if left.is_zero() && !hidden {
                hidden = true;
                if let Some(o) = &overlay {
                    o.set_indicator(Indicator::Hidden);
                }
            }
            // Nothing left to time once the fault is hidden: sleep until a command arrives.
            wake.wait(if hidden { None } else { Some(left) });
        }
    }
}
