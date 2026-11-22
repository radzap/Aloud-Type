# Research 03: Windows global key capture and text injection

Ticket: `../issues/03-windows-input-capture-injection.md`
Scope: Windows 10/11, Rust, minimal footprint, offline. Planning only; no code written.

Evidence labels used below:
- **[Doc]** stated by a first-party Microsoft Learn page (linked).
- **[Doc-silent]** the first-party page does not say; I am not asserting it.
- **[Judgment]** my engineering inference from the docs. This is not a verified fact.
- **[Spike]** must be verified on a real machine before the spec relies on it.

## 1. Recommendation (summary)

1. **Capture: one `WH_KEYBOARD_LL` low-level hook** on a dedicated thread that does almost no work. The callback timestamps the event, decides swallow or pass, posts the event to a channel, and returns. RegisterHotKey and Raw Input cannot meet the requirements (section 2).
2. **Hold and tap detection** is a small state machine fed by that hook, using the hook's own timestamps (section 3).
3. **Injection: `SendInput` with `KEYEVENTF_UNICODE`** (VK_PACKET), batched per word or phrase. It never touches the clipboard. Tag our own events with a magic `dwExtraInfo` so our hook ignores them (section 4).
4. **Elevated windows are a hard limit.** A normal-integrity app cannot inject into, or reliably capture keys from, an elevated window. The Microsoft docs say the API will not tell us it failed, so detect elevation up front and show a notice rather than failing silently. `uiAccess` is not worth it for v1 (section 5).
5. **Live-correct (Backspace) is feasible but fragile.** Ship it only as an explicitly experimental, opt-in mode, with hard safety rules (section 7). Stable stays the default.
6. **Rust bindings:** use `windows-sys` (raw, no wrappers) and hand-write about 150 lines of FFI. Do not add a general input-simulation crate (section 8).

## 2. Capture mechanisms compared

| Need | RegisterHotKey | WH_KEYBOARD_LL | Raw Input (WM_INPUT) |
|---|---|---|---|
| Key-up event (needed for hold) | No. It only posts WM_HOTKEY. [Doc] | Yes (`LLKHF_UP`). [Doc] | Yes (break flag). |
| Modifier-only key (e.g. Right Ctrl alone) | Modifiers are only combined with a separate `vk`. Modifier-only is not documented. [Doc-silent] | Yes. Sees every key including modifiers. [Doc] | Yes, receives them. |
| Swallow the key so the focused app never sees it | Yes for the registered combo. | Yes: return nonzero. [Doc] | No. `RIDEV_NOLEGACY` only suppresses legacy messages for the registering app, not for other apps. [Doc] |
| Works while another app has focus | Yes. | Yes (global). | Only with `RIDEV_INPUTSINK` plus a target window. [Doc] |
| Cost | Lowest. | Per-keystroke callback, but we control it. | Low, asynchronous. |

Sources:
- RegisterHotKey: https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-registerhotkey
- LowLevelKeyboardProc: https://learn.microsoft.com/en-us/windows/win32/winmsg/lowlevelkeyboardproc
- RAWINPUTDEVICE: https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-rawinputdevice
- Raw input overview: https://learn.microsoft.com/en-us/windows/win32/inputdev/about-raw-input

Findings:
- RegisterHotKey posts `WM_HOTKEY` when the combination matches. The docs describe no release notification, so hold-to-dictate cannot be built on it. `MOD_NOREPEAT` only suppresses auto-repeat notifications. It also fails if another app already registered the combination, and Win-key combinations are "reserved for use by the operating system". [Doc]
- Raw Input can run in the background (`RIDEV_INPUTSINK`), but the docs describe no way to stop other apps receiving the key. So the Dictation key would leak into the focused app. Harmless for an F13-F24 style key, but wrong for Ctrl/Alt/Win. [Doc, plus Doc-silent on blocking]
- The low-level hook is the only mechanism that gives us key-down, key-up, modifier-only keys and swallowing together. This is also how the AutoHotkey project implements non-trivial hotkeys: it states wildcard, "Up", custom-combination and tilde hotkeys "always use the keyboard hook", and modifier-only hotkeys fire on release unless prefixed `~`. https://www.autohotkey.com/docs/v2/Hotkeys.htm (first-party docs of a mature tool; evidence of practice, not an OS guarantee).
- Microsoft does recommend raw input over low-level hooks "in most cases", and warns hooks "tend to slow down the system" and global hooks "hurt system performance and cause conflicts with other applications". https://learn.microsoft.com/en-us/windows/win32/winmsg/about-hooks. We accept the hook anyway because raw input cannot swallow. We keep its cost minimal (below).

