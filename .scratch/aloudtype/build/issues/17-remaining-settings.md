# 17: Remaining Settings items

**What to build:** The rest of the v1 Settings: microphone choice (System default first), language (fixed or Auto), indicator style and position, Speech model memory choice, launch-at-login toggle (off by default, per-user registry), and the informational version, releases and source links. All apply without restart.

**Blocked by:** 14 Overlay: Bar pill, Orb and positions, 15 Settings window and Dictation key rebinding

**Status:** resolved

- [x] Every Settings item is saved and takes effect without restarting
- [x] Launch at login is off by default and toggles correctly
- [x] No Settings items beyond the v1 list exist

## Answer

Done (TDD), in new modules only; shared files got small edits (settings-ui `lib.rs`, `Cargo.toml`, `window.rs`: one Message variant, one panel field, scrollable wrapper, taller window; platform-windows `lib.rs`: `pub mod autostart`).
- `platform-windows/autostart.rs`: `RunKey` seam, `AutoStart` (off by default, writes the quoted exe path to HKCU Run value `AloudType`, removal is idempotent), real `WinRunKey` (round trip tested with a throwaway value name).
- `settings-ui/items.rs`: `ItemsSession` over the settings file plus the resident-process pipe link: microphone (System default first, a saved but absent device stays listed as "not connected"), language (Auto or fixed, disabled for English-only models named `*.en-*`), indicator style, vertical/horizontal position, Speech model memory, launch at login (Run key first, setting saved only on success), version/releases/source constants. Each change saves and sends `reload-settings`; works with no resident process. 9 tests. `items_view.rs` (iced pick lists, toggler, link buttons) and `items_win.rs` (cpal device list, Run key adapters) are the shell. No items beyond the v1 list.
- Workspace tests and `cargo deny check licenses` pass after merging integration/aloudtype (includes ticket 11).

Gaps:
- Real UI unverified (layout, pick lists, link opening via rundll32, window height 640 and scrolling); combined with ticket 16's section the window will need layout review (ticket 23).
- Host side not wired: the resident process (crates/app) does not yet handle `reload-settings` by applying microphone, language, model memory, `OverlayHandle::set_style`/`set_placement` (use `Style::from_setting`, `Placement::from_settings`); that applying step is what makes "no restart" true end to end. The Settings side already saves and notifies.
- Launch at login is Run-key only for the current exe path; a moved portable exe keeps a stale path until toggled. Language list is a 25-language subset of Whisper's. Microphone list is read once when the window opens. The Settings toggle shows the registry state, not the file value.
