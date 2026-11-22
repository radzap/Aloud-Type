//! Win32 tray host: a hidden message window, a `Shell_NotifyIconW` icon, a
//! popup menu and the own-drawn Startup notice. Runs a plain message loop on
//! the calling (main) thread and has no timers of its own while idle: the
//! only timer is the 3 second notice dismissal, alive only while the notice
//! is on screen. The pure decisions live in `tray_model`.
use crate::tray_model::{
    self, double_click_command, menu, placeholder_icon_bgra, Command, IconKind, StartupAction, TrayState, TrayView,
    NOTICE_DURATION_MS,
};
use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

type Hwnd = isize;

const WM_DESTROY: u32 = 0x0002;
const WM_CLOSE: u32 = 0x0010;
const WM_PAINT: u32 = 0x000F;
const WM_TIMER: u32 = 0x0113;
const WM_NULL: u32 = 0;
const WM_NCHITTEST: u32 = 0x0084;
const WM_MOUSEACTIVATE: u32 = 0x0021;
const WM_LBUTTONDBLCLK: isize = 0x0203;
const WM_RBUTTONUP: isize = 0x0205;
const WM_CONTEXTMENU: isize = 0x007B;
const WM_USER: u32 = 0x0400;
const WM_TRAY: u32 = WM_USER + 1;
const WM_APP_UPDATE: u32 = WM_USER + 2;
const NIM_ADD: u32 = 0;
const NIM_MODIFY: u32 = 1;
const NIM_DELETE: u32 = 2;
const NIF_MESSAGE: u32 = 1;
const NIF_ICON: u32 = 2;
const NIF_TIP: u32 = 4;
const WS_POPUP: u32 = 0x8000_0000;
const WS_EX_TOPMOST: u32 = 0x8;
const WS_EX_TOOLWINDOW: u32 = 0x80;
const WS_EX_LAYERED: u32 = 0x80000;
const WS_EX_NOACTIVATE: u32 = 0x0800_0000;
const SW_SHOWNOACTIVATE: i32 = 4;
const SW_SHOWNORMAL: i32 = 1;
const LWA_ALPHA: u32 = 2;
const MF_STRING: u32 = 0;
const TPM_RIGHTBUTTON: u32 = 2;
const TPM_RETURNCMD: u32 = 0x100;
const SM_CXSCREEN: i32 = 0;
const SM_CYSCREEN: i32 = 1;
const DT_CENTER: u32 = 1;
const DT_VCENTER: u32 = 4;
const DT_SINGLELINE: u32 = 0x20;
const HTTRANSPARENT: isize = -1;
const MA_NOACTIVATE: isize = 3;
const NOTICE_TIMER: usize = 1;
const NOTICE_W: i32 = 440;
const NOTICE_H: i32 = 56;

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct Point {
    x: i32,
    y: i32,
}
#[repr(C)]
#[derive(Default)]
struct Msg {
    hwnd: Hwnd,
    message: u32,
    wparam: usize,
    lparam: isize,
    time: u32,
    pt: Point,
}
#[repr(C)]
struct WndClassW {
    style: u32,
    wnd_proc: unsafe extern "system" fn(Hwnd, u32, usize, isize) -> isize,
    cls_extra: i32,
    wnd_extra: i32,
    instance: isize,
    icon: isize,
    cursor: isize,
    background: isize,
    menu_name: *const u16,
    class_name: *const u16,
}
#[repr(C)]
struct NotifyIconDataW {
    cb_size: u32,
    hwnd: Hwnd,
    id: u32,
    flags: u32,
    callback_message: u32,
    icon: isize,
    tip: [u16; 128],
    state: u32,
    state_mask: u32,
    info: [u16; 256],
    version: u32,
    info_title: [u16; 64],
    info_flags: u32,
    guid: [u8; 16],
    balloon_icon: isize,
}
#[repr(C)]
struct IconInfo {
    is_icon: i32,
    x_hotspot: u32,
    y_hotspot: u32,
    mask: isize,
    color: isize,
}
#[repr(C)]
#[derive(Default)]
struct Rect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}
#[repr(C)]
struct PaintStruct {
    hdc: isize,
    erase: i32,
    paint: Rect,
    restore: i32,
    inc_update: i32,
    reserved: [u8; 32],
}

