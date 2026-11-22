//! Background downloads against an in-memory server: the voice model, then the
//! Small model that becomes the default once a GPU is confirmed.
use aloud_type_config::{ConfigFile, Settings, State};
use aloud_type_models::{model_path, BackgroundWork, ModelInfo};
use aloud_type_settings_ui::background::run;
use aloud_type_settings_ui::download::{Http, HttpError, Response};
use aloud_type_settings_ui::ipc::{ResidentLink, Transport};
use std::cell::RefCell;
use std::io::Cursor;
use std::path::Path;
use std::rc::Rc;

const VAD_BYTES: &[u8] = b"voice-activity-model";
const SMALL_BYTES: &[u8] = b"small-speech-model-bytes";

fn sha(bytes: &[u8]) -> &'static str {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("x");
    std::fs::write(&p, bytes).unwrap();
    Box::leak(aloud_type_models::sha256_file(&p).unwrap().into_boxed_str())
}

fn info(id: &'static str, file: &'static str, bytes: &[u8]) -> ModelInfo {
    ModelInfo { id, label: id, file, url: file, size: bytes.len() as u64, sha256: sha(bytes), english_only: false }
}

/// Serves by url; records the requests.
struct Fake(Vec<(&'static str, &'static [u8])>, Rc<RefCell<Vec<String>>>);
impl Http for Fake {
    fn get(&mut self, url: &str, _from: u64) -> Result<Response, HttpError> {
        self.1.borrow_mut().push(url.to_string());
        match self.0.iter().find(|(u, _)| *u == url) {
            Some((_, b)) => Ok(Response { resumed: false, body: Box::new(Cursor::new(b.to_vec())) }),
            None => Err(HttpError::Status(404)),
        }
    }
}

#[derive(Clone, Default)]
struct Pipe(Rc<RefCell<Vec<String>>>);
impl Transport for Pipe {
    fn request(&mut self, line: &str) -> std::io::Result<String> {
        self.0.borrow_mut().push(line.to_string());
        Ok("ok\n".into())
    }
}

struct Rig {
    dir: tempfile::TempDir,
    vad: ModelInfo,
    small: ModelInfo,
    settings: ConfigFile<Settings>,
    state: ConfigFile<State>,
    pipe: Pipe,
}

fn rig() -> Rig {
    let dir = tempfile::tempdir().unwrap();
    let settings = ConfigFile::<Settings>::open(dir.path().join("settings.toml"));
    let state = ConfigFile::<State>::open(dir.path().join("state.toml"));
    Rig {
        vad: info("vad", "vad.bin", VAD_BYTES),
        small: info("small-q5_1", "small.bin", SMALL_BYTES),
        dir,
        settings,
        state,
        pipe: Pipe::default(),
    }
}

impl Rig {
    fn run(&mut self, work: BackgroundWork, http: &mut Fake) -> aloud_type_settings_ui::background::Report {
        let mut link = ResidentLink::new(self.pipe.clone());
        run(work, self.dir.path(), (&self.vad, &self.small), (&mut self.settings, &mut self.state), http, &mut link)
    }
    fn fake(&self) -> Fake {
        Fake(vec![("vad.bin", VAD_BYTES), ("small.bin", SMALL_BYTES)], Rc::default())
    }
    fn saved_settings(&self) -> Settings {
        ConfigFile::<Settings>::open(self.dir.path().join("settings.toml")).value().clone()
    }
    fn saved_state(&self) -> State {
        ConfigFile::<State>::open(self.dir.path().join("state.toml")).value().clone()
    }
}

fn exists(dir: &Path, m: &ModelInfo) -> bool {
    model_path(dir, m).exists()
}

#[test]
fn the_voice_model_is_downloaded_and_verified_without_touching_settings() {
    let mut r = rig();
    let mut http = r.fake();
    let rep = r.run(BackgroundWork { vad: true, small_default: false }, &mut http);
    assert_eq!(rep.vad, Some(Ok(())));
    assert!(exists(r.dir.path(), &r.vad));
    assert!(!rep.switched && r.pipe.0.borrow().is_empty());
    assert_eq!(http.1.borrow().len(), 1);
}

#[test]
fn small_becomes_the_default_and_the_resident_process_is_told() {
    let mut r = rig();
    let mut http = r.fake();
    let rep = r.run(BackgroundWork { vad: false, small_default: true }, &mut http);
    assert_eq!(rep.small, Some(Ok(())));
    assert!(rep.switched);
    assert!(exists(r.dir.path(), &r.small));
    assert_eq!(r.saved_settings().speech_model, "small-q5_1");
    assert!(r.saved_state().small_upgrade_started);
    assert_eq!(r.pipe.0.borrow().len(), 1);
}

#[test]
fn a_model_the_user_chose_meanwhile_is_not_overridden() {
    let mut r = rig();
    r.settings.update(|s| s.speech_model = "base.en-q5_1".into()).unwrap();
    let mut http = r.fake();
    let rep = r.run(BackgroundWork { vad: false, small_default: true }, &mut http);
    assert!(!rep.switched);
    assert_eq!(r.saved_settings().speech_model, "base.en-q5_1");
}

#[test]
fn a_failed_download_is_not_retried_at_every_start() {
    let mut r = rig();
    let mut http = Fake(vec![], Rc::default());
    let rep = r.run(BackgroundWork { vad: false, small_default: true }, &mut http);
    assert!(matches!(rep.small, Some(Err(_))));
    assert!(!rep.switched);
    assert!(r.saved_state().small_upgrade_started);
}
