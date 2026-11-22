# Audio capture, level metering, and failure detection
Type: research
Status: resolved

## Question
How should audio be captured on Windows in Rust (WASAPI via cpal or alternatives), including device enumeration and the System default choice, hot-unplug, Windows microphone privacy denial detection, level/spectrum metering cheap enough for an overlay, and reliable no-sound detection? List the distinguishable failure reasons the app can report.

## Answer
Use cpal (WASAPI, shared mode, event-driven), opening the stream per Dictation and following "System default" via cpal's default-input path (Windows reroutes; cpal reports `DeviceChanged`, not a fault). Gaps to design around: cpal capture does no resampling (app converts the native mix format to 16 kHz mono), and it does not map E_ACCESSDENIED to `PermissionDenied` (fall back to direct `windows`-crate WASAPI if that bites). Hot-unplug arrives as `AUDCLNT_E_DEVICE_INVALIDATED` -> `DeviceNotAvailable` via the error callback; rebuild on the next Dictation. Privacy denial is not documented as an API error for Win32 apps: detect with layered checks (typed error, best-effort registry consent/policy pre-check, exact-zero-silence heuristic), all needing an on-device test. Meter from the capture stream itself (RMS/peak per ~20-30 ms hop in an atomic; optional small FFT), not `IAudioMeterInformation`. No-sound = no-data stall vs digital silence vs below noise floor + margin. Twelve distinguishable fault reasons are tabulated in the findings, plus a list of open on-device tests.

Full findings with sources: [research/04-audio-capture-and-faults.md](../research/04-audio-capture-and-faults.md)