#[link(name = "user32")]
#[allow(clashing_extern_declarations)]
extern "system" {
    fn RegisterClassW(c: *const WndClassW) -> u16;
    fn CreateWindowExW(
        ex: u32, class: *const u16, title: *const u16, style: u32, x: i32, y: i32, w: i32, h: i32, parent: Hwnd,
        menu: isize, instance: isize, param: *const u8,
    ) -> Hwnd;
    fn DefWindowProcW(h: Hwnd, m: u32, w: usize, l: isize) -> isize;
    fn DestroyWindow(h: Hwnd) -> i32;
    fn GetMessageW(m: *mut Msg, h: Hwnd, min: u32, max: u32) -> i32;
    fn TranslateMessage(m: *const Msg) -> i32;
    fn DispatchMessageW(m: *const Msg) -> isize;
    fn PostMessageW(h: Hwnd, m: u32, w: usize, l: isize) -> i32;
    fn PostQuitMessage(code: i32);
    fn RegisterWindowMessageW(name: *const u16) -> u32;
    fn CreateIconIndirect(info: *const IconInfo) -> isize;
    fn DestroyIcon(i: isize) -> i32;
    fn CreatePopupMenu() -> isize;
    fn AppendMenuW(m: isize, flags: u32, id: usize, text: *const u16) -> i32;
    fn TrackPopupMenu(m: isize, flags: u32, x: i32, y: i32, r: i32, h: Hwnd, rect: *const Rect) -> i32;
    fn DestroyMenu(m: isize) -> i32;
    fn GetCursorPos(p: *mut Point) -> i32;
    fn SetForegroundWindow(h: Hwnd) -> i32;
    fn ShowWindow(h: Hwnd, cmd: i32) -> i32;
    fn SetTimer(h: Hwnd, id: usize, ms: u32, f: usize) -> usize;
    fn KillTimer(h: Hwnd, id: usize) -> i32;
    fn SetLayeredWindowAttributes(h: Hwnd, key: u32, alpha: u8, flags: u32) -> i32;
    fn GetSystemMetrics(i: i32) -> i32;
    fn BeginPaint(h: Hwnd, ps: *mut PaintStruct) -> isize;
    fn EndPaint(h: Hwnd, ps: *const PaintStruct) -> i32;
    fn GetClientRect(h: Hwnd, r: *mut Rect) -> i32;
    fn FillRect(dc: isize, r: *const Rect, brush: isize) -> i32;
    fn DrawTextW(dc: isize, text: *const u16, n: i32, r: *mut Rect, fmt: u32) -> i32;
}
#[link(name = "gdi32")]
extern "system" {
    fn CreateBitmap(w: i32, h: i32, planes: u32, bits: u32, data: *const u8) -> isize;
    fn DeleteObject(o: isize) -> i32;
    fn CreateSolidBrush(color: u32) -> isize;
    fn SetBkMode(dc: isize, mode: i32) -> i32;
    fn SetTextColor(dc: isize, color: u32) -> u32;
}
#[link(name = "shell32")]
extern "system" {
    fn Shell_NotifyIconW(msg: u32, data: *const NotifyIconDataW) -> i32;
    fn ShellExecuteW(h: Hwnd, op: *const u16, file: *const u16, params: *const u16, dir: *const u16, show: i32) -> isize;
}
#[link(name = "kernel32")]
extern "system" {
    fn GetModuleHandleW(name: *const u16) -> isize;
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Cross-thread handle: the controller thread pushes the engine's state here.
/// Cloneable; updates made before the window exists are applied when it is
/// created.
#[derive(Clone)]
pub struct TrayHandle {
    inner: Arc<Inner>,
}
struct Inner {
    hwnd: AtomicIsize,
    pending: Mutex<Option<TrayView>>,
    blocked: AtomicBool,
}

impl Default for TrayHandle {
    fn default() -> Self {
        Self::new()
    }
}

impl TrayHandle {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Inner {
                hwnd: AtomicIsize::new(0),
                pending: Mutex::new(None),
                blocked: AtomicBool::new(false),
            }),
        }
    }

    /// Reports the engine's tray state. `blocked_fault_standing` controls the
    /// privacy menu entry (`ListeningMonitor::blocked_fault_standing`).
    pub fn set_state(
        &self,
        engine: &aloud_type_core::listening::Tray,
        key_unavailable: bool,
        blocked_fault_standing: bool,
    ) {
        self.inner.blocked.store(blocked_fault_standing, Ordering::SeqCst);
        *self.inner.pending.lock().unwrap() = Some(tray_model::view(engine, key_unavailable));
        let h = self.inner.hwnd.load(Ordering::SeqCst);
        if h != 0 {
            unsafe { PostMessageW(h, WM_APP_UPDATE, 0, 0) };
        }
    }

    /// Asks the message loop to remove the icon and return from `run`.
    pub fn quit(&self) {
        let h = self.inner.hwnd.load(Ordering::SeqCst);
        if h != 0 {
            unsafe { PostMessageW(h, WM_CLOSE, 0, 0) };
        }
    }
}

