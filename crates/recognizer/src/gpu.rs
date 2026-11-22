//! GPU acceleration: probe benchmark, per-device probe cache, child-process isolation,
//! backend selection and mid-Dictation fallback to CPU.
//!
//! Everything here works against the [`Engine`] and [`GpuBackend`] seams so it is testable
//! with fakes. The real Whisper/Vulkan implementations live at the bottom of this module.
use crate::RecognizerError;
use aloud_type_core::Segment;
use std::time::Duration;

/// One decoder instance, on CPU or GPU.
pub trait Engine: Send {
    fn try_recognize(
        &mut self,
        audio: &[f32],
        language: &str,
        prompt: Option<&str>,
    ) -> Result<Vec<Segment>, RecognizerError>;
}

/// An installed GPU as identified for caching: probe results are valid for one device on one driver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpuDevice {
    pub name: String,
    pub driver: String,
    /// Backend device index (what the probe child is told to use).
    pub index: i32,
}

impl GpuDevice {
    /// Key used in the state file: `<device>|<driver>`.
    pub fn key(&self) -> String {
        format!("{}|{}", self.name, self.driver)
    }
}

/// Minimum GPU/CPU speedup for approval (spec: about 1.3x).
pub const MIN_SPEEDUP: f64 = 1.3;

#[derive(Debug, Clone, PartialEq)]
pub struct Verdict {
    pub approved: bool,
    pub speedup: f64,
    /// Human-readable reason (why rejected, or a short note when approved).
    pub reason: String,
}

impl Verdict {
    fn rejected(speedup: f64, reason: impl Into<String>) -> Self {
        Verdict { approved: false, speedup, reason: reason.into() }
    }
}

fn words_of(segs: &[Segment]) -> Vec<String> {
    segs.iter()
        .flat_map(|s| s.words.iter())
        .map(|w| w.text.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase())
        .filter(|w| !w.is_empty())
        .collect()
}

/// Benchmark `gpu` against `cpu` on the same audio. The GPU gets one untimed warm-up decode
/// (shader compilation) before the timed one. `now` is a monotonic clock.
pub fn benchmark(
    cpu: &mut dyn Engine,
    gpu: &mut dyn Engine,
    audio: &[f32],
    language: &str,
    now: &dyn Fn() -> Duration,
) -> Verdict {
    if let Err(e) = gpu.try_recognize(audio, language, None) {
        return Verdict::rejected(0.0, format!("GPU decode failed: {e}"));
    }
    let t0 = now();
    let cpu_out = match cpu.try_recognize(audio, language, None) {
        Ok(o) => o,
        Err(e) => return Verdict::rejected(0.0, format!("CPU decode failed: {e}")),
    };
    let cpu_t = now().saturating_sub(t0);
    let t1 = now();
    let gpu_out = match gpu.try_recognize(audio, language, None) {
        Ok(o) => o,
        Err(e) => return Verdict::rejected(0.0, format!("GPU decode failed: {e}")),
    };
    let gpu_t = now().saturating_sub(t1);
    let speedup = if gpu_t.is_zero() { f64::INFINITY } else { cpu_t.as_secs_f64() / gpu_t.as_secs_f64() };
    if words_of(&cpu_out) != words_of(&gpu_out) {
        return Verdict::rejected(speedup, "GPU output differs from CPU output");
    }
    if speedup < MIN_SPEEDUP {
        return Verdict::rejected(speedup, format!("speedup {speedup:.2}x is below {MIN_SPEEDUP}x"));
    }
    Verdict { approved: true, speedup, reason: format!("speedup {speedup:.2}x") }
}

// ---- selection and per-device cache ----

use aloud_type_config::{Gpu, GpuProbe, State};

/// Save a probe verdict for its device and driver (the cache entry later reused by [`select`]).
pub fn record_verdict(state: &mut State, device: &GpuDevice, v: &Verdict) {
    state.gpu_probes.insert(device.key(), GpuProbe { approved: v.approved, speedup: v.speedup });
}

/// A backend error happened during a Dictation: disable the GPU (recorded in state).
pub fn record_gpu_error(state: &mut State) {
    state.gpu_disabled_after_error = true;
}

