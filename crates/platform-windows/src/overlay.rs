//! Win32 host of the Listening indicator overlay: a click-through, topmost,
//! never-activated layered window on its own thread. The thread sleeps in
//! `GetMessage`; the only timer (about 30 fps) exists while the overlay is
//! visible or fading, so an idle app does no overlay work. All decisions
//! (smoothing, fades, growing, placement, pixels) live in `overlay_model`.
use crate::overlay_model::{self as model, Animator, Placement, Rect, Style, FRAME_MS};
use aloud_type_core::listening::Indicator;
use std::cell::RefCell;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Instant;

use crate::win32::{
    DispatchMessageW, GetForegroundWindow, GetMessageW, Hwnd, Msg, Point, ShowWindow,
    TranslateMessage,
};

const WM_DESTROY: u32 = 0x0002;
const WM_CLOSE: u32 = 0x0010;
const WM_TIMER: u32 = 0x0113;
const WM_NCHITTEST: u32 = 0x0084;
const WM_MOUSEACTIVATE: u32 = 0x0021;
const WM_USER: u32 = 0x0400;
const WM_WAKE: u32 = WM_USER + 10;
const WS_POPUP: u32 = 0x8000_0000;
const WS_EX_TOPMOST: u32 = 0x8;
const WS_EX_TRANSPARENT: u32 = 0x20;
const WS_EX_TOOLWINDOW: u32 = 0x80;
const WS_EX_LAYERED: u32 = 0x80000;
const WS_EX_NOACTIVATE: u32 = 0x0800_0000;
const SW_HIDE: i32 = 0;
const SW_SHOWNOACTIVATE: i32 = 4;
const HTTRANSPARENT: isize = -1;
const MA_NOACTIVATE: isize = 3;
const MONITOR_DEFAULTTOPRIMARY: u32 = 1;
const ULW_ALPHA: u32 = 2;
const AC_SRC_ALPHA: u8 = 1;
const TIMER_ID: usize = 1;
const DT_CENTER: u32 = 1;
const DT_WORDBREAK: u32 = 0x10;
const DT_NOPREFIX: u32 = 0x800;
const SM_CXSCREEN: i32 = 0;
const SM_CYSCREEN: i32 = 1;

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct Size {
    cx: i32,
    cy: i32,
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
#[derive(Default)]
struct WinRect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}
#[repr(C)]
#[derive(Default)]
struct MonitorInfo {
    size: u32,
    monitor: WinRect,
    work: WinRect,
    flags: u32,
}
#[repr(C)]
struct BlendFunction {
    op: u8,
    flags: u8,
    alpha: u8,
    format: u8,
}
#[repr(C)]
struct BitmapInfo {
    size: u32,
    width: i32,
    height: i32,
    planes: u16,
    bit_count: u16,
    compression: u32,
    size_image: u32,
    x_ppm: i32,
    y_ppm: i32,
    clr_used: u32,
    clr_important: u32,
    colors: [u32; 1],
}

