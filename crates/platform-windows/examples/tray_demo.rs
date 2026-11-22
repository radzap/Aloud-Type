//! Manual check for the tray: startup notice, state cycling, single instance.
//! Run twice to see the second launch ask the first to open Settings.
#[cfg(windows)]
fn main() {
    use aloud_type_core::listening::Tray;
    use aloud_type_platform_windows::instance::{acquire, Launch};
    use aloud_type_platform_windows::tray::{run, TrayHandle};
    use aloud_type_platform_windows::tray_model::{startup_action, view, Command};
    use std::{sync::mpsc::channel, thread, time::Duration};

    let guard = match acquire("AloudTypeDemo").expect("instance") {
        Launch::Second => {
            println!("second launch: asked the first instance to open Settings");
            return;
        }
        Launch::First(g) => g,
    };
    let (tx, rx) = channel();
    let tx2 = tx.clone();
    guard.on_open_settings(move || {
        let _ = tx2.send(Command::OpenSettings);
    });
    let handle = TrayHandle::new();
    let h = handle.clone();
    thread::spawn(move || {
        while let Ok(c) = rx.recv() {
            println!("command: {c:?}");
        }
    });
    let secs: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(6);
    thread::spawn(move || {
        thread::sleep(Duration::from_secs(1));
        h.set_state(&Tray::Listening, false, false);
        thread::sleep(Duration::from_secs(1));
        h.set_state(&Tray::Fault("Microphone is blocked by Windows privacy settings"), false, true);
        thread::sleep(Duration::from_secs(secs));
        h.quit();
    });
    let keys = vec!["RightCtrl".to_string()];
    run(handle, tx, startup_action(true, &keys), view(&Tray::Idle, false)).expect("tray");
    println!("tray loop ended");
}
#[cfg(not(windows))]
fn main() {}
