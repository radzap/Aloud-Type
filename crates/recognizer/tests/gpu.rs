//! GPU acceleration seams: probe benchmark, per-device cache, child-process isolation,
//! selection, and mid-Dictation fallback. All offline with fake engines.
use aloud_type_config::{Gpu, State};
use aloud_type_core::{Recognizer, Segment, Word};
use aloud_type_recognizer::gpu::*;
use aloud_type_recognizer::RecognizerError;
use std::cell::RefCell;
use std::sync::atomic::{AtomicU32, Ordering::SeqCst};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Shared fake clock.
#[derive(Clone, Default)]
struct Clk(Arc<Mutex<Duration>>);
impl Clk {
    fn get(&self) -> Duration {
        *self.0.lock().unwrap()
    }
    fn set(&self, d: Duration) {
        *self.0.lock().unwrap() = d;
    }
}

fn seg(words: &[&str]) -> Vec<Segment> {
    let ws: Vec<Word> = words
        .iter()
        .enumerate()
        .map(|(i, w)| Word {
            text: w.to_string(),
            start_ms: i as u64 * 100,
            end_ms: i as u64 * 100 + 90,
            confidence: 0.9,
        })
        .collect();
    vec![Segment {
        text: words.join(" "),
        words: ws,
        no_speech_prob: 0.0,
    }]
}

