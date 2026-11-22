//! The capture session: opens a stream only for a Dictation, converts to
//! 16 kHz mono, meters loudness, and reports Listening faults.
use super::device::{select, DeviceInfo, MicChoice, Selection};
use super::fault::{classify, CaptureError, Phase};
use super::meter::LevelMeter;
use super::privacy::{precheck, Registry, Verdict};
use super::resample::Resampler;
use aloud_type_core::listening::FaultReason;
use aloud_type_core::seams::AudioSource;
use std::sync::{Arc, Mutex};

struct Shared {
    resampler: Option<Resampler>,
    meter: LevelMeter,
    out: Vec<f32>,
    fault: Option<FaultReason>,
}

/// Handed to the backend's stream callbacks. Cheap to clone; the callbacks
/// only append and never block on anything but a short critical section.
#[derive(Clone)]
pub struct Feed {
    shared: Arc<Mutex<Shared>>,
}

impl Feed {
    /// Interleaved f32 frames in the device's native format.
    pub fn push_native(&self, interleaved: &[f32], sample_rate: u32, channels: u16) {
        let mut s = self.shared.lock().unwrap();
        // A default-device reroute may change the format mid-stream.
        let stale = s
            .resampler
            .as_ref()
            .is_none_or(|r| r.input_rate() != sample_rate || r.channels() != channels);
        if stale {
            s.resampler = Some(Resampler::new(sample_rate, channels));
        }
        let mono = s.resampler.as_mut().unwrap().process(interleaved);
        s.meter.push(&mono);
        s.out.extend_from_slice(&mono);
    }

    /// Stream error callback. Reroutes of the default device are ignored.
    pub fn report_error(&self, err: &CaptureError) {
        if let Some(reason) = classify(err, Phase::Streaming) {
            let mut s = self.shared.lock().unwrap();
            s.fault.get_or_insert(reason);
        }
    }
}

/// Platform seam over the capture library.
pub trait Backend {
    /// Dropping the stream closes it.
    type Stream;
    fn list_inputs(&self) -> Vec<DeviceInfo>;
    /// `device` is `None` for the System default endpoint.
    fn open(&self, device: Option<&str>, feed: Feed) -> Result<Self::Stream, CaptureError>;
}

pub struct AudioCapture<B: Backend> {
    backend: B,
    registry: Box<dyn Registry>,
    shared: Arc<Mutex<Shared>>,
    stream: Option<B::Stream>,
}

impl<B: Backend> AudioCapture<B> {
    pub fn new(backend: B, registry: Box<dyn Registry>) -> Self {
        let shared = Arc::new(Mutex::new(Shared {
            resampler: None,
            meter: LevelMeter::new(),
            out: Vec::new(),
            fault: None,
        }));
        Self { backend, registry, shared, stream: None }
    }

    /// Begin capture for a Dictation. Returns whether the chosen microphone
    /// was missing and the default is used instead (the 2 s note), or the
    /// Listening fault that prevents capture. No stream is opened on a fault.
    pub fn start(&mut self, choice: &MicChoice) -> Result<bool, FaultReason> {
        self.stop();
        if precheck(self.registry.as_ref()) == Verdict::Denied {
            return Err(FaultReason::Blocked);
        }
        let (device, note) = match select(choice, &self.backend.list_inputs()) {
            Selection::NoMicrophone => return Err(FaultReason::NoMicrophone),
            Selection::Default => (None, false),
            Selection::DefaultWithNote => (None, true),
            Selection::Specific(id) => (Some(id), false),
        };
        {
            let mut s = self.shared.lock().unwrap();
            *s = Shared { resampler: None, meter: LevelMeter::new(), out: Vec::new(), fault: None };
        }
        let feed = Feed { shared: self.shared.clone() };
        match self.backend.open(device.as_deref(), feed) {
            Ok(stream) => {
                self.stream = Some(stream);
                Ok(note)
            }
            Err(e) => Err(classify(&e, Phase::Opening).unwrap_or(FaultReason::MicrophoneError)),
        }
    }

