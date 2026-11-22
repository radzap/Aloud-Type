# Spec: Aloud Type

Status: ready-for-agent

## Problem Statement

People who want to get words into a computer faster than they can type have two poor choices on Windows. Built-in dictation is limited and inconsistent between apps, and most polished voice-typing tools send the user's voice to a cloud service, require an account, or cost money. Someone who wants to speak into any text field (a browser, an editor, a chat app, a form) and see the words appear as they speak has no free, private, fast option that stays out of the way.

The user needs a small background app that listens only when asked, turns speech into text entirely on their own machine, types it into whichever field has focus as they speak, and tells them clearly whether it is hearing them. It must cost almost nothing while idle, make no network calls after the one-time download of its speech model, and keep working without an account, subscription or telemetry.

## Solution

Aloud Type is a free, open-source (AGPL-3.0) Windows app written in Rust. It lives in the system tray with no main window. When the user holds or taps their Dictation key, a small overlay shows that the app is listening, and the words they speak are recognized locally by a Whisper speech model and typed into the focused text field as they are confirmed, roughly one to two seconds behind speech. Releasing the key (or tapping again in hands-free mode) ends the Dictation and flushes the remaining words.

If the app cannot hear the user (no microphone, blocked access, no sound), the overlay turns red, grows and says why. If it cannot deliver text (a protected window, a model too slow for the machine), the overlay shows an amber message. A Settings window, opened from the tray, holds the few choices that matter: the Dictation key, microphone, Speech model, language, GPU use, model memory behaviour, launch at login, and the look and position of the overlay. The app downloads its Speech model once, then never touches the network again.

## User Stories

### Dictating

1. As a user, I want to hold a key and speak, so that my words are typed into the text field I am working in without switching apps.
2. As a user, I want to tap the Dictation key to start a hands-free Dictation and tap again to end it, so that I can dictate long passages without holding a key.
3. As a user, I want the same single key to do both hold and tap, so that I only have to remember one key.
4. As a user, I want my words to appear while I am still speaking, so that I get immediate feedback instead of waiting until I finish.
5. As a user, I want words typed only once the app is confident of them, so that my text is never corrupted by later corrections or backspacing.
6. As a user, I want the last word held back until it is confirmed, so that I do not get half-recognized words.
7. As a user, I want all remaining words typed within about a second of ending a Dictation, so that nothing I said is lost.
8. As a user, I want punctuation and capital letters from the speech model used as it produces them, so that I do not have to dictate punctuation by hand.
9. As a user, I want exactly one space between consecutive chunks of dictated text and no extra spaces at the start or end, so that dictation fits cleanly into whatever I am writing.
10. As a user, I want text typed into whichever window has focus at the moment it is typed, so that the app behaves like a keyboard I am speaking into.
11. As a user, I want to keep using my real keyboard during a Dictation without cancelling it, so that I can mix typing and speaking.
12. As a user, I want silence and background noise not to produce invented words, so that stray text such as made-up thank-yous never appears in my document.
13. As a user, I want a long monologue to keep working, so that I can dictate for minutes at a time.
14. As a user who uses the Dictation key as a modifier in shortcuts, I want my shortcut to still work when I press another key while holding it, so that the app does not break my existing habits.
15. As a user, I want a Dictation started by accident through such a shortcut to be cancelled without leaving stray audio processing, so that nothing surprising is typed.

### Seeing that it is listening

16. As a user, I want a small overlay while I dictate that moves with the loudness and variation of my voice, so that I know the app is hearing me.
17. As a user, I want the overlay to be click-through and never steal focus, so that it does not interrupt what I am doing.
18. As a user, I want to choose the overlay style from a bar pill, an orb or a trace strip, so that I can pick the one I find least distracting.
19. As a user, I want the trace strip as the default style, so that the app has a good look out of the box.
20. As a user, I want to choose where the overlay appears (top or bottom, left, center or right), so that it stays out of the way of my work.
21. As a user, I want the overlay to appear and disappear with a short smooth animation, so that it feels polished and not jarring.
22. As a user on a multi-monitor or high-DPI setup, I want the overlay placed sensibly and sized correctly, so that it is visible and not blurry.
23. As a user, I want the overlay to cost nothing when no Dictation is running, so that the app does not use resources while idle.

