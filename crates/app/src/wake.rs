//! Lets the worker sleep until something happens instead of polling. Any
//! thread calls `notify()`; the worker calls `wait()` with a timeout only when
//! it has a deadline of its own. A notification is never lost: one that
//! arrives before `wait()` makes the next `wait()` return at once.
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

#[derive(Clone, Default)]
pub struct Wake(Arc<(Mutex<bool>, Condvar)>);

impl Wake {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn notify(&self) {
        let (flag, cv) = &*self.0;
        *flag.lock().unwrap_or_else(|e| e.into_inner()) = true;
        cv.notify_all();
    }

    /// A closure other threads can call to wake the worker.
    pub fn notifier(&self) -> Arc<dyn Fn() + Send + Sync> {
        let w = self.clone();
        Arc::new(move || w.notify())
    }

    /// Blocks until notified, or for at most `timeout` (`None` = until notified).
    /// Returns true if it was a notification.
    pub fn wait(&self, timeout: Option<Duration>) -> bool {
        let (flag, cv) = &*self.0;
        let mut g = flag.lock().unwrap_or_else(|e| e.into_inner());
        match timeout {
            None => {
                while !*g {
                    g = cv.wait(g).unwrap_or_else(|e| e.into_inner());
                }
            }
            Some(t) => {
                if !*g {
                    g = cv.wait_timeout(g, t).unwrap_or_else(|e| e.into_inner()).0;
                }
            }
        }
        std::mem::take(&mut *g)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn a_notification_before_the_wait_is_not_lost() {
        let w = Wake::new();
        w.notify();
        assert!(w.wait(None));
        assert!(!w.wait(Some(Duration::from_millis(5))), "consumed: the next wait times out");
    }

    #[test]
    fn another_thread_wakes_a_waiting_worker() {
        let w = Wake::new();
        let n = w.notifier();
        let t = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(30));
            n();
        });
        assert!(w.wait(None));
        t.join().unwrap();
    }

    #[test]
    fn a_timeout_returns_false_after_about_that_long() {
        let w = Wake::new();
        let t = Instant::now();
        assert!(!w.wait(Some(Duration::from_millis(40))));
        assert!(t.elapsed() >= Duration::from_millis(30));
    }
}
