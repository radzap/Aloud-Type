# Dictation key defaults and conflict handling
Type: grilling
Status: resolved
Blocked by: 03

## Question
What are the default key(s) for hold and tap-to-latch, what tap/hold thresholds apply, how are conflicts with other apps and modifier-only keys handled, and how does the rebind UI capture a key?

## Answer

Settled by grilling with the owner.

- **Single Dictation key.** One key (or combination) does both modes: hold = dictate while held; tap = latch hands-free, next key-down ends it. No separate Hands-free key in v1 (owner dropped it; this supersedes the map note "a separate key may be assigned to each mode").
- **Default:** Right Ctrl. If the keyboard has no Right Ctrl, first-run Settings shows the field empty and asks the user to press a key.
- **Threshold:** 250 ms fixed. Release before it = tap (latch); past it = hold, release ends. In latched mode the next key-down ends the Dictation with no threshold. Stored as a hidden config-file value; the spike tunes it.
- **Chord rule (modifier Dictation key):** the key is swallowed on key-down so capture starts at once. If any other key goes down while it is physically held, cancel that Dictation, discard its audio, and replay the modifier key-down plus the other key so the shortcut still works. Text already typed stays (no backspace). Does not apply to a latched Dictation.
- **Bindable keys:** a single key or a combination (modifiers plus one key), left/right modifiers distinguished. Rejected: lone typing keys (letters, digits, punctuation, Space, Enter, Tab, Backspace, Esc), Win+L, Ctrl+Alt+Del. Warned but allowed: known OS/app shortcuts (Alt+Tab, Win+letter, Ctrl+Space, Alt+F4), Right Alt (AltGr), lone Alt or Win (these get the mask-key trick so the menu bar and Start menu stay closed). No mouse buttons in v1. Copilot key is captured as whatever combination it sends.
- **Conflicts with other apps:** no detection, only the static warning list above. Our hook swallows the key, so our binding wins; if another tool's hook eats the key first, the symptom is "the key does nothing", covered by a help line in Settings.
- **Rebind capture:** click the field to enter capture ("Press a key..."). The Settings process tells the resident process to suspend Dictation keys until capture ends. The binding is recorded on release as the set of keys held at the peak. Esc or a 10 s timeout cancels. A rejected key shows its reason inline and capture continues. No Clear button, since the single key can't be empty.
- **No cancel key** during a Dictation in v1; Esc passes through to the app. Ending the Dictation is the only exit.

Spike items carried over from research 03: modifier swallowing vs target key state, mask-key behaviour on current Windows 11, hold threshold tuning.
