# Rust UI stack for tray, overlay, notice, and settings
Type: research
Status: resolved

## Question
Which Rust crates/stack (tray-icon, winit/tao, egui/iced/slint/native Win32) best deliver a tray icon with double-click and context menu, a click-through always-on-top animated overlay that does not steal focus, a toast-like Startup notice, and a Settings window, while keeping idle RAM/CPU minimal? Compare footprint and whether the Settings window can be fully freed when closed.

## Answer
Keep the resident process toolkit-free: `tray-icon` (+ `muda` menu) on a plain Win32 message loop gives tray, `DoubleClick` (Windows-only event) and context menu; the Listening indicator and Startup notice are own-drawn Win32 layered windows (`WS_EX_LAYERED | TRANSPARENT | NOACTIVATE | TOPMOST | TOOLWINDOW`, `UpdateLayeredWindow`, timer only while Dictation runs), so idle CPU is zero and no GPU memory is used. Settings runs as a separate short-lived process (`--settings`) using iced with the tiny-skia CPU renderer (~13 MB private when open per third-party reports; egui glow ~137 MB and wgpu 400+ MB private, Slint software ~14 MB but western-script text only), which guarantees it is fully freed on close; in-process iced `daemon` with window destruction is the fallback. WinRT toasts are rejected for the notice (need AUMID shortcut + COM activator). Open items: click-through confirmation, fullscreen behaviour, real measurements. Full findings and sources: [research/05-ui-stack.md](../research/05-ui-stack.md)
