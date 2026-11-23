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
        Verdict {
            approved: false,
            speedup,
            reason: reason.into(),
        }
    }
}

fn words_of(segs: &[Segment]) -> Vec<String> {
    segs.iter()
        .flat_map(|s| s.words.iter())
        .map(|w| {
            w.text
                .chars()
                .filter(|c| c.is_alphanumeric())
                .collect::<String>()
                .to_lowercase()
        })
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
    let speedup = if gpu_t.is_zero() {
        f64::INFINITY
    } else {
        cpu_t.as_secs_f64() / gpu_t.as_secs_f64()
    };
    if words_of(&cpu_out) != words_of(&gpu_out) {
        return Verdict::rejected(speedup, "GPU output differs from CPU output");
    }
    if speedup < MIN_SPEEDUP {
        return Verdict::rejected(
            speedup,
            format!("speedup {speedup:.2}x is below {MIN_SPEEDUP}x"),
        );
    }
    Verdict {
        approved: true,
        speedup,
        reason: format!("speedup {speedup:.2}x"),
    }
}

// ---- selection and per-device cache ----

use aloud_type_config::{Gpu, GpuProbe, State};

/// Save a probe verdict for its device and driver (the cache entry later reused by [`select`]).
pub fn record_verdict(state: &mut State, device: &GpuDevice, v: &Verdict) {
    state.gpu_probes.insert(
        device.key(),
        GpuProbe {
            approved: v.approved,
            speedup: v.speedup,
        },
    );
}

/// A backend error happened during a Dictation: disable the GPU (recorded in state).
pub fn record_gpu_error(state: &mut State) {
    state.gpu_disabled_after_error = true;
    state.gpu_in_use = None;
}

/// Settings "Test": forget the error flag and all saved results before probing again.
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
    /// Every device was tested and none was faster than the CPU.
    NotApproved,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selection {
    Cpu(CpuReason),
    /// Run on this device.
    Gpu(GpuDevice),
}

/// Auto preference among untested GPUs: NVIDIA, then AMD, then any other.
pub fn vendor_rank(name: &str) -> u8 {
    let l = name.to_lowercase();
    if ["nvidia", "geforce", "quadro"]
        .iter()
        .any(|v| l.contains(v))
    {
        0
    } else if ["amd", "radeon"].iter().any(|v| l.contains(v)) {
        1
    } else {
        2
    }
}

/// Decide what the resident process loads the model on, from settings, state and the
/// installed devices. Pure: it never touches the GPU backend and never runs a test.
///
/// - A device chosen in Settings is used when installed, whatever a test said.
/// - Auto uses the first device by [`vendor_rank`] (NVIDIA, AMD, others), skipping devices a
///   test rejected on their current driver.
pub fn select(setting: &Gpu, state: &State, installed: &[GpuDevice]) -> Selection {
    if *setting == Gpu::CpuOnly {
        return Selection::Cpu(CpuReason::Setting);
    }
    if state.gpu_disabled_after_error {
        return Selection::Cpu(CpuReason::DisabledAfterError);
    }
    if let Gpu::Device(name) = setting {
        return installed
            .iter()
            .find(|d| &d.name == name)
            .map(|d| Selection::Gpu(d.clone()))
            .unwrap_or(Selection::Cpu(CpuReason::NoDevice));
    }
    if installed.is_empty() {
        return Selection::Cpu(CpuReason::NoDevice);
    }
    // A test only rules devices out: one short measurement is too noisy to rank an iGPU
    // against a discrete GPU (runs on one laptop gave 4x-13x for the same GPU).
    let usable = |d: &&GpuDevice| state.gpu_probes.get(&d.key()).is_none_or(|p| p.approved);
    installed
        .iter()
        .filter(usable)
        .min_by_key(|d| vendor_rank(&d.name))
        .map(|d| Selection::Gpu(d.clone()))
        .unwrap_or(Selection::Cpu(CpuReason::NotApproved))
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
    let line = stdout
        .lines()
        .rev()
        .find(|l| l.starts_with(RESULT_PREFIX))?;
    let mut f = line.splitn(4, '\t').skip(1);
    let approved = f.next()?.parse().ok()?;
    let speedup = f.next()?.parse().ok()?;
    Some(Verdict {
        approved,
        speedup,
        reason: f.next().unwrap_or("").to_string(),
    })
}

