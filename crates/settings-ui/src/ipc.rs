//! Client side of the resident process's named pipe (spec "Settings to resident
//! communication"). Line protocol: one command per line, one reply line.
//! The resident process is the server and must also resume Dictation keys if
//! the pipe closes while suspended (Settings may be killed mid-capture).
use std::io::{self, BufRead, BufReader, Write};

pub use aloud_type_core::ipc::{Command, PIPE_NAME};

/// A connection (or the lack of one) to the resident process.
pub trait Transport {
    /// Send one command and wait for the reply line. Err when the resident
    /// process is not reachable.
    fn request(&mut self, line: &str) -> io::Result<String>;
}

/// Best-effort link: a missing resident process is not an error for Settings.
pub struct ResidentLink<T: Transport> {
    transport: T,
}

impl<T: Transport> ResidentLink<T> {
    pub fn new(transport: T) -> Self {
        Self { transport }
    }

    /// Returns whether the resident process acknowledged the command.
    pub fn send(&mut self, cmd: Command) -> bool {
        matches!(self.transport.request(cmd.encode()), Ok(r) if r.trim() == "ok")
    }
}

/// Real named-pipe client: connects per request, so it works whether or not
/// the resident process was started before Settings.
pub struct PipeClient {
    name: String,
}

impl PipeClient {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }
}

impl Default for PipeClient {
    fn default() -> Self {
        Self::new(PIPE_NAME)
    }
}

impl Transport for PipeClient {
    fn request(&mut self, line: &str) -> io::Result<String> {
        let mut f = std::fs::OpenOptions::new().read(true).write(true).open(&self.name)?;
        writeln!(f, "{line}")?;
        let mut reply = String::new();
        BufReader::new(f).read_line(&mut reply)?;
        Ok(reply)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_pipe_is_reported_as_not_acknowledged_not_as_a_panic() {
        let mut link = ResidentLink::new(PipeClient::new(r"\\.\pipe\AloudType-test-no-such-pipe"));
        assert!(!link.send(Command::SuspendKeys));
    }
}
