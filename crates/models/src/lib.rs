//! The Speech model lineup and the rule that only verified files are loaded.
//! No network code lives here: downloads belong to the Settings process, the
//! resident process only calls [`verified_path`].
use aloud_type_config::{Gpu, Settings, State, DEFAULT_SPEECH_MODEL};
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

/// One downloadable Speech model (quantised ggml Whisper, MIT licence).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelInfo {
    /// Stored in settings as `speech_model`.
    pub id: &'static str,
    pub label: &'static str,
    pub file: &'static str,
    pub url: &'static str,
    pub size: u64,
    pub sha256: &'static str,
    pub english_only: bool,
}

macro_rules! model {
    ($id:literal, $label:literal, $file:literal, $size:literal, $sha:literal, $en:literal) => {
        ModelInfo {
            id: $id,
            label: $label,
            file: $file,
            url: concat!(
                "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/",
                $file
            ),
            size: $size,
            sha256: $sha,
            english_only: $en,
        }
    };
}

/// Supported models, first-run model first. Sizes and SHA-256 values are the
/// ones published by the whisper.cpp repository on Hugging Face (LFS metadata).
pub const CATALOG: &[ModelInfo] = &[
    model!(
        "base-q5_1",
        "Base (multilingual)",
        "ggml-base-q5_1.bin",
        59_707_625,
        "422f1ae452ade6f30a004d7e5c6a43195e4433bc370bf23fac9cc591f01a8898",
        false
    ),
    model!(
        "small-q5_1",
        "Small (multilingual)",
        "ggml-small-q5_1.bin",
        190_085_487,
        "ae85e4a935d7a567bd102fe55afc16bb595bdb618e11b2fc7591bc08120411bb",
        false
    ),
    model!(
        "base.en-q5_1",
        "Base (English only)",
        "ggml-base.en-q5_1.bin",
        59_721_011,
        "4baf70dd0d7c4247ba2b81fafd9c01005ac77c2f9ef064e00dcf195d0e2fdd2f",
        true
    ),
    model!(
        "small.en-q5_1",
        "Small (English only)",
        "ggml-small.en-q5_1.bin",
        190_098_681,
        "bfdff4894dcb76bbf647d56263ea2a96645423f1669176f4844a1bf8e478ad30",
        true
    ),
    model!(
        "tiny.en-q5_1",
        "Tiny (English only, weak PCs)",
        "ggml-tiny.en-q5_1.bin",
        32_166_155,
        "c77c5766f1cef09b6b7d47f21b546cbddd4157886b3b5d6d4f709e91e66c7c2b",
        true
    ),
    model!(
        "large-v3-turbo-q5_0",
        "Large v3 turbo (needs a GPU)",
        "ggml-large-v3-turbo-q5_0.bin",
        574_041_195,
        "394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2",
        false
    ),
];

/// The Silero voice-activity model that keeps silence away from Whisper. Not
/// a choice in Settings: it is fetched alongside the Speech model.
pub const VAD_MODEL: ModelInfo = ModelInfo {
    id: "silero-vad",
    label: "Voice activity detector",
    file: "ggml-silero-v5.1.2.bin",
    url: "https://huggingface.co/ggml-org/whisper-vad/resolve/main/ggml-silero-v5.1.2.bin",
    size: 885_098,
    sha256: "29940d98d42b91fbd05ce489f3ecf7c72f0a42f027e4875919a28fb4c04ea2cf",
    english_only: false,
};

/// The model that becomes the default once a GPU is confirmed.
pub const GPU_DEFAULT_MODEL: &str = "small-q5_1";

/// The one-click first-run model (also the Settings default).
pub const FIRST_RUN_MODEL: &str = "base-q5_1";

pub fn find(id: &str) -> Option<&'static ModelInfo> {
    CATALOG.iter().find(|m| m.id == id)
}

/// English-only models cannot transcribe other languages, so Settings
/// disables the language setting for them. Unknown ids leave it enabled.
pub fn language_setting_enabled(model_id: &str) -> bool {
    !find(model_id).map(|m| m.english_only).unwrap_or(false)
}

#[derive(Debug)]
pub enum VerifyError {
    Missing,
    WrongSize { expected: u64, found: u64 },
    WrongChecksum,
    Io(io::Error),
}

