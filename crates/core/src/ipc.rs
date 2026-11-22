//! Settings <-> resident process protocol (line based, over the named pipe).
//! The resident process is the server; this module is the pure, testable part.

pub const PIPE_NAME: &str = r"\\.\pipe\AloudType";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    SuspendKeys,
    ResumeKeys,
    ReloadSettings,
}

impl Command {
    pub fn encode(self) -> &'static str {
        match self {
            Command::SuspendKeys => "suspend-keys",
            Command::ResumeKeys => "resume-keys",
            Command::ReloadSettings => "reload-settings",
        }
    }
    pub fn decode(line: &str) -> Option<Self> {
        match line.trim() {
            "suspend-keys" => Some(Command::SuspendKeys),
            "resume-keys" => Some(Command::ResumeKeys),
            "reload-settings" => Some(Command::ReloadSettings),
            _ => None,
        }
    }
}

/// Server-side state for one client connection. Guarantees Dictation keys are
/// resumed when a client that suspended them goes away (Settings killed or
/// crashed mid-capture).
#[derive(Default)]
pub struct Connection {
    suspended: bool,
}

impl Connection {
    /// Handle one request line: the command to apply (if valid) and the reply line.
    pub fn handle(&mut self, line: &str) -> (Option<Command>, &'static str) {
        match Command::decode(line) {
            Some(c) => {
                match c {
                    Command::SuspendKeys => self.suspended = true,
                    Command::ResumeKeys => self.suspended = false,
                    Command::ReloadSettings => {}
                }
                (Some(c), "ok")
            }
            None => (None, "error unknown-command"),
        }
    }

    /// The client disconnected. Returns `ResumeKeys` when it left keys suspended.
    pub fn closed(&mut self) -> Option<Command> {
        std::mem::take(&mut self.suspended).then_some(Command::ResumeKeys)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_round_trip() {
        for c in [Command::SuspendKeys, Command::ResumeKeys, Command::ReloadSettings] {
            assert_eq!(Command::decode(c.encode()), Some(c));
        }
        assert_eq!(Command::decode("bogus"), None);
    }

    #[test]
    fn valid_commands_are_applied_and_acknowledged() {
        let mut c = Connection::default();
        assert_eq!(c.handle("suspend-keys\n"), (Some(Command::SuspendKeys), "ok"));
        assert_eq!(c.handle("reload-settings"), (Some(Command::ReloadSettings), "ok"));
    }

    #[test]
    fn unknown_commands_are_refused_without_effect() {
        let mut c = Connection::default();
        assert_eq!(c.handle("format-disk").0, None);
        assert_eq!(c.closed(), None);
    }

    #[test]
    fn closing_while_suspended_resumes_the_keys() {
        let mut c = Connection::default();
        c.handle("suspend-keys");
        assert_eq!(c.closed(), Some(Command::ResumeKeys));
        assert_eq!(c.closed(), None);
    }

    #[test]
    fn closing_after_resume_does_nothing() {
        let mut c = Connection::default();
        c.handle("suspend-keys");
        c.handle("resume-keys");
        assert_eq!(c.closed(), None);
    }
}
