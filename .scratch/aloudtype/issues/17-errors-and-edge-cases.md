# Error handling and edge cases

Type: grilling
Status: resolved
Blocked by: 14

## Question

How does the app behave when: the model download fails, is interrupted or is corrupt; no model is installed yet and the user starts a Dictation; the GPU probe fails or the backend crashes mid-Dictation; the foreground window is elevated or rejects injected input; the keyboard hook is silently removed; another instance is launched; the system sleeps or resumes; the default microphone changes mid-Dictation; the recognizer cannot keep up? Decide the user-visible behaviour for each, reusing the Listening fault vocabulary where it fits.

## Answer

Decided with the owner (all recommendations accepted). Microphone-side faults are already covered by [Listening fault rules](09-listening-fault-rules.md). This ticket adds a second fault kind, the **Typing fault**, shown as an amber variant of the same overlay so that red keeps meaning "cannot hear you".

1. **Model download failure**: shown inline in Settings next to the model with a Retry button. Network problem or interruption keeps the partial file so Retry resumes. Checksum mismatch deletes the partial file and retries once automatically, then shows the error. A model that fails verification is never loaded; the resident process loads only fully verified files. Messages: "No internet connection", "Download interrupted", "File failed verification".
2. **Typing faults**: if the foreground window is elevated when the Dictation key is pressed, no Dictation starts and the overlay shows an amber notice for about 2 s: "Can't type into an administrator window". If typing is rejected partway (the injection call sends fewer events than requested), the Dictation ends with the amber message "Couldn't type into this window"; already-typed text stays.
3. **Keyboard hook lost**: the watchdog re-installs silently and retries a few times. If it cannot, the tray icon shows its fault state with the tooltip "Dictation key unavailable". No popup; a successful re-install clears it.
4. **GPU failure**: a failed probe silently means CPU, recorded in `state.toml`. A backend error during a Dictation finishes that Dictation on CPU from the buffered audio, disables the GPU for the session and records it; the GPU setting in Settings shows "GPU disabled after an error", and selecting a device again re-tests it.
5. **Second instance**: single instance via a named mutex. A second launch asks the first (via the pipe) to open the Settings window and exits.
6. **Sleep, lock, device changes**: suspend ends any Dictation and closes the audio stream; resume re-installs the hook and re-enumerates microphones. The hook does not work on the lock screen or UAC prompts and the app does not try. A default-microphone change mid-Dictation keeps the current stream; the next Dictation uses the new default; not a fault.
7. **Recognizer too slow**: a single utterance is capped at about 15 s of audio (the type-on-pause fallback already covers machines that cannot stream). If the decoder is still more than about 30 s behind real time, the Dictation ends with the amber Typing fault "Speech model is too slow for this PC. Choose a smaller model in Settings." Audio captured so far is still decoded and typed.
