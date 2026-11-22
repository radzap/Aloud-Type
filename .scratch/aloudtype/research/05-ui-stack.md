# Research 05: Rust UI stack for tray, overlay, notice, and settings

Ticket: `.scratch/aloudtype/issues/05-ui-stack.md`. Researched 2026-11-22. Windows-only, Rust, minimal idle footprint, offline.

## Recommendation (short)

Do not put a UI framework in the resident process. Split by job:

| Surface | Choice | Why |
|---|---|---|
| Tray icon, double-click, context menu | `tray-icon` (+ its re-exported `muda` menu) on a plain Win32 message loop in the main thread | Windows-only `DoubleClick` event exists; tiny; no winit/GUI toolkit needed |
| Listening indicator (overlay) | Hand-rolled Win32 layered window (via the `windows` crate), drawn with `UpdateLayeredWindow` from a small CPU bitmap; animation timer runs only while Dictation is active | Exact control of `WS_EX_NOACTIVATE`, click-through, topmost; no GPU/driver memory; window is tiny |
| Startup notice | Same own-drawn layered window mechanism (fade in/out, auto-dismiss). Fallback: `Shell_NotifyIcon` balloon (`NIF_INFO`) | A WinRT toast from an unpackaged exe needs a Start-menu shortcut with an AppUserModelID and COM activator registration, which is installer-coupled complexity for one line of text |
| Settings window | A separate short-lived process (`aloudtype.exe --settings`) using **iced with the tiny-skia CPU renderer**; it reads and writes the config file and exits on close | Process exit frees 100% of the Settings window's memory by construction; no reliance on toolkit teardown behaviour |
| Fallback for Settings if a second process is rejected | In-process iced `daemon` with tiny-skia, window destroyed on close | Third-party reports show memory returning to single-digit MB in tray (see below) |

Not recommended: eframe/egui (default wgpu or glow) and Slint for the resident process; see numbers below. egui glow and Slint `software` are viable only as the short-lived Settings process; iced preferred because the Slint software renderer's text is "limited to western scripts" per Slint docs, which matters for a language picker showing native language names.

## 1. Tray icon

- `tray-icon` supports Windows/macOS/Linux; on Windows an event loop must be running on the thread. Menus come from the re-exported `muda` crate via `TrayIconBuilder::with_menu`; menu clicks arrive via `MenuEvent::receiver()`. Source: https://docs.rs/tray-icon/latest/tray_icon/
- `TrayIconEvent` has `Click`, `DoubleClick` ("Windows only"), `Enter`, `Move`, `Leave`; enum is `#[non_exhaustive]`. Source: https://docs.rs/tray-icon/latest/tray_icon/enum.TrayIconEvent.html
- Implementation (Windows): creates a hidden top-level window of class `tray_icon_app` with `WS_EX_NOACTIVATE | WS_EX_TRANSPARENT | WS_EX_LAYERED | WS_EX_TOOLWINDOW`; maps `WM_LBUTTONDBLCLK` etc. to `DoubleClick`; handles `TaskbarCreated` to re-add the icon after Explorer restarts; `Drop` calls `Shell_NotifyIconW(NIM_DELETE)` and destroys the window; `set_visible` uses `NIS_HIDDEN`. Source: https://github.com/tauri-apps/tray-icon/blob/dev/src/platform_impl/windows/mod.rs
- Win32 guidance: right-click should show a shortcut menu; left click does the primary action; icons should be high-DPI (16x16 and 32x32). Source: https://learn.microsoft.com/en-us/windows/win32/shell/notification-area
- Note for spec: Windows only lets the user promote icons out of the overflow; the app cannot force visibility (same page above).

Implication: a bare `GetMessage`/`DispatchMessage` loop on the main thread is enough to host `tray-icon`. winit is not required for the tray. (Inference from the "event loop must be running" requirement; verify at implementation.)

## 2. Overlay (click-through, always-on-top, non-focus-stealing, animated)

Win32 facts (Microsoft Learn, extended window styles): https://learn.microsoft.com/en-us/windows/win32/winmsg/extended-window-styles
- `WS_EX_NOACTIVATE`: top-level window "does not become the foreground window when the user clicks it"; not on the taskbar by default.
- `WS_EX_TOPMOST`: stays above non-topmost windows even when deactivated.
- `WS_EX_TOOLWINDOW`: not in taskbar or Alt+Tab.
- `WS_EX_LAYERED`: layered window (top-level and, since Windows 8, child windows).
- `WS_EX_TRANSPARENT`: documented there only as a paint-order effect relative to sibling windows. Mouse pass-through for layered windows is the commonly relied-on behaviour (the tray-icon crate uses the same flag combination for its hidden window) but this page does not state it; at implementation also return `HTTRANSPARENT` from `WM_NCHITTEST` and test against clicks in apps below. Flagged as to-verify.

