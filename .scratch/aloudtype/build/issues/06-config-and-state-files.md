# 06: Config and state files

**What to build:** The config crate reads and writes the user settings file (versioned, hand-editable) and the machine-state file, each with a single writer, atomic writes, forward migration, never overwriting a newer-version file (read-only plus backup), preserving unknown keys including hidden values, and keeping the last good configuration when a file is invalid. Includes default values for every v1 setting.

**Blocked by:** 01 Workspace, shared test support and CI

**Status:** resolved

- [ ] Round-trip, migration, newer-version, unknown-key and invalid-file behaviours are covered by tests
- [ ] Writes are atomic under simulated interruption
- [ ] Defaults match the spec (Right Ctrl, System default microphone, Trace strip, Bottom and Center, Keep loaded, English)

## Answer

Done in `crates/config` only (new deps: `toml_edit`, dev `tempfile`). Seam tested: the public `ConfigFile<Schema>` API (11 integration tests in `tests/config_files.rs`).
- `ConfigFile<T: Schema, F: Fs>` generic over versioned TOML: `open`, `reload`, `update` (re-reads disk, applies change, atomic write via temp+rename, so independent writers don't clobber each other), `status`, `is_read_only`. `Settings` (all v1 settings, hidden `tap_threshold_ms`, spec defaults) and `State` (first_run_complete, per-device/driver GPU probe, gpu_disabled_after_error, download bookkeeping) implement `Schema`; `AppPaths` gives `%LOCALAPPDATA%\AloudType\{settings,state}.toml` and `models\`.
- Behaviours: unknown keys/comments/hidden values preserved (toml_edit); newer-version file is read best-effort, read-only, never written, backed up to `<file>.v<N>.bak`; migration hook run on older/versionless files (tested with a toy v2 schema, real schemas are still v1); invalid file keeps last good value (defaults at startup), and a broken file is copied to `.invalid.bak` before the first save replaces it; atomic write tested with an `Fs` fake interrupting mid-write and at rename.
- Gaps: no file watcher (resident process reload is for ticket owners to wire using `reload()`); dictation key stored as list of key-name strings (names are a convention for the hook ticket); unknown nested keys inside State's `gpu_probes`/`downloads` entries are not preserved; Windows-specific rename-over-open-file failures not simulated.
