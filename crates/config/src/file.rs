//! Generic versioned, hand-editable TOML file with a single typed view.
use crate::fs::{atomic_write, Fs, StdFs};
use std::fmt;
use std::path::{Path, PathBuf};
use toml_edit::DocumentMut;

/// How a typed config maps onto a TOML document.
pub trait Schema: Sized + Default + Clone {
    /// Current version of the file format written by this app.
    const VERSION: u32;
    /// Upgrade `doc` in place from an older version (called once per file read).
    fn migrate(doc: &mut DocumentMut, from: u32);
    /// Read the typed value; missing keys use defaults, bad values are errors.
    fn read(doc: &DocumentMut) -> Result<Self, String>;
    /// Write known keys into `doc`, leaving every other key untouched.
    fn write(&self, doc: &mut DocumentMut);
}

#[derive(Debug)]
pub enum ConfigError {
    Io(std::io::Error),
    /// The file was written by a newer app version and is never overwritten.
    NewerVersion { file: u32, app: u32 },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::Io(e) => write!(f, "{e}"),
            ConfigError::NewerVersion { file, app } => {
                write!(f, "file version {file} is newer than this app ({app}); left untouched")
            }
        }
    }
}
impl std::error::Error for ConfigError {}
impl From<std::io::Error> for ConfigError {
    fn from(e: std::io::Error) -> Self {
        ConfigError::Io(e)
    }
}

/// Result of (re)reading the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadStatus {
    /// No file yet; defaults in use.
    Missing,
    Loaded,
    /// Older file was migrated in memory (written on next save).
    Migrated { from: u32 },
    /// Newer file: values read best-effort, file is read-only, backup kept.
    ReadOnlyNewer { file_version: u32 },
    /// Invalid file; the last good value is kept (defaults on first load).
    Invalid(String),
}

pub struct ConfigFile<T: Schema, F: Fs = StdFs> {
    fs: F,
    path: PathBuf,
    value: T,
    doc: DocumentMut,
    status: LoadStatus,
    read_only: bool,
}

fn backup_path(path: &Path, suffix: &str) -> PathBuf {
    let mut n = path.file_name().unwrap_or_default().to_os_string();
    n.push(suffix);
    path.with_file_name(n)
}

impl<T: Schema> ConfigFile<T, StdFs> {
    pub fn open(path: impl Into<PathBuf>) -> Self {
        Self::open_with(StdFs, path)
    }
}

impl<T: Schema, F: Fs> ConfigFile<T, F> {
    pub fn open_with(fs: F, path: impl Into<PathBuf>) -> Self {
        let mut f = ConfigFile {
            fs,
            path: path.into(),
            value: T::default(),
            doc: DocumentMut::new(),
            status: LoadStatus::Missing,
            read_only: false,
        };
        f.reload();
        f
    }

    pub fn value(&self) -> &T {
        &self.value
    }
    pub fn status(&self) -> &LoadStatus {
        &self.status
    }
    pub fn is_read_only(&self) -> bool {
        self.read_only
    }
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Re-read the file. An invalid file keeps the last good value.
    pub fn reload(&mut self) -> &LoadStatus {
        self.status = match self.read_disk() {
            Ok(s) => s,
            Err(e) => LoadStatus::Invalid(e),
        };
        &self.status
    }

    fn read_disk(&mut self) -> Result<LoadStatus, String> {
        let text = self.fs.read_to_string(&self.path).map_err(|e| e.to_string())?;
        let Some(text) = text else {
            self.read_only = false;
            self.doc = DocumentMut::new();
            return Ok(LoadStatus::Missing);
        };
        let mut doc: DocumentMut = text.parse().map_err(|e: toml_edit::TomlError| e.to_string())?;
        let version = match doc.get("version") {
            None => 0,
            Some(v) => v
                .as_integer()
                .filter(|n| *n >= 0)
                .ok_or("`version` must be a non-negative integer")? as u32,
        };
        if version > T::VERSION {
            // Never overwrite: read-only, keep a backup, read what we understand.
            let bak = backup_path(&self.path, &format!(".v{version}.bak"));
            if !self.fs.exists(&bak) {
                let _ = self.fs.copy(&self.path, &bak);
            }
            self.read_only = true;
            if let Ok(v) = T::read(&doc) {
                self.value = v;
            }
            self.doc = doc;
            return Ok(LoadStatus::ReadOnlyNewer { file_version: version });
        }
        let migrated = version < T::VERSION && version != 0;
        if version < T::VERSION {
            T::migrate(&mut doc, version);
        }
        let value = T::read(&doc)?;
        self.value = value;
        self.doc = doc;
        self.read_only = false;
        Ok(if migrated { LoadStatus::Migrated { from: version } } else { LoadStatus::Loaded })
    }