/// Settings "re-test": forget the error flag and all saved results so the next start probes again.
pub fn request_retest(state: &mut State) {
    state.gpu_disabled_after_error = false;
    state.gpu_probes.clear();
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CpuReason {
    /// Settings say "CPU only".
    Setting,
    DisabledAfterError,
    NoDevice,
    /// Every candidate was probed and rejected.
    NotApproved,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selection {
    Cpu(CpuReason),
    /// Run on this approved device.
    Gpu(GpuDevice),
    /// These devices have no saved result for their current driver: run the probe child for them.
    NeedsProbe(Vec<GpuDevice>),
}

/// Decide what the resident process does at model load, from settings, state and the installed
/// devices. Pure: it never touches the GPU backend.
pub fn select(setting: &Gpu, state: &State, installed: &[GpuDevice]) -> Selection {
    if *setting == Gpu::CpuOnly {
        return Selection::Cpu(CpuReason::Setting);
    }
    if state.gpu_disabled_after_error {
        return Selection::Cpu(CpuReason::DisabledAfterError);
    }
    let candidates: Vec<&GpuDevice> = installed
        .iter()
        .filter(|d| match setting {
            Gpu::Device(name) => &d.name == name,
            _ => true,
        })
        .collect();
    if candidates.is_empty() {
        return Selection::Cpu(CpuReason::NoDevice);
    }
    let best = candidates
        .iter()
        .filter_map(|d| state.gpu_probes.get(&d.key()).filter(|p| p.approved).map(|p| (*d, p.speedup)))
        .max_by(|a, b| a.1.total_cmp(&b.1));
    if let Some((d, _)) = best {
        return Selection::Gpu(d.clone());
    }
    let unprobed: Vec<GpuDevice> =
        candidates.into_iter().filter(|d| !state.gpu_probes.contains_key(&d.key())).cloned().collect();
    if unprobed.is_empty() {
        Selection::Cpu(CpuReason::NotApproved)
    } else {
        Selection::NeedsProbe(unprobed)
    }
}

// ---- child-process isolation ----

use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// Command-line flag of the probe mode of the one executable.
pub const PROBE_FLAG: &str = "--probe-gpu";
/// Default probe timeout (model load, shader compilation and two decodes).
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(120);
const RESULT_PREFIX: &str = "ALOUDTYPE-PROBE-RESULT";

#[derive(Debug)]
pub enum ChildOutcome {
    Exited { success: bool, stdout: String },
    TimedOut,
    SpawnFailed(String),
}

/// Runs the probe child. The real one spawns a process; tests inject fakes.
pub trait ChildRunner {
    fn run(&self, args: &[String], timeout: Duration) -> ChildOutcome;
}

/// One-line stdout encoding of a verdict (tab separated: approved, speedup, reason).
pub fn encode_verdict(v: &Verdict) -> String {
    let reason = v.reason.replace(['\t', '\r', '\n'], " ");
    format!("{RESULT_PREFIX}\t{}\t{}\t{reason}", v.approved, v.speedup)
}

pub fn parse_verdict(stdout: &str) -> Option<Verdict> {
    let line = stdout.lines().rev().find(|l| l.starts_with(RESULT_PREFIX))?;
    let mut f = line.splitn(4, '\t').skip(1);
    let approved = f.next()?.parse().ok()?;
    let speedup = f.next()?.parse().ok()?;
    Some(Verdict { approved, speedup, reason: f.next().unwrap_or("").to_string() })
}

/// Run the probe for `device` in a child process. A hung, crashed or garbled child is a rejection
/// of that device; it can never affect the calling (resident) process.
pub fn probe_in_child(runner: &dyn ChildRunner, device: &GpuDevice, timeout: Duration) -> Verdict {
    let args = vec![PROBE_FLAG.to_string(), "--device".into(), device.index.to_string()];
    match runner.run(&args, timeout) {
        ChildOutcome::TimedOut => Verdict::rejected(0.0, format!("GPU probe timed out after {}s", timeout.as_secs())),
        ChildOutcome::SpawnFailed(e) => Verdict::rejected(0.0, format!("GPU probe could not start: {e}")),
        ChildOutcome::Exited { success: false, .. } => Verdict::rejected(0.0, "GPU probe crashed"),
        ChildOutcome::Exited { success: true, stdout } => {
            parse_verdict(&stdout).unwrap_or_else(|| Verdict::rejected(0.0, "GPU probe gave no result"))
        }
    }
}

/// Spawns a program, captures stdout, kills it at the timeout.
pub struct StdChildRunner {
    program: PathBuf,
    /// Arguments placed before the ones passed to `run`.
    prefix: Vec<String>,
}

impl StdChildRunner {
    /// The current executable (the probe is a mode of the one executable).
    pub fn current_exe() -> std::io::Result<Self> {
        Ok(Self { program: std::env::current_exe()?, prefix: vec![] })
    }
    pub fn with_program(program: impl Into<PathBuf>, prefix: Vec<String>) -> Self {
        Self { program: program.into(), prefix }
    }
}

impl ChildRunner for StdChildRunner {
    fn run(&self, args: &[String], timeout: Duration) -> ChildOutcome {
        let mut cmd = Command::new(&self.program);
        cmd.args(&self.prefix).args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => return ChildOutcome::SpawnFailed(e.to_string()),
        };
        let mut out = child.stdout.take().expect("piped");
        let reader = std::thread::spawn(move || {
            let mut s = Vec::new();
            let _ = out.read_to_end(&mut s);
            String::from_utf8_lossy(&s).into_owned()
        });
        let start = std::time::Instant::now();
        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    let stdout = reader.join().unwrap_or_default();
                    return ChildOutcome::Exited { success: status.success(), stdout };
                }
                Ok(None) if start.elapsed() >= timeout => {
                    let _ = child.kill();
                    let _ = child.wait();
                    // A grandchild may keep the pipe open, so do not wait for the reader.
                    drop(reader);
                    return ChildOutcome::TimedOut;
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(20)),
                Err(e) => return ChildOutcome::SpawnFailed(e.to_string()),
            }
        }
    }
}

