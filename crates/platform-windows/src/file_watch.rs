//! Watches one file for changes (the settings file) with `ReadDirectoryChangesW`
//! on a thread that blocks in the kernel: no timers and no polling.
use std::path::Path;
use std::sync::Arc;

const FILE_LIST_DIRECTORY: u32 = 1;
const FILE_SHARE_ALL: u32 = 7;
const OPEN_EXISTING: u32 = 3;
const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
const FILE_NOTIFY_CHANGE_FILE_NAME: u32 = 0x1;
const FILE_NOTIFY_CHANGE_SIZE: u32 = 0x8;
const FILE_NOTIFY_CHANGE_LAST_WRITE: u32 = 0x10;
const INVALID_HANDLE_VALUE: isize = -1;

#[link(name = "kernel32")]
extern "system" {
    fn CreateFileW(
        name: *const u16,
        access: u32,
        share: u32,
        security: *const u8,
        disposition: u32,
        flags: u32,
        template: isize,
    ) -> isize;
    fn ReadDirectoryChangesW(
        dir: isize,
        buf: *mut u8,
        len: u32,
        subtree: i32,
        filter: u32,
        returned: *mut u32,
        overlapped: *mut u8,
        routine: *const u8,
    ) -> i32;
}

/// Names (UTF-16, lower-cased) reported in a `FILE_NOTIFY_INFORMATION` chain.
fn changed_names(buf: &[u8]) -> Vec<String> {
    let mut names = Vec::new();
    let mut at = 0usize;
    while at + 12 <= buf.len() {
        let next = u32::from_le_bytes(buf[at..at + 4].try_into().unwrap()) as usize;
        let len = u32::from_le_bytes(buf[at + 8..at + 12].try_into().unwrap()) as usize;
        let end = (at + 12 + len).min(buf.len());
        let units: Vec<u16> = buf[at + 12..end]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        names.push(String::from_utf16_lossy(&units).to_lowercase());
        if next == 0 {
            break;
        }
        at += next;
    }
    names
}

/// Calls `on_change` whenever `file_name` in `dir` is written, created, or
/// renamed over. Runs for the life of the process.
pub fn watch_file(
    dir: &Path,
    file_name: &str,
    on_change: Arc<dyn Fn() + Send + Sync>,
) -> std::io::Result<()> {
    let wide: Vec<u16> = dir
        .as_os_str()
        .to_string_lossy()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let handle = unsafe {
        CreateFileW(
            wide.as_ptr(),
            FILE_LIST_DIRECTORY,
            FILE_SHARE_ALL,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS,
            0,
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(std::io::Error::last_os_error());
    }
    let wanted = file_name.to_lowercase();
    std::thread::spawn(move || {
        let mut buf = vec![0u8; 16 * 1024];
        loop {
            let mut returned = 0u32;
            let ok = unsafe {
                ReadDirectoryChangesW(
                    handle,
                    buf.as_mut_ptr(),
                    buf.len() as u32,
                    0,
                    FILE_NOTIFY_CHANGE_FILE_NAME
                        | FILE_NOTIFY_CHANGE_SIZE
                        | FILE_NOTIFY_CHANGE_LAST_WRITE,
                    &mut returned,
                    std::ptr::null_mut(),
                    std::ptr::null(),
                )
            };
            if ok == 0 {
                return;
            }
            // returned == 0 means the buffer overflowed: assume the file changed.
            if returned == 0 || changed_names(&buf[..returned as usize]).contains(&wanted) {
                on_change();
            }
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;
    use std::time::Duration;

    #[test]
    fn parses_a_chain_of_notifications() {
        let name =
            |s: &str| -> Vec<u8> { s.encode_utf16().flat_map(|u| u.to_le_bytes()).collect() };
        let (a, b) = (name("Settings.toml"), name("other.tmp"));
        let first_len = (12 + a.len() + 3) & !3;
        let mut buf = Vec::new();
        buf.extend((first_len as u32).to_le_bytes());
        buf.extend(3u32.to_le_bytes());
        buf.extend((a.len() as u32).to_le_bytes());
        buf.extend(&a);
        buf.resize(first_len, 0);
        buf.extend(0u32.to_le_bytes());
        buf.extend(1u32.to_le_bytes());
        buf.extend((b.len() as u32).to_le_bytes());
        buf.extend(&b);
        assert_eq!(
            changed_names(&buf),
            vec!["settings.toml".to_string(), "other.tmp".to_string()]
        );
    }

    #[test]
    fn writing_the_watched_file_notifies_and_other_files_do_not() {
        let d = tempfile::tempdir().unwrap();
        let (tx, rx) = channel();
        watch_file(
            d.path(),
            "settings.toml",
            Arc::new(move || {
                let _ = tx.send(());
            }),
        )
        .unwrap();
        std::fs::write(d.path().join("unrelated.txt"), b"x").unwrap();
        assert!(
            rx.recv_timeout(Duration::from_millis(300)).is_err(),
            "other files are ignored"
        );
        std::fs::write(d.path().join("settings.toml"), b"x").unwrap();
        assert!(rx.recv_timeout(Duration::from_secs(3)).is_ok());
    }
}