### Listening faults (red)

24. As a user, I want the overlay to turn red, grow, and show a reason when no microphone is found, so that I know why nothing is happening.
25. As a user, I want a clear message when my microphone was disconnected, so that I know to plug it back in.
26. As a user, I want a clear message when another app is using the microphone exclusively, so that I know what to close.
27. As a user, I want a clear message when Windows is blocking the microphone, so that I know the cause is system privacy settings.
28. As a user whose microphone is blocked, I want a tray menu entry that opens Windows microphone privacy settings, so that I can fix it in one step.
29. As a user, I want a message when my microphone is muted or delivers only silence, so that I can check the mute switch.
30. As a user, I want a "No sound detected" message if I start a Dictation and nothing is heard in the first few seconds, so that I notice a dead microphone quickly.
31. As a user, I want "No sound detected" to clear by itself as soon as my voice is heard, so that a slow start does not end my Dictation.
32. As a user, I want a general "Microphone error" fallback for unusual failures, so that I am never left with a silent failure.
33. As a user, I want my chosen microphone replaced by the system default for that Dictation with a brief note if it is missing, so that I can still dictate.
34. As a user, I want text already typed to stay and audio captured before an ending fault to still be typed, so that a fault does not discard my work.
35. As a user in hold mode, I want a fault to remain visible for a moment after I release the key, so that I can read it.
36. As a user in hands-free mode, I want a fault to dismiss itself after a few seconds or on my next tap, so that it does not linger.
37. As a user, I want the tray icon to show a fault state with the reason in its tooltip after an ending fault, so that I can see what went wrong even after the overlay is gone.
38. As a user, I want to retry simply by pressing the Dictation key again, so that I do not need any special recovery step.

### Typing faults (amber)

39. As a user, I want a clear amber message when the focused window is running as administrator and the app cannot type into it, so that I understand why dictation does nothing there.
40. As a user, I want a Dictation to end with an amber message when typing is rejected partway, so that I know the target window does not accept the text.
41. As a user on a slow machine, I want an amber message telling me to choose a smaller model when the speech model cannot keep up, so that I can fix the problem myself.
42. As a user, I want red to always mean "cannot hear me" and amber to mean "cannot type", so that I can tell the two kinds of problem apart at a glance.

### Tray, startup and single instance

43. As a user, I want the app to live in the system tray with no main window, so that it stays out of the way.
44. As a user, I want to double-click the tray icon to open Settings, so that configuration is one gesture away.
45. As a user, I want a tray context menu with Settings and Exit, so that I can configure or quit the app from the tray.
46. As a user, I want a brief startup notice that names my Dictation key, so that I know the app is running and how to use it.
47. As a user, I want the startup notice to appear without stealing focus and without waiting for the speech model to load, so that my launch is not disrupted.
48. As a user, I want launching the app a second time to open Settings in the running instance, so that I never end up with two copies fighting over my keyboard.
49. As a user, I want the tray icon to look different when idle, listening, and faulted, so that its state is readable at a glance.
50. As a user, I want any Dictation in progress to end cleanly when my PC suspends and the app to recover after resume, so that sleep never leaves it in a broken state.
51. As a user, I want the app to keep working if its keyboard hook is silently removed by Windows, so that the Dictation key does not just stop working.
52. As a user, I want a tray fault state with a "Dictation key unavailable" tooltip if the hook cannot be restored, so that I know why the key does nothing.

### First run and setup