/// Fake engine: advances a shared fake clock by `cost` per decode, returns `words` or errors.
struct FakeEngine {
    clock: Clk,
    cost: Duration,
    words: Vec<&'static str>,
    fail_after: Option<usize>,
    calls: usize,
}
impl FakeEngine {
    fn new(clock: &Clk, cost_ms: u64, words: &[&'static str]) -> Self {
        FakeEngine {
            clock: clock.clone(),
            cost: Duration::from_millis(cost_ms),
            words: words.to_vec(),
            fail_after: None,
            calls: 0,
        }
    }
}
impl Engine for FakeEngine {
    fn try_recognize(
        &mut self,
        _a: &[f32],
        _l: &str,
        _p: Option<&str>,
    ) -> Result<Vec<Segment>, RecognizerError> {
        self.calls += 1;
        if let Some(n) = self.fail_after {
            if self.calls > n {
                return Err(RecognizerError("device lost".into()));
            }
        }
        self.clock.set(self.clock.get() + self.cost);
        Ok(seg(&self.words))
    }
}

fn dev(name: &str, driver: &str) -> GpuDevice {
    GpuDevice {
        name: name.into(),
        driver: driver.into(),
        index: 0,
    }
}

const WORDS: &[&str] = &["ask", "not", "what", "your", "country"];

#[test]
fn probe_approves_a_gpu_that_is_fast_enough_and_matches_the_cpu_output() {
    let clock = Clk::default();
    let mut cpu = FakeEngine::new(&clock, 1300, WORDS);
    let mut gpu = FakeEngine::new(&clock, 500, WORDS);
    let now = {
        let c = clock.clone();
        move || c.get()
    };
    let v = benchmark(&mut cpu, &mut gpu, &[0.0; 16], "en", &now);
    assert!(v.approved, "{v:?}");
    assert!((v.speedup - 2.6).abs() < 1e-9, "{}", v.speedup);
}

#[test]
fn probe_rejects_a_gpu_that_is_not_meaningfully_faster() {
    let clock = Clk::default();
    let mut cpu = FakeEngine::new(&clock, 1000, WORDS);
    let mut gpu = FakeEngine::new(&clock, 800, WORDS); // 1.25x < 1.3x
    let now = {
        let c = clock.clone();
        move || c.get()
    };
    let v = benchmark(&mut cpu, &mut gpu, &[0.0; 16], "en", &now);
    assert!(!v.approved);
    assert!(v.reason.contains("speedup"), "{}", v.reason);
}

#[test]
fn probe_rejects_a_fast_gpu_whose_output_differs() {
    let clock = Clk::default();
    let mut cpu = FakeEngine::new(&clock, 1000, WORDS);
    let mut gpu = FakeEngine::new(&clock, 100, &["and", "not", "what", "your", "country"]);
    let now = {
        let c = clock.clone();
        move || c.get()
    };
    let v = benchmark(&mut cpu, &mut gpu, &[0.0; 16], "en", &now);
    assert!(!v.approved);
    assert!(v.reason.contains("output"), "{}", v.reason);
}

#[test]
fn probe_treats_case_and_punctuation_differences_as_a_match() {
    let clock = Clk::default();
    let mut cpu = FakeEngine::new(&clock, 1000, &["Ask,", "not"]);
    let mut gpu = FakeEngine::new(&clock, 100, &["ask", "not."]);
    let now = {
        let c = clock.clone();
        move || c.get()
    };
    assert!(benchmark(&mut cpu, &mut gpu, &[0.0; 16], "en", &now).approved);
}

#[test]
fn a_gpu_decode_error_during_the_probe_rejects_the_device() {
    let clock = Clk::default();
    let mut cpu = FakeEngine::new(&clock, 1000, WORDS);
    let mut gpu = FakeEngine::new(&clock, 100, WORDS);
    gpu.fail_after = Some(0);
    let now = {
        let c = clock.clone();
        move || c.get()
    };
    let v = benchmark(&mut cpu, &mut gpu, &[0.0; 16], "en", &now);
    assert!(!v.approved);
    assert!(v.reason.contains("device lost"), "{}", v.reason);
}

// ---- selection and per-device cache ----

fn ok_verdict() -> Verdict {
    Verdict {
        approved: true,
        speedup: 2.0,
        reason: "ok".into(),
    }
}

#[test]
fn a_saved_approval_for_the_same_device_and_driver_is_reused_without_a_new_probe() {
    let mut st = State::default();
    let d = dev("Radeon 780M", "31.0.1");
    record_verdict(&mut st, &d, &ok_verdict());
    assert_eq!(
        select(&Gpu::Auto, &st, std::slice::from_ref(&d)),
        Selection::Gpu(d)
    );
}

#[test]
fn a_rejection_on_an_old_driver_does_not_hold_back_the_new_driver() {
    let mut st = State::default();
    let old = dev("Radeon 780M", "31.0.1");
    record_verdict(
        &mut st,
        &old,
        &Verdict {
            approved: false,
            speedup: 0.9,
            reason: "slow".into(),
        },
    );
    let upgraded = dev("Radeon 780M", "32.0.0");
    assert_eq!(
        select(&Gpu::Auto, &st, std::slice::from_ref(&upgraded)),
        Selection::Gpu(upgraded)
    );
}

#[test]
fn without_a_test_auto_prefers_nvidia_then_amd_then_any_gpu() {
    let intel = dev("Intel(R) Iris(R) Xe Graphics", "1");
    let amd = dev("AMD Radeon(TM) 780M Graphics", "2");
    let nvidia = dev("NVIDIA GeForce RTX 3060 Laptop GPU", "3");
    let st = State::default();
    assert_eq!(
        select(
            &Gpu::Auto,
            &st,
            &[intel.clone(), amd.clone(), nvidia.clone()]
        ),
        Selection::Gpu(nvidia)
    );
    assert_eq!(
        select(&Gpu::Auto, &st, &[intel.clone(), amd.clone()]),
        Selection::Gpu(amd)
    );
    assert_eq!(
        select(&Gpu::Auto, &st, std::slice::from_ref(&intel)),
        Selection::Gpu(intel)
    );
}

#[test]
fn a_device_a_test_rejected_is_skipped_unless_picked_by_name() {
    let intel = dev("Intel(R) Iris(R) Xe Graphics", "1");
    let nvidia = dev("NVIDIA GeForce RTX 3060", "3");
    let mut st = State::default();
    record_verdict(&mut st, &intel, &ok_verdict());
    record_verdict(
        &mut st,
        &nvidia,
        &Verdict {
            approved: false,
            speedup: 0.0,
            reason: "crashed".into(),
        },
    );
    assert_eq!(
        select(&Gpu::Auto, &st, &[intel.clone(), nvidia.clone()]),
        Selection::Gpu(intel)
    );
    // A device picked by name is used even when a test rejected it.
    assert_eq!(
        select(
            &Gpu::Device(nvidia.name.clone()),
            &st,
            std::slice::from_ref(&nvidia)
        ),
        Selection::Gpu(nvidia)
    );
}

#[test]
fn a_saved_rejection_is_not_probed_again_and_runs_on_cpu() {
    let mut st = State::default();
    let d = dev("UHD 620", "27.0");
    record_verdict(
        &mut st,
        &d,
        &Verdict {
            approved: false,
            speedup: 0.8,
            reason: "slow".into(),
        },
    );
    assert_eq!(
        select(&Gpu::Auto, &st, &[d]),
        Selection::Cpu(CpuReason::NotApproved)
    );
}

#[test]
fn cpu_only_never_selects_or_probes_a_gpu() {
    let d = dev("RTX", "5");
    assert_eq!(
        select(&Gpu::CpuOnly, &State::default(), &[d]),
        Selection::Cpu(CpuReason::Setting)
    );
}

#[test]
fn no_gpu_installed_means_cpu() {
    assert_eq!(
        select(&Gpu::Auto, &State::default(), &[]),
        Selection::Cpu(CpuReason::NoDevice)
    );
}

#[test]
fn a_gpu_disabled_after_an_error_stays_on_cpu_until_retested() {
    let mut st = State::default();
    let d = dev("RTX", "5");
    record_verdict(&mut st, &d, &ok_verdict());
    record_gpu_error(&mut st);
    assert!(st.gpu_disabled_after_error);
    assert_eq!(
        select(&Gpu::Auto, &st, std::slice::from_ref(&d)),
        Selection::Cpu(CpuReason::DisabledAfterError)
    );
    // Test from Settings clears the flag and the saved results.
    request_retest(&mut st);
    assert!(!st.gpu_disabled_after_error);
    assert_eq!(
        select(&Gpu::Auto, &st, std::slice::from_ref(&d)),
        Selection::Gpu(d)
    );
}

#[test]
fn a_specific_device_setting_uses_only_that_device() {
    let mut st = State::default();
    let a = dev("Intel UHD", "1");
    let b = dev("NVIDIA RTX", "2");
    record_verdict(&mut st, &a, &ok_verdict());
    record_verdict(&mut st, &b, &ok_verdict());
    assert_eq!(
        select(
            &Gpu::Device("NVIDIA RTX".into()),
            &st,
            &[a.clone(), b.clone()]
        ),
        Selection::Gpu(b.clone())
    );
    assert_eq!(
        select(&Gpu::Device("Missing".into()), &st, &[a, b]),
        Selection::Cpu(CpuReason::NoDevice)
    );
}

#[test]
fn auto_keeps_the_vendor_order_among_approved_devices_whatever_the_measured_speed() {
    // A short test is noisy: an iGPU once measured faster than the RTX on the same laptop.
    let mut st = State::default();
    let intel = dev("Intel(R) Iris(R) Xe Graphics", "1");
    let nvidia = dev("NVIDIA GeForce RTX 3060", "2");
    for (d, speedup) in [(&intel, 6.8), (&nvidia, 6.4)] {
        record_verdict(
            &mut st,
            d,
            &Verdict {
                approved: true,
                speedup,
                reason: String::new(),
            },
        );
    }
    assert_eq!(
        select(&Gpu::Auto, &st, &[intel, nvidia.clone()]),
        Selection::Gpu(nvidia)
    );
}

// ---- child-process isolation ----

struct FakeChild(RefCell<Option<ChildOutcome>>, RefCell<Vec<String>>);
impl FakeChild {
    fn new(o: ChildOutcome) -> Self {
        FakeChild(RefCell::new(Some(o)), RefCell::new(vec![]))
    }
}
impl ChildRunner for FakeChild {
    fn run(&self, args: &[String], _t: Duration) -> ChildOutcome {
        *self.1.borrow_mut() = args.to_vec();
        self.0.borrow_mut().take().unwrap()
    }
}

#[test]
fn the_child_reports_its_verdict_over_stdout_and_is_told_the_device() {
    let v = Verdict {
        approved: true,
        speedup: 2.5,
        reason: "speedup 2.50x".into(),
    };
    let child = FakeChild::new(ChildOutcome::Exited {
        success: true,
        stdout: format!("noise\n{}\n", encode_verdict(&v)),
    });
    let mut d = dev("RTX", "5");
    d.index = 2;
    let got = probe_in_child(&child, &d, Duration::from_secs(60));
    assert_eq!(got, v);
    assert_eq!(
        *child.1.borrow(),
        vec![
            "--probe-gpu".to_string(),
            "--device".into(),
            "2".into(),
            "--device-name".into(),
            "RTX".into()
        ]
    );
}

#[test]
fn a_hung_probe_is_a_rejection_not_a_hang() {
    let child = FakeChild::new(ChildOutcome::TimedOut);
    let v = probe_in_child(&child, &dev("RTX", "5"), Duration::from_secs(1));
    assert!(!v.approved);
    assert!(v.reason.contains("timed out"), "{}", v.reason);
}

#[test]
fn a_crashed_probe_is_a_rejection() {
    let child = FakeChild::new(ChildOutcome::Exited {
        success: false,
        stdout: String::new(),
    });
    let v = probe_in_child(&child, &dev("RTX", "5"), Duration::from_secs(1));
    assert!(!v.approved);
    assert!(v.reason.contains("crashed"), "{}", v.reason);
}

#[test]
fn garbage_output_and_spawn_failure_are_rejections() {
    let v = probe_in_child(
        &FakeChild::new(ChildOutcome::Exited {
            success: true,
            stdout: "hello".into(),
        }),
        &dev("a", "b"),
        Duration::from_secs(1),
    );
    assert!(!v.approved);
    let v = probe_in_child(
        &FakeChild::new(ChildOutcome::SpawnFailed("no exe".into())),
        &dev("a", "b"),
        Duration::from_secs(1),
    );
    assert!(!v.approved && v.reason.contains("no exe"));
}

#[test]
fn a_rejection_from_a_hung_probe_is_cached_so_it_is_not_retried_every_start() {
    let mut st = State::default();
    let d = dev("RTX", "5");
    let v = probe_in_child(
        &FakeChild::new(ChildOutcome::TimedOut),
        &d,
        Duration::from_secs(1),
    );
    record_verdict(&mut st, &d, &v);
    assert_eq!(
        select(&Gpu::Auto, &st, &[d]),
        Selection::Cpu(CpuReason::NotApproved)
    );
}

#[cfg(windows)]
mod real_process {
    use super::*;
    fn cmd(args: &[&str]) -> StdChildRunner {
        StdChildRunner::with_program("cmd", args.iter().map(|s| s.to_string()).collect())
    }