#[link(name = "user32")]
extern "system" {
    fn RegisterClassW(c: *const WndClassW) -> u16;
    fn CreateWindowExW(
        ex: u32,
        class: *const u16,
        title: *const u16,
        style: u32,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        parent: Hwnd,
        menu: isize,
        instance: isize,
        param: *const u8,
    ) -> Hwnd;
    fn DefWindowProcW(h: Hwnd, m: u32, w: usize, l: isize) -> isize;
    fn DestroyWindow(h: Hwnd) -> i32;
    fn PostMessageW(h: Hwnd, m: u32, w: usize, l: isize) -> i32;
    fn PostQuitMessage(code: i32);
    fn SetTimer(h: Hwnd, id: usize, ms: u32, f: usize) -> usize;
    fn KillTimer(h: Hwnd, id: usize) -> i32;
    fn GetSystemMetrics(i: i32) -> i32;
    fn MonitorFromWindow(h: Hwnd, flags: u32) -> isize;
    fn GetMonitorInfoW(m: isize, info: *mut MonitorInfo) -> i32;
    fn GetDC(h: Hwnd) -> isize;
    fn ReleaseDC(h: Hwnd, dc: isize) -> i32;
    fn DrawTextW(dc: isize, text: *const u16, n: i32, r: *mut WinRect, fmt: u32) -> i32;
    fn UpdateLayeredWindow(
        h: Hwnd,
        dst_dc: isize,
        dst_pos: *const Point,
        size: *const Size,
        src_dc: isize,
        src_pos: *const Point,
        key: u32,
        blend: *const BlendFunction,
        flags: u32,
    ) -> i32;
}
#[link(name = "gdi32")]
extern "system" {
    fn CreateCompatibleDC(dc: isize) -> isize;
    fn CreateDIBSection(
        dc: isize,
        bmi: *const BitmapInfo,
        usage: u32,
        bits: *mut *mut c_void,
        section: isize,
        offset: u32,
    ) -> isize;
    fn SelectObject(dc: isize, o: isize) -> isize;
    fn DeleteDC(dc: isize) -> i32;
    fn DeleteObject(o: isize) -> i32;
    fn SetBkMode(dc: isize, mode: i32) -> i32;
    fn SetTextColor(dc: isize, color: u32) -> u32;
    fn CreateFontW(
        h: i32,
        w: i32,
        esc: i32,
        orient: i32,
        weight: i32,
        italic: u32,
        underline: u32,
        strike: u32,
        charset: u32,
        out_prec: u32,
        clip_prec: u32,
        quality: u32,
        pitch: u32,
        face: *const u16,
    ) -> isize;
}
#[link(name = "shcore")]
extern "system" {
    fn GetDpiForMonitor(m: isize, kind: u32, x: *mut u32, y: *mut u32) -> i32;
}
#[link(name = "kernel32")]
extern "system" {
    fn GetModuleHandleW(name: *const u16) -> isize;
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

struct Shared {
    latest: Mutex<Indicator>,
    placement: Mutex<Placement>,
    style: Mutex<Style>,
    /// A wake message is pending or the timer is running.
    awake: AtomicBool,
    /// The animation timer is running.
    ticking: AtomicBool,
    hwnd: AtomicIsize,
}

/// Cross-thread handle to the overlay. Cloneable; cheap to call every tick.
#[derive(Clone)]
pub struct OverlayHandle {
    shared: Arc<Shared>,
}

impl OverlayHandle {
    /// Reports the engine's indicator state (`ListeningMonitor::indicator`).
    /// Safe to call at any rate; while hidden and unchanged it costs nothing.
    pub fn set_indicator(&self, ind: Indicator) {
        let visible = ind != Indicator::Hidden;
        *self.shared.latest.lock().unwrap() = ind;
        if visible && !self.shared.awake.swap(true, Ordering::SeqCst) {
            let h = self.shared.hwnd.load(Ordering::SeqCst);
            if h != 0 {
                unsafe { PostMessageW(h, WM_WAKE, 0, 0) };
            }
        }
    }

    pub fn set_placement(&self, p: Placement) {
        *self.shared.placement.lock().unwrap() = p;
    }

    /// Chooses the indicator style. Applies from the next Dictation (the
    /// overlay latches the style when it appears), no restart needed.
    pub fn set_style(&self, s: Style) {
        *self.shared.style.lock().unwrap() = s;
    }

    /// True while the overlay's frame timer is running.
    pub fn is_ticking(&self) -> bool {
        self.shared.ticking.load(Ordering::SeqCst)
    }

    /// Destroys the window and ends the overlay thread.
    pub fn shutdown(&self) {
        let h = self.shared.hwnd.load(Ordering::SeqCst);
        if h != 0 {
            unsafe { PostMessageW(h, WM_CLOSE, 0, 0) };
        }
    }

    #[cfg(test)]
    fn hwnd(&self) -> Hwnd {
        self.shared.hwnd.load(Ordering::SeqCst)
    }
}

struct Host {
    shared: Arc<Shared>,
    animator: Animator,
    started: Instant,
    visible: bool,
    work: Rect,
    scale: f32,
}

thread_local! {
    static HOST: RefCell<Option<Host>> = const { RefCell::new(None) };
}

/// Work area and DPI scale of the monitor holding the foreground window
/// (primary when there is none), read when a Dictation's overlay appears.
unsafe fn current_monitor() -> (Rect, f32) {
    let mon = MonitorFromWindow(GetForegroundWindow(), MONITOR_DEFAULTTOPRIMARY);
    let mut info = MonitorInfo {
        size: std::mem::size_of::<MonitorInfo>() as u32,
        ..Default::default()
    };
    let work = if mon != 0 && GetMonitorInfoW(mon, &mut info) != 0 {
        Rect {
            left: info.work.left,
            top: info.work.top,
            right: info.work.right,
            bottom: info.work.bottom,
        }
    } else {
        Rect {
            left: 0,
            top: 0,
            right: GetSystemMetrics(SM_CXSCREEN),
            bottom: GetSystemMetrics(SM_CYSCREEN),
        }
    };
    let (mut dx, mut dy) = (96u32, 96u32);
    if mon != 0 && GetDpiForMonitor(mon, 0, &mut dx, &mut dy) != 0 {
        dx = 96;
    }
    (work, dx.max(96) as f32 / 96.0)
}

unsafe fn draw_text(dc: isize, lines: &[model::TextLine]) {
    SetBkMode(dc, 1);
    let face = wide("Segoe UI");
    for l in lines {
        let font = CreateFontW(
            -(l.font_px.round() as i32),
            0,
            0,
            0,
            if l.bold { 600 } else { 400 },
            0,
            0,
            0,
            1,
            0,
            0,
            4, // ANTIALIASED_QUALITY: grayscale, no colour fringes on the layered surface
            0,
            face.as_ptr(),
        );
        let old = SelectObject(dc, font);
        SetTextColor(
            dc,
            l.color[0] as u32 | (l.color[1] as u32) << 8 | (l.color[2] as u32) << 16,
        );
        let mut r = WinRect {
            left: l.rect.left,
            top: l.rect.top,
            right: l.rect.right,
            bottom: l.rect.bottom,
        };
        let text = wide(&l.text);
        DrawTextW(
            dc,
            text.as_ptr(),
            -1,
            &mut r,
            DT_CENTER | DT_WORDBREAK | DT_NOPREFIX,
        );
        SelectObject(dc, old);
        DeleteObject(font);
    }
}

/// Renders the frame into a DIB and presents it with per-pixel alpha.
unsafe fn present(hwnd: Hwnd, host: &Host, frame: &model::Frame, placement: Placement) {
    let (w, h, px) = model::render(frame, host.scale);
    let lines = model::text_lines(frame, host.scale);
    let screen = GetDC(0);
    let mem = CreateCompatibleDC(screen);
    let bmi = BitmapInfo {
        size: 40,
        width: w as i32,
        height: -(h as i32),
        planes: 1,
        bit_count: 32,
        compression: 0,
        size_image: 0,
        x_ppm: 0,
        y_ppm: 0,
        clr_used: 0,
        clr_important: 0,
        colors: [0],
    };
    let mut bits: *mut c_void = std::ptr::null_mut();
    let dib = CreateDIBSection(screen, &bmi, 0, &mut bits, 0, 0);
    if dib != 0 && !bits.is_null() {
        let old = SelectObject(mem, dib);
        let buf = std::slice::from_raw_parts_mut(bits as *mut u32, w * h);
        buf.copy_from_slice(&px);
        if !lines.is_empty() {
            draw_text(mem, &lines);
            model::restore_alpha(&px, buf);
        }
        let (x, y) = model::place(host.work, w as i32, h as i32, placement, host.scale);
        let blend = BlendFunction {
            op: 0,
            flags: 0,
            alpha: (frame.opacity.clamp(0.0, 1.0) * 255.0).round() as u8,
            format: AC_SRC_ALPHA,
        };
        UpdateLayeredWindow(
            hwnd,
            screen,
            &Point { x, y },
            &Size {
                cx: w as i32,
                cy: h as i32,
            },
            mem,
            &Point::default(),
            0,
            &blend,
            ULW_ALPHA,
        );
        SelectObject(mem, old);
    }
    if dib != 0 {
        DeleteObject(dib);
    }
    DeleteDC(mem);
    ReleaseDC(0, screen);
}

/// One timer frame (or the wake): sync state, animate, present or go idle.
unsafe fn frame(hwnd: Hwnd, starting: bool) {
    HOST.with(|cell| {
        let mut guard = cell.borrow_mut();
        let Some(host) = guard.as_mut() else { return };
        let now = host.started.elapsed().as_millis() as u64;
        let latest = host.shared.latest.lock().unwrap().clone();
        if starting && !host.animator.active() {
            let (work, scale) = current_monitor();
            host.work = work;
            host.scale = scale;
        }
        let style = *host.shared.style.lock().unwrap();
        host.animator.set_styled(&latest, style, now);
        let placement = *host.shared.placement.lock().unwrap();
        match host.animator.tick(now) {
            Some(f) => {
                present(hwnd, host, &f, placement);
                if !host.visible {
                    ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                    host.visible = true;
                }
                if !host.shared.ticking.swap(true, Ordering::SeqCst) {
                    SetTimer(hwnd, TIMER_ID, FRAME_MS, 0);
                }
            }
            None => {
                KillTimer(hwnd, TIMER_ID);
                host.shared.ticking.store(false, Ordering::SeqCst);
                if host.visible {
                    ShowWindow(hwnd, SW_HIDE);
                    host.visible = false;
                }
                host.shared.awake.store(false, Ordering::SeqCst);
                // A state set while going to sleep must not be lost.
                if *host.shared.latest.lock().unwrap() != Indicator::Hidden
                    && !host.shared.awake.swap(true, Ordering::SeqCst)
                {
                    PostMessageW(hwnd, WM_WAKE, 0, 0);
                }
            }
        }
    });
}

unsafe extern "system" fn proc_(hwnd: Hwnd, msg: u32, w: usize, l: isize) -> isize {
    match msg {
        WM_WAKE => {
            frame(hwnd, true);
            0
        }
        WM_TIMER => {
            frame(hwnd, false);
            0
        }
        WM_NCHITTEST => HTTRANSPARENT,
        WM_MOUSEACTIVATE => MA_NOACTIVATE,
        WM_CLOSE => {
            KillTimer(hwnd, TIMER_ID);
            DestroyWindow(hwnd);
            0
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, w, l),
    }
}

/// Starts the overlay thread and returns once its window exists. The window
/// is hidden until the first non-hidden `set_indicator`.
pub fn spawn(placement: Placement) -> std::io::Result<OverlayHandle> {
    let shared = Arc::new(Shared {
        latest: Mutex::new(Indicator::Hidden),
        placement: Mutex::new(placement),
        style: Mutex::new(Style::default()),
        awake: AtomicBool::new(false),
        ticking: AtomicBool::new(false),
        hwnd: AtomicIsize::new(0),
    });
    let (tx, rx) = mpsc::channel::<std::io::Result<()>>();
    let thread_shared = shared.clone();
    std::thread::Builder::new()
        .name("overlay".into())
        .spawn(move || unsafe {
            let class = wide("AloudTypeOverlay");
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
            let hwnd = CreateWindowExW(
                WS_EX_TOPMOST
                    | WS_EX_TOOLWINDOW
                    | WS_EX_NOACTIVATE
                    | WS_EX_LAYERED
                    | WS_EX_TRANSPARENT,
                class.as_ptr(),
                std::ptr::null(),
                WS_POPUP,
                0,
                0,
                1,
                1,
                0,
                0,
                GetModuleHandleW(std::ptr::null()),
                std::ptr::null(),
            );
            if hwnd == 0 {
                let _ = tx.send(Err(std::io::Error::last_os_error()));
                return;
            }
            HOST.with(|h| {
                *h.borrow_mut() = Some(Host {
                    shared: thread_shared.clone(),
                    animator: Animator::new(),
                    started: Instant::now(),
                    visible: false,
                    work: Rect {
                        left: 0,
                        top: 0,
                        right: 1920,
                        bottom: 1080,
                    },
                    scale: 1.0,
                })
            });
            thread_shared.hwnd.store(hwnd, Ordering::SeqCst);
            let _ = tx.send(Ok(()));
            let mut m = Msg::default();
            while GetMessageW(&mut m, 0, 0, 0) > 0 {
                TranslateMessage(&m);
                DispatchMessageW(&m);
            }
            thread_shared.hwnd.store(0, Ordering::SeqCst);
            HOST.with(|h| *h.borrow_mut() = None);
        })?;
    rx.recv()
        .map_err(|_| std::io::Error::other("overlay thread failed"))??;
    Ok(OverlayHandle { shared })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::win32::IsWindowVisible;
    use aloud_type_core::listening::FaultReason;
    use std::time::Duration;

    #[link(name = "user32")]
    extern "system" {
        fn GetWindowLongW(h: Hwnd, index: i32) -> i32;
        fn SendMessageW(h: Hwnd, m: u32, w: usize, l: isize) -> isize;
        fn GetWindowRect(h: Hwnd, r: *mut WinRect) -> i32;
    }

    fn wait(ms: u64) {
        std::thread::sleep(Duration::from_millis(ms));
    }

    #[test]
    fn window_is_click_through_topmost_never_activated_and_only_ticks_during_a_dictation() {
        let o = spawn(Placement::default()).unwrap();
        let h = o.hwnd();
        let fg = unsafe { GetForegroundWindow() };
        let ex = unsafe { GetWindowLongW(h, -20) } as u32;
        for flag in [
            WS_EX_TOPMOST,
            WS_EX_TOOLWINDOW,
            WS_EX_NOACTIVATE,
            WS_EX_LAYERED,
            WS_EX_TRANSPARENT,
        ] {
            assert_ne!(ex & flag, 0, "missing style {flag:#x}");
        }
        assert_eq!(
            unsafe { SendMessageW(h, WM_NCHITTEST, 0, 0) },
            HTTRANSPARENT
        );
        assert_eq!(
            unsafe { SendMessageW(h, WM_MOUSEACTIVATE, 0, 0) },
            MA_NOACTIVATE
        );
        // Idle: hidden, no timer.
        wait(100);
        assert!(!o.is_ticking());
        assert_eq!(unsafe { IsWindowVisible(h) }, 0);
        // Dictation: visible and ticking, focus untouched.
        o.set_indicator(Indicator::Listening {
            level: 0.6,
            note: None,
        });
        wait(400);
        assert!(o.is_ticking());
        assert_ne!(unsafe { IsWindowVisible(h) }, 0);
        assert_eq!(unsafe { GetForegroundWindow() }, fg);
        let mut r = WinRect::default();
        unsafe { GetWindowRect(h, &mut r) };
        assert_eq!((r.right - r.left, r.bottom - r.top), (150, 22));
        // Listening fault grows the window to the red size.
        o.set_indicator(Indicator::ListeningFault {
            reason: FaultReason::NoMicrophone,
            message: FaultReason::NoMicrophone.message(),
            recoverable: false,
        });
        wait(600);
        unsafe { GetWindowRect(h, &mut r) };
        assert_eq!((r.right - r.left, r.bottom - r.top), (280, 66));
        assert_eq!(unsafe { GetForegroundWindow() }, fg);
        // End: fades out, hides, the timer stops.
        o.set_indicator(Indicator::Hidden);
        wait(600);
        assert!(!o.is_ticking());
        assert_eq!(unsafe { IsWindowVisible(h) }, 0);
        // A second Dictation works again.
        o.set_indicator(Indicator::Listening {
            level: 0.1,
            note: None,
        });
        wait(300);
        assert!(o.is_ticking());
        // A style change waits for the next Dictation: switch to Bar pill, end, restart.
        o.set_style(Style::BarPill);
        wait(100);
        unsafe { GetWindowRect(h, &mut r) };
        assert_eq!((r.right - r.left, r.bottom - r.top), (150, 22));
        o.set_indicator(Indicator::Hidden);
        wait(600);
        o.set_indicator(Indicator::Listening {
            level: 0.4,
            note: None,
        });
        wait(300);
        unsafe { GetWindowRect(h, &mut r) };
        assert_eq!((r.right - r.left, r.bottom - r.top), (76, 26));
        o.set_style(Style::Orb);
        o.set_indicator(Indicator::Hidden);
        wait(600);
        o.set_indicator(Indicator::Listening {
            level: 0.4,
            note: None,
        });
        wait(300);
        unsafe { GetWindowRect(h, &mut r) };
        assert_eq!((r.right - r.left, r.bottom - r.top), (40, 40));
        o.shutdown();
    }
}
