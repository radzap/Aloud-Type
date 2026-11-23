//! Classify capture failures into Listening fault reasons.
use aloud_type_core::listening::FaultReason;

/// Backend-neutral failure category (mapped from the capture library).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureErrorKind {
    DeviceNotAvailable,
    DeviceBusy,
    /// Default endpoint rerouted; the stream keeps running.
    DeviceChanged,
    StreamInvalidated,
    /// A buffer underrun/overrun or gap in the audio: a glitch, the stream keeps running.
    /// Some drivers (Intel Smart Sound microphone arrays) report one right after the stream opens.
    Glitch,
    HostUnavailable,
    UnsupportedConfig,
    PermissionDenied,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureError {
    pub kind: CaptureErrorKind,
    pub hresult: Option<u32>,
    pub detail: String,
}

impl CaptureError {
    pub fn new(kind: CaptureErrorKind) -> Self {
        Self {
            kind,
            hresult: None,
            detail: String::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// While opening the stream at Dictation start.
    Opening,
    /// While the stream is delivering audio.
    Streaming,
}

pub const E_ACCESSDENIED: u32 = 0x8007_0005;

/// `None` means the error is not a fault (default device rerouted, or a glitch).
pub fn classify(err: &CaptureError, phase: Phase) -> Option<FaultReason> {
    if err.hresult == Some(E_ACCESSDENIED) {
        return Some(FaultReason::Blocked);
    }
    use CaptureErrorKind::*;
    match err.kind {
        DeviceChanged | Glitch => None,
        PermissionDenied => Some(FaultReason::Blocked),
        DeviceBusy => Some(FaultReason::InUse),
        DeviceNotAvailable => Some(match phase {
            Phase::Opening => FaultReason::NoMicrophone,
            Phase::Streaming => FaultReason::Disconnected,
        }),
        StreamInvalidated => Some(FaultReason::Disconnected),
        HostUnavailable | UnsupportedConfig | Other => Some(FaultReason::MicrophoneError),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use CaptureErrorKind::*;

    fn c(kind: CaptureErrorKind, phase: Phase) -> Option<FaultReason> {
        classify(&CaptureError::new(kind), phase)
    }

    #[test]
    fn missing_device_at_open_is_no_microphone() {
        assert_eq!(
            c(DeviceNotAvailable, Phase::Opening),
            Some(FaultReason::NoMicrophone)
        );
    }

    #[test]
    fn unplugging_while_streaming_is_disconnected() {
        assert_eq!(
            c(DeviceNotAvailable, Phase::Streaming),
            Some(FaultReason::Disconnected)
        );
        assert_eq!(
            c(StreamInvalidated, Phase::Streaming),
            Some(FaultReason::Disconnected)
        );
    }

    #[test]
    fn exclusive_use_is_in_use() {
        assert_eq!(c(DeviceBusy, Phase::Opening), Some(FaultReason::InUse));
    }

    #[test]
    fn denied_is_blocked_including_raw_access_denied_hresult() {
        assert_eq!(
            c(PermissionDenied, Phase::Opening),
            Some(FaultReason::Blocked)
        );
        let e = CaptureError {
            kind: Other,
            hresult: Some(0x8007_0005),
            detail: String::new(),
        };
        assert_eq!(classify(&e, Phase::Opening), Some(FaultReason::Blocked));
    }

    #[test]
    fn default_reroute_is_not_a_fault() {
        assert_eq!(c(DeviceChanged, Phase::Streaming), None);
    }

    #[test]
    fn an_audio_glitch_is_not_a_fault() {
        assert_eq!(c(Glitch, Phase::Streaming), None);
        assert_eq!(c(Glitch, Phase::Opening), None);
    }

    #[test]
    fn everything_else_is_a_generic_microphone_error() {
        for k in [HostUnavailable, UnsupportedConfig, Other] {
            assert_eq!(c(k, Phase::Opening), Some(FaultReason::MicrophoneError));
        }
    }
}