    #[test]
    fn a_real_child_that_prints_is_captured() {
        let r = cmd(&["/C", "echo", "PROBE-OK"]);
        match r.run(&[], Duration::from_secs(20)) {
            ChildOutcome::Exited { success, stdout } => {
                assert!(success);
                assert!(stdout.contains("PROBE-OK"), "{stdout}");
            }
            o => panic!("{o:?}"),
        }
    }

    #[test]
    fn a_real_child_that_hangs_is_killed_at_the_timeout() {
        let r = StdChildRunner::with_program(
            "powershell",
            vec![
                "-NoProfile".into(),
                "-Command".into(),
                "Start-Sleep -Seconds 30".into(),
            ],
        );
        let t = std::time::Instant::now();
        assert!(matches!(
            r.run(&[], Duration::from_millis(400)),
            ChildOutcome::TimedOut
        ));
        assert!(t.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn a_real_child_that_fails_is_reported() {
        let r = cmd(&["/C", "exit", "3"]);
        assert!(matches!(
            r.run(&[], Duration::from_secs(20)),
            ChildOutcome::Exited { success: false, .. }
        ));
    }
}

// ---- opening the recognizer; fallback ----

#[derive(Default)]
struct Counters {
    library_loads: AtomicU32,
    cpu_loads: AtomicU32,
    gpu_loads: AtomicU32,
}
struct FakeBackend {
    c: Arc<Counters>,
    clock: Clk,
    gpu_fail_after: Option<usize>,
    library_fails: bool,
}
impl FakeBackend {
    fn new() -> (Self, Arc<Counters>) {
        let c = Arc::new(Counters::default());
        (
            FakeBackend {
                c: c.clone(),
                clock: Clk::default(),
                gpu_fail_after: None,
                library_fails: false,
            },
            c,
        )
    }
}
impl GpuBackend for FakeBackend {
    fn load_library(&self) -> Result<(), RecognizerError> {
        self.c.library_loads.fetch_add(1, SeqCst);
        if self.library_fails {
            Err(RecognizerError("no vulkan-1.dll".into()))
        } else {
            Ok(())
        }
    }
    fn load_cpu(&self) -> Result<Box<dyn Engine>, RecognizerError> {
        self.c.cpu_loads.fetch_add(1, SeqCst);
        Ok(Box::new(FakeEngine::new(
            &self.clock,
            10,
            &["cpu", "words"],
        )))
    }
    fn load_gpu(&self, _d: &GpuDevice) -> Result<Box<dyn Engine>, RecognizerError> {
        self.c.gpu_loads.fetch_add(1, SeqCst);
        let mut e = FakeEngine::new(&self.clock, 1, &["gpu", "words"]);
        e.fail_after = self.gpu_fail_after;
        Ok(Box::new(e))
    }
}

fn text(segs: &[Segment]) -> String {
    segs.iter()
        .map(|s| s.text.clone())
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn with_the_gpu_unapproved_the_backend_library_is_never_loaded() {
    for sel in [
        Selection::Cpu(CpuReason::Setting),
        Selection::Cpu(CpuReason::NotApproved),
        Selection::Cpu(CpuReason::DisabledAfterError),
        Selection::Cpu(CpuReason::NoDevice),
    ] {
        let (b, c) = FakeBackend::new();
        let mut r = open_recognizer(Box::new(b), &sel, Box::new(|_| {}));
        assert!(!r.using_gpu());
        assert_eq!(text(&r.recognize(&[0.0], "en", None)), "cpu words");
        assert_eq!(c.library_loads.load(SeqCst), 0, "{sel:?}");
        assert_eq!(c.gpu_loads.load(SeqCst), 0);
    }
}

#[test]
fn an_approved_device_loads_the_library_and_decodes_on_the_gpu() {
    let (b, c) = FakeBackend::new();
    let mut r = open_recognizer(
        Box::new(b),
        &Selection::Gpu(dev("RTX", "5")),
        Box::new(|_| {}),
    );
    assert!(r.using_gpu());
    assert_eq!(text(&r.recognize(&[0.0], "en", None)), "gpu words");
    assert_eq!(
        (
            c.library_loads.load(SeqCst),
            c.gpu_loads.load(SeqCst),
            c.cpu_loads.load(SeqCst)
        ),
        (1, 1, 0)
    );
}

#[test]
fn a_gpu_error_mid_dictation_finishes_that_chunk_on_cpu_with_the_same_audio_and_disables_the_gpu() {
    let (mut b, c) = FakeBackend::new();
    b.gpu_fail_after = Some(1); // first decode fine, second fails
    let errors = Arc::new(Mutex::new(Vec::<String>::new()));
    let e2 = errors.clone();
    let mut r = open_recognizer(
        Box::new(b),
        &Selection::Gpu(dev("RTX", "5")),
        Box::new(move |m| e2.lock().unwrap().push(m.to_string())),
    );
    assert_eq!(text(&r.recognize(&[0.0], "en", None)), "gpu words");
    // The failing decode still returns words (from CPU), nothing is lost.
    assert_eq!(text(&r.recognize(&[0.0], "en", None)), "cpu words");
    assert!(!r.using_gpu());
    assert_eq!(errors.lock().unwrap().len(), 1);
    assert!(errors.lock().unwrap()[0].contains("device lost"));
    // Later decodes stay on CPU and do not report again.
    assert_eq!(text(&r.recognize(&[0.0], "en", None)), "cpu words");
    assert_eq!(
        (
            errors.lock().unwrap().len(),
            c.cpu_loads.load(SeqCst),
            c.gpu_loads.load(SeqCst)
        ),
        (1, 1, 1)
    );
}

#[test]
fn a_backend_that_cannot_load_falls_back_to_cpu_and_reports_the_error() {
    let (mut b, _c) = FakeBackend::new();
    b.library_fails = true;
    let errors = Arc::new(Mutex::new(Vec::<String>::new()));
    let e2 = errors.clone();
    let mut r = open_recognizer(
        Box::new(b),
        &Selection::Gpu(dev("RTX", "5")),
        Box::new(move |m| e2.lock().unwrap().push(m.to_string())),
    );
    assert!(!r.using_gpu());
    assert_eq!(text(&r.recognize(&[0.0], "en", None)), "cpu words");
    assert_eq!(errors.lock().unwrap().len(), 1);
}

// ---- probe child body and adapter discovery ----

#[test]
fn the_probe_child_body_benchmarks_through_the_backend() {
    let (b, c) = FakeBackend::new();
    // Fake engines: cpu "cpu words" at 10 ms, gpu "gpu words" at 1 ms: output differs, so rejected.
    let clock = b.clock.clone();
    let now = move || clock.get();
    let v = probe_with_backend(&b, &dev("RTX", "5"), &[0.0; 16], "en", &now);
    assert!(!v.approved);
    assert_eq!(
        (
            c.library_loads.load(SeqCst),
            c.gpu_loads.load(SeqCst),
            c.cpu_loads.load(SeqCst)
        ),
        (1, 1, 1)
    );
}

#[test]
fn the_probe_child_body_rejects_when_the_backend_library_will_not_load() {
    let (mut b, _c) = FakeBackend::new();
    b.library_fails = true;
    let v = probe_with_backend(&b, &dev("RTX", "5"), &[0.0; 16], "en", &|| Duration::ZERO);
    assert!(!v.approved);
    assert!(v.reason.contains("vulkan-1.dll"), "{}", v.reason);
}

#[test]
fn display_adapters_and_driver_versions_come_from_registry_output_without_loading_a_gpu_backend() {
    let k = r"HKEY_LOCAL_MACHINE\SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}";
    let lines = [
        String::new(),
        format!(r"{k}\0000"),
        "    DriverDesc    REG_SZ    Microsoft Basic Display Adapter".into(),
        "    DriverVersion    REG_SZ    10.0.1".into(),
        String::new(),
        format!(r"{k}\0001"),
        "    DriverDesc    REG_SZ    AMD Radeon(TM) 780M Graphics".into(),
        "    DriverVersion    REG_SZ    31.0.21001.2011".into(),
        String::new(),
        format!(r"{k}\Properties"),
        "    Other    REG_SZ    x".into(),
    ];
    let reg = lines.join("\r\n");
    let got = parse_display_adapters(&reg);
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].name, "AMD Radeon(TM) 780M Graphics");
    assert_eq!(got[0].driver, "31.0.21001.2011");
    assert_eq!(got[0].index, 0);
}

#[cfg(not(feature = "vulkan"))]
#[test]
fn a_cpu_only_build_rejects_the_gpu_in_the_probe_without_needing_a_model() {
    let b = WhisperBackend::new(std::path::Path::new("no-such-model.bin"));
    let v = probe_with_backend(&b, &dev("RTX", "5"), &[0.0; 16], "en", &|| Duration::ZERO);
    assert!(!v.approved);
    assert!(v.reason.contains("no GPU backend"), "{}", v.reason);
}

#[test]
fn the_embedded_probe_clip_decodes_to_about_eleven_seconds() {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/jfk.wav"
    ))
    .unwrap();
    let a = decode_wav(&bytes).unwrap();
    assert!(
        (10.5..11.5).contains(&(a.len() as f32 / 16000.0)),
        "{}",
        a.len()
    );
    assert!(decode_wav(b"nope").is_none());
}