// ---- opening the recognizer, mid-Dictation fallback ----

use aloud_type_core::Recognizer;

/// Loads engines. `load_library` is the only call that touches the optional GPU backend library,
/// and the resident process makes it only for an approved device.
pub trait GpuBackend: Send {
    fn load_library(&self) -> Result<(), RecognizerError>;
    fn load_cpu(&self) -> Result<Box<dyn Engine>, RecognizerError>;
    fn load_gpu(&self, device: &GpuDevice) -> Result<Box<dyn Engine>, RecognizerError>;
}

/// Recognizer that decodes on the GPU when one was approved and finishes on CPU if the GPU fails.
pub struct GpuFallback {
    backend: Box<dyn GpuBackend>,
    gpu: Option<Box<dyn Engine>>,
    cpu: Option<Box<dyn Engine>>,
    on_gpu_error: Box<dyn FnMut(&str) + Send>,
}

/// Open the recognizer for `selection`. Anything but `Selection::Gpu` loads CPU only and never
/// touches the backend library. `on_gpu_error` is called once when the GPU is given up for the
/// session (the host records it in state via [`record_gpu_error`]).
pub fn open_recognizer(
    backend: Box<dyn GpuBackend>,
    selection: &Selection,
    on_gpu_error: Box<dyn FnMut(&str) + Send>,
) -> GpuFallback {
    let mut r = GpuFallback { backend, gpu: None, cpu: None, on_gpu_error };
    match selection {
        Selection::Gpu(d) => {
            let loaded = r.backend.load_library().and_then(|_| r.backend.load_gpu(d));
            match loaded {
                Ok(e) => r.gpu = Some(e),
                Err(e) => {
                    (r.on_gpu_error)(&e.to_string());
                    r.cpu = r.load_cpu_logged();
                }
            }
        }
        _ => r.cpu = r.load_cpu_logged(),
    }
    r
}

impl GpuFallback {
    pub fn using_gpu(&self) -> bool {
        self.gpu.is_some()
    }

    fn load_cpu_logged(&self) -> Option<Box<dyn Engine>> {
        match self.backend.load_cpu() {
            Ok(e) => Some(e),
            Err(e) => {
                eprintln!("recognizer: cannot load CPU model: {e}");
                None
            }
        }
    }

    /// Decode, failing over from GPU to CPU on a backend error. The same buffered audio is
    /// re-decoded on CPU, so no words are lost.
    pub fn try_recognize(
        &mut self,
        audio: &[f32],
        language: &str,
        prompt: Option<&str>,
    ) -> Result<Vec<Segment>, RecognizerError> {
        if let Some(gpu) = self.gpu.as_mut() {
            match gpu.try_recognize(audio, language, prompt) {
                Ok(s) => return Ok(s),
                Err(e) => {
                    self.gpu = None; // release the GPU model and backend resources
                    (self.on_gpu_error)(&e.to_string());
                }
            }
        }
        if self.cpu.is_none() {
            self.cpu = Some(self.backend.load_cpu()?);
        }
        self.cpu.as_mut().expect("loaded").try_recognize(audio, language, prompt)
    }
}

