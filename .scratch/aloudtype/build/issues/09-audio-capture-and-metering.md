# 09: Audio capture and metering

**What to build:** The real microphone feeds the engine: shared-mode event-driven capture opened only for a Dictation, converted to 16 kHz mono, with loudness metering every 20-30 ms. System default follows the Windows default and a default change mid-Dictation keeps the current stream. The capture layer classifies failures into the Listening fault reasons (no microphone, disconnected, in use, blocked by Windows, muted or silent, other) and runs the registry privacy pre-check that faults only on an explicit Deny.

**Blocked by:** 05 Listening faults and indicator state

**Status:** resolved

- [ ] Real audio reaches the engine and the level reflects loudness
- [ ] Unplugging, muting and blocking the microphone each produce the right reason
- [ ] A missing chosen microphone falls back to System default with the 2 s note
- [ ] No audio stream is open while idle

## Answer

Implemented in `crates/platform-windows/src/audio/` (new modules; `lib.rs` only gains `pub mod audio;`):
- `session.rs`: `AudioCapture<B: Backend>` implements the core `AudioSource` seam. `start(&MicChoice)` runs the privacy pre-check, selects the device, opens the stream and returns `Ok(fallback_note)` or `Err(FaultReason)`; `stop()` drops the stream (no stream exists while idle); `take_fault()` surfaces mid-Dictation faults and closes the stream; `level()` gives 0..1 loudness. Default-device reroute (`DeviceChanged`) is ignored and the resampler rebuilds if the format changes.
- `fault.rs`: capture error classification into the Listening fault reasons (no microphone at open vs disconnected while streaming, in use, blocked incl. raw E_ACCESSDENIED, generic error). `privacy.rs`: registry pre-check (policy `LetAppsAccessMicrophone` 2/1, ConsentStore `microphone` and `NonPackaged` `Value`, HKCU/HKLM); only explicit Deny faults. `device.rs`: System default / specific / missing-with-note / none. `resample.rs`: streaming downmix + area-averaging resample to 16 kHz mono. `meter.rs`: 25 ms RMS hops, -60..-10 dBFS to 0..1, fast attack, slow release. `cpal_backend.rs`: cpal 0.18.2 (pinned) WASAPI shared-mode backend (f32 and i16 formats).
- 40 unit tests in the crate pass (fake backend, fake registry); workspace builds and tests pass after merging integration/aloudtype.

Gaps (need a real device; not verified headlessly):
- Real audio reaching the engine, the level on real speech, and unplug/mute/block each producing the right reason are only tested through the fake backend. The cpal backend is compile-checked only.
- cpal drops the HRESULT for unclassified errors, so E_ACCESSDENIED is recovered by matching "access is denied"/"80070005" in the message text; unverified. Exact-zero silence (mute/block) and the 1 s stall are detected by the core `ListeningMonitor` from the audio this layer feeds, not here.
- Registry value layout/precedence, cpal's default-device role, open latency and Bluetooth behaviour remain on the spike list. The wiring of `AudioCapture` into the host loop (calling `take_fault` and feeding `ListeningMonitor`) belongs to ticket 11. Samples are shared via a short mutex in the stream callback (simple; not lock-free). Missing-device detection relies on the endpoint list; a device vanishing between listing and opening yields NoMicrophone.