53. As a new user, I want Settings to open automatically on first start, so that I am guided straight to setup.
54. As a new user, I want a sensible Dictation key, microphone and model preselected, so that setup is mostly one click.
55. As a new user, I want to download the starting speech model with one click and see its progress, so that I know setup is progressing.
56. As a new user, I want Settings to keep opening on each start until a speech model is installed, so that I cannot end up with a running app that does nothing.
57. As a new user who presses the Dictation key before setup is finished, I want the Settings window brought to the front, so that I am pointed to what is missing.
58. As a returning user, I want later starts to show only the brief startup notice, so that the app is quiet once configured.

### Settings

59. As a user, I want to rebind the Dictation key by clicking a field and pressing the key or combination I want, so that rebinding is obvious.
60. As a user, I want the key capture to cancel on Escape or after a timeout and to explain why a key is rejected, so that I never get stuck.
61. As a user, I want lone typing keys, Space, Enter and other unsafe keys rejected, so that I cannot bind a key that makes normal typing impossible.
62. As a user, I want a warning (but not a block) for common system shortcuts, so that I can make an informed choice.
63. As a user, I want the default Dictation key to be Right Ctrl, so that it works without configuration and rarely collides with typing.
64. As a user, I want the Dictation key not to fire while I am rebinding it, so that capture does not start a Dictation.
65. As a user, I want to choose my microphone from a list that starts with "System default", so that I follow my Windows setting unless I choose otherwise.
66. As a user, I want to choose a speech model from a short list with sizes and see which are downloaded, so that I can trade accuracy for speed.
67. As a user, I want selecting a model that is not yet downloaded to download it, so that I do not hunt for files.
68. As a user, I want downloads to resume after interruption and to be verified before use, so that a flaky connection never leaves me with a broken model.
69. As a user, I want clear messages when a download fails ("No internet connection", "Download interrupted", "File failed verification") with a Retry button, so that I know what to do.
70. As a user, I want to choose a fixed language (English by default) or Auto, so that recognition matches how I speak.
71. As a user choosing an English-only model, I want the language choice disabled, so that Settings never offers something that cannot work.
72. As a user, I want to choose whether the speech model stays loaded or is unloaded when idle, so that I can trade responsiveness against memory.
73. As a user, I want GPU acceleration chosen automatically on first start with a fallback to CPU and a manual override, so that fast hardware is used without effort.
74. As a user, I want Settings to tell me if the GPU was disabled after an error and let me re-test it, so that a one-off failure does not permanently slow me down.
75. As a user, I want a launch-at-login toggle that is off by default, so that the app only starts with Windows if I ask.
76. As a user, I want Settings to show the app version, a link to releases, and a link to the source code, so that I can find updates and the code.
77. As a user, I want the Settings window to use no memory when it is closed, so that configuring the app does not leave a heavy process behind.
78. As a user, I want changes in Settings to take effect without restarting the app, so that tuning is immediate.

### Privacy, performance and distribution

79. As a privacy-conscious user, I want the app to make no network calls except downloading a speech model I asked for, so that my voice and text never leave my machine.
80. As a privacy-conscious user, I want no account, no telemetry and no logging by default, so that nothing about my use is recorded.
81. As a developer diagnosing a problem, I want a verbose command-line flag that writes a log without ever recording audio or dictated text, so that I can debug safely.
82. As a user on a modest laptop, I want the app to use essentially no CPU when idle and keep its memory small, so that it does not slow my machine.
83. As a user, I want the app to use the CPU well during a Dictation without starving other apps, so that my machine stays responsive.
84. As a user, I want a per-user installer that needs no administrator rights and a portable zip, so that I can install or just run it.
85. As a user, I want uninstalling to ask before deleting my settings and models, so that I do not lose gigabytes of downloads by accident.
86. As a user, I want upgrades to leave my settings and models untouched, so that updating is painless.
87. As a user, I want updates to be manual and the app never to check for updates itself, so that the no-network promise is absolute.
88. As an open-source user, I want the licence and third-party notices included with every release and the source linked, so that the project's licensing obligations are met.
89. As a user whose config file is newer than the app, I want the app never to overwrite it, so that downgrading does not destroy my settings.
90. As a power user, I want unknown and hidden settings preserved when Settings saves, so that my hand edits survive.