    /// Close the stream (idempotent). Samples captured so far stay readable.
    pub fn stop(&mut self) {
        self.stream = None;
    }

    pub fn is_open(&self) -> bool {
        self.stream.is_some()
    }

    /// A fault raised mid-Dictation by the stream. Closes the stream; the
    /// host forwards the reason to the Listening monitor.
    pub fn take_fault(&mut self) -> Option<FaultReason> {
        let fault = self.shared.lock().unwrap().fault.take();
        if fault.is_some() {
            self.stop();
        }
        fault
    }

    /// Loudness 0..1 for the Listening indicator.
    pub fn level(&self) -> f32 {
        self.shared.lock().unwrap().meter.level()
    }
}

impl<B: Backend> AudioSource for AudioCapture<B> {
    fn read(&mut self) -> Vec<f32> {
        std::mem::take(&mut self.shared.lock().unwrap().out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::fault::CaptureErrorKind;
    use crate::audio::privacy::Hive;
    use std::cell::RefCell;
    use std::rc::Rc;

    struct NoReg;
    impl Registry for NoReg {
        fn read_dword(&self, _: Hive, _: &str, _: &str) -> Option<u32> {
            None
        }
        fn read_string(&self, _: Hive, _: &str, _: &str) -> Option<String> {
            None
        }
    }
    struct DenyReg;
    impl Registry for DenyReg {
        fn read_dword(&self, _: Hive, _: &str, _: &str) -> Option<u32> {
            None
        }
        fn read_string(&self, _: Hive, _: &str, _: &str) -> Option<String> {
            Some("Deny".into())
        }
    }

    /// Fake stream counts live instances; keeps the feed so tests can push.
    struct FakeStream {
        live: Rc<RefCell<usize>>,
    }
    impl Drop for FakeStream {
        fn drop(&mut self) {
            *self.live.borrow_mut() -= 1;
        }
    }
    #[derive(Clone)]
    struct FakeBackend {
        devices: Vec<DeviceInfo>,
        open_error: Option<CaptureError>,
        live: Rc<RefCell<usize>>,
        opened_with: Rc<RefCell<Vec<Option<String>>>>,
        feed: Rc<RefCell<Option<Feed>>>,
    }
    impl FakeBackend {
        fn new(ids: &[&str]) -> Self {
            Self {
                devices: ids.iter().map(|i| DeviceInfo { id: i.to_string(), name: i.to_string() }).collect(),
                open_error: None,
                live: Rc::default(),
                opened_with: Rc::default(),
                feed: Rc::default(),
            }
        }
        fn feed(&self) -> Feed {
            self.feed.borrow().clone().unwrap()
        }
    }
    impl Backend for FakeBackend {
        type Stream = FakeStream;
        fn list_inputs(&self) -> Vec<DeviceInfo> {
            self.devices.clone()
        }
        fn open(&self, device: Option<&str>, feed: Feed) -> Result<FakeStream, CaptureError> {
            self.opened_with.borrow_mut().push(device.map(String::from));
            if let Some(e) = &self.open_error {
                return Err(e.clone());
            }
            *self.feed.borrow_mut() = Some(feed);
            *self.live.borrow_mut() += 1;
            Ok(FakeStream { live: self.live.clone() })
        }
    }

    fn capture(b: &FakeBackend) -> AudioCapture<FakeBackend> {
        AudioCapture::new(b.clone(), Box::new(NoReg))
    }

    #[test]
    fn no_stream_is_open_while_idle_and_it_closes_on_stop() {
        let b = FakeBackend::new(&["a"]);
        let mut c = capture(&b);
        assert_eq!(*b.live.borrow(), 0);
        assert_eq!(c.start(&MicChoice::SystemDefault), Ok(false));
        assert_eq!(*b.live.borrow(), 1);
        c.stop();
        assert_eq!(*b.live.borrow(), 0);
        assert!(!c.is_open());
    }

    #[test]
    fn audio_arrives_as_16k_mono_and_level_follows_loudness() {
        let b = FakeBackend::new(&["a"]);
        let mut c = capture(&b);
        c.start(&MicChoice::SystemDefault).unwrap();
        b.feed().push_native(&vec![0.5; 4800 * 2], 48_000, 2); // 100 ms
        let samples = c.read();
        assert_eq!(samples.len(), 1600);
        assert!(c.level() > 0.9);
        assert!(c.read().is_empty());
        b.feed().push_native(&vec![0.0; 4800 * 2], 48_000, 2);
        c.read();
        assert!(c.level() < 0.5);
    }

    #[test]
    fn privacy_deny_faults_blocked_without_opening_a_stream() {
        let b = FakeBackend::new(&["a"]);
        let mut c = AudioCapture::new(b.clone(), Box::new(DenyReg));
        assert_eq!(c.start(&MicChoice::SystemDefault), Err(FaultReason::Blocked));
        assert!(b.opened_with.borrow().is_empty());
    }

    #[test]
    fn no_microphones_faults_no_microphone() {
        let b = FakeBackend::new(&[]);
        assert_eq!(capture(&b).start(&MicChoice::SystemDefault), Err(FaultReason::NoMicrophone));
    }

    #[test]
    fn missing_chosen_microphone_uses_default_and_asks_for_the_note() {
        let b = FakeBackend::new(&["a"]);
        let mut c = capture(&b);
        assert_eq!(c.start(&MicChoice::Specific("gone".into())), Ok(true));
        assert_eq!(b.opened_with.borrow().as_slice(), &[None]);
    }

    #[test]
    fn present_chosen_microphone_is_opened_by_id() {
        let b = FakeBackend::new(&["a", "b"]);
        let mut c = capture(&b);
        assert_eq!(c.start(&MicChoice::Specific("b".into())), Ok(false));
        assert_eq!(b.opened_with.borrow().as_slice(), &[Some("b".to_string())]);
    }

    #[test]
    fn open_errors_are_classified() {
        let mut b = FakeBackend::new(&["a"]);
        b.open_error = Some(CaptureError::new(CaptureErrorKind::DeviceBusy));
        assert_eq!(capture(&b).start(&MicChoice::SystemDefault), Err(FaultReason::InUse));
        b.open_error = Some(CaptureError::new(CaptureErrorKind::PermissionDenied));
        assert_eq!(capture(&b).start(&MicChoice::SystemDefault), Err(FaultReason::Blocked));
    }

    #[test]
    fn unplugging_mid_dictation_reports_disconnected_and_closes_the_stream() {
        let b = FakeBackend::new(&["a"]);
        let mut c = capture(&b);
        c.start(&MicChoice::SystemDefault).unwrap();
        b.feed().report_error(&CaptureError::new(CaptureErrorKind::DeviceNotAvailable));
        assert_eq!(c.take_fault(), Some(FaultReason::Disconnected));
        assert!(!c.is_open());
        assert_eq!(c.take_fault(), None);
    }

    #[test]
    fn default_device_change_keeps_the_stream_and_the_audio_flowing() {
        let b = FakeBackend::new(&["a"]);
        let mut c = capture(&b);
        c.start(&MicChoice::SystemDefault).unwrap();
        b.feed().push_native(&vec![0.1; 480], 48_000, 1);
        b.feed().report_error(&CaptureError::new(CaptureErrorKind::DeviceChanged));
        assert_eq!(c.take_fault(), None);
        assert!(c.is_open());
        // New default runs at 44.1 kHz stereo.
        b.feed().push_native(&vec![0.1; 441 * 2], 44_100, 2);
        assert_eq!(c.read().len(), 160 + 160);
    }
}