### Hook hazards and the mitigations they require

- **Message loop required.** The hook "is called in the context of the thread that installed it" via a message sent to that thread, so that thread must pump messages. [Doc]
- **Timeout and silent removal.** The callback must finish within `LowLevelHooksTimeout`. Since Windows 10 1709 the maximum allowed is 1000 ms. On Windows 7 and later a timed-out hook "is silently removed without being called. There is no way for the application to know whether the hook is removed." [Doc]
  - Mitigation [Judgment]: dedicated hook thread; the callback only classifies the event, pushes it to a lock-free channel, and returns in microseconds. Microsoft's own note says to run hooks "on a dedicated thread that passes the work off to a worker thread and then immediately returns."
  - Add a watchdog: if no event is seen after a long idle while `GetAsyncKeyState` shows a key down, or periodically, re-install the hook. Because removal is undetectable, a heartbeat is the only recovery path. [Spike: tune the interval]
- **Chain etiquette.** Always call `CallNextHookEx` for events we do not consume, or other apps' hooks starve. [Doc]
- **Async key state inside the callback.** The callback runs before the key's async state updates, so `GetAsyncKeyState` is not valid there. Keep our own down/up table. [Doc]
- **Injected input is visible and flaggable.** `KBDLLHOOKSTRUCT.flags` has `LLKHF_INJECTED` (bit 4) and `LLKHF_LOWER_IL_INJECTED` (bit 1), plus `dwExtraInfo`. https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-kbdllhookstruct. Our hook must ignore its own injected typing, using the injected flag plus a magic `dwExtraInfo` tag so it does not treat our output as a Dictation key press. It should still treat other injected input (remapper tools) normally. [Doc, design is Judgment]
- **Hook installed by a non-elevated process does not see keys going to elevated windows.** See section 5. [Spike]

## 3. Hold vs tap detection, modifier-only keys

Design [Judgment, built on the Doc facts above]:

- State machine per configured Dictation key: `Idle -> Pending(down_ts) -> Holding | Latched`.
  - Key-down: record timestamp, enter Pending, start dictation capture immediately (so no first-word loss).
  - Key-up before the hold threshold: this was a tap. Enter Latched (hands-free) and keep dictating; the next tap ends it.
  - Key still down after the threshold: Holding. Key-up ends the Dictation.
  - The hold threshold is a spec parameter (ticket 08). A typical starting range is 200-300 ms. [Spike: tune with real users]