## Implementation Decisions

### Product shape and scope

- **Name and licence.** The app is **Aloud Type**; the crate and binary use the joined/hyphenated forms of that name. Licence is AGPL-3.0 (choose "or later" unless the owner says otherwise). Every code dependency reviewed is compatible; the Speech models are downloaded on demand and are never part of the distribution. Releases ship the licence text, a third-party notices file generated by `cargo-about`, `cargo-deny` runs in CI, and a source link appears in Settings.
- **Platform.** Windows only in v1, behind a platform seam so other operating systems can be added later.
- **Fully offline after setup.** No network calls except the model download. No accounts, telemetry or text rewriting.
- **Vocabulary.** The project glossary defines Dictation, Dictation key, Streaming insertion, Listening indicator, Listening fault, Typing fault, Tray presence, Startup notice, Settings window and Speech model. All user-facing text, logs and code use these terms.

### Processes and module layout

- One executable with three modes: the resident process (default), a short-lived Settings window (`--settings`), and a GPU probe (`--probe-gpu`) run as a child process with a timeout.
- A single Cargo workspace with five crates: **core** (no OS code: the Dictation state machine, the confirmed-prefix stabilizer, fault rules, spacing rules), **platform-windows** (keyboard hook, text injection, audio device handling, tray, overlay), **recognizer** (the two traits and the Whisper implementation), **config** (shared settings types and file I/O), **settings-ui** (the Settings window).
- **core** depends only on traits implemented by platform-windows and recognizer. Those traits are the platform seam and the recognizer seam, and let core be exercised with fake keyboards, fake audio and recorded clips.
- **Recognizer interface.** Two engine-agnostic traits. The recognition trait takes a window of 16 kHz mono audio plus the language and an optional prompt of recent text, and returns text segments with word timings, word confidence and a no-speech probability. The voice-gate trait reports speech or non-speech for an audio slice (a Silero voice-activity model sits behind it). v1 supports Whisper-style windowed decoding only. A second engine (a true-streaming one) may be added behind the same seam later.
- **Threads in the resident process.** Plain threads and channels, no async runtime. The main thread runs the Win32 message loop (tray, overlay). A dedicated hook thread only classifies key events and posts them. A controller thread owns the Dictation state machine and is the single owner of state. The audio callback feeds a ring buffer; a recognizer worker decodes from it; an injector sends text to the focused window.
- **Settings to resident communication.** The config file is the source of truth for saved settings, watched and reloaded by the resident process. A named pipe (resident process as server) carries live commands and status: suspend Dictation keys during key capture, model switch and unload requests, download progress, open-Settings requests from a second launch. Settings still works when the resident process is not running.
- **Model downloads** are owned by the Settings process: resumable partial files, checksum verification, then moved into the models folder. The resident process loads only verified files, keeping any HTTP client out of the resident process.
- **Failure supervision.** The controller restarts a panicked recognizer or audio worker once, then shows a Listening or Typing fault if the restart fails. There is no whole-process auto-restart.
- **Single instance** via a named mutex; a second launch asks the first to open Settings and exits.
- **Logging.** None by default. A verbose command-line flag writes a log file in the app data folder; audio and dictated text are never logged, with or without the flag.

### Speech engine, models and GPU