pub fn sha256_file(path: &Path) -> io::Result<String> {
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    let mut s = String::new();
    for b in h.finalize() {
        write!(s, "{b:02x}").unwrap();
    }
    Ok(s)
}

pub fn model_path(dir: &Path, m: &ModelInfo) -> PathBuf {
    dir.join(m.file)
}

/// Full verification (size, then SHA-256). The resident process loads a model
/// only from a path returned here.
pub fn verified_path(dir: &Path, m: &ModelInfo) -> Result<PathBuf, VerifyError> {
    let p = model_path(dir, m);
    let meta = std::fs::metadata(&p).map_err(|e| match e.kind() {
        io::ErrorKind::NotFound => VerifyError::Missing,
        _ => VerifyError::Io(e),
    })?;
    if meta.len() != m.size {
        return Err(VerifyError::WrongSize {
            expected: m.size,
            found: meta.len(),
        });
    }
    match sha256_file(&p) {
        Ok(h) if h.eq_ignore_ascii_case(m.sha256) => Ok(p),
        Ok(_) => Err(VerifyError::WrongChecksum),
        Err(e) => Err(VerifyError::Io(e)),
    }
}

/// Cheap check used to render Settings and decide first-run completion: the
/// file exists with the right size. Files only reach the models folder after a
/// full verification by the downloader, so this is not a loading gate.
pub fn is_installed(dir: &Path, m: &ModelInfo) -> bool {
    std::fs::metadata(model_path(dir, m))
        .map(|x| x.len() == m.size)
        .unwrap_or(false)
}

/// First-run completion: a usable Speech model is installed.
pub fn setup_complete(dir: &Path) -> bool {
    CATALOG.iter().any(|m| is_installed(dir, m))
}

/// Downloads the resident process should have the Settings program do in the
/// background (the resident process itself never touches the network).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BackgroundWork {
    /// The voice-activity model is missing.
    pub vad: bool,
    /// A GPU is confirmed: fetch the Small model and make it the default.
    pub small_default: bool,
}

impl BackgroundWork {
    pub fn any(self) -> bool {
        self.vad || self.small_default
    }
}

