//! The resident host: wires the keyboard hook, audio capture, recognizer,
//! Streaming insertion and text injection into one controller. OS-free; the
//! Windows adapters are plugged in by `main.rs`, fakes by the tests.
mod controller;
pub mod testing;

pub use controller::*;
