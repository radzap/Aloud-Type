//! Model downloads, owned by the Settings process (the resident process never
//! contains an HTTP client). Resumable `.part` files, SHA-256 verification,
//! one automatic retry after a corrupted download, then an atomic move into
//! the models folder so a half-written or unverified file is never loadable.
use aloud_type_models::{model_path, sha256_file, ModelInfo};
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

/// The three user-facing failures (spec "Model downloads and failures").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadError {
    NoInternet,
    Interrupted,
    FailedVerification,
}

impl DownloadError {
    pub fn message(self) -> &'static str {
        match self {
            DownloadError::NoInternet => "No internet connection",
            DownloadError::Interrupted => "Download interrupted",
            DownloadError::FailedVerification => "File failed verification",
        }
    }
}

#[derive(Debug)]
pub enum HttpError {
    /// Could not reach the server at all.
    Offline,
    /// The server answered with a non-success status.
    Status(u16),
}

pub struct Response {
    /// True when the server honoured the Range request (206): the body
    /// continues the partial file. False means the body is the whole file.
    pub resumed: bool,
    pub body: Box<dyn Read>,
}

pub trait Http {
    /// GET `url`, asking for bytes from offset `from` onwards when `from > 0`.
    fn get(&mut self, url: &str, from: u64) -> Result<Response, HttpError>;
}

/// The real HTTP client (ureq with the operating system's TLS).
pub struct UreqHttp {
    agent: ureq::Agent,
}

impl Default for UreqHttp {
    fn default() -> Self {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .tls_config(
                ureq::tls::TlsConfig::builder()
                    .provider(ureq::tls::TlsProvider::NativeTls)
                    .build(),
            )
            .timeout_connect(Some(std::time::Duration::from_secs(15)))
            .timeout_recv_body(Some(std::time::Duration::from_secs(30)))
            .build()
            .into();
        Self { agent }
    }
}

impl Http for UreqHttp {
    fn get(&mut self, url: &str, from: u64) -> Result<Response, HttpError> {
        let mut req = self.agent.get(url);
        if from > 0 {
            req = req.header("Range", format!("bytes={from}-"));
        }
        let resp = req.call().map_err(|_| HttpError::Offline)?;
        let status = resp.status().as_u16();
        match status {
            200 | 206 => Ok(Response {
                resumed: status == 206,
                body: Box::new(resp.into_body().into_reader()),
            }),
            s => Err(HttpError::Status(s)),
        }
    }
}

pub fn part_path(dir: &Path, m: &ModelInfo) -> PathBuf {
    dir.join(format!("{}.part", m.file))
}

/// Downloads `m` into `dir`. `progress(done, total)` is called as bytes arrive
/// (`done` includes bytes from an earlier attempt). On success the verified
/// file is at `model_path(dir, m)`.
pub fn download_model(
    m: &ModelInfo,
    dir: &Path,
    http: &mut dyn Http,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<(), DownloadError> {
    fs::create_dir_all(dir).map_err(|_| DownloadError::Interrupted)?;
    let part = part_path(dir, m);
    // A checksum mismatch deletes the file and retries once automatically.
    for attempt in 0..2 {
        fetch(m, &part, http, progress)?;
        if sha256_file(&part)
            .map(|h| h.eq_ignore_ascii_case(m.sha256))
            .unwrap_or(false)
        {
            fs::rename(&part, model_path(dir, m)).map_err(|_| DownloadError::Interrupted)?;
            return Ok(());
        }
        let _ = fs::remove_file(&part);
        if attempt == 1 {
            break;
        }
    }
    Err(DownloadError::FailedVerification)
}

/// Brings the `.part` file to exactly `m.size` bytes, resuming when possible.
fn fetch(
    m: &ModelInfo,
    part: &Path,
    http: &mut dyn Http,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<(), DownloadError> {
    let mut have = fs::metadata(part).map(|x| x.len()).unwrap_or(0);
    if have > m.size {
        let _ = fs::remove_file(part);
        have = 0;
    }
    if have == m.size {
        return Ok(());
    }
    let resp = match http.get(m.url, have) {
        Ok(r) => r,
        Err(HttpError::Offline) => return Err(DownloadError::NoInternet),
        Err(HttpError::Status(416)) => {
            // Partial file no longer matches the server: start over next time.
            let _ = fs::remove_file(part);
            return Err(DownloadError::Interrupted);
        }
        Err(HttpError::Status(_)) => return Err(DownloadError::Interrupted),
    };
    let mut out = if resp.resumed && have > 0 {
        fs::OpenOptions::new().append(true).open(part)
    } else {
        have = 0;
        fs::File::create(part)
    }
    .map_err(|_| DownloadError::Interrupted)?;
    let mut body = resp.body;
    let mut buf = vec![0u8; 64 * 1024];
    progress(have, m.size);
    loop {
        match body.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                out.write_all(&buf[..n])
                    .map_err(|_| DownloadError::Interrupted)?;
                have += n as u64;
                progress(have.min(m.size), m.size);
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => return Err(DownloadError::Interrupted),
        }
    }
    drop(out);
    if have == m.size {
        Ok(())
    } else if have > m.size {
        // More bytes than expected: certainly not this model.
        Ok(())
    } else {
        Err(DownloadError::Interrupted)
    }
}
