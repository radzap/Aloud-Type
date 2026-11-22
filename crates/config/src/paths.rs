use std::path::{Path, PathBuf};

/// Locations under the per-user local application data folder (not roaming).
#[derive(Debug, Clone)]
pub struct AppPaths {
    pub app_dir: PathBuf,
}

impl AppPaths {
    pub fn under(local_app_data: &Path) -> Self {
        AppPaths {
            app_dir: local_app_data.join("AloudType"),
        }
    }
    pub fn from_env() -> Option<Self> {
        std::env::var_os("LOCALAPPDATA").map(|p| Self::under(Path::new(&p)))
    }
    pub fn settings_file(&self) -> PathBuf {
        self.app_dir.join("settings.toml")
    }
    pub fn state_file(&self) -> PathBuf {
        self.app_dir.join("state.toml")
    }
    pub fn models_dir(&self) -> PathBuf {
        self.app_dir.join("models")
    }
}