struct Host {
    handle: TrayHandle,
    commands: Sender<Command>,
    state: TrayState,
    icons: [isize; 3],
    icon_added: bool,
    taskbar_created: u32,
    notice_text: Vec<u16>,
}

thread_local! {
    static HOST: RefCell<Option<Host>> = const { RefCell::new(None) };
}

fn icon_index(k: IconKind) -> usize {
    match k {
        IconKind::Idle => 0,
        IconKind::Listening => 1,
        IconKind::Fault => 2,
    }
}

fn make_icon(kind: IconKind) -> isize {
    const SIZE: usize = 32;
    let color = placeholder_icon_bgra(kind, SIZE);
    let mask = vec![0u8; SIZE * SIZE / 8];
    unsafe {
        let hc = CreateBitmap(SIZE as i32, SIZE as i32, 1, 32, color.as_ptr());
        let hm = CreateBitmap(SIZE as i32, SIZE as i32, 1, 1, mask.as_ptr());
        let info = IconInfo { is_icon: 1, x_hotspot: 0, y_hotspot: 0, mask: hm, color: hc };
        let icon = CreateIconIndirect(&info);
        DeleteObject(hc);
        DeleteObject(hm);
        icon
    }
}

fn notify_data(hwnd: Hwnd, flags: u32, icon: isize, tip: &str) -> NotifyIconDataW {
    let mut d = NotifyIconDataW {
        cb_size: std::mem::size_of::<NotifyIconDataW>() as u32,
        hwnd,
        id: 1,
        flags,
        callback_message: WM_TRAY,
        icon,
        tip: [0; 128],
        state: 0,
        state_mask: 0,
        info: [0; 256],
        version: 0,
        info_title: [0; 64],
        info_flags: 0,
        guid: [0; 16],
        balloon_icon: 0,
    };
    for (i, c) in tip.encode_utf16().take(127).enumerate() {
        d.tip[i] = c;
    }
    d
}

fn apply_view(host: &mut Host, hwnd: Hwnd, view: &TrayView) {
    let icon = host.icons[icon_index(view.icon)];
    let d = notify_data(hwnd, NIF_MESSAGE | NIF_ICON | NIF_TIP, icon, &view.tooltip);
    unsafe { Shell_NotifyIconW(if host.icon_added { NIM_MODIFY } else { NIM_ADD }, &d) };
    host.icon_added = true;
}

fn show_menu(hwnd: Hwnd, blocked: bool) -> Option<Command> {
    let items = menu(blocked);
    unsafe {
        let m = CreatePopupMenu();
        for (i, item) in items.iter().enumerate() {
            AppendMenuW(m, MF_STRING, i + 1, wide(item.label).as_ptr());
        }
        let mut p = Point::default();
        GetCursorPos(&mut p);
        // Required so the menu dismisses when the user clicks elsewhere.
        SetForegroundWindow(hwnd);
        let picked = TrackPopupMenu(m, TPM_RIGHTBUTTON | TPM_RETURNCMD, p.x, p.y, 0, hwnd, std::ptr::null());
        PostMessageW(hwnd, WM_NULL, 0, 0);
        DestroyMenu(m);
        (picked > 0).then(|| items[picked as usize - 1].command)
    }
}

