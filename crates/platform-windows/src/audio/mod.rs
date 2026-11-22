//! Microphone capture: fault classification, privacy pre-check, device choice,
//! resampling to 16 kHz mono, loudness metering and the capture session.
pub mod cpal_backend;
pub mod device;
pub mod fault;
pub mod meter;
pub mod privacy;
pub mod resample;
pub mod session;
