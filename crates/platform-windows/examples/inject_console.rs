//! Manual check: after 5 s, types a scripted phrase into the focused window.
//! Run: cargo run -p aloud-type-platform-windows --example inject_console
//! Focus Notepad / a browser field / a chat app (or an admin window to see the
//! elevated refusal) during the countdown.
use aloud_type_platform_windows::inject::{check_can_start, InjectingSink};
use aloud_type_platform_windows::inject_win::{ForegroundElevation, SendInputBackend};
use aloud_type_core::streaming::TextSink;
use std::time::Duration;

fn main() {
    println!("Focus the target window; typing in 5 s...");
    std::thread::sleep(Duration::from_secs(5));
    if let Err(f) = check_can_start(&mut ForegroundElevation) {
        println!("amber: {}", f.message());
        return;
    }
    let (mut sink, fault) = InjectingSink::new(SendInputBackend);
    for w in ["Hello", " world,", " na\u{ef}ve", " caf\u{e9}", " \u{65e5}\u{672c}\u{8a9e}", " \u{1F600}\u{1F680}", " done."] {
        sink.type_text(w);
    }
    let f = *fault.lock().unwrap();
    match f {
        Some(f) => println!("amber: {}", f.message()),
        None => println!("typed ok"),
    }
}
