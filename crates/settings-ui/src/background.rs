//! Downloads the resident process asks for without a window (it has no HTTP
//! client of its own): the voice-activity model, and the Small model that
//! becomes the default once a GPU is confirmed. Run as
//! `aloud-type-settings-ui --background-models`.
use crate::download::{download_model, DownloadError, Http, UreqHttp};
use crate::ipc::{Command, PipeClient, ResidentLink, Transport};
use aloud_type_config::{AppPaths, ConfigFile, Language, Settings, State, DEFAULT_SPEECH_MODEL};
use aloud_type_models::{
    background_work, find, BackgroundWork, ModelInfo, GPU_DEFAULT_MODEL, VAD_MODEL,
};
use std::path::Path;

/// Command-line flag that selects this mode.
pub const FLAG: &str = "--background-models";

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub vad: Option<Result<(), DownloadError>>,
    pub small: Option<Result<(), DownloadError>>,
    /// The Small model was made the default and the resident process told.
    pub switched: bool,
}

/// Does the work that is due. `vad` and `small` say which files those are
/// (the real catalog entries outside tests).
pub fn run<T: Transport>(
    work: BackgroundWork,
    dir: &Path,
    (vad, small): (&ModelInfo, &ModelInfo),
    (settings, state): (&mut ConfigFile<Settings>, &mut ConfigFile<State>),
    http: &mut dyn Http,
    link: &mut ResidentLink<T>,
) -> Report {
    let mut report = Report::default();
    if work.vad {
        report.vad = Some(download_model(vad, dir, http, &mut |_, _| {}));
    }
    if work.small_default {
        // Offered once: a failed attempt is not retried at every start.
        let _ = state.update(|s| s.small_upgrade_started = true);
        let result = download_model(small, dir, http, &mut |_, _| {});
        if result.is_ok() {
            settings.reload();
            if settings.value().speech_model == DEFAULT_SPEECH_MODEL {
                let switched = settings
                    .update(|s| {
                        s.speech_model = small.id.to_string();
                        if small.english_only {
                            s.language = Language::Fixed("en".into());
                        }
                    })
                    .is_ok();
                if switched {
                    link.send(Command::ReloadSettings);
                }
                report.switched = switched;
            }
        }
        report.small = Some(result);
    }
    report
}

/// The real thing: reads the user's files, uses the network and the pipe.
pub fn run_default() -> i32 {
    let Some(paths) = AppPaths::from_env() else {
        return 1;
    };
    let mut settings = ConfigFile::<Settings>::open(paths.settings_file());
    let mut state = ConfigFile::<State>::open(paths.state_file());
    let dir = paths.models_dir();
    let work = background_work(settings.value(), state.value(), &dir);
    if !work.any() {
        return 0;
    }
    let small = find(GPU_DEFAULT_MODEL).expect("small model is in the catalog");
    let mut link = ResidentLink::new(PipeClient::default());
    let r = run(
        work,
        &dir,
        (&VAD_MODEL, small),
        (&mut settings, &mut state),
        &mut UreqHttp::default(),
        &mut link,
    );
    i32::from(r.vad.is_some_and(|v| v.is_err()) || r.small.is_some_and(|v| v.is_err()))
}
