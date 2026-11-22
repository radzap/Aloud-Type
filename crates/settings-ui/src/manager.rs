//! Model manager behind the Settings window: the model list with download
//! status, selection, download outcomes and first-run state. Pure logic over
//! the config files; the actual transfer is `download::download_model`, run by
//! the window on a worker thread and reported back through `finish_download`.
use crate::download::{part_path, DownloadError};
use crate::ipc::{Command, ResidentLink, Transport};
use aloud_type_config::{ConfigFile, Language, Settings, State};
use aloud_type_models::{
    find, is_installed, language_setting_enabled, ModelInfo, CATALOG, FIRST_RUN_MODEL,
};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelStatus {
    NotDownloaded,
    Downloading {
        done: u64,
        total: u64,
    },
    Downloaded,
    /// The last attempt failed; the row shows the message and Retry.
    Failed(DownloadError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelRow {
    pub id: &'static str,
    pub label: &'static str,
    pub size_text: String,
    pub status: ModelStatus,
    pub selected: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Selection {
    Selected,
    /// Not downloaded yet: the caller starts a download, which selects it when it finishes.
    NeedsDownload,
    Unknown,
}

pub struct ModelManager<T: Transport> {
    dir: PathBuf,
    settings: ConfigFile<Settings>,
    state: ConfigFile<State>,
    link: ResidentLink<T>,
    downloading: BTreeMap<&'static str, (u64, u64)>,
    failed: BTreeMap<&'static str, DownloadError>,
    /// Model to select once its download finishes.
    pending_select: Option<&'static str>,
}

/// Decimal megabytes, matching the sizes the model repository lists.
pub fn size_text(bytes: u64) -> String {
    if bytes >= 1_000_000_000 {
        format!("{:.2} GB", bytes as f64 / 1e9)
    } else {
        format!("{:.0} MB", bytes as f64 / 1e6)
    }
}

impl<T: Transport> ModelManager<T> {
    pub fn new(
        dir: PathBuf,
        settings: ConfigFile<Settings>,
        state: ConfigFile<State>,
        transport: T,
    ) -> Self {
        Self {
            dir,
            settings,
            state,
            link: ResidentLink::new(transport),
            downloading: BTreeMap::new(),
            failed: BTreeMap::new(),
            pending_select: None,
        }
    }

    pub fn models_dir(&self) -> &std::path::Path {
        &self.dir
    }

    pub fn selected(&self) -> &str {
        &self.settings.value().speech_model
    }

    pub fn first_run_model(&self) -> &'static ModelInfo {
        find(FIRST_RUN_MODEL).expect("first-run model is in the catalog")
    }

    /// First run is incomplete until a usable Speech model is installed.
    /// Settings opens at every start while this is true.
    pub fn needs_setup(&self) -> bool {
        !aloud_type_models::setup_complete(&self.dir)
    }

    /// The language setting is disabled for English-only models.
    pub fn language_enabled(&self) -> bool {
        language_setting_enabled(self.selected())
    }

    pub fn rows(&self) -> Vec<ModelRow> {
        CATALOG
            .iter()
            .map(|m| {
                let status = if let Some(&(done, total)) = self.downloading.get(m.id) {
                    ModelStatus::Downloading { done, total }
                } else if let Some(&e) = self.failed.get(m.id) {
                    ModelStatus::Failed(e)
                } else if is_installed(&self.dir, m) {
                    ModelStatus::Downloaded
                } else {
                    ModelStatus::NotDownloaded
                };
                ModelRow {
                    id: m.id,
                    label: m.label,
                    size_text: size_text(m.size),
                    status,
                    selected: m.id == self.selected(),
                }
            })
            .collect()
    }

    /// The user picked a model in the list.
    pub fn select(&mut self, id: &str) -> Selection {
        let Some(m) = find(id) else {
            return Selection::Unknown;
        };
        if is_installed(&self.dir, m) {
            self.pending_select = None;
            self.apply_selection(m);
            Selection::Selected
        } else {
            self.pending_select = Some(m.id);
            Selection::NeedsDownload
        }
    }

    fn apply_selection(&mut self, m: &ModelInfo) {
        let _ = self.settings.update(|s| {
            s.speech_model = m.id.to_string();
            if m.english_only {
                s.language = Language::Fixed("en".into());
            }
        });
        self.link.send(Command::ReloadSettings);
    }

    /// A download is about to run (first attempt or Retry).
    pub fn start_download(&mut self, id: &str) {
        let Some(m) = find(id) else { return };
        self.failed.remove(m.id);
        let have = std::fs::metadata(part_path(&self.dir, m))
            .map(|x| x.len())
            .unwrap_or(0);
        self.downloading.insert(m.id, (have.min(m.size), m.size));
        if self.pending_select.is_none() && self.needs_setup() {
            // The one-click first-run download also becomes the chosen model.
            self.pending_select = Some(m.id);
        }
    }

    pub fn progress(&mut self, id: &str, done: u64, total: u64) {
        if let Some(m) = find(id) {
            self.downloading.insert(m.id, (done, total));
        }
    }

    /// The worker finished. On success first run is complete and the model is
    /// selected when that is what the user asked for.
    pub fn finish_download(&mut self, id: &str, result: Result<(), DownloadError>) {
        let Some(m) = find(id) else { return };
        self.downloading.remove(m.id);
        match result {
            Ok(()) => {
                let _ = self.state.update(|s| {
                    s.first_run_complete = true;
                    s.downloads.remove(m.id);
                });
                if self.pending_select == Some(m.id) {
                    self.pending_select = None;
                    self.apply_selection(m);
                }
            }
            Err(e) => {
                self.failed.insert(m.id, e);
                let kept = std::fs::metadata(part_path(&self.dir, m))
                    .map(|x| x.len())
                    .unwrap_or(0);
                let _ = self.state.update(|s| {
                    if kept > 0 {
                        s.downloads.insert(m.id.to_string(), kept);
                    } else {
                        s.downloads.remove(m.id);
                    }
                });
            }
        }
    }
}
