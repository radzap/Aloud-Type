# Audio capture, level metering, and failure detection (ticket 04)

Scope: Windows-only, Rust, fast, minimal footprint, offline. Research only; no app code.
Verification legend: **[V]** verified against a primary source fetched for this ticket; **[U]** not confirmed by a primary doc, needs an on-device test before the spec relies on it.

## 1. Capture API choice

**Recommendation: capture with `cpal` (WASAPI backend), shared mode, event-driven, using the default-device path for "System default".** Keep a thin `AudioSource` seam so a direct `windows`-crate WASAPI implementation can replace it if cpal's gaps (below) bite.

Why cpal fits:
- Latest published release per its changelog is 0.18.2 (2026-08-16); master's `Cargo.toml` is at 0.19.0. It uses the `windows` crate directly (no C++ runtime). [CHANGELOG](https://raw.githubusercontent.com/RustAudio/cpal/master/CHANGELOG.md), [Cargo.toml](https://raw.githubusercontent.com/RustAudio/cpal/master/Cargo.toml) [V]
- Event-driven capture: the capture thread waits on a command event and the WASAPI audio event, drains packets via `GetNextPacketSize`, and sleeps between events (no busy loop). Default flags are `AUDCLNT_STREAMFLAGS_EVENTCALLBACK`. [stream.rs](https://raw.githubusercontent.com/RustAudio/cpal/master/src/host/wasapi/stream.rs), [device.rs](https://raw.githubusercontent.com/RustAudio/cpal/master/src/host/wasapi/device.rs) [V]
- Default-device streams are activated through the virtual `DEVINTERFACE_AUDIO_CAPTURE` GUID, so "the Windows audio engine automatically reroutes the stream when the system default device changes"; cpal reports this as `ErrorKind::DeviceChanged` (stream stays alive, no rebuild). [device.rs](https://raw.githubusercontent.com/RustAudio/cpal/master/src/host/wasapi/device.rs), [error.rs](https://raw.githubusercontent.com/RustAudio/cpal/master/src/error.rs) [V]
- Enumeration is `EnumAudioEndpoints(eAll, DEVICE_STATE_ACTIVE)`, so only active endpoints appear; specific devices are identified by endpoint ID. [device.rs](https://raw.githubusercontent.com/RustAudio/cpal/master/src/host/wasapi/device.rs) [V]
- Typed error enum `ErrorKind`: `DeviceBusy`, `DeviceChanged`, `DeviceNotAvailable`, `HostUnavailable`, `PermissionDenied`, `StreamInvalidated`, `UnsupportedConfig`, `BackendError`, and others. [error.rs](https://raw.githubusercontent.com/RustAudio/cpal/master/src/error.rs) [V]

cpal gaps the spec must design around:
1. **No resampling/format conversion on capture.** Output streams set `AUTOCONVERTPCM`; capture streams do not, so "only native formats will work" (the shared-mode mix format from `GetMixFormat`). The app must take the device's native rate/channels/format (typically f32, 48 kHz, 1-2 ch) and downmix and resample to 16 kHz mono itself. [device.rs](https://raw.githubusercontent.com/RustAudio/cpal/master/src/host/wasapi/device.rs) [V]. (`AUTOCONVERTPCM | SRC_DEFAULT_QUALITY` would let Windows resample, but needs a direct WASAPI implementation; flags documented at [IAudioClient::Initialize](https://learn.microsoft.com/en-us/windows/win32/api/audioclient/nf-audioclient-iaudioclient-initialize).)
2. **`E_ACCESSDENIED` is not mapped to `PermissionDenied`.** cpal's HRESULT mapping (`impl From<windows::core::Error> for Error`) covers only AUDCLNT_E_* codes; anything else becomes `BackendError`. If Windows returns `E_ACCESSDENIED`, the app must inspect the message/HRESULT itself or detect denial another way (section 4). [wasapi.rs](https://raw.githubusercontent.com/RustAudio/cpal/master/src/host/wasapi.rs) [V]
3. **Error-callback only; thread ends on hard errors.** Errors from COM calls in `process_input` or the wait go to the error callback and the capture thread breaks. Default-device changes do not end the thread. After any non-`DeviceChanged` error, drop and rebuild the stream. [stream.rs](https://raw.githubusercontent.com/RustAudio/cpal/master/src/host/wasapi/stream.rs) [V]
4. **API churn.** The changelog shows `ErrorKind` unification in 0.18.0, a behaviour flip-flop on `DeviceChanged` in 0.18.2/Unreleased, and an unreleased removal of `ErrorKind::Xrun`. Pin an exact version and always include a fallback match arm. [CHANGELOG](https://raw.githubusercontent.com/RustAudio/cpal/master/CHANGELOG.md) [V]

Alternatives:
- **Direct `windows`-crate WASAPI** (`IMMDeviceEnumerator`, `IAudioClient`, `IAudioCaptureClient`): full control (AUTOCONVERT, E_ACCESSDENIED, `AUDCLNT_BUFFERFLAGS_SILENT`, `IMMNotificationClient`), more unsafe code (rough estimate a few hundred lines). The escape hatch if gap 1 or 2 proves costly; the `windows` crate is likely a dependency anyway.
- The safe `wasapi` wrapper crate and WinRT `MediaCapture` were not examined from primary source; not needed given the above.

Footprint: shared-mode event-driven capture wakes once per engine period (query via `GetDevicePeriod`; typically 10 ms) only while Dictation is active. Open the stream on Dictation start and close it on end, so nothing is held open at idle (also avoids the Windows mic-in-use indicator while idle). Measure open latency; if too high, keep the `IAudioClient` initialized but stopped.

## 2. Device enumeration and the "System default" choice

- Settings stores either a `default` sentinel or a specific **endpoint ID string** (cpal's device identity). [device.rs](https://raw.githubusercontent.com/RustAudio/cpal/master/src/host/wasapi/device.rs) [V]
- List: "System default" first, then each active capture endpoint's friendly name. Selecting "System default" uses cpal's default-input path, which follows Windows' default and reroutes automatically. [V]
- Windows has console/multimedia/communications roles; which one cpal's `default_input_device()` resolves was not confirmed. Do not expose roles in v1. [U]
- The device list is a snapshot: re-enumerate when Settings opens. A live refresh needs `IMMNotificationClient` (section 3), optional.
- Disabled/unplugged/not-present devices are not listed. If the saved specific device is absent at Dictation start, report "selected microphone not found" (distinct reason, section 7); whether to fall back to System default is ticket 09's decision.

## 3. Hot-unplug and device change

- `AUDCLNT_E_DEVICE_INVALIDATED`: endpoint unplugged, or hardware reconfigured, disabled, removed, or otherwise unavailable; `AUDCLNT_E_ENDPOINT_CREATE_FAILED` on Initialize has the same causes. [IAudioClient::Initialize](https://learn.microsoft.com/en-us/windows/win32/api/audioclient/nf-audioclient-iaudioclient-initialize) [V]
- cpal maps both to `DeviceNotAvailable`; `AUDCLNT_E_DEVICE_IN_USE` to `DeviceBusy`; `AUDCLNT_E_SERVICE_NOT_RUNNING` to `HostUnavailable`; `AUDCLNT_E_RESOURCES_INVALIDATED` to `StreamInvalidated`; the unsupported-format/buffer family to `UnsupportedConfig`. [wasapi.rs](https://raw.githubusercontent.com/RustAudio/cpal/master/src/host/wasapi.rs) [V]
- Default-device stream: if the default disappears with no replacement cpal emits `DeviceNotAvailable`; if a replacement exists it emits `DeviceChanged` and keeps running. [stream.rs](https://raw.githubusercontent.com/RustAudio/cpal/master/src/host/wasapi/stream.rs) [V]
- `IMMNotificationClient` (via `RegisterEndpointNotificationCallback`) offers `OnDefaultDeviceChanged`, `OnDeviceAdded`, `OnDeviceRemoved`, `OnDeviceStateChanged` (ACTIVE/DISABLED/NOTPRESENT/UNPLUGGED), `OnPropertyValueChanged`. [Device Events](https://learn.microsoft.com/en-us/windows/win32/coreaudio/device-events) [V]

Recommended policy:
- The stream error callback is the primary signal (no extra COM registration, smallest footprint). `DeviceNotAvailable` or `StreamInvalidated` mid-Dictation is a Listening fault ("microphone disconnected"): end capture, drop the stream.
- `DeviceChanged` on a default stream is not a fault; flush resampler state and continue.
- Never block in the error callback (cpal itself avoids invoking it from notification threads to prevent deadlock [stream.rs] [V]); post a message to the app loop.
- Rebuild lazily on the next Dictation start; no retry loops.
- `IMMNotificationClient` only while Settings is open, to refresh the mic list. Optional.

## 4. Microphone privacy denial detection (the hard part)

Established by primary sources:
- Windows has a global microphone switch and per-app consent; Win32 desktop apps are governed by "Let desktop apps access your microphone". Group Policy `LetAppsAccessMicrophone` (HKLM `Software\Policies\Microsoft\Windows\AppPrivacy`, REG_DWORD, 2 = Force Deny) is documented. [Privacy connections](https://learn.microsoft.com/en-us/windows/privacy/manage-connections-from-windows-operating-system-components-to-microsoft-services) [V]
- Windows 11 24H2+ adds per-app Allow/Deny lists for unpackaged desktop apps, keyed on a signing-derived namespace value, plus "Force Deny Unsigned". Order: Force Deny, Force Allow, global `LetAppsAccess*`, per-app user consent. Denial is applied "silently". The page does **not** state what a WASAPI capture call returns. [Desktop app privacy](https://learn.microsoft.com/en-us/windows/client-management/manage-desktop-app-privacy-access) [V]
- Implication: an **unsigned AloudType binary can be blocked on managed machines** by Force Deny Unsigned. Code-signing helps; keep a distinct message.
- For WinRT `MediaCapture`, `0x80070005` (E_ACCESSDENIED) means "microphone disabled in settings" per Microsoft's speech-recognition docs (seen via search summary only). That is not the WASAPI path. [speech recognition](https://learn.microsoft.com/en-us/windows/apps/design/input/speech-recognition) [V, summary only]
- The `IAudioClient::Initialize` return-code table has **no permission/privacy code** and no E_ACCESSDENIED entry. [Initialize](https://learn.microsoft.com/en-us/windows/win32/api/audioclient/nf-audioclient-iaudioclient-initialize) [V]

Not established by a primary doc **[U]**:
- Support/Q&A reports describe a denied desktop-app mic as capture that starts fine but delivers **silence** rather than an API error. Treat as the expected behaviour but verify on a test machine (global off, desktop-apps off, policy Force Deny, unsigned Force Deny) before the spec asserts it. Whether newer Windows 11 builds instead fail `Activate`/`Initialize` with E_ACCESSDENIED also needs a test.
- Consent registry location: per-user `HKCU\Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\microphone` (`Value` = Allow/Deny; `NonPackaged` subkey for desktop apps) and HKLM equivalent. Commonly used to read the toggle, but not found in the Microsoft pages fetched. Best-effort only, never sole authority.

Recommended layered detection (cheapest first):
1. **Typed API error**: `PermissionDenied`, or `BackendError` with HRESULT `0x80070005`, becomes "Access denied". (Needs HRESULT inspection because of cpal gap 2, or direct WASAPI.)
2. **Registry pre-check at Dictation start** (microseconds): read the policy value and ConsentStore `microphone`. If Deny, report "Access denied" without opening the stream.
3. **Digital-silence heuristic**: exact-zero samples (or `AUDCLNT_BUFFERFLAGS_SILENT` packets: "Treat all of the data in the packet as silence and ignore the actual data values" [AUDCLNT_BUFFERFLAGS](https://learn.microsoft.com/en-us/windows/win32/api/audioclient/ne-audioclient-_audclnt_bufferflags) [V]) for the whole window. Real mics have a noise floor, so exact zeros imply mute or block. Message should name both Windows privacy and hardware/endpoint mute. Needs on-device test [U]. cpal does not expose the SILENT flag; exact-zero detection on f32 samples is the substitute.
4. Fault UI may offer a link to `ms-settings:privacy-microphone` [U, not fetched].

## 5. Level / spectrum metering cheap enough for an overlay

**A. Compute from the capture stream (recommended).** Samples already flow to the recognizer. Per ~20-30 ms hop on the 16 kHz mono signal compute RMS and peak (dBFS) with fast attack / slow release smoothing. Cost is about one multiply-add per sample (~16k samples/s), negligible. Publish through one atomic (f32 bits); the overlay reads it at its own frame rate (30 fps is enough). Works for any device, no extra COM. For "variation", keep a short window of recent hops; for an optional spectrum look, a 256/512-point real FFT every ~33 ms into 8-16 log bands costs microseconds. Do metering in the consumer thread; copy packets into a ring buffer in the callback and do not allocate there.

**B. `IAudioMeterInformation::GetPeakValue`.** Peak 0.0-1.0, measured pre endpoint-volume attenuation for capture, works in shared mode; in exclusive mode a software meter reports 0.0. [IAudioMeterInformation](https://learn.microsoft.com/en-us/windows/win32/api/endpointvolume/nn-endpointvolume-iaudiometerinformation) [V] Needs a polling timer and separate COM activation, gives peak only, and reflects hardware gain rather than what the recognizer receives. Not recommended.

## 6. Reliable no-sound detection

Evaluate on the 16 kHz mono stream over a "no sound after a while" window (length is ticket 09's decision; placeholder ~3-5 s after Dictation starts, reset on each key press):

1. **No data**: stream started but no packets for > ~1 s. Event-driven shared capture delivers packets every period even when quiet, so a stall means a broken stream/driver. Reason: "Microphone not delivering audio".
2. **Digital silence**: all samples exactly 0 across the window. Causes: privacy block, hardware mute, muted endpoint, dead device. Reason: "Microphone muted or blocked".
3. **Low level / no voice**: nonzero noise floor but never rising enough. Use a dynamic threshold: noise floor (e.g. 10th-percentile RMS over the window) plus ~10-15 dB, with an absolute minimum around -60 dBFS; optionally gate on the recognizer's voice-activity signal. Thresholds are placeholders to tune on real mics [U]. Reason: "No sound detected".

Only fault while the Dictation key is still held/latched. After a silence fault, optionally query `IAudioEndpointVolume` mute/volume to refine the message (cold path only; API not fetched here [U]).

## 7. Distinguishable failure reasons

| # | Reason | Detection | Confidence |
|---|---|---|---|
| 1 | No microphone present | empty active-endpoint list / no default input / `DeviceNotAvailable` at build | [V] |
| 2 | Selected microphone not found | saved endpoint ID absent from active list | [V] |
| 3 | Microphone disconnected during dictation | `AUDCLNT_E_DEVICE_INVALIDATED` via error callback | [V] |
| 4 | Microphone in use exclusively by another app | `AUDCLNT_E_DEVICE_IN_USE` -> `DeviceBusy` | [V] |
| 5 | Windows blocked microphone access (privacy setting/policy) | `PermissionDenied` / E_ACCESSDENIED, registry Deny, or digital-silence heuristic | [U] |
| 6 | Blocked as unsigned app by organization policy | same signals as 5; may fold into 5's text on managed devices | policy [V], detection [U] |
| 7 | Windows audio service not running | `AUDCLNT_E_SERVICE_NOT_RUNNING` -> `HostUnavailable` | [V] |
| 8 | Microphone format unsupported / cannot be opened | `UnsupportedConfig` | [V] |
| 9 | Microphone not delivering audio (stream stalled) | no packets > ~1 s | design |
| 10 | Microphone muted or blocked (digital silence) | exact zeros over window | [U] |
| 11 | No sound detected | level never above noise floor + margin | [U] |
| 12 | Unknown audio error (show HRESULT in details) | `BackendError` / `Other` | [V] |

Reasons 5 and 10 are indistinguishable by signal alone; the registry pre-check is what separates them. Keep the raw HRESULT/ErrorKind for a details line or log. Not faults: `DeviceChanged` (default reroute) and isolated `AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY` glitches ([flags](https://learn.microsoft.com/en-us/windows/win32/api/audioclient/ne-audioclient-_audclnt_bufferflags) [V]).

## 8. Open on-device tests before the spec asserts these

1. Win32 WASAPI capture behaviour (signed and unsigned) with: global mic toggle off; desktop-apps toggle off; `LetAppsAccessMicrophone` = 2; Win11 24H2 Force Deny / Force Deny Unsigned. Record: API error vs exact-zero silence, and whether `Activate`/`Initialize` returns E_ACCESSDENIED.
2. ConsentStore registry layout and HKCU/HKLM precedence for desktop apps.
3. Which role cpal's `default_input_device()` uses.
4. Shared-mode open/start latency (open-per-dictation vs keep-initialized).
5. Noise-floor and speech thresholds on laptop array, USB, and Bluetooth mics.
6. Bluetooth headsets: opening the mic can switch profile/format; confirm `DeviceChanged` vs `StreamInvalidated` and that rebuild-on-next-start recovers.

## Sources (fetched)
- cpal: [device.rs](https://raw.githubusercontent.com/RustAudio/cpal/master/src/host/wasapi/device.rs), [stream.rs](https://raw.githubusercontent.com/RustAudio/cpal/master/src/host/wasapi/stream.rs), [wasapi.rs](https://raw.githubusercontent.com/RustAudio/cpal/master/src/host/wasapi.rs), [error.rs](https://raw.githubusercontent.com/RustAudio/cpal/master/src/error.rs), [CHANGELOG.md](https://raw.githubusercontent.com/RustAudio/cpal/master/CHANGELOG.md), [Cargo.toml](https://raw.githubusercontent.com/RustAudio/cpal/master/Cargo.toml)
- Microsoft: [IAudioClient::Initialize](https://learn.microsoft.com/en-us/windows/win32/api/audioclient/nf-audioclient-iaudioclient-initialize), [Device Events](https://learn.microsoft.com/en-us/windows/win32/coreaudio/device-events), [AUDCLNT_BUFFERFLAGS](https://learn.microsoft.com/en-us/windows/win32/api/audioclient/ne-audioclient-_audclnt_bufferflags), [IAudioMeterInformation](https://learn.microsoft.com/en-us/windows/win32/api/endpointvolume/nn-endpointvolume-iaudiometerinformation), [Desktop app privacy policies](https://learn.microsoft.com/en-us/windows/client-management/manage-desktop-app-privacy-access), [Windows privacy / LetAppsAccessMicrophone](https://learn.microsoft.com/en-us/windows/privacy/manage-connections-from-windows-operating-system-components-to-microsoft-services)
