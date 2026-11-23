//! OS-free host wiring: applies saved settings to the live parts without a
//! restart, routes Settings pipe commands, and publishes controller state to
//! the tray and overlay. The Windows adapters are plugged in by `main.rs`.
use crate::{DictationSettings, Mic};
use aloud_type_config::{
    HorizontalPosition, IndicatorStyle, Language, ModelMemory, Settings, VerticalPosition,
};
use aloud_type_core::ipc::Command;
use aloud_type_core::keynames::vk_for_name;
use aloud_type_core::lifecycle::LifecycleConfig;
use aloud_type_core::listening::{Indicator, Tray};
use std::cell::RefCell;
use std::rc::Rc;

/// Key codes of the Dictation key (one key or a combination). `None` when a
/// stored name is unknown, so a bad file never unbinds the key.
pub fn dictation_vks(s: &Settings) -> Option<Vec<u32>> {
    let v: Option<Vec<u32>> = s.dictation_key.iter().map(|k| vk_for_name(k)).collect();
    v.filter(|v| !v.is_empty())
}

const MODIFIER_VKS: [u32; 11] = [
    0x10, 0x11, 0x12, 0xA0, 0xA1, 0xA2, 0xA3, 0xA4, 0xA5, 0x5B, 0x5C,
];

/// The chord rule applies only when the Dictation key is a lone modifier.
pub fn lifecycle_for(s: &Settings) -> LifecycleConfig {
    let modifier_key =
        matches!(dictation_vks(s).as_deref(), Some([vk]) if MODIFIER_VKS.contains(vk));
    LifecycleConfig {
        tap_threshold_ms: s.tap_threshold_ms as u64,
        modifier_key,
    }
}

pub fn dictation_settings(s: &Settings) -> DictationSettings {
    DictationSettings {
        language: match &s.language {
            Language::Auto => "auto".into(),
            Language::Fixed(c) => c.clone(),
        },
        // English-only models always dictate English.
        language_from_keyboard: s.language_from_keyboard
            && aloud_type_models::language_setting_enabled(&s.speech_model),
        microphone: s
            .microphone
            .clone()
            .map_or(Mic::SystemDefault, Mic::Specific),
    }
}

/// The language one Dictation uses: the keyboard layout's (a locale name such as "sk-SK", read
/// when the Dictation key was pressed) when that is on and Whisper knows it, else the setting.
pub fn dictation_language(d: &DictationSettings, keyboard_locale: Option<&str>) -> String {
    if d.language_from_keyboard {
        if let Some(code) =
            keyboard_locale.and_then(aloud_type_models::languages::whisper_language_for_locale)
        {
            return code.to_string();
        }
    }
    d.language.clone()
}

/// The notice shown when the language Dictation uses was changed in Settings.
pub fn language_notice(s: &Settings) -> String {
    let what = if !aloud_type_models::language_setting_enabled(&s.speech_model) {
        "English".to_string()
    } else if s.language_from_keyboard {
        "follows the keyboard layout".to_string()
    } else {
        match &s.language {
            Language::Auto => "detected automatically".to_string(),
            Language::Fixed(c) => aloud_type_models::languages::language_name(c)
                .map_or_else(|| c.clone(), str::to_string),
        }
    };
    format!("Dictation language: {what}")
}

/// The notice shown when a Speech model finished loading after a change (model, GPU, first
/// download). `gpu` is the device it runs on, `None` for the CPU.
pub fn model_ready_notice(model_id: &str, gpu: Option<&str>) -> String {
    let name = aloud_type_models::find(model_id).map_or(model_id, |m| m.label);
    let on = match gpu {
        // Adapter names carry trademark marks: "Intel(R) Iris(R) Xe Graphics".
        Some(d) => ["(R)", "(TM)", "(C)"]
            .iter()
            .fold(d.to_string(), |s, m| s.replace(m, ""))
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" "),
        None => "the CPU".to_string(),
    };
    format!("Speech model {name} ready on {on}")
}

/// What the host can change live. Implemented by `main.rs` over the real hook,
/// overlay, residency and tray; by fakes in tests.
pub trait Targets {
    fn set_keys(&mut self, vks: &[u32]);
    fn set_suspended(&mut self, suspended: bool);
    fn set_lifecycle(&mut self, cfg: LifecycleConfig);
    fn set_style(&mut self, style: IndicatorStyle);
    fn set_placement(&mut self, v: VerticalPosition, h: HorizontalPosition);
    fn set_memory(&mut self, m: ModelMemory);
    fn request_model(&mut self, id: &str);
    /// The GPU choice changed: load the current model again with it.
    fn reload_gpu(&mut self);
    /// Settings "Test": measure every GPU against the CPU (slow; runs in the background).
    fn retest_gpu(&mut self);
    /// A brief tray notice.
    fn notify(&mut self, text: &str);
}

/// A freshly read settings file: the settings in force and, if the file just
/// turned invalid, the notice to show.
pub struct Loaded {
    pub settings: Settings,
    pub notice: Option<String>,
}

pub struct SettingsHost<T: Targets> {
    targets: T,
    source: Box<dyn FnMut() -> Loaded>,
    applied: Option<Settings>,
    current: Rc<RefCell<DictationSettings>>,
}