#[test]
fn probe_child_main_with_bad_arguments_exits_nonzero_without_output() {
    assert_eq!(probe_child_main(&["--probe-gpu".to_string()]), 2);
}

// ---- host entry point ----

struct CountingRunner(RefCell<u32>, Verdict);
impl ChildRunner for CountingRunner {
    fn run(&self, _a: &[String], _t: Duration) -> ChildOutcome {
        *self.0.borrow_mut() += 1;
        ChildOutcome::Exited {
            success: true,
            stdout: encode_verdict(&self.1),
        }
    }
}

fn read_state(sp: &std::path::Path) -> State {
    aloud_type_config::ConfigFile::<State>::open(sp)
        .value()
        .clone()
}

#[test]
fn the_host_loads_on_the_preferred_gpu_without_a_test_and_records_it() {
    let dir = tempfile::tempdir().unwrap();
    let sp = dir.path().join("state.toml");
    let adapters = [dev("Intel UHD", "1"), dev("NVIDIA RTX", "2")];
    let (b, c) = FakeBackend::new();
    let mut r = open_for_host(Box::new(b), &Gpu::Auto, &sp, &adapters);
    assert!(r.using_gpu());
    assert_eq!(text(&r.recognize(&[0.0], "en", None)), "gpu words");
    assert_eq!(c.library_loads.load(SeqCst), 1);
    let st = read_state(&sp);
    assert!(st.gpu_probes.is_empty(), "no test at start");
    assert_eq!(st.gpu_adapters, ["Intel UHD", "NVIDIA RTX"]);
    assert_eq!(st.gpu_in_use.as_deref(), Some("NVIDIA RTX"));
    assert!(!st.gpu_load_pending);
}