- **Engine.** whisper.cpp through the whisper-rs crate (pin the crate version and the vendored whisper.cpp commit; keep the recognizer seam so the engine can be swapped; sherpa-onnx is the identified fallback). CPU is the baseline and needs AVX2.
- **Models** (quantised ggml Whisper, downloaded from the public whisper.cpp model repository, licensed MIT): base q5_1 (about 60 MB, multilingual) is the first-run download and the GPU probe model. small q5_1 (about 190 MB) becomes the default once a GPU is confirmed, downloading in the background. English-only variants and large-v3-turbo (GPU opt-in only) complete the list. English-only models disable the language setting.
- **Language.** A fixed language (English default) or Auto. Auto picks one language per utterance; mid-sentence code-switching is not guaranteed.
- **GPU.** Vulkan through whisper.cpp, built so the Vulkan backend is an optional separate DLL. On first start the probe runs in a child process with a timeout, benchmarks GPU against CPU on the real model, requires a meaningful speedup (about 1.3x) and matching output, and records the result per device and driver. The resident process loads the backend only when the probe approved it and the setting allows it. A backend error during a Dictation finishes that Dictation on CPU from the buffered audio and disables the GPU for the session, recorded in state. Settings offers Auto, CPU only, or a specific device.
- **Model residency.** The model loads on a worker thread at app start so Dictation is available immediately; a Dictation started before the load finishes buffers audio and waits. The Settings item "Speech model memory" is Keep loaded (default) or Unload when idle (freed 10 minutes after the last Dictation and reloaded in parallel with audio capture on the next). Swapping models loads the new one while the old keeps serving, then swaps.
- **Performance targets** (soft design targets, not gates). Idle: 0% CPU, no timers or polling, no audio stream open, working set at most 25 MB and private bytes at most 40 MB excluding the model. During Dictation: decode threads = min(physical cores minus 1, 4) at least 1, decode worker at below-normal priority, hook and audio threads at normal or above; peak memory about idle plus model runtime plus 50 MB. Latency from end of speech to last character typed for a 3-8 s utterance: p50 at most 1.0 s and p95 at most 2.0 s on a 2020-era 4-core laptop with the default CPU model; p50 at most 0.5 s with a confirmed GPU. If the default model is too slow, prefer a smaller default over relaxing the target.

### Dictation key and capture

- **Single key, two modes.** Hold dictates while the key is held; a quick tap latches hands-free Dictation, and the next key-down ends it. The threshold is 250 ms, stored as a hidden config value for tuning. Releasing before the threshold is a tap; past it, a hold whose release ends the Dictation.
- **Default key** is Right Ctrl; if the keyboard has no Right Ctrl, first-run Settings shows the field empty and asks the user to press a key.
- **Capture** uses one low-level keyboard hook on a dedicated thread. The callback only classifies events, swallows or passes them, and posts to a channel. A watchdog re-installs the hook if Windows removes it; if that fails, the tray shows a fault state with "Dictation key unavailable".
- **Chord rule.** For a modifier Dictation key, the key is swallowed on key-down so capture starts at once. If any other key goes down while it is physically held, cancel that Dictation, discard its audio, and replay the modifier key-down plus the other key so the shortcut still works. Text already typed stays. The rule does not apply to a latched Dictation.
- **Bindable keys.** A single key or a combination (modifiers plus one key), distinguishing left and right modifiers. Rejected: lone typing keys (letters, digits, punctuation, Space, Enter, Tab, Backspace, Esc), Win+L and Ctrl+Alt+Del. Warned but allowed: known system or app shortcuts (such as Alt+Tab, Win+letter, Ctrl+Space, Alt+F4), Right Alt (AltGr), and a lone Alt or Win (these use a mask-key trick so the menu bar and Start menu stay closed). No mouse buttons.
- **Conflicts** are not detected beyond the static warning list; the app's hook wins, and Settings carries a help line for the case where another tool intercepts the key first.
- **Rebind capture.** Clicking the field enters capture ("Press a key..."); the Settings process tells the resident process to suspend Dictation keys. The binding is recorded on release as the set of keys held at their peak. Esc or a 10 second timeout cancels; a rejected key shows its reason and capture continues. There is no cancel key during a Dictation in v1.

### Audio, recognition and Streaming insertion

