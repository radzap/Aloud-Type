//! The model manager behind the Settings window: rows, selection, download
//! outcomes, first-run state.
use aloud_type_config::{AppPaths, ConfigFile, Language, Settings, State};
use aloud_type_models::{find, model_path, FIRST_RUN_MODEL};
use aloud_type_settings_ui::download::DownloadError;
use aloud_type_settings_ui::ipc::{Command, Transport};
use aloud_type_settings_ui::manager::{ModelManager, ModelStatus, Selection};
use std::cell::RefCell;
use std::io;
use std::rc::Rc;

#[derive(Clone, Default)]
struct Sent(Rc<RefCell<Vec<String>>>);
impl Transport for Sent {
    fn request(&mut self, line: &str) -> io::Result<String> {
        self.0.borrow_mut().push(line.to_string());
        Ok("ok".into())
    }
}

fn manager(root: &std::path::Path) -> (ModelManager<Sent>, Sent) {
    let paths = AppPaths::under(root);
    std::fs::create_dir_all(paths.models_dir()).unwrap();
    let sent = Sent::default();
    let m = ModelManager::new(
        paths.models_dir(),
        ConfigFile::<Settings>::open(paths.settings_file()),
        ConfigFile::<State>::open(paths.state_file()),
        sent.clone(),
    );
    (m, sent)
}

fn install(root: &std::path::Path, id: &str) {
    let m = find(id).unwrap();
    std::fs::create_dir_all(AppPaths::under(root).models_dir()).unwrap();
    let f = std::fs::File::create(model_path(&AppPaths::under(root).models_dir(), m)).unwrap();
    f.set_len(m.size).unwrap();
}

fn status(m: &ModelManager<Sent>, id: &str) -> ModelStatus {
    m.rows().into_iter().find(|r| r.id == id).unwrap().status
}

#[test]
fn the_list_shows_every_supported_model_with_size_and_download_status() {
    let d = tempfile::tempdir().unwrap();
    install(d.path(), "small-q5_1");
    let (m, _) = manager(d.path());
    let rows = m.rows();
    assert_eq!(rows.len(), aloud_type_models::CATALOG.len());
    let small = rows.iter().find(|r| r.id == "small-q5_1").unwrap();
    assert_eq!(small.status, ModelStatus::Downloaded);
    assert_eq!(small.size_text, "190 MB");
    assert_eq!(status(&m, "base-q5_1"), ModelStatus::NotDownloaded);
}

#[test]
fn before_any_model_is_installed_setup_is_needed_and_the_base_model_is_preselected() {
    let d = tempfile::tempdir().unwrap();
    let (m, _) = manager(d.path());
    assert!(m.needs_setup());
    assert_eq!(m.selected(), FIRST_RUN_MODEL);
    assert_eq!(m.first_run_model().id, "base-q5_1");
}

#[test]
fn selecting_an_installed_model_saves_it_and_tells_the_resident_process() {
    let d = tempfile::tempdir().unwrap();
    install(d.path(), "small-q5_1");
    let (mut m, sent) = manager(d.path());
    assert_eq!(m.select("small-q5_1"), Selection::Selected);
    assert_eq!(m.selected(), "small-q5_1");
    let saved = ConfigFile::<Settings>::open(AppPaths::under(d.path()).settings_file());
    assert_eq!(saved.value().speech_model, "small-q5_1");
    assert_eq!(*sent.0.borrow(), vec![Command::ReloadSettings.encode()]);
}

#[test]
fn selecting_a_model_that_is_not_downloaded_asks_for_a_download_and_keeps_the_old_choice() {
    let d = tempfile::tempdir().unwrap();
    let (mut m, _) = manager(d.path());
    assert_eq!(m.select("small-q5_1"), Selection::NeedsDownload);
    assert_eq!(m.selected(), FIRST_RUN_MODEL);
    assert_eq!(status(&m, "small-q5_1"), ModelStatus::NotDownloaded);
}

#[test]
fn a_finished_download_completes_first_run_and_selects_the_model_that_was_asked_for() {
    let d = tempfile::tempdir().unwrap();
    let (mut m, sent) = manager(d.path());
    m.select("small-q5_1");
    m.start_download("small-q5_1");
    assert_eq!(status(&m, "small-q5_1"), ModelStatus::Downloading { done: 0, total: 190_085_487 });
    install(d.path(), "small-q5_1"); // what the downloader leaves behind
    m.finish_download("small-q5_1", Ok(()));
    assert_eq!(status(&m, "small-q5_1"), ModelStatus::Downloaded);
    assert_eq!(m.selected(), "small-q5_1");
    assert!(!m.needs_setup());
    let state = ConfigFile::<State>::open(AppPaths::under(d.path()).state_file());
    assert!(state.value().first_run_complete);
    assert!(sent.0.borrow().contains(&"reload-settings".to_string()));
}

#[test]
fn a_failed_download_shows_the_error_with_retry_and_remembers_the_partial_bytes() {
    let d = tempfile::tempdir().unwrap();
    let (mut m, _) = manager(d.path());
    m.start_download("base-q5_1");
    std::fs::write(
        aloud_type_settings_ui::download::part_path(
            &AppPaths::under(d.path()).models_dir(),
            find("base-q5_1").unwrap(),
        ),
        vec![0u8; 1234],
    )
    .unwrap();
    m.finish_download("base-q5_1", Err(DownloadError::Interrupted));
    assert_eq!(status(&m, "base-q5_1"), ModelStatus::Failed(DownloadError::Interrupted));
    assert!(m.needs_setup());
    let state = ConfigFile::<State>::open(AppPaths::under(d.path()).state_file());
    assert_eq!(state.value().downloads.get("base-q5_1"), Some(&1234));
    // Retry clears the error and shows progress again.
    m.start_download("base-q5_1");
    assert!(matches!(status(&m, "base-q5_1"), ModelStatus::Downloading { done: 1234, .. }));
}

#[test]
fn english_only_models_disable_the_language_setting_and_force_english() {
    let d = tempfile::tempdir().unwrap();
    install(d.path(), "base.en-q5_1");
    std::fs::write(AppPaths::under(d.path()).settings_file(), "version = 1
language = \"auto\"
").unwrap();
    let (mut m, _) = manager(d.path());
    assert!(m.language_enabled());
    m.select("base.en-q5_1");
    assert!(!m.language_enabled());
    let saved = ConfigFile::<Settings>::open(AppPaths::under(d.path()).settings_file());
    assert_eq!(saved.value().language, Language::Fixed("en".into()));
}
