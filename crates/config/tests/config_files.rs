use aloud_type_config::*;
use std::cell::Cell;
use std::io;
use std::path::Path;
use toml_edit::DocumentMut;

fn dir() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

#[test]
fn defaults_match_the_spec() {
    let d = tempfile::tempdir().unwrap();
    let f = ConfigFile::<Settings>::open(d.path().join("settings.toml"));
    let s = f.value();
    assert_eq!(*f.status(), LoadStatus::Missing);
    assert_eq!(s.dictation_key, vec!["RightCtrl".to_string()]);
    assert_eq!(s.microphone, None);
    assert_eq!(s.indicator_style, IndicatorStyle::TraceStrip);
    assert_eq!(s.indicator_vertical, VerticalPosition::Bottom);
    assert_eq!(s.indicator_horizontal, HorizontalPosition::Center);
    assert_eq!(s.model_memory, ModelMemory::KeepLoaded);
    assert_eq!(s.language, Language::Fixed("en".into()));
    assert!(!s.launch_at_login);
    assert_eq!(s.gpu, Gpu::Auto);
    assert_eq!(s.tap_threshold_ms, 250);
}

#[test]
fn settings_round_trip_through_a_new_open() {
    let d = dir();
    let p = d.path().join("sub").join("settings.toml");
    let mut f = ConfigFile::<Settings>::open(&p);
    f.update(|s| {
        s.dictation_key = vec!["LeftCtrl".into(), "F9".into()];
        s.microphone = Some("USB Mic".into());
        s.language = Language::Auto;
        s.gpu = Gpu::Device("NVIDIA".into());
        s.model_memory = ModelMemory::UnloadWhenIdle;
        s.launch_at_login = true;
        s.indicator_style = IndicatorStyle::Orb;
        s.indicator_vertical = VerticalPosition::Top;
        s.indicator_horizontal = HorizontalPosition::Right;
    })
    .unwrap();
    let expected = f.value().clone();
    let g = ConfigFile::<Settings>::open(&p);
    assert_eq!(*g.status(), LoadStatus::Loaded);
    assert_eq!(*g.value(), expected);
    let text = std::fs::read_to_string(&p).unwrap();
    assert!(text.contains("version = 1"));
    assert!(!text.contains("tap_threshold_ms"), "hidden value is not written unless set");
}

#[test]
fn unknown_keys_comments_and_hidden_values_survive_a_save() {
    let d = dir();
    let p = d.path().join("settings.toml");
    std::fs::write(
        &p,
        "version = 1\n# my note\ntap_threshold_ms = 300\nfuture_thing = \"keep me\"\n[extra]\nx = 1\n",
    )
    .unwrap();
    let mut f = ConfigFile::<Settings>::open(&p);
    assert_eq!(f.value().tap_threshold_ms, 300);
    f.update(|s| s.launch_at_login = true).unwrap();
    let text = std::fs::read_to_string(&p).unwrap();
    assert!(text.contains("# my note"));
    assert!(text.contains("tap_threshold_ms = 300"));
    assert!(text.contains("future_thing = \"keep me\""));
    assert!(text.contains("[extra]"));
    assert!(text.contains("launch_at_login = true"));
}

#[test]
fn newer_version_file_is_never_overwritten_and_is_backed_up() {
    let d = dir();
    let p = d.path().join("settings.toml");
    let original = "version = 99\nlaunch_at_login = true\nnew_key = 5\n";
    std::fs::write(&p, original).unwrap();
    let mut f = ConfigFile::<Settings>::open(&p);
    assert_eq!(*f.status(), LoadStatus::ReadOnlyNewer { file_version: 99 });
    assert!(f.is_read_only());
    assert!(f.value().launch_at_login, "known values still read");
    let err = f.update(|s| s.launch_at_login = false).unwrap_err();
    assert!(matches!(err, ConfigError::NewerVersion { file: 99, .. }));
    assert_eq!(std::fs::read_to_string(&p).unwrap(), original);
    let bak = d.path().join("settings.toml.v99.bak");
    assert_eq!(std::fs::read_to_string(bak).unwrap(), original);
}

#[test]
fn older_file_migrates_forward_on_read() {
    // A toy schema at version 2 that renamed `colour` to `color` in the v1 -> v2 step.
    #[derive(Default, Clone, Debug, PartialEq)]
    struct Toy {
        color: String,
    }
    impl Schema for Toy {
        const VERSION: u32 = 2;
        fn migrate(doc: &mut DocumentMut, from: u32) {
            if from < 2 {
                if let Some(v) = doc.remove("colour") {
                    doc["color"] = v;
                }
            }
        }
        fn read(doc: &DocumentMut) -> Result<Self, String> {
            Ok(Toy { color: doc.get("color").and_then(|v| v.as_str()).unwrap_or("").into() })
        }
        fn write(&self, doc: &mut DocumentMut) {
            doc["color"] = toml_edit::value(self.color.as_str());
        }
    }
    let d = dir();
    let p = d.path().join("toy.toml");
    std::fs::write(&p, "version = 1\ncolour = \"red\"\nother = 1\n").unwrap();
    let mut f = ConfigFile::<Toy>::open(&p);
    assert_eq!(*f.status(), LoadStatus::Migrated { from: 1 });
    assert_eq!(f.value().color, "red");
    f.update(|t| t.color = "blue".into()).unwrap();
    let text = std::fs::read_to_string(&p).unwrap();
    assert!(text.contains("version = 2") && text.contains("color = \"blue\"") && text.contains("other = 1"));
    assert!(!text.contains("colour"));
}

