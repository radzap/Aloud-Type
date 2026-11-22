//! User settings (written only by the Settings window process).
use crate::file::{h, Schema};
use toml_edit::DocumentMut;

macro_rules! string_enum {
    ($name:ident { $($var:ident => $s:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum $name { $($var),+ }
        impl $name {
            pub fn as_str(self) -> &'static str { match self { $(Self::$var => $s),+ } }
            pub fn parse(s: &str) -> Option<Self> { match s { $($s => Some(Self::$var),)+ _ => None } }
        }
    };
}

string_enum!(ModelMemory { KeepLoaded => "keep-loaded", UnloadWhenIdle => "unload-when-idle" });
string_enum!(IndicatorStyle { BarPill => "bar-pill", Orb => "orb", TraceStrip => "trace-strip" });
string_enum!(VerticalPosition { Top => "top", Bottom => "bottom" });
string_enum!(HorizontalPosition { Left => "left", Center => "center", Right => "right" });

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Gpu {
    Auto,
    CpuOnly,
    Device(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Language {
    Auto,
    /// Fixed language code, e.g. "en".
    Fixed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// Keys held together as the Dictation key (names as captured by the hook).
    pub dictation_key: Vec<String>,
    /// None means "System default".
    pub microphone: Option<String>,
    pub speech_model: String,
    pub language: Language,
    pub gpu: Gpu,
    pub model_memory: ModelMemory,
    pub launch_at_login: bool,
    pub indicator_style: IndicatorStyle,
    pub indicator_vertical: VerticalPosition,
    pub indicator_horizontal: HorizontalPosition,
    /// Hidden tuning value; only written to the file when changed from the default.
    pub tap_threshold_ms: u32,
}

pub const DEFAULT_TAP_THRESHOLD_MS: u32 = 250;
pub const DEFAULT_SPEECH_MODEL: &str = "base-q5_1";

impl Default for Settings {
    fn default() -> Self {
        Settings {
            dictation_key: vec!["RightCtrl".into()],
            microphone: None,
            speech_model: DEFAULT_SPEECH_MODEL.into(),
            language: Language::Fixed("en".into()),
            gpu: Gpu::Auto,
            model_memory: ModelMemory::KeepLoaded,
            launch_at_login: false,
            indicator_style: IndicatorStyle::TraceStrip,
            indicator_vertical: VerticalPosition::Bottom,
            indicator_horizontal: HorizontalPosition::Center,
            tap_threshold_ms: DEFAULT_TAP_THRESHOLD_MS,
        }
    }
}

fn parse_enum<T>(doc: &DocumentMut, k: &str, parse: fn(&str) -> Option<T>) -> Result<Option<T>, String> {
    match h::get_str(doc, k)? {
        None => Ok(None),
        Some(s) => parse(&s).map(Some).ok_or(format!("`{k}` has unknown value \"{s}\"")),
    }
}

impl Schema for Settings {
    const VERSION: u32 = 1;

    fn migrate(_doc: &mut DocumentMut, _from: u32) {
        // v0 (no version key) has the same layout as v1.
    }

    fn read(doc: &DocumentMut) -> Result<Self, String> {
        let d = Settings::default();
        let language = match h::get_str(doc, "language")? {
            None => d.language,
            Some(s) if s == "auto" => Language::Auto,
            Some(s) => Language::Fixed(s),
        };
        let gpu = match h::get_str(doc, "gpu")? {
            None => d.gpu,
            Some(s) if s == "auto" => Gpu::Auto,
            Some(s) if s == "cpu" => Gpu::CpuOnly,
            Some(s) => Gpu::Device(s.strip_prefix("device:").unwrap_or(&s).to_string()),
        };
        let tap = match h::get_int(doc, "tap_threshold_ms")? {
            None => d.tap_threshold_ms,
            Some(n) if (1..=10_000).contains(&n) => n as u32,
            Some(n) => return Err(format!("`tap_threshold_ms` out of range: {n}")),
        };
        Ok(Settings {
            dictation_key: h::get_str_array(doc, "dictation_key")?.unwrap_or(d.dictation_key),
            microphone: h::get_str(doc, "microphone")?.filter(|s| !s.is_empty()),
            speech_model: h::get_str(doc, "speech_model")?.unwrap_or(d.speech_model),
            language,
            gpu,
            model_memory: parse_enum(doc, "model_memory", ModelMemory::parse)?.unwrap_or(d.model_memory),
            launch_at_login: h::get_bool(doc, "launch_at_login")?.unwrap_or(d.launch_at_login),
            indicator_style: parse_enum(doc, "indicator_style", IndicatorStyle::parse)?.unwrap_or(d.indicator_style),
            indicator_vertical: parse_enum(doc, "indicator_vertical", VerticalPosition::parse)?
                .unwrap_or(d.indicator_vertical),
            indicator_horizontal: parse_enum(doc, "indicator_horizontal", HorizontalPosition::parse)?
                .unwrap_or(d.indicator_horizontal),
            tap_threshold_ms: tap,
        })
    }

    fn write(&self, doc: &mut DocumentMut) {
        h::set_str_array(doc, "dictation_key", &self.dictation_key);
        match &self.microphone {
            Some(m) => h::set_str(doc, "microphone", m),
            None => h::remove(doc, "microphone"),
        }
        h::set_str(doc, "speech_model", &self.speech_model);
        h::set_str(
            doc,
            "language",
            match &self.language {
                Language::Auto => "auto",
                Language::Fixed(c) => c,
            },
        );
        let gpu = match &self.gpu {
            Gpu::Auto => "auto".to_string(),
            Gpu::CpuOnly => "cpu".to_string(),
            Gpu::Device(d) => format!("device:{d}"),
        };
        h::set_str(doc, "gpu", &gpu);
        h::set_str(doc, "model_memory", self.model_memory.as_str());
        h::set_bool(doc, "launch_at_login", self.launch_at_login);
        h::set_str(doc, "indicator_style", self.indicator_style.as_str());
        h::set_str(doc, "indicator_vertical", self.indicator_vertical.as_str());
        h::set_str(doc, "indicator_horizontal", self.indicator_horizontal.as_str());
        if self.tap_threshold_ms != DEFAULT_TAP_THRESHOLD_MS || doc.get("tap_threshold_ms").is_some() {
            h::set_int(doc, "tap_threshold_ms", self.tap_threshold_ms as i64);
        }
    }
}
