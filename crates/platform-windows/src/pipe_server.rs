//! Resident side of the Settings pipe: a thread serving one client at a time.
//! Protocol and per-connection rules live in `aloud_type_core::ipc`.
use aloud_type_core::ipc::{Command, Connection};
use std::io;

type Handle = isize;
const INVALID_HANDLE_VALUE: Handle = -1;
const PIPE_ACCESS_DUPLEX: u32 = 3;
const PIPE_TYPE_BYTE_WAIT_REJECT_REMOTE: u32 = 0x8;
const ERROR_PIPE_CONNECTED: i32 = 535;

#[link(name = "kernel32")]
extern "system" {
    fn CreateNamedPipeW(name: *const u16, open: u32, mode: u32, max: u32, out: u32, inp: u32, timeout: u32, sa: *const u8) -> Handle;
    fn ConnectNamedPipe(h: Handle, overlapped: *mut u8) -> i32;
    fn DisconnectNamedPipe(h: Handle) -> i32;
    fn ReadFile(h: Handle, buf: *mut u8, len: u32, read: *mut u32, overlapped: *mut u8) -> i32;
    fn WriteFile(h: Handle, buf: *const u8, len: u32, written: *mut u32, overlapped: *mut u8) -> i32;
    fn CloseHandle(h: Handle) -> i32;
}

/// Runs for the life of the process (the thread blocks in the kernel while idle).
pub struct PipeServer;

impl PipeServer {
    /// `on_command` is called on the server thread for each valid command, and
    /// with `ResumeKeys` when a client disconnects while keys are suspended.
    pub fn start(name: &str, on_command: impl Fn(Command) + Send + 'static) -> io::Result<PipeServer> {
        let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
        // Create the first instance here so a failure is reported to the caller.
        let first = create(&wide)?;
        std::thread::spawn(move || {
            let mut pipe = first;
            loop {
                serve_client(pipe, &on_command);
                unsafe { CloseHandle(pipe) };
                pipe = match create(&wide) {
                    Ok(p) => p,
                    Err(_) => return,
                };
            }
        });
        Ok(PipeServer)
    }
}

fn create(wide: &[u16]) -> io::Result<Handle> {
    let h = unsafe {
        CreateNamedPipeW(wide.as_ptr(), PIPE_ACCESS_DUPLEX, PIPE_TYPE_BYTE_WAIT_REJECT_REMOTE, 1, 4096, 4096, 0, std::ptr::null())
    };
    if h == INVALID_HANDLE_VALUE {
        Err(io::Error::last_os_error())
    } else {
        Ok(h)
    }
}

fn serve_client(pipe: Handle, on_command: &impl Fn(Command)) {
    let connected = unsafe { ConnectNamedPipe(pipe, std::ptr::null_mut()) } != 0
        || io::Error::last_os_error().raw_os_error() == Some(ERROR_PIPE_CONNECTED);
    if !connected {
        return;
    }
    let mut conn = Connection::default();
    let mut pending = Vec::<u8>::new();
    let mut buf = [0u8; 512];
    'client: loop {
        let mut n = 0u32;
        let ok = unsafe { ReadFile(pipe, buf.as_mut_ptr(), buf.len() as u32, &mut n, std::ptr::null_mut()) };
        if ok == 0 || n == 0 {
            break;
        }
        pending.extend_from_slice(&buf[..n as usize]);
        if pending.len() > 8192 {
            break; // not our protocol
        }
        while let Some(i) = pending.iter().position(|b| *b == b'\n') {
            let line: Vec<u8> = pending.drain(..=i).collect();
            let (cmd, reply) = conn.handle(&String::from_utf8_lossy(&line));
            if let Some(c) = cmd {
                on_command(c);
            }
            let out = format!("{reply}\n");
            let mut w = 0u32;
            if unsafe { WriteFile(pipe, out.as_ptr(), out.len() as u32, &mut w, std::ptr::null_mut()) } == 0 {
                break 'client;
            }
        }
    }
    if let Some(c) = conn.closed() {
        on_command(c);
    }
    unsafe { DisconnectNamedPipe(pipe) };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::sync::mpsc::channel;
    use std::time::Duration;

    fn name(tag: &str) -> String {
        format!(r"\\.\pipe\AloudType-test-{}-{tag}", std::process::id())
    }

    fn request(f: &mut std::fs::File, line: &str) -> String {
        writeln!(f, "{line}").unwrap();
        let mut r = String::new();
        let mut br = BufReader::new(&*f);
        br.read_line(&mut r).unwrap();
        r
    }

    #[test]
    fn commands_are_delivered_and_acknowledged_over_a_real_pipe() {
        let n = name("cmds");
        let (tx, rx) = channel();
        PipeServer::start(&n, move |c| tx.send(c).unwrap()).unwrap_or_else(|e| panic!("{n}: {e}"));
        let mut f = std::fs::OpenOptions::new().read(true).write(true).open(&n).unwrap();
        assert_eq!(request(&mut f, "reload-settings"), "ok\n");
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), Command::ReloadSettings);
        assert_eq!(request(&mut f, "nonsense"), "error unknown-command\n");
    }

    #[test]
    fn dropping_the_client_while_suspended_resumes_the_keys_and_serves_the_next_client() {
        let n = name("drop");
        let (tx, rx) = channel();
        PipeServer::start(&n, move |c| tx.send(c).unwrap()).unwrap_or_else(|e| panic!("{n}: {e}"));
        {
            let mut f = std::fs::OpenOptions::new().read(true).write(true).open(&n).unwrap();
            assert_eq!(request(&mut f, "suspend-keys"), "ok\n");
            assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), Command::SuspendKeys);
        }
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), Command::ResumeKeys);
        let mut second = loop {
            match std::fs::OpenOptions::new().read(true).write(true).open(&n) {
                Ok(f) => break f,
                Err(_) => std::thread::sleep(Duration::from_millis(20)),
            }
        };
        assert_eq!(request(&mut second, "resume-keys"), "ok\n");
    }
}
