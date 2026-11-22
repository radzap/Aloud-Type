# Config persistence, first-run flow and Startup notice

Type: grilling
Status: resolved
Blocked by: 14

## Question

Where does the config file live, in what format, and how is it shared between the resident and Settings processes (atomic writes, change detection, migration)? What exactly happens on first run (Settings opens, model download, mic and key setup, completion criteria) and on later starts, and what does the Startup notice say and how long does it show?

## Answer

Decided with the owner:
- **Locations and format**: everything under `%LOCALAPPDATA%\AloudType\` (machine-specific settings, so not roaming). User settings in `config.toml` (readable, hand-editable TOML with an integer `version`); models in `models\`.
- **Who writes what**: two files with single writers. `config.toml` holds user settings and is written only by the Settings process. `state.toml` holds machine-generated values (GPU probe result per device and driver, first-run-complete flag, download bookkeeping) and is written only by the process that produced the value. All writes are atomic (temp file then rename). The resident process watches `config.toml`, applies changes live where it can (otherwise at the next Dictation), and on an invalid file keeps the last good settings and shows a tray notice.
- **Migration and hand-edits**: `version` migrates forward on read. A config from a newer app version is never overwritten: the app runs read-only on it and leaves a backup. Unknown keys are preserved when Settings saves, so hand-edits and hidden values (e.g. the 250 ms tap threshold) survive.
- **First-run completion**: setup is complete once a usable Speech model is installed. Until then the Settings window opens automatically on every start, with the Dictation key defaulted to Right Ctrl, the microphone on System default, and the base model download one click away.
- **Dictation key before setup is complete**: no "finish setup" message (owner's change to the proposal). The Settings window is brought to the front instead. (Interpretation of the owner's answer; revisit in the spec if wrong.)
- **Startup notice**: a small own-drawn notice for about 3 seconds at every start, saying "Aloud Type is running. Hold Right Ctrl to dictate." with the user's actual key name. It never takes focus and does not wait for the model to load. There is no setup variant: when setup is incomplete the Settings window opens instead.