fn dispatch(hwnd: Hwnd, cmd: Command, host: &Host) {
    match cmd {
        Command::OpenMicrophonePrivacySettings => unsafe {
            ShellExecuteW(
                0,
                wide("open").as_ptr(),
                wide("ms-settings:privacy-microphone").as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                SW_SHOWNORMAL,
            );
        },
        Command::Exit => unsafe {
            PostMessageW(hwnd, WM_CLOSE, 0, 0);
        },
        Command::OpenSettings => {}
    }
    let _ = host.commands.send(cmd);
}

unsafe extern "system" fn tray_proc(hwnd: Hwnd, msg: u32, w: usize, l: isize) -> isize {
    match msg {
        WM_TRAY => {
            let cmd = match l & 0xFFFF {
                WM_LBUTTONDBLCLK => Some(double_click_command()),
                WM_RBUTTONUP | WM_CONTEXTMENU => {
                    let blocked = HOST
                        .with(|h| h.borrow().as_ref().is_some_and(|h| h.handle.inner.blocked.load(Ordering::SeqCst)));
                    show_menu(hwnd, blocked)
                }
                _ => None,
            };
            if let Some(cmd) = cmd {
                HOST.with(|h| {
                    if let Some(host) = h.borrow().as_ref() {
                        dispatch(hwnd, cmd, host);
                    }
                });
            }
            0
        }
        WM_APP_UPDATE => {
            HOST.with(|h| {
                if let Some(host) = h.borrow_mut().as_mut() {
                    let pending = host.handle.inner.pending.lock().unwrap().take();
                    if let Some(view) = pending {
                        if let Some(v) = host.state.update(view) {
                            apply_view(host, hwnd, &v);
                        }
                    }
                }
            });
            0
        }
        WM_CLOSE => {
            DestroyWindow(hwnd);
            0
        }
        WM_DESTROY => {
            let d = notify_data(hwnd, 0, 0, "");
            Shell_NotifyIconW(NIM_DELETE, &d);
            HOST.with(|h| {
                if let Some(host) = h.borrow().as_ref() {
                    for i in host.icons {
                        DestroyIcon(i);
                    }
                }
            });
            PostQuitMessage(0);
            0
        }
        m => {
            let tc = HOST.with(|h| h.borrow().as_ref().map(|h| h.taskbar_created));
            if m != 0 && Some(m) == tc {
                // Explorer restarted: put the icon back.
                HOST.with(|h| {
                    if let Some(host) = h.borrow_mut().as_mut() {
                        host.icon_added = false;
                        if let Some(v) = host.state.current().cloned() {
                            apply_view(host, hwnd, &v);
                        }
                    }
                });
                return 0;
            }
            DefWindowProcW(hwnd, m, w, l)
        }
    }
}

