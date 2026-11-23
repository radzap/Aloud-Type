//! The language of the keyboard layout the user is typing with right now (Win+Space and the
//! taskbar switch it per window), read from the thread that owns the focused window.
use crate::win32::{GetForegroundWindow, GetWindowThreadProcessId, Hwnd};

#[repr(C)]
#[derive(Default)]
struct Rect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[repr(C)]
#[derive(Default)]
struct GuiThreadInfo {
    cb_size: u32,
    flags: u32,
    hwnd_active: Hwnd,
    hwnd_focus: Hwnd,
    hwnd_capture: Hwnd,
    hwnd_menu_owner: Hwnd,
    hwnd_move_size: Hwnd,
    hwnd_caret: Hwnd,
    rc_caret: Rect,
}

#[link(name = "user32")]
extern "system" {
    fn GetGUIThreadInfo(thread: u32, info: *mut GuiThreadInfo) -> i32;
    fn GetKeyboardLayout(thread: u32) -> isize;
}

#[link(name = "kernel32")]
extern "system" {
    fn LCIDToLocaleName(lcid: u32, name: *mut u16, len: i32, flags: u32) -> i32;
}

const LOCALE_ALLOW_NEUTRAL_NAMES: u32 = 0x0800_0000;
const LOCALE_NAME_MAX_LENGTH: usize = 85;

/// Locale name ("sk-SK", "en-US") of the keyboard layout active in the focused window, or
/// `None` when there is no foreground window or the layout has no known locale.
pub fn focused_keyboard_locale() -> Option<String> {
    unsafe {
        let fg = GetForegroundWindow();
        if fg == 0 {
            return None;
        }
        let fg_thread = GetWindowThreadProcessId(fg, std::ptr::null_mut());
        // The focused control can belong to another thread than the top-level window.
        let mut info = GuiThreadInfo {
            cb_size: std::mem::size_of::<GuiThreadInfo>() as u32,
            ..Default::default()
        };
        let thread = if GetGUIThreadInfo(fg_thread, &mut info) != 0 && info.hwnd_focus != 0 {
            GetWindowThreadProcessId(info.hwnd_focus, std::ptr::null_mut())
        } else {
            fg_thread
        };
        let hkl = GetKeyboardLayout(thread);
        if hkl == 0 {
            return None;
        }
        // The low word of the layout handle is its language id (an LCID with default sorting).
        let lang_id = (hkl as usize & 0xFFFF) as u32;
        let mut buf = [0u16; LOCALE_NAME_MAX_LENGTH];
        let n = LCIDToLocaleName(
            lang_id,
            buf.as_mut_ptr(),
            buf.len() as i32,
            LOCALE_ALLOW_NEUTRAL_NAMES,
        );
        if n <= 1 {
            return None;
        }
        Some(String::from_utf16_lossy(&buf[..n as usize - 1]))
    }
}