- Use the event timestamps from `KBDLLHOOKSTRUCT.time`, not wall-clock reads in the worker, to avoid jitter from the hand-off. [Judgment]
- **Auto-repeat:** the hook delivers repeated key-downs while the key is held (the "previous key state" bit exists only for window messages, https://learn.microsoft.com/en-us/windows/win32/inputdev/about-keyboard-input). Track our own down-state and ignore repeated downs. Do not rely on a repeat flag in the hook.
- **Swallowing:** for the Dictation key, return nonzero from the callback on both down and up so the focused app never sees it (and so held-key auto-repeat does not type). [Doc: nonzero return stops propagation]
- **Modifier-only keys (Ctrl, Alt, Shift, Win):** capture works in the hook, but two side effects need handling.
  1. **Menu/Start activation.** A lone Alt or Win press-release activates the menu bar or Start menu. AutoHotkey sends a "mask key" (default left Ctrl) so that a swallowed Win/Alt tap does not trigger the menu: "to prevent the Start menu or the active window's menu bar from activating at unexpected times." https://www.autohotkey.com/docs/v2/lib/A_MenuMaskKey.htm. Plan for the same trick if Alt/Win are allowed as Dictation keys. [Doc of a third-party tool; Spike on current Windows 11]
  2. **Modifier held while we type.** Streaming insertion types while the user is still holding the key (hold mode). If that key is Ctrl/Alt/Win/Shift and it was passed through, the target app sees "Ctrl held" and may treat injected characters as shortcuts. `SendInput` itself "does not reset the keyboard's current state. Any keys that are already pressed ... might interfere with the events that this function generates." https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput. Swallowing the modifier's down event in the hook should keep it out of the target's key state, but this is not documented. [Spike]
  - Consequence for ticket 08: prefer a non-modifier default Dictation key (an F-key, F13-F24, Pause, Scroll Lock, or a mouse-adjacent key). Offer modifier-only choices as an advanced option with the caveats above. [Judgment]
- Key combinations (e.g. Ctrl+Space) as the Dictation key: the hook must track modifier state itself and swallow the combo's final key. Specify "key combination = N modifiers + one non-modifier key" and treat "modifier-only" as a single-key special case.
- Rebinding UI: a "press a key" capture field can reuse the same hook (the Settings process listens in capture mode). No separate mechanism needed. [Judgment]

## 4. Text injection

### Method: `SendInput` + `KEYEVENTF_UNICODE`

- `KEYBDINPUT` with `KEYEVENTF_UNICODE` sets `wVk = 0` and puts a UTF-16 code unit in `wScan`; the system "synthesizes a VK_PACKET keystroke". The docs explicitly say this "supports nonkeyboard-input methods - such as handwriting recognition or voice recognition - as if it were text input". The foreground thread gets `WM_KEYDOWN`/`WM_KEYUP` with `VK_PACKET`, and `TranslateMessage` turns it into `WM_CHAR` with the original character. https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-keybdinput [Doc]
- The flag "can only be combined with `KEYEVENTF_KEYUP`", so each character is a down+up pair. [Doc]
- `SendInput` inserts the events in one call "serially" and they "are not interspersed with other keyboard or mouse input events" from the user or other `SendInput` calls. That atomicity is useful: a whole word cannot be split by the user's own keystroke. [Doc] https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput
- Characters beyond U+FFFF (emoji, some CJK extensions) must be sent as two UTF-16 code units (a surrogate pair), both inside the same call so they are never separated. The Microsoft page does not spell this out for KEYEVENTF_UNICODE (only that the touch keyboard uses surrogates); the surrogate behaviour is established by community reports, not a first-party statement. [Doc-silent; Spike] Whisper output is mostly BMP text, so the risk is small.
- Layout independence: because the character is carried in `wScan`, the active keyboard layout does not change the result. This is why it is preferred over VK-based typing. [Judgment, consistent with the Doc]

### Clipboard avoidance

The default is no clipboard. Reasons: no clobbering the user's clipboard, no clipboard-history (Win+V) leaks, no dependence on paste shortcuts that differ per app (Ctrl+V vs Ctrl+Shift+V in terminals). [Judgment] A paste fallback (save clipboard, set text, send paste, restore) is a possible opt-in per-app workaround but is out of v1 scope unless the spike finds an important app that drops VK_PACKET. [Spike]

### Throughput and pacing

- Send per recognised word/phrase batch, not per key. The Microsoft docs give no rate limit. Some apps drop or reorder under floods. [Doc-silent] Plan a chunk size cap (start around a few dozen characters per call) and measure in a browser, Electron app, terminal and Office. [Spike]
- Check the return value: it is "the number of events that it successfully inserted"; zero means "the input was already blocked by another thread". [Doc]

### Apps that reject or mishandle injected input [Judgment + Spike; Microsoft docs do not enumerate]

- Apps that read hardware-level input (many games, anti-cheat, some remote-desktop clients and VM consoles) may ignore synthesized input entirely.
- Apps that ignore `VK_PACKET` or read only `WM_KEYDOWN` virtual keys (some custom-drawn controls, legacy editors) will show nothing.
- Elevated windows (section 5).
- Terminals and consoles: the Windows console path handles injected characters differently; test conhost and Windows Terminal. [Spike]
- IMEs: with an active East Asian IME composition, behaviour of injected packets versus composition is not documented in the pages I fetched. The Text Services Framework page describes TSF as the system framework for keyboard, handwriting and speech text services (https://learn.microsoft.com/en-us/windows/win32/tsf/text-services-framework) and says nothing about VK_PACKET interplay. [Doc-silent; Spike] English default makes this low priority for v1; document as a known limitation if Auto language is used with a CJK IME active.
- Safety against our own hook: tag events with a magic `dwExtraInfo` and ignore them in the hook (section 2).

## 5. Elevated windows and UIPI

- "This function is subject to UIPI. Applications are permitted to inject input only into applications that are at an equal or lesser integrity level." And critically: "This function fails when it is blocked by UIPI. Note that neither GetLastError nor the return value will indicate the failure was caused by UIPI blocking." https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput [Doc]
- So text typed into an app running "as administrator" is silently lost when AloudType runs at normal integrity. `GetAsyncKeyState` can also fail under UIPI. https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getasynckeystate [Doc]
- Hook side: hooks and key capture across the integrity boundary are also affected by UIPI. Real-world behaviour (a normal-integrity low-level hook not seeing keys while an elevated window has focus) is widely reported, but the Microsoft pages I fetched do not state it directly. [Spike]
- **Ways out:**
  - Run AloudType elevated: works but is a bad default (UAC prompt at login, larger attack surface, violates minimal footprint philosophy). Not recommended. [Judgment]
  - `uiAccess="true"` manifest: lets the app drive higher-IL UI, but requires an Authenticode signature, install in a secure location such as Program Files, and "UIAccess should not be used by applications that are not assistive technologies". Also, UIAccess launched by a non-admin user only reaches "medium+" and still "cannot access elevated UI". https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-securityoverview [Doc]. Reject for v1: signing and policy cost, and a dictation tool is a borderline case for the "assistive technology" rule.
- **Recommended behaviour:** before injecting, inspect the foreground window's process token (integrity level / elevation). If higher than ours, do not type; show a Listening-indicator-style notice ("cannot type into this elevated window"). Silent loss is the worst outcome. Add to the "Error and edge cases" item in the map. [Judgment, grounded in the Doc statement that failure is silent]
- Secure desktop (UAC prompts, lock screen) is SYSTEM-level; UIAccess has no effect there. Out of scope. [Doc]

## 6. Interaction effects to design around

- **Own-key feedback loop:** our injected characters pass through every global hook, including ours. Filter on the injected flag and the tag. [Doc: flags exist]
- **Other keyboard tools** (remappers, other hooks) sit in the same chain; hook order is last-installed-first. Always forward unconsumed events. [Doc]
- **Focus changes mid-Dictation:** `SendInput` goes to whichever window has focus at call time. If the user alt-tabs while speaking, text lands in the new window. Option: record the target window at Dictation start and stop/pause typing if the foreground window changes. [Judgment]

## 7. Live-correct: sending Backspace

### What is technically possible

- Backspace is just `SendInput` with `VK_BACK` (down+up). Doc support exists for injecting virtual-key events. [Doc: KEYBDINPUT, SendInput]
- Atomic batches mean "N backspaces then new text" can go as one call without user keystrokes interleaving. [Doc]

### Why it is fragile (all [Judgment] unless noted; none are first-party guarantees)

1. **No read-back.** We cannot reliably know what is actually in the target field, only what we sent. Any divergence (autocomplete, auto-correct, input masks, max-length, auto-indent, smart quotes, "replace selection" fields) makes Backspace delete the wrong thing.
2. **Backspace semantics differ.** One Backspace may remove a UTF-16 unit, a code point, or a whole grapheme cluster (emoji, combining marks) depending on the app. Restrict Live-correct to simple BMP characters and track by characters we typed, not bytes.
3. **User interference.** If the user moves the caret, clicks, or types during recognition, Backspaces corrupt their text. The hook can detect real user keystrokes (non-injected) and mouse use is detectable with a second hook (extra cost) or by checking foreground window changes. A single real key or focus change must abort correction and fall back to Stable for the rest of that Dictation.
4. **Undo history pollution.** Rapid type-delete-type fills the app's undo stack with churn, and some apps treat Backspace-bursts as separate undo steps. User impact: Ctrl+Z becomes noisy.
5. **Visible flicker and cost.** Each correction is visible text jitter, and consumes `SendInput` throughput.
6. **Elevated windows and apps dropping events** make half-applied corrections possible (backspaces applied, replacement text lost). `SendInput` reports only a count of events inserted, not that the app processed them. [Doc: return value is events inserted]
7. **Editors with special Backspace** (indent-aware deletion in code editors, tab-stop deletion, chat boxes that reformat) break the 1 Backspace = 1 character assumption.

### Guardrails if it ships (to feed ticket 07)

- Opt-in, labelled experimental; Stable remains default (as already settled in the map).
- Only rewrite a bounded **tail** (the last few words not yet "stabilised"), never earlier text. Ticket 07 (streaming stabilisation rules) defines the window.
- Keep an exact count of characters typed in the current unstable tail; Backspace only that many.
- Abort-and-freeze (switch to Stable for the rest of the Dictation) on: any non-injected key or mouse click detected, foreground window change, `SendInput` returning fewer events than requested, or elevated target.
- Send the Backspaces and replacement text in one `SendInput` batch to keep it atomic with respect to user keystrokes.
- Exclude known-bad targets by default (terminals, code editors, password fields) if the spike confirms problems. [Spike]

**Verdict:** feasible for plain-text fields in mainstream apps, high-risk in general. Do not make it default. Ship only if the spike shows acceptable behaviour across the target app set (browser text fields, Electron chat apps, Notepad, Word, a terminal); otherwise defer past v1. This is a decision for the spec; the research outcome is "possible, guarded, experimental".

## 8. Rust implementation notes

- Bindings: `windows-sys` (raw C-style bindings) or `windows` (typed). Both Apache-2.0 / MIT. The windows-rs maintainers advise reusable libraries to avoid the umbrella crates for dependency weight and recommend focused or generated bindings; for a single binary app `windows-sys` is the lightest. https://github.com/microsoft/windows-rs. Required API areas: `SetWindowsHookExW`, `CallNextHookEx`, `SendInput`, `GetMessageW`, token/integrity query (`OpenProcessToken`, `GetTokenInformation`), `GetForegroundWindow`, `GetWindowThreadProcessId`. Feature names to be confirmed from the crate's own docs at implementation time.
- `enigo` (MIT) is an input-simulation crate that supports Windows, but its README does not document how it injects text on Windows, so I cannot confirm it uses the VK_PACKET path or tags events. https://github.com/enigo-rs/enigo. Recommendation: hand-write the small amount of FFI. This avoids an unknown dependency, keeps the binary small, and lets us tag `dwExtraInfo`. [Judgment]
- Threading: hook thread (message loop, callback only classifies and sends to a channel) -> state-machine thread (hold/tap) -> orchestrator. Injection runs on a separate worker, not inside the hook callback, to avoid the 1000 ms timeout and re-entrancy.
- Platform seam (map requirement): define a small `Input` trait with `events() -> Stream<KeyEvent>`, `swallow(rule)`, `type_text(&str)`, `backspace(n)`, `foreground_is_elevated()`. Windows implements it; the seam hides the above.

## 9. Open items for the spike (verify before the spec locks)

1. Normal-integrity LL hook behaviour while an elevated window has focus (does it receive events? does swallowing still work?).
2. Swallowing a modifier Dictation key: does the target app's key state ever show the modifier down? Does Alt/Win masking still prevent menu/Start on current Windows 11?
3. Hook watchdog: confirm undetectable removal behaviour and choose a reinstall heartbeat.
4. `SendInput` chunk size/pacing in Chromium, Electron, Notepad, Word, Windows Terminal, conhost.
5. Surrogate pairs and non-BMP characters in the same targets.
6. IME composition interplay (CJK IME active) with VK_PACKET.
7. Live-correct Backspace behaviour in the same app set; undo-stack impact; grapheme-cluster deletion.
8. Whether Raw Input sees our own injected events (relevant only if raw input is ever used for rebind capture).

## Sources

First-party (Microsoft Learn):
- SendInput: https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput
- KEYBDINPUT: https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-keybdinput
- KBDLLHOOKSTRUCT: https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-kbdllhookstruct
- LowLevelKeyboardProc: https://learn.microsoft.com/en-us/windows/win32/winmsg/lowlevelkeyboardproc
- Hooks overview: https://learn.microsoft.com/en-us/windows/win32/winmsg/about-hooks
- RegisterHotKey: https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-registerhotkey
- Raw input overview: https://learn.microsoft.com/en-us/windows/win32/inputdev/about-raw-input
- RAWINPUTDEVICE: https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-rawinputdevice
- Keyboard input overview: https://learn.microsoft.com/en-us/windows/win32/inputdev/about-keyboard-input
- GetAsyncKeyState: https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getasynckeystate
- UIAccess / assistive tech security: https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-securityoverview
- Text Services Framework: https://learn.microsoft.com/en-us/windows/win32/tsf/text-services-framework

Project documentation used as evidence of practice (not OS guarantees):
- AutoHotkey v2 hotkeys: https://www.autohotkey.com/docs/v2/Hotkeys.htm
- AutoHotkey A_MenuMaskKey: https://www.autohotkey.com/docs/v2/lib/A_MenuMaskKey.htm
- windows-rs: https://github.com/microsoft/windows-rs
- enigo: https://github.com/enigo-rs/enigo