    /// Single-writer update: re-read the file, apply `f`, write atomically.
    /// Keys not known to the schema (including hidden values) are preserved.
    pub fn update(&mut self, f: impl FnOnce(&mut T)) -> Result<(), ConfigError> {
        self.reload();
        if let LoadStatus::ReadOnlyNewer { file_version } = self.status {
            return Err(ConfigError::NewerVersion { file: file_version, app: T::VERSION });
        }
        if matches!(self.status, LoadStatus::Invalid(_)) && self.fs.exists(&self.path) {
            // Preserve the user's broken hand edit before replacing it.
            let bak = backup_path(&self.path, ".invalid.bak");
            self.fs.copy(&self.path, &bak)?;
            self.doc = DocumentMut::new();
        }
        let mut next = self.value.clone();
        f(&mut next);
        let mut doc = self.doc.clone();
        next.write(&mut doc);
        doc["version"] = toml_edit::value(T::VERSION as i64);
        atomic_write(&self.fs, &self.path, doc.to_string().as_bytes())?;
        self.value = next;
        self.doc = doc;
        self.status = LoadStatus::Loaded;
        Ok(())
    }
}

// Small helpers for Schema implementations.
pub(crate) mod h {
    use toml_edit::{value, Array, DocumentMut, Item, Table};

    pub fn get_str(doc: &DocumentMut, k: &str) -> Result<Option<String>, String> {
        match doc.get(k) {
            None => Ok(None),
            Some(v) => v.as_str().map(|s| Some(s.to_string())).ok_or(format!("`{k}` must be a string")),
        }
    }
    pub fn get_bool(doc: &DocumentMut, k: &str) -> Result<Option<bool>, String> {
        match doc.get(k) {
            None => Ok(None),
            Some(v) => v.as_bool().map(Some).ok_or(format!("`{k}` must be true or false")),
        }
    }
    pub fn get_int(doc: &DocumentMut, k: &str) -> Result<Option<i64>, String> {
        match doc.get(k) {
            None => Ok(None),
            Some(v) => v.as_integer().map(Some).ok_or(format!("`{k}` must be an integer")),
        }
    }
    pub fn get_str_array(doc: &DocumentMut, k: &str) -> Result<Option<Vec<String>>, String> {
        match doc.get(k) {
            None => Ok(None),
            Some(v) => {
                let a = v.as_array().ok_or(format!("`{k}` must be a list of strings"))?;
                a.iter()
                    .map(|x| x.as_str().map(String::from).ok_or(format!("`{k}` must be a list of strings")))
                    .collect::<Result<Vec<_>, _>>()
                    .map(Some)
            }
        }
    }
    pub fn set_str(doc: &mut DocumentMut, k: &str, v: &str) {
        if doc.get(k).and_then(Item::as_str) != Some(v) {
            doc[k] = value(v);
        }
    }
    pub fn set_bool(doc: &mut DocumentMut, k: &str, v: bool) {
        if doc.get(k).and_then(Item::as_bool) != Some(v) {
            doc[k] = value(v);
        }
    }
    pub fn set_int(doc: &mut DocumentMut, k: &str, v: i64) {
        if doc.get(k).and_then(Item::as_integer) != Some(v) {
            doc[k] = value(v);
        }
    }
    pub fn set_str_array(doc: &mut DocumentMut, k: &str, v: &[String]) {
        if get_str_array(doc, k).ok().flatten().as_deref() != Some(v) {
            let mut a = Array::new();
            for s in v {
                a.push(s.as_str());
            }
            doc[k] = value(a);
        }
    }
    pub fn remove(doc: &mut DocumentMut, k: &str) {
        doc.remove(k);
    }
    pub fn table<'a>(doc: &'a mut DocumentMut, k: &str) -> &'a mut Table {
        if !doc.get(k).map(Item::is_table).unwrap_or(false) {
            doc[k] = Item::Table(Table::new());
        }
        doc[k].as_table_mut().unwrap()
    }
}