#[test]
fn on_cpu_the_host_records_no_gpu_in_use_and_never_loads_the_backend() {
    let dir = tempfile::tempdir().unwrap();
    let sp = dir.path().join("state.toml");
    let (b, c) = FakeBackend::new();
    let r = open_for_host(Box::new(b), &Gpu::CpuOnly, &sp, &[dev("RTX", "5")]);
    assert!(!r.using_gpu());
    assert_eq!(c.library_loads.load(SeqCst), 0);
    assert_eq!(read_state(&sp).gpu_in_use, None);
}

#[test]
fn a_gpu_load_that_crashed_the_process_disables_the_gpu_at_the_next_start() {
    let dir = tempfile::tempdir().unwrap();
    let sp = dir.path().join("state.toml");
    // The previous start set the marker and never cleared it.
    aloud_type_config::ConfigFile::<State>::open(&sp)
        .update(|s| s.gpu_load_pending = true)
        .unwrap();
    let (b, c) = FakeBackend::new();
    let r = open_for_host(Box::new(b), &Gpu::Auto, &sp, &[dev("RTX", "5")]);
    assert!(!r.using_gpu());
    assert_eq!(c.library_loads.load(SeqCst), 0);
    let st = read_state(&sp);
    assert!(st.gpu_disabled_after_error);
    assert!(!st.gpu_load_pending);
    // Picking the GPU again in Settings clears the error.
    clear_error_on_disk(&sp);
    let (b, _c) = FakeBackend::new();
    assert!(open_for_host(Box::new(b), &Gpu::Auto, &sp, &[dev("RTX", "5")]).using_gpu());
}