- **Capture** via WASAPI shared mode, event-driven, behind a thin seam. The stream opens when a Dictation starts and closes when it ends; no stream is open while idle. "System default" follows Windows' default device; a default change mid-Dictation keeps the current stream and the next Dictation uses the new default. The app resamples to 16 kHz mono.
- **Level metering** computes loudness (RMS and peak) from the capture stream every 20-30 ms and publishes it for the overlay; an optional small spectrum can drive an equalizer look.
- **Voice gating.** A voice-activity gate sits in front of the recognizer so non-speech never reaches the decoder. Segments with a high no-speech probability or matching a short blocklist of known hallucinations are dropped. A single utterance is capped at about 15 seconds.
- **Confirmed-prefix streaming.** While a Dictation runs the growing utterance is re-decoded as fast as the machine allows, and only the leading words that two consecutive decodes agree on are typed. If the machine cannot keep up, the app automatically falls back to typing the whole utterance when the speaker pauses. The last word of a pending chunk is held back until a later decode confirms it or the Dictation ends; ending flushes everything.
- **Typing.** Text is injected as Unicode keyboard input batched per word, never through the clipboard, with the app's own events tagged so its hook ignores them. Whisper's punctuation and capitals are used as-is. Only spacing is managed: no leading space on a Dictation's first chunk, one space before each later chunk, nothing added at the end. Text goes to whichever window has focus at insertion time, like a keyboard; real keystrokes during a Dictation do not cancel it.
- **Typing modes.** Stable is the only mode. A type-then-backspace correction mode was evaluated and cut from v1.

### Listening indicator, tray and notices

- **Overlay.** A hand-drawn, click-through, always-on-top layered window that never takes focus, driven at about 30 fps by a timer that runs only during a Dictation (loudness drawn with fast attack and slow release). Three styles ship: Bar pill, Orb and Trace strip; Trace strip is the default and the choice is a Settings item. Position is two independent selections (Top or Bottom; Left, Center or Right), default Bottom and Center, with per-monitor DPI and taskbar-aware placement. Timings: about 120 ms in, 180 ms out, about 220 ms to grow into a fault state.
- **Listening fault** (red, grows, shows the reason plus a short hint when known). Ending faults (the mic cannot be heard at all) stop capture and end the Dictation; the recoverable "No sound detected" keeps listening and clears itself once a voice is heard. "No sound detected" fires only if no voice is heard in the first 4 seconds of a Dictation; silence after a voice has been heard never faults. Always applicable: no audio packets for more than 1 second (stall) and exact digital silence for 4 seconds are ending faults. Seven reason messages: "No microphone found", "Microphone disconnected", "Microphone in use by another app", "Windows is blocking the microphone", "Microphone is muted or silent", "No sound detected" (recoverable), "Microphone error"; raw error codes appear only in the optional verbose log.
- **Missing chosen microphone** with others present: fall back to System default for that Dictation with a non-red note "Using default microphone" for about 2 seconds; the saved choice is kept.
- **Privacy pre-check.** At Dictation start the app reads the Windows microphone consent and policy values; only an explicit Deny faults (without opening the stream). Missing or unknown values fall through to the normal path.
- **Dismissal.** Hold mode: red while the key is held, then lingers 2 seconds after release. Hands-free mode: an ending fault auto-dismisses after 5 seconds or on the next tap, which only dismisses and does not start a Dictation. A key press during a lingering fault closes it and starts a new Dictation with a fresh pre-check. There is no throttling: each press retries.
- **Text around faults.** Already-typed text stays; audio captured before an ending fault is flushed through the recognizer and typed.
- **Typing fault** (amber variant of the overlay; red remains reserved for Listening faults). If the foreground window is elevated at key press, no Dictation starts and the overlay shows "Can't type into an administrator window" for about 2 seconds. If typing is rejected partway, the Dictation ends with "Couldn't type into this window". If the decoder is more than about 30 seconds behind real time, the Dictation ends with "Speech model is too slow for this PC. Choose a smaller model in Settings."; audio captured so far is still decoded and typed.
- **Tray.** Tray icon with three states (idle, listening, fault; fault shows the reason in the tooltip until the next successful Dictation). Double-click opens Settings; the context menu has Settings, Exit, and, only while the microphone-blocked fault stands, "Open microphone privacy settings". The recoverable "No sound detected" never changes the tray icon.
- **Startup notice.** A small own-drawn notice for about 3 seconds at every start: "Aloud Type is running. Hold Right Ctrl to dictate." using the user's actual key name. It never takes focus and does not wait for the model to load. If setup is incomplete the Settings window opens instead.
- **Sleep and session.** Suspend ends any Dictation and closes the audio stream; resume re-installs the hook and re-enumerates microphones. The hook does not work on the lock screen or elevated prompts and the app does not try.