/// What background downloading is due. Nothing before first run completes.
pub fn background_work(settings: &Settings, state: &State, dir: &Path) -> BackgroundWork {
    if !setup_complete(dir) {
        return BackgroundWork::default();
    }
    let gpu_confirmed = settings.gpu != Gpu::CpuOnly
        && !state.gpu_disabled_after_error
        && state.gpu_probes.values().any(|p| p.approved);
    let small = find(GPU_DEFAULT_MODEL).expect("small model is in the catalog");
    BackgroundWork {
        vad: !is_installed(dir, &VAD_MODEL),
        small_default: gpu_confirmed
            && settings.speech_model == DEFAULT_SPEECH_MODEL
            && !state.small_upgrade_started
            && !is_installed(dir, small),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // sha256("hello") from an independent source.
    const HELLO: &str = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";
    fn tiny() -> ModelInfo {
        ModelInfo {
            id: "t",
            label: "t",
            file: "ggml-t.bin",
            url: "",
            size: 5,
            sha256: HELLO,
            english_only: false,
        }
    }

    #[test]
    fn a_file_with_the_right_checksum_is_loadable() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("ggml-t.bin"), b"hello").unwrap();
        assert!(verified_path(d.path(), &tiny()).is_ok());
    }

    #[test]
    fn unverified_files_are_refused() {
        let d = tempfile::tempdir().unwrap();
        let m = tiny();
        assert!(matches!(
            verified_path(d.path(), &m),
            Err(VerifyError::Missing)
        ));
        std::fs::write(d.path().join("ggml-t.bin"), b"hell").unwrap();
        assert!(matches!(
            verified_path(d.path(), &m),
            Err(VerifyError::WrongSize { .. })
        ));
        std::fs::write(d.path().join("ggml-t.bin"), b"hellp").unwrap();
        assert!(matches!(
            verified_path(d.path(), &m),
            Err(VerifyError::WrongChecksum)
        ));
    }

    #[test]
    fn english_only_models_disable_the_language_setting() {
        assert!(!language_setting_enabled("base.en-q5_1"));
        assert!(!language_setting_enabled("small.en-q5_1"));
        assert!(language_setting_enabled("base-q5_1"));
        assert!(language_setting_enabled("large-v3-turbo-q5_0"));
    }

    #[test]
    fn setup_is_complete_only_when_a_model_file_of_the_right_size_exists() {
        let d = tempfile::tempdir().unwrap();
        assert!(!setup_complete(d.path()));
        let m = find(FIRST_RUN_MODEL).unwrap();
        std::fs::write(model_path(d.path(), m), b"partial").unwrap();
        assert!(!setup_complete(d.path()));
        let f = std::fs::File::create(model_path(d.path(), m)).unwrap();
        f.set_len(m.size).unwrap();
        assert!(setup_complete(d.path()));
    }

    #[test]
    fn catalog_is_well_formed() {
        for m in CATALOG {
            assert_eq!(m.sha256.len(), 64, "{}", m.id);
            assert!(m.url.ends_with(m.file), "{}", m.id);
            assert_eq!(m.english_only, m.id.contains(".en"), "{}", m.id);
        }
        assert_eq!(CATALOG[0].id, FIRST_RUN_MODEL);
    }

    fn installed_base(dir: &Path) {
        let m = find(FIRST_RUN_MODEL).unwrap();
        std::fs::File::create(model_path(dir, m))
            .unwrap()
            .set_len(m.size)
            .unwrap();
    }

    fn confirmed_gpu() -> State {
        let mut st = State::default();
        st.gpu_probes.insert(
            "RTX 4060|31.0.1".into(),
            aloud_type_config::GpuProbe {
                approved: true,
                speedup: 3.0,
            },
        );
        st
    }

    #[test]
    fn vad_model_is_well_formed_and_not_a_settings_choice() {
        assert_eq!(VAD_MODEL.sha256.len(), 64);
        assert!(VAD_MODEL.url.ends_with(VAD_MODEL.file));
        assert!(CATALOG.iter().all(|m| m.id != VAD_MODEL.id));
    }

    #[test]
    fn nothing_downloads_in_the_background_before_first_run_is_complete() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(
            background_work(&Settings::default(), &confirmed_gpu(), d.path()),
            BackgroundWork::default()
        );
    }

    #[test]
    fn a_missing_voice_model_is_fetched_once_a_speech_model_is_installed() {
        let d = tempfile::tempdir().unwrap();
        installed_base(d.path());
        let w = background_work(&Settings::default(), &State::default(), d.path());
        assert_eq!(
            w,
            BackgroundWork {
                vad: true,
                small_default: false
            }
        );
        std::fs::File::create(model_path(d.path(), &VAD_MODEL))
            .unwrap()
            .set_len(VAD_MODEL.size)
            .unwrap();
        assert!(!background_work(&Settings::default(), &State::default(), d.path()).any());
    }

    #[test]
    fn small_becomes_the_default_only_with_a_confirmed_gpu_the_default_model_and_no_earlier_attempt(
    ) {
        let d = tempfile::tempdir().unwrap();
        installed_base(d.path());
        let due = |s: &Settings, st: &State| background_work(s, st, d.path()).small_default;
        assert!(due(&Settings::default(), &confirmed_gpu()));
        assert!(
            !due(&Settings::default(), &State::default()),
            "no confirmed GPU"
        );
        let unapproved = {
            let mut st = State::default();
            st.gpu_probes.insert(
                "x|1".into(),
                aloud_type_config::GpuProbe {
                    approved: false,
                    speedup: 0.5,
                },
            );
            st
        };
        assert!(!due(&Settings::default(), &unapproved));
        let cpu_only = Settings {
            gpu: Gpu::CpuOnly,
            ..Settings::default()
        };
        assert!(!due(&cpu_only, &confirmed_gpu()), "user chose CPU only");
        let chose = Settings {
            speech_model: "base.en-q5_1".into(),
            ..Settings::default()
        };
        assert!(
            !due(&chose, &confirmed_gpu()),
            "the user's own model choice stands"
        );
        let mut errored = confirmed_gpu();
        errored.gpu_disabled_after_error = true;
        assert!(!due(&Settings::default(), &errored));
        let mut tried = confirmed_gpu();
        tried.small_upgrade_started = true;
        assert!(!due(&Settings::default(), &tried), "offered once");
        let small = find(GPU_DEFAULT_MODEL).unwrap();
        std::fs::File::create(model_path(d.path(), small))
            .unwrap()
            .set_len(small.size)
            .unwrap();
        assert!(
            !due(&Settings::default(), &confirmed_gpu()),
            "already installed"
        );
    }
}