unsafe extern "system" fn notice_proc(hwnd: Hwnd, msg: u32, w: usize, l: isize) -> isize {
    match msg {
        WM_PAINT => {
            let mut ps: PaintStruct = std::mem::zeroed();
            let dc = BeginPaint(hwnd, &mut ps);
            let mut r = Rect::default();
            GetClientRect(hwnd, &mut r);
            let brush = CreateSolidBrush(0x0030_2A26);
            FillRect(dc, &r, brush);
            DeleteObject(brush);
            SetBkMode(dc, 1);
            SetTextColor(dc, 0x00F5_F5F5);
            let text = HOST.with(|h| h.borrow().as_ref().map(|h| h.notice_text.clone())).unwrap_or_default();
            DrawTextW(dc, text.as_ptr(), -1, &mut r, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
            EndPaint(hwnd, &ps);
            0
        }
        WM_TIMER => {
            KillTimer(hwnd, NOTICE_TIMER);
            DestroyWindow(hwnd);
            0
        }
        WM_NCHITTEST => HTTRANSPARENT,
        WM_MOUSEACTIVATE => MA_NOACTIVATE,
        _ => DefWindowProcW(hwnd, msg, w, l),
    }
}

unsafe fn register(name: &str, proc_: unsafe extern "system" fn(Hwnd, u32, usize, isize) -> isize) -> Vec<u16> {
    let class = wide(name);
    let wc = WndClassW {
        style: 0,
        wnd_proc: proc_,
        cls_extra: 0,
        wnd_extra: 0,
        instance: GetModuleHandleW(std::ptr::null()),
        icon: 0,
        cursor: 0,
        background: 0,
        menu_name: std::ptr::null(),
        class_name: class.as_ptr(),
    };
    RegisterClassW(&wc);
    class
}

/// Shows the Startup notice: own-drawn, topmost, never activated, click
/// through, self-dismissing after about 3 seconds. Returns immediately (the
/// message loop in `run` services it).
unsafe fn show_notice(text: &str) {
    let class = register("AloudTypeNotice", notice_proc);
    let (sw, sh) = (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN));
    let hwnd = CreateWindowExW(
        WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_LAYERED,
        class.as_ptr(),
        std::ptr::null(),
        WS_POPUP,
        (sw - NOTICE_W) / 2,
        sh - NOTICE_H - 120,
        NOTICE_W,
        NOTICE_H,
        0,
        0,
        GetModuleHandleW(std::ptr::null()),
        std::ptr::null(),
    );
    if hwnd == 0 {
        return;
    }
    HOST.with(|h| {
        if let Some(host) = h.borrow_mut().as_mut() {
            host.notice_text = wide(text);
        }
    });
    SetLayeredWindowAttributes(hwnd, 0, 235, LWA_ALPHA);
    ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    SetTimer(hwnd, NOTICE_TIMER, NOTICE_DURATION_MS, 0);
}

/// Creates the tray icon and (per `startup`) the Startup notice, then runs the
/// message loop on the calling thread until Exit or `TrayHandle::quit`.
/// Commands (Settings, privacy, Exit) are also forwarded on `commands`; the
/// privacy entry is opened here, the rest are the caller's to act on. When
/// `startup` is `OpenSettings`, `Command::OpenSettings` is sent instead of
/// showing the notice.
pub fn run(
    handle: TrayHandle,
    commands: Sender<Command>,
    startup: StartupAction,
    initial: TrayView,
) -> std::io::Result<()> {
    unsafe {
        let class = register("AloudTypeTray", tray_proc);
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            class.as_ptr(),
            std::ptr::null(),
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            GetModuleHandleW(std::ptr::null()),
            std::ptr::null(),
        );
        if hwnd == 0 {
            return Err(std::io::Error::last_os_error());
        }
        handle.inner.hwnd.store(hwnd, Ordering::SeqCst);
        let icons = [make_icon(IconKind::Idle), make_icon(IconKind::Listening), make_icon(IconKind::Fault)];
        let mut host = Host {
            handle: handle.clone(),
            commands: commands.clone(),
            state: TrayState::default(),
            icons,
            icon_added: false,
            taskbar_created: RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()),
            notice_text: Vec::new(),
        };
        // A state reported before the window existed wins over the initial one.
        let first = handle.inner.pending.lock().unwrap().take().unwrap_or(initial);
        if let Some(v) = host.state.update(first) {
            apply_view(&mut host, hwnd, &v);
        }
        HOST.with(|h| *h.borrow_mut() = Some(host));
        match startup {
            StartupAction::Notice(text) => show_notice(&text),
            StartupAction::OpenSettings => {
                let _ = commands.send(Command::OpenSettings);
            }
        }
        let mut m = Msg::default();
        while GetMessageW(&mut m, 0, 0, 0) > 0 {
            TranslateMessage(&m);
            DispatchMessageW(&m);
        }
        HOST.with(|h| *h.borrow_mut() = None);
        handle.inner.hwnd.store(0, Ordering::SeqCst);
    }
    Ok(())
}
