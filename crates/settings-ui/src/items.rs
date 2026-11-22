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

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const RELEASES_URL: &str = "https://github.com/radzap/Aloud-Type/releases";
pub const SOURCE_URL: &str = "https://github.com/radzap/Aloud-Type";

/// Common Whisper languages offered for a fixed choice (code, name).
pub const LANGUAGES: &[(&str, &str)] = &[
    ("en", "English"),
    ("es", "Spanish"),
    ("fr", "French"),
    ("de", "German"),
    ("it", "Italian"),
    ("pt", "Portuguese"),
    ("nl", "Dutch"),
    ("pl", "Polish"),
    ("ru", "Russian"),
    ("uk", "Ukrainian"),
    ("cs", "Czech"),
    ("sv", "Swedish"),
    ("da", "Danish"),
    ("no", "Norwegian"),
    ("fi", "Finnish"),
    ("el", "Greek"),
    ("tr", "Turkish"),
    ("he", "Hebrew"),
    ("ar", "Arabic"),
    ("hi", "Hindi"),
    ("zh", "Chinese"),
    ("ja", "Japanese"),
    ("ko", "Korean"),
    ("vi", "Vietnamese"),
    ("id", "Indonesian"),
];

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
        }
    }

    /// Give the session the state file so the GPU choices and error notice can be shown.
    pub fn with_state(mut self, state: ConfigFile<State>) -> Self {
        self.state = Some(state);
        self
    }

    /// Auto, CPU only, then every device a probe has seen (names from the state file).
    pub fn gpu_options(&self) -> Vec<(Gpu, String)> {
        let mut out = vec![
            (Gpu::Auto, "Auto".to_string()),
            (Gpu::CpuOnly, "CPU only".to_string()),
        ];
        let mut names: Vec<String> = self
            .state
            .as_ref()
            .map(|s| {
                s.value()
                    .gpu_probes
                    .keys()
                    .map(|k| k.split('|').next().unwrap_or(k).to_string())
                    .collect()
            })
            .unwrap_or_default();
        if let Gpu::Device(d) = &self.config.value().gpu {
            names.push(d.clone());
        }
        names.sort();
        names.dedup();
        out.extend(names.into_iter().map(|n| (Gpu::Device(n.clone()), n)));
        out
    }

    pub fn set_gpu(&mut self, gpu: Gpu) {
        self.save(|s| s.gpu = gpu);
    }

    /// Shown in Settings when a backend error switched the GPU off for the session.
    pub fn gpu_notice(&self) -> Option<&'static str> {
        let st = self.state.as_ref()?;
        st.value()
            .gpu_disabled_after_error
            .then_some("GPU disabled after an error")
    }

    /// Re-test: the resident process forgets the error and saved results and probes again.
    pub fn retest_gpu(&mut self) {
        self.link.send(Command::RetestGpu);
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
    fn gpu_choices_error_notice_and_retest() {
        let (s, fake, d, p) = session(Login::default());
        let sp = d.path().join("state.toml");
        let mut st = ConfigFile::<State>::open(&sp);
        st.update(|x| {
            x.gpu_probes.insert(
                "RTX 4060|31.0.1".into(),
                aloud_type_config::GpuProbe {
                    approved: true,
                    speedup: 3.0,
                },
            );
            x.gpu_disabled_after_error = true;
        })
        .unwrap();
        let mut s = s.with_state(ConfigFile::open(&sp));
        let labels: Vec<_> = s.gpu_options().into_iter().map(|o| o.1).collect();
        assert_eq!(labels, ["Auto", "CPU only", "RTX 4060"]);
        assert_eq!(s.gpu_notice(), Some("GPU disabled after an error"));
        s.set_gpu(Gpu::Device("RTX 4060".into()));
        assert_eq!(reread(&p).gpu, Gpu::Device("RTX 4060".into()));
        s.retest_gpu();
        assert_eq!(fake.log.borrow().last().unwrap(), "retest-gpu");
    }

    #[test]
    fn language_choices_include_english_and_codes_are_unique() {
        assert_eq!(LANGUAGES[0].0, "en");
        let mut c: Vec<_> = LANGUAGES.iter().map(|l| l.0).collect();
        c.sort();
        c.dedup();
        assert_eq!(c.len(), LANGUAGES.len());
    }
}