impl Recognizer for GpuFallback {
    fn recognize(&mut self, audio: &[f32], language: &str, prompt: Option<&str>) -> Vec<Segment> {
        self.try_recognize(audio, language, prompt).unwrap_or_else(|e| {
            eprintln!("recognizer: {e}");
            Vec::new()
        })
    }
}

// ---- probe child body ----

/// Body of the probe child: load the backend, load CPU and GPU models, benchmark.
pub fn probe_with_backend(
    backend: &dyn GpuBackend,
    device: &GpuDevice,
    audio: &[f32],
    language: &str,
    now: &dyn Fn() -> Duration,
) -> Verdict {
    if let Err(e) = backend.load_library() {
        return Verdict::rejected(0.0, format!("GPU backend unavailable: {e}"));
    }
    let mut gpu = match backend.load_gpu(device) {
        Ok(e) => e,
        Err(e) => return Verdict::rejected(0.0, format!("GPU model load failed: {e}")),
    };
    let mut cpu = match backend.load_cpu() {
        Ok(e) => e,
        Err(e) => return Verdict::rejected(0.0, format!("CPU model load failed: {e}")),
    };
    benchmark(cpu.as_mut(), gpu.as_mut(), audio, language, now)
}

// ---- adapter discovery (no GPU backend loaded) ----

/// Parse `reg query <display class key> /s` output into display adapters with driver versions.
/// Discovering adapters this way keeps the GPU backend library unloaded in the resident process.
pub fn parse_display_adapters(reg_output: &str) -> Vec<GpuDevice> {
    let mut out: Vec<GpuDevice> = Vec::new();
    let (mut desc, mut ver): (Option<String>, Option<String>) = (None, None);
    let flush = |desc: &mut Option<String>, ver: &mut Option<String>, out: &mut Vec<GpuDevice>| {
        if let (Some(d), Some(v)) = (desc.take(), ver.take()) {
            let l = d.to_lowercase();
            if !l.contains("basic display") && !l.contains("remote display") {
                let index = out.len() as i32;
                out.push(GpuDevice { name: d, driver: v, index });
            }
        }
        *desc = None;
        *ver = None;
    };
    for line in reg_output.lines() {
        let line = line.trim();
        if line.starts_with("HKEY_") {
            flush(&mut desc, &mut ver, &mut out);
        } else if let Some(v) = value_of(line, "DriverDesc") {
            desc = Some(v);
        } else if let Some(v) = value_of(line, "DriverVersion") {
            ver = Some(v);
        }
    }
    flush(&mut desc, &mut ver, &mut out);
    out
}

fn value_of(line: &str, name: &str) -> Option<String> {
    let rest = line.strip_prefix(name)?.trim_start();
    let rest = rest.strip_prefix("REG_SZ")?;
    Some(rest.trim().to_string())
}

