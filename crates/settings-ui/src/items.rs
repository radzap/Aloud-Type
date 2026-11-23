//! The remaining Settings items: microphone, language, Listening indicator
//! style and position, Speech model memory and launch at login, plus the
//! informational version and links. Every change is saved and the resident
//! process is told to reload, so it applies without a restart.
use crate::ipc::{Command, ResidentLink, Transport};
use aloud_type_config::{
    ConfigFile, Gpu, HorizontalPosition, IndicatorStyle, Language, ModelMemory, Settings, State,
    VerticalPosition,
};
use std::io;
use std::time::{Duration, Instant};

/// How long Settings watches the state file after a GPU change (the model reloads meanwhile).
const GPU_WATCH: Duration = Duration::from_secs(60);
/// Give up waiting for a GPU test after this long (each device may take up to the probe timeout).
const GPU_TEST_LIMIT: Duration = Duration::from_secs(300);

/// An adapter name as shown in Settings: without the "(R)" and "(TM)" marks.
pub fn gpu_label(name: &str) -> String {
    let mut s = name.to_string();
    for m in ["(R)", "(r)", "(TM)", "(tm)", "(C)"] {
        s = s.replace(m, "");
    }
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn probe_device(key: &str) -> &str {
    key.split('|').next().unwrap_or(key)
}

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const RELEASES_URL: &str = "https://github.com/radzap/Aloud-Type/releases";
pub const SOURCE_URL: &str = "https://github.com/radzap/Aloud-Type";

pub use aloud_type_models::languages::LANGUAGES;

/// English-only Whisper models are named like `base.en-q5_1`.
pub fn is_english_only(model: &str) -> bool {
    model
        .split('-')
        .next()
        .is_some_and(|stem| stem.ends_with(".en"))
}

/// Per-user "launch at login" (the Windows Run key in the real build).
pub trait LoginItem {
    fn is_enabled(&self) -> bool;
    fn set_enabled(&mut self, enabled: bool) -> io::Result<()>;
}

/// Capture devices as (id, name).
pub trait MicLister {
    fn list(&self) -> Vec<(String, String)>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MicOption {
    /// `None` is "System default".
    pub id: Option<String>,
    pub label: String,
}

pub struct ItemsSession<T: Transport, L: LoginItem, M: MicLister> {
    config: ConfigFile<Settings>,
    link: ResidentLink<T>,
    login: L,
    mics: M,
    /// Machine state (GPU probe results), read-only here; the resident process writes it.
    state: Option<ConfigFile<State>>,
    error: Option<String>,
    /// A GPU test is running in the resident process until this time at the latest.
    gpu_test_until: Option<Instant>,
    /// Keep re-reading the state file until then (a GPU change reloads the model).
    gpu_watch_until: Option<Instant>,
    /// The resident process could not be reached for a GPU test.
    gpu_test_unreachable: bool,
}

impl<T: Transport, L: LoginItem, M: MicLister> ItemsSession<T, L, M> {
    pub fn new(config: ConfigFile<Settings>, transport: T, login: L, mics: M) -> Self {
        Self {
            config,
            link: ResidentLink::new(transport),
            login,
            mics,
            state: None,
            error: None,
            gpu_test_until: None,
            gpu_watch_until: None,
            gpu_test_unreachable: false,
        }
    }

    /// Give the session the state file so the GPU choices and error notice can be shown.
    pub fn with_state(mut self, state: ConfigFile<State>) -> Self {
        self.state = Some(state);
        self
    }

    /// Auto, CPU only, then every installed GPU (the resident process saves the list), any
    /// device a test has seen, and the saved choice even if it is gone.
    pub fn gpu_options(&self) -> Vec<(Gpu, String)> {
        let mut out = vec![
            (Gpu::Auto, "Auto".to_string()),
            (Gpu::CpuOnly, "CPU only".to_string()),
        ];
        let mut names: Vec<String> = Vec::new();
        if let Some(st) = &self.state {
            let st = st.value();
            names.extend(st.gpu_adapters.iter().cloned());
            names.extend(st.gpu_probes.keys().map(|k| probe_device(k).to_string()));
        }
        if let Gpu::Device(d) = &self.config.value().gpu {
            names.push(d.clone());
        }
        let mut seen = std::collections::HashSet::new();
        names.retain(|n| seen.insert(n.clone()));
        out.extend(names.into_iter().map(|n| {
            let label = gpu_label(&n);
            (Gpu::Device(n), label)
        }));
        out
    }

    pub fn set_gpu(&mut self, gpu: Gpu) {
        self.save(|s| s.gpu = gpu);
        self.gpu_watch_until = Some(Instant::now() + GPU_WATCH);
    }

    /// Shown in Settings when a backend error switched the GPU off.
    pub fn gpu_notice(&self) -> Option<&'static str> {
        let st = self.state.as_ref()?;
        st.value()
            .gpu_disabled_after_error
            .then_some("GPU disabled after an error. Pick a GPU or press Test to try again.")
    }

    /// Any GPU to test.
    pub fn has_gpus(&self) -> bool {
        self.state
            .as_ref()
            .is_some_and(|s| !s.value().gpu_adapters.is_empty())
    }

    pub fn gpu_testing(&self) -> bool {
        self.gpu_test_until.is_some()
    }

    /// Lines under the GPU choice: what the model runs on and the last test's results.
    pub fn gpu_status(&self) -> Vec<String> {
        if self.gpu_testing() {
            return vec![
                "Testing each GPU against the CPU. This takes a few seconds per GPU.".into(),
            ];
        }
        let mut out = Vec::new();
        if self.gpu_test_unreachable {
            out.push("Aloud Type is not running, so the GPUs could not be tested.".into());
        }
        let Some(st) = &self.state else { return out };
        let st = st.value();
        out.push(match &st.gpu_in_use {
            Some(d) => format!("Running on {}", gpu_label(d)),
            None => "Running on the CPU".into(),
        });
        for (key, p) in &st.gpu_probes {
            let name = gpu_label(probe_device(key));
            out.push(if p.approved {
                format!("Test: {name} is {:.1}x faster than the CPU", p.speedup)
            } else if p.speedup > 0.0 {
                format!("Test: {name} not used ({:.1}x the CPU speed)", p.speedup)
            } else {
                format!("Test: {name} failed")
            });
        }
        out
    }

    /// Settings "Test": clear old results, ask the resident process to measure every GPU,
    /// and watch the state file for the results ([`Self::poll_gpu`]).
    pub fn retest_gpu(&mut self) {
        if let Some(st) = self.state.as_mut() {
            let _ = st.update(|x| {
                x.gpu_probes.clear();
                x.gpu_disabled_after_error = false;
            });
        }
        self.gpu_test_unreachable = !self.link.send(Command::RetestGpu);
        if !self.gpu_test_unreachable {
            self.gpu_test_until = Some(Instant::now() + GPU_TEST_LIMIT);
        }
    }

    /// Keep re-reading the state file for a while: at the first launch the resident process
    /// saves the GPUs just after Settings opens, and a finished download loads the model.
    /// True when the window should start polling (it was not already).
    pub fn watch_gpu(&mut self) -> bool {
        let polling = self.gpu_test_until.is_some() || self.gpu_watch_until.is_some();
        let until = Instant::now() + GPU_WATCH;
        self.gpu_watch_until = Some(self.gpu_watch_until.map_or(until, |u| u.max(until)));
        !polling
    }

    /// Re-read the state file. True while a test runs or a GPU change is still settling, so
    /// the window keeps polling.
    pub fn poll_gpu(&mut self) -> bool {
        self.poll_gpu_at(Instant::now())
    }

    fn poll_gpu_at(&mut self, now: Instant) -> bool {
        if let Some(st) = self.state.as_mut() {
            st.reload();
        }
        if let Some(until) = self.gpu_test_until {
            let done = self.state.as_ref().is_some_and(|st| {
                let st = st.value();
                !st.gpu_adapters.is_empty()
                    && st
                        .gpu_adapters
                        .iter()
                        .all(|a| st.gpu_probes.keys().any(|k| probe_device(k) == a.as_str()))
            });
            if done || now >= until {
                self.gpu_test_until = None;
                // The resident process now reloads the model with the results.
                self.gpu_watch_until = Some(now + GPU_WATCH);
            }
        }
        if self.gpu_watch_until.is_some_and(|u| now >= u) {
            self.gpu_watch_until = None;
        }
        self.gpu_test_until.is_some() || self.gpu_watch_until.is_some()
    }

    pub fn settings(&self) -> &Settings {
        self.config.value()
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// The registry is the truth for the toggle's display.
    pub fn launch_at_login(&self) -> bool {
        self.login.is_enabled()
    }

    /// System default first, then each device; a saved but absent choice stays visible.
    pub fn mic_options(&self) -> Vec<MicOption> {
        let mut out = vec![MicOption {
            id: None,
            label: "System default".into(),
        }];
        let devices = self.mics.list();
        out.extend(devices.iter().map(|(id, name)| MicOption {
            id: Some(id.clone()),
            label: name.clone(),
        }));
        if let Some(saved) = &self.config.value().microphone {
            if !devices.iter().any(|(id, _)| id == saved) {
                out.push(MicOption {
                    id: Some(saved.clone()),
                    label: format!("{saved} (not connected)"),
                });
            }
        }
        out
    }

    /// Re-read the settings file: another part of the window (the Speech
    /// model list) writes it through its own handle.
    pub fn reload(&mut self) {
        self.config.reload();
    }

    /// English-only Speech models cannot take another language.
    pub fn language_enabled(&self) -> bool {
        !is_english_only(&self.config.value().speech_model)
    }

    fn save(&mut self, change: impl FnOnce(&mut Settings)) {
        match self.config.update(change) {
            Ok(()) => {
                self.error = None;
                self.link.send(Command::ReloadSettings);
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    pub fn set_microphone(&mut self, id: Option<String>) {
        self.save(|s| s.microphone = id);
    }

    pub fn set_language(&mut self, language: Language) {
        if self.language_enabled() {
            self.save(|s| s.language = language);
        }
    }

    /// Multilingual models only: dictate in the keyboard layout's language.
    pub fn set_language_from_keyboard(&mut self, on: bool) {
        if self.language_enabled() {
            self.save(|s| s.language_from_keyboard = on);
        }
    }

    pub fn set_indicator_style(&mut self, v: IndicatorStyle) {
        self.save(|s| s.indicator_style = v);
    }

    pub fn set_vertical(&mut self, v: VerticalPosition) {
        self.save(|s| s.indicator_vertical = v);
    }

    pub fn set_horizontal(&mut self, v: HorizontalPosition) {
        self.save(|s| s.indicator_horizontal = v);
    }

    pub fn set_model_memory(&mut self, v: ModelMemory) {
        self.save(|s| s.model_memory = v);
    }

    /// Writes the Run key first; the setting is saved only if that worked.
    pub fn set_launch_at_login(&mut self, on: bool) {
        if let Err(e) = self.login.set_enabled(on) {
            self.error = Some(format!("Could not change launch at login: {e}"));
            return;
        }
        self.save(|s| s.launch_at_login = on);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[derive(Clone, Default)]
    struct Fake {
        log: Rc<RefCell<Vec<String>>>,
    }
    impl Transport for Fake {
        fn request(&mut self, line: &str) -> io::Result<String> {
            self.log.borrow_mut().push(line.into());
            Ok("ok\n".into())
        }
    }
    #[derive(Clone, Default)]
    struct Login {
        on: Rc<RefCell<bool>>,
        fail: bool,
    }
    impl LoginItem for Login {
        fn is_enabled(&self) -> bool {
            *self.on.borrow()
        }
        fn set_enabled(&mut self, e: bool) -> io::Result<()> {
            if self.fail {
                return Err(io::ErrorKind::PermissionDenied.into());
            }
            *self.on.borrow_mut() = e;
            Ok(())
        }
    }
    struct Mics(Vec<(&'static str, &'static str)>);
    impl MicLister for Mics {
        fn list(&self) -> Vec<(String, String)> {
            self.0
                .iter()
                .map(|(a, b)| (a.to_string(), b.to_string()))
                .collect()
        }
    }

    type S = ItemsSession<Fake, Login, Mics>;
    fn session(login: Login) -> (S, Fake, tempfile::TempDir, std::path::PathBuf) {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("settings.toml");
        let fake = Fake::default();
        let mics = Mics(vec![("a", "USB Mic"), ("b", "Headset")]);
        (
            ItemsSession::new(ConfigFile::open(&p), fake.clone(), login, mics),
            fake,
            d,
            p,
        )
    }
    fn reread(p: &std::path::Path) -> Settings {
        ConfigFile::<Settings>::open(p).value().clone()
    }

    #[test]
    fn microphone_list_starts_with_system_default() {
        let (s, ..) = session(Login::default());
        let o = s.mic_options();
        assert_eq!(
            o[0],
            MicOption {
                id: None,
                label: "System default".into()
            }
        );
        assert_eq!(o.len(), 3);
        assert_eq!(o[1].label, "USB Mic");
    }

    #[test]
    fn saved_but_absent_microphone_stays_selectable() {
        let (mut s, ..) = session(Login::default());
        s.set_microphone(Some("gone".into()));
        let o = s.mic_options();
        assert_eq!(o.last().unwrap().id.as_deref(), Some("gone"));
        assert!(o.last().unwrap().label.contains("not connected"));
    }

    #[test]
    fn every_item_is_saved_and_the_resident_process_reloads() {
        let (mut s, fake, _d, p) = session(Login::default());
        s.set_microphone(Some("b".into()));
        s.set_language(Language::Auto);
        s.set_indicator_style(IndicatorStyle::Orb);
        s.set_vertical(VerticalPosition::Top);
        s.set_horizontal(HorizontalPosition::Right);
        s.set_model_memory(ModelMemory::UnloadWhenIdle);
        assert_eq!(fake.log.borrow().len(), 6);
        assert!(fake.log.borrow().iter().all(|l| l == "reload-settings"));
        let r = reread(&p);
        assert_eq!(r.microphone.as_deref(), Some("b"));
        assert_eq!(r.language, Language::Auto);
        assert_eq!(r.indicator_style, IndicatorStyle::Orb);
        assert_eq!(r.indicator_vertical, VerticalPosition::Top);
        assert_eq!(r.indicator_horizontal, HorizontalPosition::Right);
        assert_eq!(r.model_memory, ModelMemory::UnloadWhenIdle);
    }

    #[test]
    fn choosing_system_default_clears_the_saved_microphone() {
        let (mut s, _f, _d, p) = session(Login::default());
        s.set_microphone(Some("b".into()));
        s.set_microphone(None);
        assert_eq!(reread(&p).microphone, None);
    }

    #[test]
    fn works_without_a_resident_process() {
        struct Down;
        impl Transport for Down {
            fn request(&mut self, _: &str) -> io::Result<String> {
                Err(io::ErrorKind::NotFound.into())
            }
        }
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("settings.toml");
        let mut s = ItemsSession::new(ConfigFile::open(&p), Down, Login::default(), Mics(vec![]));
        s.set_indicator_style(IndicatorStyle::Orb);
        assert_eq!(reread(&p).indicator_style, IndicatorStyle::Orb);
        assert!(s.error().is_none());
    }

    #[test]
    fn language_is_disabled_for_english_only_models() {
        assert!(is_english_only("base.en-q5_1"));
        assert!(is_english_only("small.en"));
        assert!(!is_english_only("base-q5_1"));
        assert!(!is_english_only("large-v3-turbo-q5_0"));
        let (mut s, _f, _d, p) = session(Login::default());
        assert!(s.language_enabled());
        s.config
            .update(|x| x.speech_model = "base.en-q5_1".into())
            .unwrap();
        assert!(!s.language_enabled());
        s.set_language(Language::Auto);
        assert_ne!(reread(&p).language, Language::Auto);
    }

    #[test]
    fn a_model_chosen_elsewhere_in_the_window_unlocks_the_language_after_reload() {
        let (mut s, _f, _d, p) = session(Login::default());
        s.config
            .update(|x| x.speech_model = "tiny.en-q5_1".into())
            .unwrap();
        assert!(!s.language_enabled());
        // The model list writes the same file through its own handle.
        ConfigFile::<Settings>::open(&p)
            .update(|x| x.speech_model = "base-q5_1".into())
            .unwrap();
        s.reload();
        assert!(s.language_enabled());
        s.set_language(Language::Fixed("cs".into()));
        assert_eq!(reread(&p).language, Language::Fixed("cs".into()));
    }

    #[test]
    fn keyboard_language_is_saved_for_multilingual_models_only() {
        let (mut s, fake, _d, p) = session(Login::default());
        s.set_language_from_keyboard(true);
        assert!(reread(&p).language_from_keyboard);
        assert_eq!(fake.log.borrow().last().unwrap(), "reload-settings");
        s.set_language_from_keyboard(false);
        s.config
            .update(|x| x.speech_model = "base.en-q5_1".into())
            .unwrap();
        s.set_language_from_keyboard(true);
        assert!(!reread(&p).language_from_keyboard);
    }

    #[test]
    fn gpus_saved_after_settings_opened_show_up_while_watching() {
        let (s, _fake, d, _p) = session(Login::default());
        let sp = d.path().join("state.toml");
        let mut s = s.with_state(ConfigFile::open(&sp));
        assert!(!s.has_gpus());
        assert!(s.watch_gpu(), "starts polling");
        assert!(!s.watch_gpu(), "already polling");
        ConfigFile::<State>::open(&sp)
            .update(|x| x.gpu_adapters = vec!["RTX 4060".into()])
            .unwrap();
        assert!(s.poll_gpu());
        assert!(s.has_gpus());
        assert_eq!(s.gpu_options().last().unwrap().1, "RTX 4060");
    }

    #[test]
    fn launch_at_login_is_off_by_default_and_toggles() {
        let login = Login::default();
        let (mut s, _f, _d, p) = session(login.clone());
        assert!(!s.launch_at_login());
        assert!(!s.settings().launch_at_login);
        s.set_launch_at_login(true);
        assert!(*login.on.borrow() && s.launch_at_login());
        assert!(reread(&p).launch_at_login);
        s.set_launch_at_login(false);
        assert!(!*login.on.borrow());
        assert!(!reread(&p).launch_at_login);
    }

    #[test]
    fn failed_registry_write_is_reported_and_not_saved() {
        let (mut s, fake, _d, p) = session(Login {
            fail: true,
            ..Default::default()
        });
        s.set_launch_at_login(true);
        assert!(s.error().unwrap().contains("launch at login"));
        assert!(!reread(&p).launch_at_login);
        assert!(fake.log.borrow().is_empty());
    }

    #[test]
    fn gpu_choices_come_from_the_installed_adapters_with_readable_names() {
        let (s, _fake, d, p) = session(Login::default());
        let sp = d.path().join("state.toml");
        ConfigFile::<State>::open(&sp)
            .update(|x| {
                x.gpu_adapters = vec![
                    "Intel(R) Iris(R) Xe Graphics".into(),
                    "NVIDIA GeForce RTX 3060 Laptop GPU".into(),
                ];
                x.gpu_in_use = Some("NVIDIA GeForce RTX 3060 Laptop GPU".into());
            })
            .unwrap();
        let mut s = s.with_state(ConfigFile::open(&sp));
        let opts = s.gpu_options();
        let labels: Vec<_> = opts.iter().map(|o| o.1.as_str()).collect();
        assert_eq!(
            labels,
            [
                "Auto",
                "CPU only",
                "Intel Iris Xe Graphics",
                "NVIDIA GeForce RTX 3060 Laptop GPU"
            ]
        );
        // The saved value is the exact adapter name, not the label.
        s.set_gpu(opts[2].0.clone());
        assert_eq!(
            reread(&p).gpu,
            Gpu::Device("Intel(R) Iris(R) Xe Graphics".into())
        );
        assert_eq!(
            s.gpu_status(),
            ["Running on NVIDIA GeForce RTX 3060 Laptop GPU"]
        );
    }

    #[test]
    fn gpu_error_notice_and_test_results() {
        let (s, fake, d, _p) = session(Login::default());
        let sp = d.path().join("state.toml");
        let mut st = ConfigFile::<State>::open(&sp);
        st.update(|x| {
            x.gpu_adapters = vec!["RTX 4060".into(), "UHD 620".into()];
            x.gpu_disabled_after_error = true;
        })
        .unwrap();
        let mut s = s.with_state(ConfigFile::open(&sp));
        assert!(s
            .gpu_notice()
            .unwrap()
            .starts_with("GPU disabled after an error"));
        assert!(s.has_gpus());

        s.retest_gpu();
        assert_eq!(fake.log.borrow().last().unwrap(), "retest-gpu");
        assert!(s.gpu_testing());
        assert!(s.gpu_notice().is_none(), "the test clears the error");
        let t0 = Instant::now();
        assert!(s.poll_gpu_at(t0), "no results yet");

        // The resident process saves one verdict per adapter.
        st.update(|x| {
            x.gpu_probes.insert(
                "RTX 4060|31.0.1".into(),
                aloud_type_config::GpuProbe {
                    approved: true,
                    speedup: 3.04,
                },
            );
            x.gpu_probes.insert(
                "UHD 620|27.0".into(),
                aloud_type_config::GpuProbe {
                    approved: false,
                    speedup: 0.8,
                },
            );
        })
        .unwrap();
        assert!(s.poll_gpu_at(t0), "done testing; still watching the reload");
        assert!(!s.gpu_testing());
        assert_eq!(
            s.gpu_status(),
            [
                "Running on the CPU",
                "Test: RTX 4060 is 3.0x faster than the CPU",
                "Test: UHD 620 not used (0.8x the CPU speed)"
            ]
        );
        assert!(!s.poll_gpu_at(t0 + GPU_WATCH), "stops watching");
    }

    #[test]
    fn a_test_without_a_resident_process_says_so() {
        struct Down;
        impl Transport for Down {
            fn request(&mut self, _: &str) -> io::Result<String> {
                Err(io::ErrorKind::NotFound.into())
            }
        }
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("settings.toml");
        let mut s = ItemsSession::new(ConfigFile::open(&p), Down, Login::default(), Mics(vec![]))
            .with_state(ConfigFile::open(d.path().join("state.toml")));
        s.retest_gpu();
        assert!(!s.gpu_testing());
        assert!(s.gpu_status()[0].contains("not running"));
    }

    #[test]
    fn language_choices_are_sorted_by_name_and_codes_are_unique() {
        let names: Vec<_> = LANGUAGES.iter().map(|l| l.1).collect();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted, "sorted by name");
        for code in ["en", "sk", "cs"] {
            assert!(LANGUAGES.iter().any(|l| l.0 == code), "{code}");
        }
        assert!(
            !LANGUAGES.iter().any(|l| l.0 == "yue"),
            "Cantonese needs a large-v3 model"
        );
        let mut c: Vec<_> = LANGUAGES.iter().map(|l| l.0).collect();
        c.sort();
        c.dedup();
        assert_eq!(c.len(), LANGUAGES.len());
    }
}