/// Run the probe for `device` in a child process. A hung, crashed or garbled child is a rejection
/// of that device; it can never affect the calling (resident) process.
pub fn probe_in_child(runner: &dyn ChildRunner, device: &GpuDevice, timeout: Duration) -> Verdict {
    // The name lets the child find the device in the backend's own order, which differs from
    // the registry order the index comes from.
    let args = vec![
        PROBE_FLAG.to_string(),
        "--device".into(),
        device.index.to_string(),
        "--device-name".into(),
        device.name.clone(),
    ];
    match runner.run(&args, timeout) {
        ChildOutcome::TimedOut => Verdict::rejected(
            0.0,
            format!("GPU probe timed out after {}s", timeout.as_secs()),
        ),
        ChildOutcome::SpawnFailed(e) => {
            Verdict::rejected(0.0, format!("GPU probe could not start: {e}"))
        }
        ChildOutcome::Exited { success: false, .. } => Verdict::rejected(0.0, "GPU probe crashed"),
        ChildOutcome::Exited {
            success: true,
            stdout,
        } => parse_verdict(&stdout)
            .unwrap_or_else(|| Verdict::rejected(0.0, "GPU probe gave no result")),
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
        Ok(Self {
            program: std::env::current_exe()?,
            prefix: vec![],
        })
    }
    pub fn with_program(program: impl Into<PathBuf>, prefix: Vec<String>) -> Self {
        Self {
            program: program.into(),
            prefix,
        }
    }
}

impl ChildRunner for StdChildRunner {
    fn run(&self, args: &[String], timeout: Duration) -> ChildOutcome {
        let mut cmd = Command::new(&self.program);
        cmd.args(&self.prefix)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
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
                    return ChildOutcome::Exited {
                        success: status.success(),
                        stdout,
                    };
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
    let mut r = GpuFallback {
        backend,
        gpu: None,
        cpu: None,
        on_gpu_error,
    };
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
        self.cpu
            .as_mut()
            .expect("loaded")
            .try_recognize(audio, language, prompt)
    }
}

impl Recognizer for GpuFallback {
    fn recognize(&mut self, audio: &[f32], language: &str, prompt: Option<&str>) -> Vec<Segment> {
        self.try_recognize(audio, language, prompt)
            .unwrap_or_else(|e| {
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
                out.push(GpuDevice {
                    name: d,
                    driver: v,
                    index,
                });
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

/// Index of `device` among the GPU backend's devices (`backend_names`, in backend order).
/// Adapters are discovered from the registry, whose order differs from the backend's (Vulkan
/// lists discrete GPUs first), so devices are matched by name: exact (ignoring case), else one
/// name containing the other. A device without a name (probe started by hand with only
/// `--device N`) keeps its index. `None`: the backend cannot see this adapter.
pub fn backend_index(backend_names: &[String], device: &GpuDevice) -> Option<i32> {
    let want = device.name.trim().to_lowercase();
    if want.is_empty() {
        return Some(device.index);
    }
    let names: Vec<String> = backend_names
        .iter()
        .map(|n| n.trim().to_lowercase())
        .collect();
    names
        .iter()
        .position(|n| *n == want)
        .or_else(|| {
            names
                .iter()
                .position(|n| !n.is_empty() && (n.contains(&want) || want.contains(n.as_str())))
        })
        .map(|i| i as i32)
}

fn value_of(line: &str, name: &str) -> Option<String> {
    let rest = line.strip_prefix(name)?.trim_start();
    let rest = rest.strip_prefix("REG_SZ")?;
    Some(rest.trim().to_string())
}

/// Registry key of the Windows "Display adapters" device class (GUID_DEVCLASS_DISPLAY).
pub const DISPLAY_CLASS_KEY: &str =
    r"HKLM\SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}";

/// Installed display adapters, via `reg.exe` (Windows only; empty elsewhere or on failure).
/// Takes a fraction of a second; it runs at every model load.
pub fn installed_adapters() -> Vec<GpuDevice> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let out = Command::new("reg")
            // Only values whose name contains "Driver": the full subtree dump is megabytes of
            // driver data and took over a minute on an NVIDIA machine.
            .args(["query", DISPLAY_CLASS_KEY, "/s", "/v", "/f", "Driver"])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            // reg.exe is a console program: without this, a console window flashes up at
            // every model load because the app itself has no console.
            .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
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
        Self {
            model: model.to_path_buf(),
            threads: crate::decode_threads(num_cpus::get_physical()),
        }
    }
}

impl GpuBackend for WhisperBackend {
    fn load_library(&self) -> Result<(), RecognizerError> {
        // The backend is linked statically. ggml's backend registry initialises Vulkan and
        // catches its errors (no vulkan-1.dll, no driver, Vulkan below 1.2), leaving the backend
        // unregistered. Enumerating devices directly would throw across the FFI boundary then,
        // so check the registry first.
        #[cfg(feature = "vulkan")]
        {
            let reg = unsafe { whisper_rs_sys::ggml_backend_reg_by_name(c"Vulkan".as_ptr()) };
            if reg.is_null() {
                return Err(RecognizerError(
                    "Vulkan is unavailable (no vulkan-1.dll or no Vulkan 1.2 driver)".into(),
                ));
            }
            if whisper_rs::vulkan::list_devices().is_empty() {
                return Err(RecognizerError("no Vulkan device".into()));
            }
            Ok(())
        }
        #[cfg(not(feature = "vulkan"))]
        Err(RecognizerError("this build has no GPU backend".into()))
    }
    fn load_cpu(&self) -> Result<Box<dyn Engine>, RecognizerError> {
        Ok(Box::new(WhisperRecognizer::load_on(
            &self.model,
            self.threads,
            None,
        )?))
    }
    fn load_gpu(&self, device: &GpuDevice) -> Result<Box<dyn Engine>, RecognizerError> {
        #[cfg(feature = "vulkan")]
        let index = {
            let names: Vec<String> = whisper_rs::vulkan::list_devices()
                .into_iter()
                .map(|d| d.name)
                .collect();
            backend_index(&names, device).ok_or_else(|| {
                RecognizerError(format!("Vulkan does not list the GPU \"{}\"", device.name))
            })?
        };
        #[cfg(not(feature = "vulkan"))]
        let index = device.index;
        Ok(Box::new(WhisperRecognizer::load_on(
            &self.model,
            self.threads,
            Some(index),
        )?))
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
                b[i + 8..end]
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0)
                    .collect(),
            );
        }
        i += 8 + len + (len & 1);
    }
    None
}

