# 06: Config and state files

**What to build:** The config crate reads and writes the user settings file (versioned, hand-editable) and the machine-state file, each with a single writer, atomic writes, forward migration, never overwriting a newer-version file (read-only plus backup), preserving unknown keys including hidden values, and keeping the last good configuration when a file is invalid. Includes default values for every v1 setting.

**Blocked by:** 01 Workspace, shared test support and CI

**Status:** ready-for-agent

- [ ] Round-trip, migration, newer-version, unknown-key and invalid-file behaviours are covered by tests
- [ ] Writes are atomic under simulated interruption
- [ ] Defaults match the spec (Right Ctrl, System default microphone, Trace strip, Bottom and Center, Keep loaded, English)
