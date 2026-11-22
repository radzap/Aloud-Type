# 01: Workspace, shared test support and CI

**What to build:** A Cargo workspace with the five planned crates (core, platform-windows, recognizer, config, settings-ui) that builds, with the trait seams in core declared and the shared test support in place: a fake keyboard, fake audio source, fake recognizer and fake clock. One smoke test drives the core through a fake keyboard and clock. CI runs build, tests and cargo-deny.

**Blocked by:** None (can start immediately)

**Status:** ready-for-agent

- [ ] Workspace builds and tests pass on a clean Windows machine
- [ ] Fakes for keyboard, audio, recognizer and clock are reusable by later tickets
- [ ] One smoke test exercises core purely through the fakes
- [ ] CI runs cargo-deny and fails on a disallowed licence
