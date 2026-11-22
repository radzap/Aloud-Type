# Aloud Type

Aloud Type is a free, fully local desktop app that turns the user's speech into text typed into whatever text field has focus, anywhere in the OS.

## Language

**Dictation**:
The act of speaking while the app is listening so that the speech is typed into the focused text field.
_Avoid_: Recording, voice typing

**Dictation key**:
The user-chosen key (or key combination) that starts and ends a Dictation.
_Avoid_: Hotkey, shortcut, trigger

**Streaming insertion**:
Typing recognized words into the focused text field as they are recognized, rather than after the Dictation ends.
_Avoid_: Live transcription, real-time paste

**Listening indicator**:
The small on-screen overlay shown during a Dictation that visualizes the loudness and variation of the voice, so the user knows the app is hearing them.
_Avoid_: Equalizer, waveform, HUD

**Listening fault**:
A state in which a Dictation cannot hear the user (no microphone, no access, or no voice heard soon after it starts). The Listening indicator turns red, grows, and shows the reason. Most faults end the Dictation; "no sound" clears by itself once the voice is heard.
_Avoid_: Error state, mic error

**Typing fault**:
A state in which a Dictation cannot deliver text (blocked or rejected target window, or a Speech model too slow for the machine). The Listening indicator shows an amber variant with the reason; red stays reserved for Listening faults.
_Avoid_: Injection error, output error

**Tray presence**:
The app's resting state: no main window, only a tray icon offering Settings and Exit.
_Avoid_: Background mode, minimized

**Startup notice**:
The brief flash notification shown when the app launches, before it settles into Tray presence.
_Avoid_: Splash screen, toast

**Settings window**:
The window opened from the tray icon where the user configures the app.
_Avoid_: Preferences, options dialog

**Speech model**:
The free, local Whisper model that converts audio to text. Downloaded once; after that the app makes no network calls.
_Avoid_: Engine, backend