Per-pixel alpha animation: `UpdateLayeredWindow` with `ULW_ALPHA` and a `BLENDFUNCTION` updates position, size, content and translucency; the OS repaints windows below automatically ("seamless animation"); it always updates the whole window, and "the layered window should be as small as possible" for best performance. Source: https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-updatelayeredwindow

Why not winit/egui for the overlay: winit exposes `with_active(false)`, `set_cursor_hittest`, `WindowLevel::AlwaysOnTop` (documented as "just a hint to the OS"), `with_transparent`, `with_skip_taskbar` and `with_no_redirection_bitmap` (sets `WS_EX_NOREDIRECTIONBITMAP`). Sources: https://docs.rs/winit/latest/winit/window/struct.Window.html (via raw `src/window.rs` on branch v0.30.x) and `src/platform/windows.rs` on the same branch. I could not confirm from the files read that winit applies `WS_EX_NOACTIVATE`, and its `transparent` path uses `DwmEnableBlurBehindWindow`, not a layered per-pixel-alpha surface (https://github.com/rust-windowing/winit/blob/v0.30.x/src/platform_impl/windows/window.rs). A toolkit window also pulls in a GPU or large CPU surface for an element that is a few hundred pixels wide. A raw layered window avoids both.

Animation budget: run a timer (e.g. ~30-60 Hz) only between key-down and end of Dictation; hide the window and kill the timer otherwise, so idle CPU is zero. Loudness feeds from the audio thread via an atomic value. Draw with `tiny-skia` (CPU, pure Rust) or direct pixel writes into a premultiplied-alpha DIB section.

## 3. Startup notice

- Unpackaged desktop apps can only raise WinRT toasts with a Start-menu shortcut carrying `System.AppUserModel.ID`; `CreateToastNotifier(appId)` must be passed every time; click activation needs a COM `INotificationActivationCallback` registration; desktop apps cannot schedule toasts. Source: https://learn.microsoft.com/en-us/windows/win32/shell/quickstart-sending-desktop-toast (and https://learn.microsoft.com/en-us/previous-versions/windows/desktop/legacy/hh802762(v=vs.85)).
- `Shell_NotifyIcon` can add the icon and show a notification in one call (`NIF_INFO`); title max 48 chars, body 200 chars (English); notifications during quiet time are discarded; the user can suppress them; system accessibility timeout overrides the requested timeout. Source: https://learn.microsoft.com/en-us/windows/win32/shell/notification-area
- Recommendation: own-drawn notice on the same overlay mechanism (consistent look, no registration, honours our own dismiss rules, works when the user has silenced notifications). If a native look is preferred, use the balloon path, accepting that it can be suppressed or dropped. Which one is a spec decision; both are cheap.

## 4. Settings window: footprint and freeing

Evidence is mostly third-party measurement (issues, discussions, PRs); the toolkits' own docs give few numbers. Treat as indicative, re-measure on a prototype.