### Settings window

- A short-lived separate process using a lightweight CPU-rendered UI toolkit, so closing it frees everything. The resident process uses no UI toolkit (tray and menu crates, hand-drawn overlay and notice).
- Contents (nothing else in v1): Dictation key, microphone (System default first), Speech model choice with download status and progress, language, GPU (Auto, CPU only, or a device; shows "GPU disabled after an error" when applicable), Speech model memory (Keep loaded or Unload when idle), launch at login, Listening indicator style, Listening indicator position. Informational: app version, releases link, source link.
- Changes apply without restart. Saved settings are written by the Settings process only.

### Configuration and state

- All user data lives under the per-user local application data folder in an app folder, not the roaming profile. User settings are in a readable, hand-editable TOML file with an integer version; models are in a models subfolder.
- Two files with single writers: the settings file is written only by the Settings process; a state file holds machine-generated values (GPU probe result per device and driver, first-run-complete, download bookkeeping) and is written only by the process that produced a value. All writes are atomic. The resident process watches the settings file, applies changes live where possible (otherwise at the next Dictation) and, on an invalid file, keeps the last good settings and shows a tray notice.
- A settings file from a newer app version is never overwritten (the app runs read-only on it and leaves a backup). Unknown keys are preserved on save, including hidden values such as the tap threshold. Older files migrate forward on read.
- **First-run completion** means a usable Speech model is installed. Until then the Settings window opens at every start, with the Dictation key defaulted, the microphone on System default and the base model one click away; the Dictation key brings Settings to the front.

### Model downloads and failures

- Failures appear inline in Settings next to the model with Retry. A network problem or interruption keeps the partial file so Retry resumes; a checksum mismatch deletes it and retries once automatically, then shows the error. Messages: "No internet connection", "Download interrupted", "File failed verification". A model that fails verification is never loaded.

### Packaging and distribution

