# 14: Overlay: Bar pill, Orb and positions

**What to build:** The two other indicator styles (Bar pill and Orb), each with a fault layout that fits the reason text, and all six positions (Top or Bottom with Left, Center or Right), with per-monitor DPI and taskbar-aware placement, selectable through the settings values.

**Blocked by:** 13 Overlay: Trace strip

**Status:** resolved

- [x] All three styles render and fault correctly at several DPIs
- [x] All six positions place the overlay correctly relative to the taskbar and monitor
- [x] Style and position changes apply to the next Dictation without a restart

## Answer

Done (TDD), in `crates/platform-windows` overlay modules only (`overlay_model.rs`, `overlay.rs`); no new dependencies.
- Model: `Style` (BarPill, Orb, TraceStrip; `Style::from_setting("bar-pill"|"orb"|"trace-strip")`), `Placement::from_settings(vertical, horizontal)` for the config strings (no config-crate dependency, as in 13). `look(indicator, style)` gives each style's listening size (strip 150x22, pill 76x26, orb 40x40) and fault layouts: pill 300x52 (40 without hint), orb 310x56 with the orb kept on the left and text to its right, amber 44/62 high with the one/two-line decision computed from the text width available per style (`Look.one_line`). Renderer: pill shape, seven centre-first bars, orb halo (grows with loudness) plus steady disc, bodyless orb while listening, generic anti-aliased rounded-rect/circle blending. `Animator::set_styled` latches the style only when the overlay appears, so a style change applies to the next Dictation; faults grow from the style's own listening size. Text layout is now centred from `Look` insets and height.
- Host: `OverlayHandle::set_style(Style)` and the existing `set_placement(Placement)`, both read on each frame; no restart. All six positions use the monitor work area (taskbar-aware, any taskbar edge) and DPI scaled margin (tested in `overlay_model`).
- Tests: 13 new model tests (styles, sizes, fault growth, pixels, DPI 1.5/2.0, six positions incl. left taskbar, setting-value mapping, style-latching) plus an extended real-window test (150x22, then 76x26 and 40x40 after switching style across Dictations). Whole workspace passes.

Gaps (need on-device checking, ticket 23): look of the pill and orb (colours, glyph proportions, text fit; character width is estimated, so a very long message may wrap differently than predicted); the prototype's mic and mic-slash icon is not drawn in the pill or orb faults; real multi-monitor/mixed-DPI and taskbar on each edge (placement maths tested, only the single-monitor Win32 path ran; DPI is read from the foreground window's monitor when the overlay appears); manifest per-monitor DPI awareness belongs to the host executable; wiring Settings values into `set_style`/`set_placement` is the integration ticket's job.
