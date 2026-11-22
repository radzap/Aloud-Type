//! Failure supervision: a panicked recognizer or audio worker is restarted
//! once, then a Listening or Typing fault is shown. No whole-process restart.
use crate::listening::FaultReason;
use std::panic::{catch_unwind, AssertUnwindSafe};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Worker {
    Recognizer,
    Audio,
}

/// The fault shown when a worker could not be kept alive. The audio worker
/// maps to a red Listening fault, the recognizer to an amber Typing fault.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerFault {
    Listening(FaultReason),
    Typing(&'static str),
}

impl WorkerFault {
    pub fn for_worker(w: Worker) -> Self {
        match w {
            Worker::Audio => WorkerFault::Listening(FaultReason::MicrophoneError),
            Worker::Recognizer => WorkerFault::Typing("Speech recognition stopped working"),
        }
    }
    pub fn message(self) -> &'static str {
        match self {
            WorkerFault::Listening(r) => r.message(),
            WorkerFault::Typing(m) => m,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Restart,
    Fault(WorkerFault),
}

/// Restart budget: one restart per worker for the life of the process.
#[derive(Debug, Default)]
pub struct Supervisor {
    restarted_recognizer: bool,
    restarted_audio: bool,
}

impl Supervisor {
    pub fn new() -> Self {
        Self::default()
    }

    /// A worker panicked (or failed to restart).
    pub fn on_failure(&mut self, w: Worker) -> Verdict {
        let used = match w {
            Worker::Recognizer => &mut self.restarted_recognizer,
            Worker::Audio => &mut self.restarted_audio,
        };
        if *used {
            Verdict::Fault(WorkerFault::for_worker(w))
        } else {
            *used = true;
            Verdict::Restart
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Report {
    /// The worker panicked and is being started again.
    Restarted(Worker),
    /// The restart budget is spent; show this fault.
    Faulted(Worker, WorkerFault),
}

/// Runs `body` (a worker loop that returns when asked to stop). A panic
/// restarts it once through the shared `supervisor`; a second panic reports a
/// fault and returns. Returns true when the body ended normally.
pub fn run_supervised(
    supervisor: &std::sync::Mutex<Supervisor>,
    worker: Worker,
    mut body: impl FnMut(),
    mut report: impl FnMut(Report),
) -> bool {
    loop {
        if catch_unwind(AssertUnwindSafe(&mut body)).is_ok() {
            return true;
        }
        let verdict = supervisor
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .on_failure(worker);
        match verdict {
            Verdict::Restart => report(Report::Restarted(worker)),
            Verdict::Fault(f) => {
                report(Report::Faulted(worker, f));
                return false;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    fn silent<R>(f: impl FnOnce() -> R) -> R {
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let r = f();
        std::panic::set_hook(prev);
        r
    }

    #[test]
    fn a_healthy_worker_runs_once_with_no_reports() {
        let sup = Mutex::new(Supervisor::new());
        let (mut runs, mut reports) = (0, vec![]);
        let ok = run_supervised(&sup, Worker::Audio, || runs += 1, |r| reports.push(r));
        assert!(ok && runs == 1 && reports.is_empty());
    }

    #[test]
    fn one_panic_restarts_the_worker_and_it_recovers() {
        let sup = Mutex::new(Supervisor::new());
        let (mut runs, mut reports) = (0, vec![]);
        let ok = silent(|| {
            run_supervised(
                &sup,
                Worker::Recognizer,
                || {
                    runs += 1;
                    if runs == 1 {
                        panic!("boom");
                    }
                },
                |r| reports.push(r),
            )
        });
        assert!(ok);
        assert_eq!(runs, 2);
        assert_eq!(reports, vec![Report::Restarted(Worker::Recognizer)]);
    }

    #[test]
    fn a_second_panic_shows_a_fault_instead_of_restarting_again() {
        let sup = Mutex::new(Supervisor::new());
        let (mut runs, mut reports) = (0, vec![]);
        let ok = silent(|| {
            run_supervised(
                &sup,
                Worker::Audio,
                || {
                    runs += 1;
                    panic!("boom")
                },
                |r| reports.push(r),
            )
        });
        assert!(!ok);
        assert_eq!(runs, 2);
        assert_eq!(
            reports,
            vec![
                Report::Restarted(Worker::Audio),
                Report::Faulted(
                    Worker::Audio,
                    WorkerFault::Listening(FaultReason::MicrophoneError)
                ),
            ]
        );
    }

    #[test]
    fn recognizer_fault_is_a_typing_fault_with_a_message() {
        let mut s = Supervisor::new();
        assert_eq!(s.on_failure(Worker::Recognizer), Verdict::Restart);
        let Verdict::Fault(f) = s.on_failure(Worker::Recognizer) else {
            panic!()
        };
        assert!(matches!(f, WorkerFault::Typing(_)));
        assert_eq!(f.message(), "Speech recognition stopped working");
    }

    #[test]
    fn budgets_are_per_worker() {
        let mut s = Supervisor::new();
        assert_eq!(s.on_failure(Worker::Audio), Verdict::Restart);
        assert_eq!(s.on_failure(Worker::Recognizer), Verdict::Restart);
    }
}
