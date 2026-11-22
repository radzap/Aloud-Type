//! Win32 declarations shared by more than one adapter, so each function has
//! exactly one signature in the crate. Window handles are `isize` everywhere.
pub type Hwnd = isize;

#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

#[repr(C)]
#[derive(Default)]
pub struct Msg {
    pub hwnd: Hwnd,
    pub message: u32,
    pub wparam: usize,
    pub lparam: isize,
    pub time: u32,
    pub pt: Point,
}

#[link(name = "user32")]
extern "system" {
    pub fn GetMessageW(msg: *mut Msg, hwnd: Hwnd, min: u32, max: u32) -> i32;
    pub fn TranslateMessage(msg: *const Msg) -> i32;
    pub fn DispatchMessageW(msg: *const Msg) -> isize;
    pub fn GetForegroundWindow() -> Hwnd;
    pub fn GetWindowThreadProcessId(hwnd: Hwnd, pid: *mut u32) -> u32;
    pub fn ShowWindow(hwnd: Hwnd, cmd: i32) -> i32;
    pub fn SetForegroundWindow(hwnd: Hwnd) -> i32;
    pub fn IsWindowVisible(hwnd: Hwnd) -> i32;
    pub fn IsIconic(hwnd: Hwnd) -> i32;
    pub fn EnumWindows(callback: extern "system" fn(Hwnd, isize) -> i32, param: isize) -> i32;
    pub fn AllowSetForegroundWindow(pid: u32) -> i32;
    pub fn PostMessageW(hwnd: Hwnd, msg: u32, wparam: usize, lparam: isize) -> i32;
}
