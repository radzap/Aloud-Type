//! Download behaviour against a local test HTTP server (never the real network).
use aloud_type_models::{model_path, ModelInfo};
use aloud_type_settings_ui::download::{download_model, part_path, DownloadError, UreqHttp};
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

const LEN: usize = 100_000;
const GOOD_SHA: &str = "cd2df694e424bc7968cc37f47751019e5ca0cd1bdf2e479ea537c3a1c32ee1aa";

fn content() -> Vec<u8> {
    (0..LEN).map(|i| (i % 251) as u8).collect()
}

#[derive(Clone, Copy)]
enum Mode {
    Full,
    /// Advertise the full length but close the connection after `n` body bytes.
    CutAfter(usize),
    /// Always answer 200 with the whole file, even to Range requests.
    IgnoreRange,
    /// Serve the body with one byte flipped.
    Corrupt,
}

struct Server {
    url: String,
    /// Range start requested by each request, in order.
    ranges: Arc<Mutex<Vec<u64>>>,
}

fn serve(script: Vec<Mode>) -> Server {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/ggml-test.bin", l.local_addr().unwrap());
    let ranges = Arc::new(Mutex::new(Vec::new()));
    let r2 = ranges.clone();
    thread::spawn(move || {
        for (i, s) in l.incoming().enumerate() {
            let Ok(s) = s else { break };
            let mode = script.get(i).copied().unwrap_or(Mode::Full);
            let _ = handle(s, mode, &r2);
        }
    });
    Server { url, ranges }
}

fn handle(mut s: TcpStream, mode: Mode, ranges: &Mutex<Vec<u64>>) -> std::io::Result<()> {
    let mut rd = BufReader::new(s.try_clone()?);
    let mut from = 0u64;
    loop {
        let mut line = String::new();
        rd.read_line(&mut line)?;
        if line.trim().is_empty() {
            break;
        }
        if let Some(v) = line.to_ascii_lowercase().strip_prefix("range: bytes=") {
            from = v.trim().trim_end_matches('-').parse().unwrap();
        }
    }
    ranges.lock().unwrap().push(from);
    let mut data = content();
    if let Mode::Corrupt = mode {
        data[500] ^= 0xff;
    }
    let (status, body, extra) = match mode {
        Mode::IgnoreRange => ("200 OK", &data[..], String::new()),
        _ if from > 0 => (
            "206 Partial Content",
            &data[from as usize..],
            format!("Content-Range: bytes {from}-{}/{LEN}\r\n", LEN - 1),
        ),
        _ => ("200 OK", &data[..], String::new()),
    };
    write!(s, "HTTP/1.1 {status}\r\nContent-Length: {}\r\n{extra}Connection: close\r\n\r\n", body.len())?;
    let n = match mode {
        Mode::CutAfter(n) => n.min(body.len()),
        _ => body.len(),
    };
    s.write_all(&body[..n])?;
    s.flush()
}

fn model(url: &str, sha: &'static str) -> ModelInfo {
    ModelInfo {
        id: "test",
        label: "Test",
        file: "ggml-test.bin",
        url: Box::leak(url.to_string().into_boxed_str()),
        size: LEN as u64,
        sha256: sha,
        english_only: false,
    }
}

fn run(m: &ModelInfo, dir: &std::path::Path) -> (Result<(), DownloadError>, Vec<(u64, u64)>) {
    let mut seen = Vec::new();
    let r = download_model(m, dir, &mut UreqHttp::default(), &mut |d, t| seen.push((d, t)));
    (r, seen)
}

#[test]
fn a_download_is_verified_and_moved_into_the_models_folder_with_progress() {
    let srv = serve(vec![Mode::Full]);
    let dir = tempfile::tempdir().unwrap();
    let m = model(&srv.url, GOOD_SHA);
    let (r, seen) = run(&m, dir.path());
    assert_eq!(r, Ok(()));
    assert_eq!(std::fs::read(model_path(dir.path(), &m)).unwrap(), content());
    assert!(!part_path(dir.path(), &m).exists());
    assert_eq!(*seen.last().unwrap(), (LEN as u64, LEN as u64));
    assert!(seen.windows(2).all(|w| w[0].0 <= w[1].0));
}

#[test]
fn an_interrupted_download_keeps_the_partial_file_and_the_next_try_resumes() {
    let srv = serve(vec![Mode::CutAfter(40_000), Mode::Full]);
    let dir = tempfile::tempdir().unwrap();
    let m = model(&srv.url, GOOD_SHA);
    let (r, _) = run(&m, dir.path());
    assert_eq!(r, Err(DownloadError::Interrupted));
    let kept = std::fs::metadata(part_path(dir.path(), &m)).unwrap().len();
    assert_eq!(kept, 40_000);
    assert!(!model_path(dir.path(), &m).exists(), "partial file must not be loadable");

    let (r, seen) = run(&m, dir.path());
    assert_eq!(r, Ok(()));
    assert_eq!(*srv.ranges.lock().unwrap(), vec![0, 40_000]);
    assert_eq!(seen.first().unwrap().0, 40_000, "progress continues from the partial file");
    assert_eq!(std::fs::read(model_path(dir.path(), &m)).unwrap(), content());
}

#[test]
fn a_server_that_ignores_range_restarts_from_zero_without_corruption() {
    let srv = serve(vec![Mode::IgnoreRange]);
    let dir = tempfile::tempdir().unwrap();
    let m = model(&srv.url, GOOD_SHA);
    std::fs::write(part_path(dir.path(), &m), &content()[..30_000]).unwrap();
    let (r, _) = run(&m, dir.path());
    assert_eq!(r, Ok(()));
    assert_eq!(std::fs::read(model_path(dir.path(), &m)).unwrap(), content());
}

#[test]
fn a_corrupted_download_is_retried_once_automatically_and_can_succeed() {
    let srv = serve(vec![Mode::Corrupt, Mode::Full]);
    let dir = tempfile::tempdir().unwrap();
    let m = model(&srv.url, GOOD_SHA);
    let (r, _) = run(&m, dir.path());
    assert_eq!(r, Ok(()));
    assert_eq!(*srv.ranges.lock().unwrap(), vec![0, 0], "the retry starts from scratch");
}

#[test]
fn a_download_that_keeps_failing_verification_is_rejected_and_deleted() {
    let srv = serve(vec![Mode::Corrupt, Mode::Corrupt, Mode::Full]);
    let dir = tempfile::tempdir().unwrap();
    let m = model(&srv.url, GOOD_SHA);
    let (r, _) = run(&m, dir.path());
    assert_eq!(r, Err(DownloadError::FailedVerification));
    assert_eq!(srv.ranges.lock().unwrap().len(), 2, "exactly one automatic retry");
    assert!(!model_path(dir.path(), &m).exists());
    assert!(!part_path(dir.path(), &m).exists());
}

#[test]
fn an_unreachable_server_is_reported_as_no_internet() {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/x.bin", l.local_addr().unwrap());
    drop(l);
    let dir = tempfile::tempdir().unwrap();
    let (r, _) = run(&model(&url, GOOD_SHA), dir.path());
    assert_eq!(r, Err(DownloadError::NoInternet));
}

#[test]
fn the_user_facing_messages_match_the_spec() {
    assert_eq!(DownloadError::NoInternet.message(), "No internet connection");
    assert_eq!(DownloadError::Interrupted.message(), "Download interrupted");
    assert_eq!(DownloadError::FailedVerification.message(), "File failed verification");
}