#[test]
fn the_settings_test_probes_every_adapter_and_saves_each_verdict() {
    let dir = tempfile::tempdir().unwrap();
    let sp = dir.path().join("state.toml");
    aloud_type_config::ConfigFile::<State>::open(&sp)
        .update(|s| {
            s.gpu_disabled_after_error = true;
            s.gpu_probes.insert(
                "Old|1".into(),
                aloud_type_config::GpuProbe {
                    approved: true,
                    speedup: 9.0,
                },
            );
        })
        .unwrap();
    let runner = CountingRunner(RefCell::new(0), ok_verdict());
    let adapters = [dev("Intel UHD", "1"), dev("NVIDIA RTX", "2")];
    test_all_on_disk(&runner, &adapters, &sp, Duration::from_secs(5));
    assert_eq!(*runner.0.borrow(), 2);
    let st = read_state(&sp);
    assert!(!st.gpu_disabled_after_error);
    assert_eq!(
        st.gpu_probes.keys().cloned().collect::<Vec<_>>(),
        ["Intel UHD|1", "NVIDIA RTX|2"]
    );
}

#[test]
fn a_mid_dictation_gpu_error_is_recorded_in_the_state_file_and_survives_restart() {
    let dir = tempfile::tempdir().unwrap();
    let sp = dir.path().join("state.toml");
    let adapters = [dev("RTX", "5")];
    let (mut b, _c) = FakeBackend::new();
    b.gpu_fail_after = Some(0);
    let mut r = open_for_host(Box::new(b), &Gpu::Auto, &sp, &adapters);
    assert_eq!(text(&r.recognize(&[0.0], "en", None)), "cpu words");
    let st = read_state(&sp);
    assert!(st.gpu_disabled_after_error);
    assert_eq!(st.gpu_in_use, None);
    let (b, c) = FakeBackend::new();
    let r = open_for_host(Box::new(b), &Gpu::Auto, &sp, &adapters);
    assert!(!r.using_gpu());
    assert_eq!(c.library_loads.load(SeqCst), 0);
}

