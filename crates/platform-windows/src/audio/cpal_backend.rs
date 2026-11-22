//! `Backend` over cpal (WASAPI, shared mode, event-driven). Compile-checked
//! only: needs a real microphone to exercise (see ticket 09 gaps).
use super::device::DeviceInfo;
use super::fault::{CaptureError, CaptureErrorKind, E_ACCESSDENIED};
use super::session::{Backend, Feed};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{DeviceId, ErrorKind, SampleFormat};

pub struct CpalBackend;

/// Map a cpal error. cpal drops the HRESULT for anything it cannot classify,
/// so E_ACCESSDENIED is recovered from the message text.
pub fn map_error(e: &cpal::Error) -> CaptureError {
    let kind = match e.kind() {
        ErrorKind::DeviceNotAvailable => CaptureErrorKind::DeviceNotAvailable,
        ErrorKind::DeviceBusy => CaptureErrorKind::DeviceBusy,
        ErrorKind::DeviceChanged => CaptureErrorKind::DeviceChanged,
        ErrorKind::StreamInvalidated => CaptureErrorKind::StreamInvalidated,
        ErrorKind::HostUnavailable => CaptureErrorKind::HostUnavailable,
        ErrorKind::UnsupportedConfig => CaptureErrorKind::UnsupportedConfig,
        ErrorKind::PermissionDenied => CaptureErrorKind::PermissionDenied,
        _ => CaptureErrorKind::Other,
    };
    let detail = e.to_string();
    let lower = detail.to_ascii_lowercase();
    let hresult = (lower.contains("access is denied") || lower.contains("80070005"))
        .then_some(E_ACCESSDENIED);
    CaptureError {
        kind,
        hresult,
        detail,
    }
}

fn convert_i16(data: &[i16]) -> Vec<f32> {
    data.iter().map(|s| *s as f32 / 32768.0).collect()
}

impl Backend for CpalBackend {
    type Stream = cpal::Stream;

    fn list_inputs(&self) -> Vec<DeviceInfo> {
        let host = cpal::default_host();
        let Ok(devices) = host.input_devices() else {
            return vec![];
        };
        devices
            .filter_map(|d| {
                let id = d.id().ok()?.to_string();
                let name = d
                    .description()
                    .map(|x| x.name().to_string())
                    .unwrap_or_else(|_| id.clone());
                Some(DeviceInfo { id, name })
            })
            .collect()
    }

    fn open(&self, device: Option<&str>, feed: Feed) -> Result<cpal::Stream, CaptureError> {
        let host = cpal::default_host();
        let dev = match device {
            None => host.default_input_device(),
            Some(id) => id
                .parse::<DeviceId>()
                .ok()
                .and_then(|id| host.device_by_id(&id)),
        }
        .ok_or_else(|| CaptureError::new(CaptureErrorKind::DeviceNotAvailable))?;

        let supported = dev.default_input_config().map_err(|e| map_error(&e))?;
        let (rate, channels, format) = (
            supported.sample_rate(),
            supported.channels(),
            supported.sample_format(),
        );
        let config = cpal::StreamConfig {
            channels,
            sample_rate: rate,
            buffer_size: cpal::BufferSize::Default,
        };
        let err_feed = feed.clone();
        let on_err = move |e: cpal::Error| err_feed.report_error(&map_error(&e));
        let stream = match format {
            SampleFormat::F32 => dev.build_input_stream(
                config,
                move |data: &[f32], _| feed.push_native(data, rate, channels),
                on_err,
                None,
            ),
            SampleFormat::I16 => dev.build_input_stream(
                config,
                move |data: &[i16], _| feed.push_native(&convert_i16(data), rate, channels),
                on_err,
                None,
            ),
            _ => return Err(CaptureError::new(CaptureErrorKind::UnsupportedConfig)),
        }
        .map_err(|e| map_error(&e))?;
        stream.play().map_err(|e| map_error(&e))?;
        Ok(stream)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn access_denied_text_is_recovered_as_hresult() {
        let e =
            cpal::Error::with_message(ErrorKind::BackendError, "Access is denied. (os error 5)");
        assert_eq!(map_error(&e).hresult, Some(E_ACCESSDENIED));
    }

    #[test]
    fn kinds_map_across() {
        let e = cpal::Error::with_message(ErrorKind::DeviceBusy, "x");
        assert_eq!(map_error(&e).kind, CaptureErrorKind::DeviceBusy);
    }

    #[test]
    fn listing_does_not_panic_without_a_microphone() {
        let _ = CpalBackend.list_inputs();
    }
}
