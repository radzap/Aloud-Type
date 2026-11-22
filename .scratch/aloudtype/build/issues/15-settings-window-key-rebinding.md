# 15: Settings window and Dictation key rebinding

**What to build:** A short-lived settings process opened from the tray that talks to the resident process over the named pipe and edits the user settings file. First setting: click-to-capture rebinding of the Dictation key with the rules (single key or combination, left and right modifiers distinguished, reject lone typing keys, Win+L and Ctrl+Alt+Del, warn on system shortcuts, Esc or 10 s timeout cancels, the resident hook suspended while capturing, the help line for intercepted keys).

**Blocked by:** 06 Config and state files, 07 Windows keyboard hook, 12 Tray and Startup notice

**Status:** resolved

- [ ] Settings opens from the tray and frees all memory when closed
- [ ] Rebinding works end to end and applies without restart
- [ ] Rejected and warned keys show the right messages
- [ ] Settings works when the resident process is not running

## Answer

Partly done (TDD, vertical slices), in `crates/settings-ui` only (deps: `aloud-type-config`, dev `tempfile`). Seams tested: `rules::validate`, `capture::Capture` (events + caller-supplied clock), and `session::RebindSession` (fake `Transport` + real `ConfigFile` in a temp dir). 25 tests.
- `rules`: rejects lone typing keys (letters, digits, punctuation, Space, Enter, Tab, Backspace, Esc, also with only Shift), Win+L, Ctrl+Alt+Del, empty and multi-non-modifier bindings, each with a message; warns for Alt+Tab, Alt+F4, Win+letter, Ctrl+Space, Ctrl/Alt+Esc, Right Alt, lone Alt/Win.
- `keys`: VK to stored key names (`RightCtrl`, `A`, `F5`; left/right modifiers distinct) and a canonical order (modifiers, then key).
- `capture`: binding is the peak held set recorded on full release; Esc cancels; 10 s timeout cancels (restarts after a rejection, which shows its reason and keeps capturing).
- `ipc`: line protocol (`suspend-keys`, `resume-keys`, `reload-settings`, reply `ok`), `Transport` trait, best-effort `ResidentLink`, and `PipeClient` (connects per request to `\.\pipe\AloudType` via std file open).
- `session`: begin_capture sends suspend; Done saves `dictation_key` via `ConfigFile::update` then sends reload; every end path (Done, Esc, timeout, drop mid-capture) sends resume; with the resident process absent it still saves.

Gaps (not verified or not built):
- No GUI window yet (iced + tiny-skia per research 05), no help line text rendering, no `--settings` process entry/tray spawn wiring, no memory-freed check. The pure logic above is the model the window should drive; the window must feed raw key events (needs a keyboard source in the Settings process, e.g. its own focused-window key messages, since the resident hook is suspended).
- Resident side not built: the pipe server, handling suspend/resume/reload, auto-resume on pipe close, and reloading settings. The hook (ticket 07) takes a single VK; combinations and Right Alt/lone Alt mask handling for combos are not supported there yet, so a combination binding saves but will not work until the hook is extended.
- `PipeClient` against a live pipe server untested (only the not-running case). Help line wording for intercepted keys is not written. The "applies without restart" criterion depends on the resident reload wiring.
