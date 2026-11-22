//! Thread priority for the decode worker (spec: below normal while the hook
//! and audio threads stay at normal or above).

const THREAD_PRIORITY_BELOW_NORMAL: i32 = -1;

#[link(name = "kernel32")]
extern "system" {
    fn GetCurrentThread() -> isize;
    fn SetThreadPriority(thread: isize, priority: i32) -> i32;
    fn GetThreadPriority(thread: isize) -> i32;
}

/// Lowers the calling thread to below-normal priority. Returns whether it worked.
pub fn set_current_below_normal() -> bool {
    unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL) != 0 }
}

/// The calling thread's priority (0 is normal, -1 below normal).
pub fn current() -> i32 {
    unsafe { GetThreadPriority(GetCurrentThread()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_thread_can_be_lowered_to_below_normal_without_touching_others() {
        let main_priority = current();
        let lowered = std::thread::spawn(|| {
            assert!(set_current_below_normal());
            current()
        })
        .join()
        .unwrap();
        assert_eq!(lowered, -1);
        assert_eq!(current(), main_priority);
        assert!(main_priority >= 0, "the test thread stays at normal or above");
    }
}