#[test]
fn versionless_file_is_read_as_the_oldest_version() {
    let d = dir();
    let p = d.path().join("settings.toml");
    std::fs::write(&p, "launch_at_login = true\n").unwrap();
    let f = ConfigFile::<Settings>::open(&p);
    assert!(f.value().launch_at_login);
}

#[test]
fn invalid_file_keeps_the_last_good_settings() {
    let d = dir();
    let p = d.path().join("settings.toml");
    let mut f = ConfigFile::<Settings>::open(&p);
    f.update(|s| s.indicator_style = IndicatorStyle::Orb).unwrap();
    for bad in ["this is = = not toml", "indicator_style = \"blob\"\n", "launch_at_login = \"yes\"\n"] {
        std::fs::write(&p, bad).unwrap();
        assert!(matches!(f.reload(), LoadStatus::Invalid(_)), "{bad}");
        assert_eq!(f.value().indicator_style, IndicatorStyle::Orb);
    }
    std::fs::write(&p, "version = 1\nindicator_style = \"bar-pill\"\n").unwrap();
    f.reload();
    assert_eq!(f.value().indicator_style, IndicatorStyle::BarPill);
}

#[test]
fn invalid_file_at_startup_uses_defaults_and_is_backed_up_before_save() {
    let d = dir();
    let p = d.path().join("settings.toml");
    std::fs::write(&p, "oops = = =").unwrap();
    let mut f = ConfigFile::<Settings>::open(&p);
    assert!(matches!(f.status(), LoadStatus::Invalid(_)));
    assert_eq!(*f.value(), Settings::default());
    f.update(|s| s.launch_at_login = true).unwrap();
    assert_eq!(std::fs::read_to_string(d.path().join("settings.toml.invalid.bak")).unwrap(), "oops = = =");
    assert_eq!(*ConfigFile::<Settings>::open(&p).status(), LoadStatus::Loaded);
}

/// Fs that fails at a chosen step, leaving a truncated temp file like a crash would.
struct Interrupting {
    fail_on_rename: bool,
    renames: Cell<u32>,
}
impl Fs for Interrupting {
    fn read_to_string(&self, p: &Path) -> io::Result<Option<String>> {
        StdFs.read_to_string(p)
    }
    fn write_new(&self, p: &Path, b: &[u8]) -> io::Result<()> {
        if self.fail_on_rename {
            return StdFs.write_new(p, b);
        }
        StdFs.write_new(p, &b[..b.len() / 2])?; // half written, then "power loss"
        Err(io::Error::other("interrupted"))
    }
    fn rename(&self, f: &Path, t: &Path) -> io::Result<()> {
        self.renames.set(self.renames.get() + 1);
        if self.fail_on_rename {
            return Err(io::Error::other("interrupted"));
        }
        StdFs.rename(f, t)
    }
    fn copy(&self, f: &Path, t: &Path) -> io::Result<()> {
        StdFs.copy(f, t)
    }
    fn exists(&self, p: &Path) -> bool {
        StdFs.exists(p)
    }
    fn remove(&self, p: &Path) -> io::Result<()> {
        StdFs.remove(p)
    }
    fn create_dir_all(&self, p: &Path) -> io::Result<()> {
        StdFs.create_dir_all(p)
    }
}

#[test]
fn interrupted_writes_leave_the_previous_file_intact() {
    for fail_on_rename in [false, true] {
        let d = dir();
        let p = d.path().join("settings.toml");
        ConfigFile::<Settings>::open(&p).update(|s| s.launch_at_login = true).unwrap();
        let before = std::fs::read_to_string(&p).unwrap();
        let mut f = ConfigFile::<Settings, _>::open_with(
            Interrupting { fail_on_rename, renames: Cell::new(0) },
            &p,
        );
        assert!(f.update(|s| s.launch_at_login = false).is_err());
        assert_eq!(std::fs::read_to_string(&p).unwrap(), before);
        assert!(f.value().launch_at_login, "in-memory value unchanged after failure");
        assert!(!d.path().join("settings.toml.tmp").exists(), "temp file cleaned up");
        assert_eq!(*ConfigFile::<Settings>::open(&p).status(), LoadStatus::Loaded);
    }
}

#[test]
fn state_round_trips_and_updates_merge_with_other_writers() {
    let d = dir();
    let p = d.path().join("state.toml");
    let mut probe_proc = ConfigFile::<State>::open(&p);
    let mut settings_proc = ConfigFile::<State>::open(&p);
    assert!(!probe_proc.value().first_run_complete);
    probe_proc
        .update(|s| {
            s.gpu_probes.insert("RTX 4060|551.23".into(), GpuProbe { approved: true, speedup: 2.5 });
        })
        .unwrap();
    // A second writer holding a stale view must not lose the first writer's value.
    settings_proc.update(|s| s.first_run_complete = true).unwrap();
    settings_proc.update(|s| { s.downloads.insert("small-q5_1".into(), 1234); }).unwrap();
    let g = ConfigFile::<State>::open(&p);
    let s = g.value();
    assert!(s.first_run_complete);
    assert_eq!(s.gpu_probes["RTX 4060|551.23"], GpuProbe { approved: true, speedup: 2.5 });
    assert_eq!(s.downloads["small-q5_1"], 1234);
    settings_proc.update(|s| { s.downloads.clear(); }).unwrap();
    assert!(ConfigFile::<State>::open(&p).value().downloads.is_empty());
}

#[test]
fn app_paths_live_under_local_app_data() {
    let a = AppPaths::under(Path::new("C:/Users/u/AppData/Local"));
    assert!(a.settings_file().ends_with("AloudType/settings.toml"));
    assert!(a.state_file().ends_with("AloudType/state.toml"));
    assert!(a.models_dir().ends_with("AloudType/models"));
}