/// Entry point of `--probe-gpu --device N [--device-name NAME] --model PATH`: benchmark, print
/// the verdict line, and return the process exit code. Runs inside the child process only.
/// With a name, the device is looked up by name in the backend's list (see [`backend_index`]).
pub fn probe_child_main(args: &[String]) -> i32 {
    let arg = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
    };
    let (Some(model), Some(index)) = (
        arg("--model"),
        arg("--device").and_then(|d| d.parse::<i32>().ok()),
    ) else {
        eprintln!("usage: --probe-gpu --device N --model PATH");
        return 2;
    };
    let Some(audio) = decode_wav(PROBE_WAV) else {
        return 2;
    };
    let backend = WhisperBackend::new(Path::new(model));
    let device = GpuDevice {
        name: arg("--device-name").cloned().unwrap_or_default(),
        driver: String::new(),
        index,
    };
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

/// Settings changed the GPU choice: the user asked for the GPU again, so forget an earlier error.
pub fn clear_error_on_disk(state_path: &Path) {
    update_state(state_path, |st| st.gpu_disabled_after_error = false);
}

/// Settings "Test": clear the error flag and saved results, then probe every adapter in a
/// child process and save each verdict. Slow (seconds per device): run it off the
/// Dictation thread, then reload the model so Auto can use the results.
pub fn test_all_on_disk(
    runner: &dyn ChildRunner,
    adapters: &[GpuDevice],
    state_path: &Path,
    timeout: Duration,
) {
    update_state(state_path, request_retest);
    for d in adapters {
        let v = probe_in_child(runner, d, timeout);
        update_state(state_path, |st| record_verdict(st, d, &v));
    }
}

fn adapter_names(adapters: &[GpuDevice]) -> Vec<String> {
    let mut names: Vec<String> = adapters.iter().map(|d| d.name.clone()).collect();
    names.dedup();
    names
}

/// Save the installed adapters for Settings without loading a model (first launch: there is
/// no Speech model yet, so [`open_for_host`] does not run until one is downloaded).
pub fn save_adapters(state_path: &Path, adapters: &[GpuDevice]) {
    let names = adapter_names(adapters);
    update_state(state_path, |st| st.gpu_adapters = names);
}

/// Open the recognizer for the resident process: no test runs here. The adapter list is saved
/// for Settings, the device comes from [`select`], and only a selected device loads the GPU
/// backend. A GPU load that crashed the process (marker still set at the next start) disables
/// the GPU; a later backend error is saved the same way.
pub fn open_for_host(
    backend: Box<dyn GpuBackend>,
    setting: &Gpu,
    state_path: &Path,
    adapters: &[GpuDevice],
) -> GpuFallback {
    let names = adapter_names(adapters);
    update_state(state_path, |st| {
        if st.gpu_load_pending {
            st.gpu_load_pending = false;
            record_gpu_error(st);
        }
        st.gpu_adapters = names;
    });
    let state = ConfigFile::<State>::open(state_path).value().clone();
    let selection = select(setting, &state, adapters);
    if matches!(selection, Selection::Gpu(_)) {
        update_state(state_path, |st| st.gpu_load_pending = true);
    }
    let path = state_path.to_path_buf();
    let r = open_recognizer(
        backend,
        &selection,
        Box::new(move |_| update_state(&path, record_gpu_error)),
    );
    let in_use = match (&selection, r.using_gpu()) {
        (Selection::Gpu(d), true) => Some(d.name.clone()),
        _ => None,
    };
    update_state(state_path, |st| {
        st.gpu_load_pending = false;
        st.gpu_in_use = in_use;
    });
    r
}