- Two forms from one build: a portable zip (executable, GPU backend library, licence, third-party notices) and a per-user installer built with Inno Setup (no administrator rights, installs under the user's local programs folder, Start menu entry, uninstaller). No MSIX in v1.
- Launch at login is a Settings toggle, off by default, using the per-user registry Run key; the installer asks once whether to enable it and the portable zip never touches the registry unless the toggle is flipped.
- Program files are separate from user data so upgrades and reinstalls never touch models; the uninstaller asks before deleting user data. The portable zip shares the same data location.
- Updates are manual only: no update checks; Settings shows the version and a releases link.
- Releases are code-signed when a certificate is available (unsigned apps trigger SmartScreen and can be blocked from the microphone on managed Windows 11 machines); until then they ship unsigned with a README note.
- **Release checklist:** trademark searches for the name "Aloud Type" (USPTO, EUIPO, WIPO), Microsoft Store name check, winget identifier check, domain check, `cargo-deny` and `cargo-about` runs, a smoke test on a clean Windows 11 machine, and the on-device spikes below. The name is not formally cleared; "aloud" can suggest text-to-speech, so descriptions and store text must say voice-to-text.

### Tray icon brief

- A simple, high-contrast glyph that remains legible at 16 to 32 px, with three states: idle (neutral), listening (active accent), and fault (red or amber tint with a clear alert cue). The concept may be generated with an image-generation tool and then traced into a clean vector with few shapes for the tray sizes; the final icon set must be checked on light and dark taskbars.

## Testing Decisions

- **What makes a good test.** A test drives external behaviour only: what the user types, sees and hears (typed text, overlay state, tray state, saved files) in response to key events, audio and recognizer output. It does not inspect internal state or call private functions, and it survives refactors that keep behaviour the same.
- **Seam 1 — the core Dictation engine.** The highest seam, covering nearly all logic. Tests send the engine key events (down and up with timestamps), audio frames, and recognizer results from a fake recognizer and fake voice gate, with a fake clock, and assert the typed text, the overlay state (level, red Listening fault with reason, amber Typing fault) and the tray state. It covers: hold versus tap and the 250 ms threshold; the chord-cancel rule and replay; confirmed-prefix streaming, the held-back last word, spacing between chunks and the fallback to type-on-pause; the hallucination filter and no-speech handling; the 4 second no-sound and 1 second stall rules, recoverable versus ending faults, fault dismissal timings and recovery; the missing-microphone fallback note; the 15 second utterance cap and the "model too slow" fault; behaviour before setup is complete; and flush on end. Fixtures are recorded audio clips and scripted recognizer outputs.
- **Seam 2 — the config crate.** Reads and writes of the settings and state files: migration, a newer version never overwritten, unknown keys preserved, atomic writes, and keeping the last good configuration when a file is invalid.
- **Not covered by automated tests.** The Windows-specific code (keyboard hook, text injection, audio device handling, tray, overlay, GPU probe) is verified on a real machine with the on-device spike list below, because real behaviour cannot be faked.
- **On-device spike list** (to be completed before the spec's assumptions are relied on): (1) what the audio call returns when Windows blocks the microphone for a desktop app, whether exact-zero silence arrives instead of an error, and whether the registry pre-check matches; (2) which default audio role the capture library selects, stream-open latency, and Bluetooth headset behaviour; (3) no-sound thresholds against the 4 second window on several real microphones; (4) overlay click-through and no-focus-steal, plus fullscreen, multi-monitor and DPI behaviour; (5) hook behaviour while an elevated window has focus, modifier swallowing against target key state, and mask-key behaviour for Alt and Win on current Windows 11; (6) iGPU streaming-chunk latency on real Intel and AMD integrated graphics, and the size and memory of the GPU backend library in the real build; (7) text injection into common apps (browser, editor, chat, terminal) including IME and surrogate-pair characters; (8) CPU latency on two or three laptops against the latency targets (nice to have, not a release gate); (9) a licence audit of the full transitive crate tree.
- **Prior art.** None; this is a new codebase. Seam tests should be the first tests written, and the fake keyboard, fake audio source, fake recognizer and fake clock should be built as shared test support.

## Out of Scope

- macOS and Linux support (the platform seam leaves room for them).
- Cloud transcription, accounts, telemetry, text rewriting by a language model, or voice command mode.
- A type-then-backspace correction typing mode (may return later as an opt-in experiment).
- GPU-specific tuning beyond auto-detect, fallback and a manual override.
- Guarantees for switching languages in the middle of a sentence.
- Bundling a speech model in the installer.
- Automatic update checks.
- Mouse-button Dictation keys, a separate hands-free key, a cancel key during a Dictation, and additional Settings beyond those listed.
- Generating the final tray icon artwork (done at implementation time from the brief above).
- Formal trademark clearance (a checklist item before public release).

## Further Notes

- **Prototype.** The Listening indicator styles and fault layouts were prototyped in a throwaway page (`.scratch/aloudtype/prototypes/10-listening-indicator-PROTOTYPE.html`); use it as the visual reference for the three styles, positions and the red fault state.
- **Research.** Findings behind these decisions, with sources and unverified items, are in `.scratch/aloudtype/research/` (runtime and streaming, GPU, input capture and injection, audio capture and faults, UI stack, models and languages, licences, name availability).
- **Known risks to watch.** The Whisper binding crate is thinly maintained (pin versions, keep the seam); GPU gains for streaming-size chunks on integrated graphics are unproven; Windows microphone privacy-denial behaviour for desktop apps is undocumented; overlay click-through and focus behaviour are unverified; the app cannot type into administrator windows.
- **Build prerequisites.** A Windows build needs a Visual Studio C++ toolchain, CMake and LLVM/clang.