#[test]
fn adapters_are_read_from_the_display_adapter_device_class() {
    // GUID_DEVCLASS_DISPLAY. A wrong GUID finds no adapters, so the GPU is never probed.
    assert!(DISPLAY_CLASS_KEY.ends_with(r"\{4d36e968-e325-11ce-bfc1-08002be10318}"));
}

fn names(n: &[&str]) -> Vec<String> {
    n.iter().map(|s| s.to_string()).collect()
}

#[test]
fn registry_adapters_are_matched_to_the_backend_device_order_by_name() {
    // Registry order: Intel 0, NVIDIA 1. Vulkan order: discrete first.
    let vk = names(&[
        "NVIDIA GeForce RTX 3060 Laptop GPU",
        "Intel(R) Iris(R) Xe Graphics",
    ]);
    let mut intel = dev("Intel(R) Iris(R) Xe Graphics", "30.0.101.2079");
    intel.index = 0;
    let mut nvidia = dev("NVIDIA GeForce RTX 3060 Laptop GPU", "32.0.15.6109");
    nvidia.index = 1;
    assert_eq!(backend_index(&vk, &intel), Some(1));
    assert_eq!(backend_index(&vk, &nvidia), Some(0));
}

#[test]
fn backend_names_match_ignoring_case_and_extra_words() {
    let vk = names(&["AMD Radeon(TM) 780M Graphics (RADV PHOENIX)"]);
    assert_eq!(
        backend_index(&vk, &dev("amd radeon(tm) 780m graphics", "1")),
        Some(0)
    );
}

#[test]
fn an_adapter_the_backend_cannot_see_has_no_backend_index() {
    let vk = names(&["NVIDIA GeForce RTX 3060 Laptop GPU"]);
    assert_eq!(
        backend_index(&vk, &dev("Microsoft Hyper-V Video", "1")),
        None
    );
}

#[test]
fn a_probe_started_with_only_an_index_keeps_that_index() {
    let mut d = dev("", "");
    d.index = 1;
    assert_eq!(backend_index(&names(&["a", "b"]), &d), Some(1));
}
