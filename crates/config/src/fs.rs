//! Filesystem seam so interrupted writes can be simulated in tests.
use std::io;
use std::path::Path;

pub trait Fs {
    fn read_to_string(&self, path: &Path) -> io::Result<Option<String>>;
    /// Write and flush a new file (truncating).
    fn write_new(&self, path: &Path, bytes: &[u8]) -> io::Result<()>;
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;
    fn copy(&self, from: &Path, to: &Path) -> io::Result<()>;
    fn exists(&self, path: &Path) -> bool;
    fn remove(&self, path: &Path) -> io::Result<()>;
    fn create_dir_all(&self, path: &Path) -> io::Result<()>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct StdFs;

impl Fs for StdFs {
    fn read_to_string(&self, path: &Path) -> io::Result<Option<String>> {
        match std::fs::read_to_string(path) {
            Ok(s) => Ok(Some(s)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }
    fn write_new(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        use std::io::Write;
        let mut f = std::fs::File::create(path)?;
        f.write_all(bytes)?;
        f.sync_all()
    }
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        std::fs::rename(from, to)
    }
    fn copy(&self, from: &Path, to: &Path) -> io::Result<()> {
        std::fs::copy(from, to).map(|_| ())
    }
    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }
    fn remove(&self, path: &Path) -> io::Result<()> {
        match std::fs::remove_file(path) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }
    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        std::fs::create_dir_all(path)
    }
}

/// Atomic replace: write a sibling temp file, then rename over the target.
/// An interruption at any point leaves the target either old or new, never partial.
pub fn atomic_write(fs: &dyn Fs, path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs.create_dir_all(dir)?;
    }
    let mut tmp_name = path.file_name().unwrap_or_default().to_os_string();
    tmp_name.push(".tmp");
    let tmp = path.with_file_name(tmp_name);
    let result = fs.write_new(&tmp, bytes).and_then(|_| fs.rename(&tmp, path));
    if result.is_err() {
        let _ = fs.remove(&tmp);
    }
    result
}