/// Installed display adapters, via `reg.exe` (Windows only; empty elsewhere or on failure).
pub fn installed_adapters() -> Vec<GpuDevice> {
    #[cfg(windows)]
    {
        let out = Command::new("reg")
            .args(["query", r"HKLM\SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bf1c-08002be10318}", "/s"])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output();
        match out {
            Ok(o) => parse_display_adapters(&String::from_utf8_lossy(&o.stdout)),
            Err(_) => Vec::new(),
        }
    }
    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

// ---- real Whisper engines ----

use crate::WhisperRecognizer;
use std::path::Path;

impl Engine for WhisperRecognizer {
    fn try_recognize(
        &mut self,
        audio: &[f32],
        language: &str,
        prompt: Option<&str>,
    ) -> Result<Vec<Segment>, RecognizerError> {
        WhisperRecognizer::try_recognize(self, audio, language, prompt)
    }
}

/// Real backend: Whisper models on CPU, or on a Vulkan device when built with the `vulkan` feature.
pub struct WhisperBackend {
    model: PathBuf,
    threads: usize,
}

impl WhisperBackend {
    pub fn new(model: &Path) -> Self {
        Self { model: model.to_path_buf(), threads: crate::decode_threads(num_cpus::get_physical()) }
    }
}

impl GpuBackend for WhisperBackend {
    fn load_library(&self) -> Result<(), RecognizerError> {
        // With the optional-DLL build this is where the backend DLL gets loaded. In the current
        // static build, enumerating the Vulkan devices initialises the backend (and fails if
        // vulkan-1.dll or a driver is missing).
        #[cfg(feature = "vulkan")]
        {
            if whisper_rs::vulkan::list_devices().is_empty() {
                return Err(RecognizerError("no Vulkan device".into()));
            }
            Ok(())
        }
        #[cfg(not(feature = "vulkan"))]
        Err(RecognizerError("this build has no GPU backend".into()))
    }
    fn load_cpu(&self) -> Result<Box<dyn Engine>, RecognizerError> {
        Ok(Box::new(WhisperRecognizer::load_on(&self.model, self.threads, None)?))
    }
    fn load_gpu(&self, device: &GpuDevice) -> Result<Box<dyn Engine>, RecognizerError> {
        Ok(Box::new(WhisperRecognizer::load_on(&self.model, self.threads, Some(device.index))?))
    }
}

/// Embedded probe clip: the public-domain JFK sample the recognizer tests use (16 kHz mono).
const PROBE_WAV: &[u8] = include_bytes!("../tests/fixtures/jfk.wav");

/// Decode a 16-bit PCM mono WAV into f32 samples.
pub fn decode_wav(b: &[u8]) -> Option<Vec<f32>> {
    if b.len() < 12 || &b[0..4] != b"RIFF" {
        return None;
    }
    let mut i = 12;
    while i + 8 <= b.len() {
        let len = u32::from_le_bytes(b[i + 4..i + 8].try_into().ok()?) as usize;
        if &b[i..i + 4] == b"data" {
            let end = (i + 8 + len).min(b.len());
            return Some(
                b[i + 8..end].chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0).collect(),
            );
        }
        i += 8 + len + (len & 1);
    }
    None
}

/// Entry point of `--probe-gpu --device N --model PATH`: benchmark, print the verdict line, and
/// return the process exit code. Runs inside the child process only.
pub fn probe_child_main(args: &[String]) -> i32 {
    let arg = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1));
    let (Some(model), Some(index)) = (arg("--model"), arg("--device").and_then(|d| d.parse::<i32>().ok())) else {
        eprintln!("usage: --probe-gpu --device N --model PATH");
        return 2;
    };
    let Some(audio) = decode_wav(PROBE_WAV) else { return 2 };
    let backend = WhisperBackend::new(Path::new(model));
    let device = GpuDevice { name: String::new(), driver: String::new(), index };
    let t0 = std::time::Instant::now();
    let v = probe_with_backend(&backend, &device, &audio, "en", &move || t0.elapsed());
    println!("{}", encode_verdict(&v));
    0
}

/// The probe runner for the resident process: this executable in `--probe-gpu` mode, told which model.
pub fn probe_runner(model: &Path) -> std::io::Result<StdChildRunner> {
    let mut r = StdChildRunner::current_exe()?;
    r.prefix = vec!["--model".into(), model.display().to_string()];
    Ok(r)
}

// ---- host entry point ----

use aloud_type_config::ConfigFile;

fn update_state(path: &Path, f: impl FnOnce(&mut State)) {
    let mut file = ConfigFile::<State>::open(path);
    if let Err(e) = file.update(f) {
        eprintln!("gpu: cannot save state: {e}");
    }
}

/// Settings "re-test" on the resident side: clear the error flag and saved results on disk.
pub fn retest_on_disk(state_path: &Path) {
    update_state(state_path, request_retest);
}

/// Open the recognizer for the resident process: select from settings and saved state, run the
/// probe child for devices without a result for their current driver, save results, and load the
/// GPU backend only for an approved device. A later backend error is saved to the state file.
pub fn open_for_host(
    backend: Box<dyn GpuBackend>,
    setting: &Gpu,
    state_path: &Path,
    adapters: &[GpuDevice],
    runner: &dyn ChildRunner,
    probe_timeout: Duration,
) -> GpuFallback {
    let mut state = ConfigFile::<State>::open(state_path).value().clone();
    let mut selection = select(setting, &state, adapters);
    if let Selection::NeedsProbe(devices) = &selection {
        let devices = devices.clone();
        update_state(state_path, |st| {
            for d in &devices {
                let v = probe_in_child(runner, d, probe_timeout);
                record_verdict(st, d, &v);
            }
        });
        state = ConfigFile::<State>::open(state_path).value().clone();
        selection = select(setting, &state, adapters);
    }
    let path = state_path.to_path_buf();
    open_recognizer(backend, &selection, Box::new(move |_| update_state(&path, record_gpu_error)))
}