impl<T: Targets> SettingsHost<T> {
    pub fn new(targets: T, source: Box<dyn FnMut() -> Loaded>) -> Self {
        Self {
            targets,
            source,
            applied: None,
            current: Rc::new(RefCell::new(DictationSettings::default())),
        }
    }

    /// The language and microphone read when a Dictation starts.
    pub fn dictation_settings(&self) -> Rc<RefCell<DictationSettings>> {
        self.current.clone()
    }

    pub fn settings(&self) -> Option<&Settings> {
        self.applied.as_ref()
    }

    /// Read the settings file and apply what changed (everything on the first call).
    pub fn reload(&mut self) {
        let Loaded {
            settings: new,
            notice,
        } = (self.source)();
        if let Some(n) = notice {
            self.targets.notify(&n);
        }
        let old = self.applied.as_ref();
        if old.map(|o| &o.dictation_key) != Some(&new.dictation_key) {
            if let Some(vks) = dictation_vks(&new) {
                self.targets.set_keys(&vks);
            }
        }
        let life = lifecycle_for(&new);
        if old.map(lifecycle_for) != Some(life) {
            self.targets.set_lifecycle(life);
        }
        if old.map(|o| o.indicator_style) != Some(new.indicator_style) {
            self.targets.set_style(new.indicator_style);
        }
        let place = (new.indicator_vertical, new.indicator_horizontal);
        if old.map(|o| (o.indicator_vertical, o.indicator_horizontal)) != Some(place) {
            self.targets.set_placement(place.0, place.1);
        }
        if old.map(|o| o.model_memory) != Some(new.model_memory) {
            self.targets.set_memory(new.model_memory);
        }
        let model_changed = old.map(|o| &o.speech_model) != Some(&new.speech_model);
        if model_changed {
            self.targets.request_model(&new.speech_model);
        }
        // A model change already loads with the new GPU choice.
        if !model_changed && old.is_some_and(|o| o.gpu != new.gpu) {
            self.targets.reload_gpu();
        }
        if old.is_some_and(|o| language_notice(o) != language_notice(&new)) {
            self.targets.notify(&language_notice(&new));
        }
        *self.current.borrow_mut() = dictation_settings(&new);
        self.applied = Some(new);
    }

    /// A command from the Settings window.
    pub fn on_command(&mut self, c: Command) {
        match c {
            Command::SuspendKeys => self.targets.set_suspended(true),
            Command::ResumeKeys => self.targets.set_suspended(false),
            Command::ReloadSettings => self.reload(),
            Command::RetestGpu => self.targets.retest_gpu(),
        }
    }
}

/// Where controller state is shown.
pub trait Surfaces {
    fn indicator(&mut self, i: &Indicator);
    fn tray(&mut self, t: &Tray, key_unavailable: bool, blocked_fault_standing: bool);
}

/// Pushes the indicator every tick (it carries the live level and is free
/// while hidden) and the tray only when it changed.
#[derive(Default)]
pub struct Publisher {
    tray: Option<(Tray, bool, bool)>,
}

impl Publisher {
    pub fn publish(
        &mut self,
        s: &mut dyn Surfaces,
        indicator: &Indicator,
        tray: &Tray,
        key_unavailable: bool,
        blocked: bool,
    ) {
        s.indicator(indicator);
        let now = (tray.clone(), key_unavailable, blocked);
        if self.tray.as_ref() != Some(&now) {
            s.tray(tray, key_unavailable, blocked);
            self.tray = Some(now);
        }
    }
}

/// A supervisor report for the worker driving `c`: a restart drops the broken
/// Dictation; a spent budget shows the fault (tray and indicator) and returns
/// true so the host stays resident instead of exiting.
pub fn handle_report<K: aloud_type_core::Keyboard, C: aloud_type_core::Clock + Clone>(
    c: &mut crate::Controller<K, C>,
    r: aloud_type_core::supervisor::Report,
) -> bool {
    use aloud_type_core::supervisor::Report;
    match r {
        Report::Restarted(_) => {
            c.recover();
            false
        }
        Report::Faulted(_, f) => {
            c.worker_fault(f);
            true
        }
    }
}

/// The Settings window lives in its own executable next to this one, so the
/// resident process links no network code (the model downloader is Settings').
pub fn settings_exe_beside(current_exe: &std::path::Path) -> std::path::PathBuf {
    current_exe.with_file_name("aloud-type-settings-ui.exe")
}

/// Starts the Settings program's background downloads (the resident process
/// has no network code of its own).
pub trait BackgroundSpawner {
    fn spawn_background_models(&mut self);
}

/// Decides when the voice model or the GPU default model is due, once each
/// time it is armed (after a model load, or after settings or a GPU re-test change).
#[derive(Default)]
pub struct BackgroundModels {
    spawned_for_current_files: bool,
}

impl BackgroundModels {
    /// Spawns the downloader when something is due and it was not already
    /// started for the files as they are now.
    pub fn check(
        &mut self,
        settings: &Settings,
        state: &aloud_type_config::State,
        models_dir: &std::path::Path,
        spawner: &mut dyn BackgroundSpawner,
    ) {
        if self.spawned_for_current_files {
            return;
        }
        if aloud_type_models::background_work(settings, state, models_dir).any() {
            self.spawned_for_current_files = true;
            spawner.spawn_background_models();
        }
    }

    /// Settings or the GPU verdict may have changed what is due.
    pub fn rearm(&mut self) {
        self.spawned_for_current_files = false;
    }
}
