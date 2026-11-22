//! Win32 backend for text injection: `SendInput` with `KEYEVENTF_UNICODE`
//! (never the clipboard), events tagged with `INJECT_TAG` so the app's own
//! hook ignores them; and the foreground-elevation probe.
use crate::classify::INJECT_TAG;
use crate::inject::{ElevationProbe, InputBackend, Key, KeyEv};
use crate::win32::{GetForegroundWindow, GetWindowThreadProcessId};

const INPUT_KEYBOARD: u32 = 1;
const KEYEVENTF_KEYUP: u32 = 2;
const KEYEVENTF_UNICODE: u32 = 4;
const VK_RETURN: u16 = 0x0D;
const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
const TOKEN_QUERY: u32 = 8;
const TOKEN_ELEVATION: i32 = 20;

#[repr(C)]
struct KeybdInput {
    vk: u16,
    scan: u16,
    flags: u32,
    time: u32,
    extra_info: usize,
}
#[repr(C)]
struct Input {
    kind: u32,
    ki: KeybdInput,
    _pad: [usize; 1], // INPUT's union is sized by MOUSEINPUT
}

#[link(name = "user32")]
extern "system" {
    fn SendInput(n: u32, inputs: *const Input, size: i32) -> u32;
}
#[link(name = "kernel32")]
extern "system" {
    fn OpenProcess(access: u32, inherit: i32, pid: u32) -> isize;
    fn CloseHandle(h: isize) -> i32;
    fn GetCurrentProcess() -> isize;
}
#[link(name = "advapi32")]
extern "system" {
    fn OpenProcessToken(process: isize, access: u32, token: *mut isize) -> i32;
    fn GetTokenInformation(
        token: isize,
        class: i32,
        info: *mut u32,
        len: u32,
        ret: *mut u32,
    ) -> i32;
}

fn to_input(e: &KeyEv) -> Input {
    let up = if e.up { KEYEVENTF_KEYUP } else { 0 };
    let ki = match e.key {
        Key::Unit(u) => KeybdInput {
            vk: 0,
            scan: u,
            flags: KEYEVENTF_UNICODE | up,
            time: 0,
            extra_info: INJECT_TAG,
        },
        Key::Return => KeybdInput {
            vk: VK_RETURN,
            scan: 0,
            flags: up,
            time: 0,
            extra_info: INJECT_TAG,
        },
    };
    Input {
        kind: INPUT_KEYBOARD,
        ki,
        _pad: [0],
    }
}

/// `SendInput`-based backend.
#[derive(Default)]
pub struct SendInputBackend;

impl InputBackend for SendInputBackend {
    fn send(&mut self, events: &[KeyEv]) -> usize {
        let inputs: Vec<Input> = events.iter().map(to_input).collect();
        unsafe {
            SendInput(
                inputs.len() as u32,
                inputs.as_ptr(),
                std::mem::size_of::<Input>() as i32,
            ) as usize
        }
    }
}

fn token_elevated(process: isize) -> Option<bool> {
    unsafe {
        let mut tok = 0isize;
        if OpenProcessToken(process, TOKEN_QUERY, &mut tok) == 0 {
            return None;
        }
        let mut elevated = 0u32;
        let mut ret = 0u32;
        let ok = GetTokenInformation(tok, TOKEN_ELEVATION, &mut elevated, 4, &mut ret);
        CloseHandle(tok);
        (ok != 0).then_some(elevated != 0)
    }
}

/// Probe: foreground process is elevated while we are not. If the process
/// cannot be opened at all (a higher-integrity process), it counts as elevated.
#[derive(Default)]
pub struct ForegroundElevation;

impl ElevationProbe for ForegroundElevation {
    fn foreground_elevated(&mut self) -> bool {
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd == 0 {
                return false;
            }
            if token_elevated(GetCurrentProcess()) == Some(true) {
                return false; // we are elevated too: UIPI allows injection
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, &mut pid);
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if h == 0 {
                return true;
            }
            let r = token_elevated(h);
            CloseHandle(h);
            r.unwrap_or(true)
        }
    }
}
