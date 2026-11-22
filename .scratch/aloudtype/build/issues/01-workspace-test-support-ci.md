# 01: Workspace, shared test support and CI

**What to build:** A Cargo workspace with the five planned crates (core, platform-windows, recognizer, config, settings-ui) that builds, with the trait seams in core declared and the shared test support in place: a fake keyboard, fake audio source, fake recognizer and fake clock. One smoke test drives the core through a fake keyboard and clock. CI runs build, tests and cargo-deny.

**Blocked by:** None (can start immediately)

**Status:** resolved

- [x] Workspace builds and tests pass on a clean Windows machine
- [x] Fakes for keyboard, audio, recognizer and clock are reusable by later tickets
- [x] One smoke test exercises core purely through the fakes
- [x] CI runs cargo-deny and fails on a disallowed licence

## Answer

Done (TDD, one tracer-bullet smoke test then fakes). Seams tested: the public `Core` driver and the `Keyboard`/`Clock` traits, plus each fake's own behaviour.
- Cargo workspace (`crates/*`): `aloud-type-core`, `-platform-windows`, `-recognizer`, `-config`, `-settings-ui`. Core declares the seams in `seams.rs` (Keyboard, AudioSource, Clock, Recognizer, VoiceGate, Segment/Word types) and ships reusable fakes in `testing` (FakeKeyboard, FakeAudioSource, FakeClock, FakeRecognizer, FakeVoiceGate). Fakes are `Rc`-based and not `Send`; later tickets needing threads may need to switch them to `Arc<Mutex>`.
- Smoke test `core_observes_dictation_key_events_at_the_fake_clock_time` drives `Core` through the fake keyboard and clock only.
- CI `.github/workflows/ci.yml`: Windows build + test (`--locked`) and a cargo-deny job (`deny.toml`: licences, bans, sources). Verified locally that deny passes, and that a GPL-2.0-only licence is rejected.
- Gaps: the workflow itself has not run on GitHub; "clean Windows machine" verified only on this dev machine. Recognizer trait lives in core (not the recognizer crate) so core depends on no sibling crate. The Recognizer/AudioSource shapes are first drafts that tickets 03/09/10 may refine.