**egui / eframe**
- Reactive repaint model: repaints only when requested (`request_repaint`, `request_repaint_after`). Source: https://docs.rs/egui/latest/egui/struct.Context.html
- Windows, `run_simple_native`, defaults off (egui issue 7761, data updated Apr 2026): Task Manager glow 31 MB / wgpu-Vulkan 90 MB / wgpu-DX12 142 MB; Working Set 54 / 129 / 176 MB; Private Bytes 137 / 438 / 400 MB; binary (release+LTO) 5.62 MB glow vs 11.3 MB wgpu; startup glow 100-300 ms vs wgpu 1 to ~7 s. Source: https://github.com/emilk/egui/issues/7761
- eframe defaults to wgpu; glow is opt-in (https://github.com/emilk/egui/blob/main/crates/eframe/README.md).
- Closing and reopening: current eframe reuses a thread-local winit event loop "so we can support closing and opening an eframe window multiple times. This is just a limitation of winit", driven by `run_app_on_demand`. Source: https://github.com/emilk/egui/blob/main/crates/eframe/src/native/run.rs. Related open friction: no running on an existing event loop (https://github.com/emilk/egui/issues/2875); wgpu buffers not freed while minimised (https://github.com/emilk/egui/issues/7434).
- Verdict: closable, but wgpu/GL driver state makes the footprint large; only acceptable in a separate process.

**Slint**
- Marketing claim: runtime "fits in less than 300KiB RAM" (microcontroller context): https://slint.dev/. Backends: winit (default), qt, linuxkms; renderers femtovg, skia, software (no GPU, lightweight, "text rendering is limited to western scripts", no rotation/scaling), Vello experimental. Selected via `SLINT_BACKEND=winit-software` or `renderer-*` features: https://docs.slint.dev/latest/docs/slint/guide/backends-and-renderers/backends_and_renderers/
- Windows measurements (discussion 3376, Win10, 1080p): femtovg 51.1 MB, skia-opengl 52.6 MB, skia-d3d 17.5 MB, winit-software 14.0 MB; maintainer says rendering buffers dominate and scale with window size. Source: https://github.com/slint-ui/slint/discussions/3376
- Freeing: a reporter on Linux saw memory stay at 167 MB after dropping the UI even with `SLINT_DESTROY_WINDOW_ON_HIDE=1` (https://github.com/slint-ui/slint/discussions/6718); Windows skia-opengl showed memory not freed right after hiding windows (https://github.com/slint-ui/slint/discussions/5854); a Slint 1.18.0 Windows regression took renderer-skia idle from ~19.5 to ~155.5 MB (https://github.com/slint-ui/slint/issues/13470). Not reliable for in-process freeing.
- Licensing: GPL, a royalty-free permissive desktop license, or paid; details on their pricing page (not verified here): https://slint.dev/

**iced**
- Multi-windowed `Program`; `iced::daemon` runs without a window ("daemons that run in the background"). Source: https://docs.rs/iced/latest/iced/
- Reported Windows numbers (third-party PR, private / working set, Windows 11 1440p): GPU renderer window open 262/175 MB; CPU tiny-skia window open 13/33 MB; in tray with window destroyed, 8 MB private and ~1-3 MB working set. Source: https://github.com/sionirvine/calen/pull/5. Other reports of tiny-skia vs wgpu: https://github.com/indep-arg/MimirDLP/pull/9, https://github.com/FAZuH/hyprlay/pull/21, https://github.com/yapfast/yapfast/pull/274 (not individually opened; found via search, treat as unverified). Caveats: one reporter saw a large CPU-renderer frame buffer cost on macOS (irrelevant to Windows-only), and one issue reports `tiny_skia::mask` warning spam under heavy UI trees (https://github.com/stencil-hq/tern-sdk/issues/393).
- Verdict: best evidence that a window can be destroyed and memory returned in-process; MIT-licensed; text shaping handles non-Latin scripts better than Slint software (not verified here, check at prototype).

**Native Win32 / `windows` crate**
- Smallest possible footprint, full control, but a Settings form with dropdowns, key capture, progress bar and tab order hand-built in Win32 is high effort. The `windows` crate is the official Microsoft binding, gated by Cargo features (docs stub: https://docs.rs/windows/latest/windows/; feature search https://microsoft.github.io/windows-rs/features). Use it for tray host loop, overlay, startup shortcut, and hotkey/hook work in sibling tickets.

## 5. Why a separate Settings process

- Guarantees "fully freed when closed" regardless of toolkit/driver quirks documented above (winit event-loop reuse, wgpu not freeing, Slint not releasing).
- Costs: launching takes tens to hundreds of ms (egui glow 100-300 ms startup per issue 7761; iced tiny-skia not measured); config handoff needs a protocol. Simple option: Settings writes the config file atomically; main process watches it (or receives a `WM_COPYDATA`/named-event ping) and reloads; model download progress is owned by the main process and exposed via a small named pipe, or the Settings process performs the download itself and writes the result.
- Single-instance behaviour: tray double-click when Settings is already open should focus the existing window (named mutex + `SetForegroundWindow` from the launcher, which is allowed because the tray click gives the launcher foreground rights; verify).

## 6. Open items for the spec / prototype

1. Measure real numbers on the target stack: main process idle (tray only) working set and private bytes; overlay active; Settings process open.
2. Confirm click-through and no-focus-steal of the layered overlay in fullscreen apps, elevated windows, and games (borderless vs exclusive fullscreen will not show topmost windows).
3. Decide Startup notice mechanism (own-drawn vs balloon).
4. Choose Settings toolkit (iced tiny-skia recommended) and confirm complex-script text rendering for the language list.
5. Decide Settings as separate process (recommended) vs in-process daemon.
6. Overlay position setting needs multi-monitor and DPI handling (per-monitor DPI awareness manifest).

## Source index

- tray-icon docs: https://docs.rs/tray-icon/latest/tray_icon/ ; events: https://docs.rs/tray-icon/latest/tray_icon/enum.TrayIconEvent.html ; Windows impl: https://github.com/tauri-apps/tray-icon/blob/dev/src/platform_impl/windows/mod.rs
- Win32 extended styles: https://learn.microsoft.com/en-us/windows/win32/winmsg/extended-window-styles
- UpdateLayeredWindow: https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-updatelayeredwindow
- Notification area: https://learn.microsoft.com/en-us/windows/win32/shell/notification-area
- Desktop toast quickstart: https://learn.microsoft.com/en-us/windows/win32/shell/quickstart-sending-desktop-toast
- winit: https://docs.rs/winit/latest/winit/window/struct.Window.html ; https://github.com/rust-windowing/winit/tree/v0.30.x
- egui: https://docs.rs/egui/latest/egui/struct.Context.html ; https://github.com/emilk/egui/issues/7761 ; https://github.com/emilk/egui/blob/main/crates/eframe/src/native/run.rs
- Slint: https://slint.dev/ ; backends doc above ; https://github.com/slint-ui/slint/discussions/3376
- iced: https://docs.rs/iced/latest/iced/ ; https://github.com/sionirvine/calen/pull/5
- windows-rs: https://docs.rs/windows/latest/windows/
