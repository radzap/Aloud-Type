//! Fakes for the controller's platform seams.
use crate::{Capture, Mic, StartGate, TypingFaults};
use aloud_type_core::listening::FaultReason;
use aloud_type_core::streaming::TextSink;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Default)]
struct CapState {
    open: bool,
    queue: Vec<f32>,
    fail_start: Option<FaultReason>,
    fault: Option<FaultReason>,
}

#[derive(Clone, Default)]
pub struct FakeCapture(Rc<RefCell<CapState>>);

impl FakeCapture {
    pub fn is_open(&self) -> bool {
        self.0.borrow().open
    }
    pub fn push(&self, samples: &[f32]) {
        self.0.borrow_mut().queue.extend_from_slice(samples);
    }
    pub fn fail_start(&self, r: FaultReason) {
        self.0.borrow_mut().fail_start = Some(r);
    }
    pub fn raise_fault(&self, r: FaultReason) {
        self.0.borrow_mut().fault = Some(r);
    }
}

impl Capture for FakeCapture {
    fn start(&mut self, _: &Mic) -> Result<bool, FaultReason> {
        let mut s = self.0.borrow_mut();
        if let Some(r) = s.fail_start.take() {
            return Err(r);
        }
        s.open = true;
        s.queue.clear();
        Ok(false)
    }
    fn stop(&mut self) {
        self.0.borrow_mut().open = false;
    }
    fn take_fault(&mut self) -> Option<FaultReason> {
        self.0.borrow_mut().fault.take()
    }
    fn read(&mut self) -> Vec<f32> {
        std::mem::take(&mut self.0.borrow_mut().queue)
    }
}

#[derive(Default)]
struct SinkState {
    text: String,
    reject_next: Option<String>,
    fault: Option<String>,
}

/// Records typed text; `reject_next` makes the next typing fail like a blocked window.
#[derive(Clone, Default)]
pub struct FakeSink(Rc<RefCell<SinkState>>);

impl FakeSink {
    pub fn text(&self) -> String {
        self.0.borrow().text.clone()
    }
    pub fn reject_next(&self, message: &str) {
        self.0.borrow_mut().reject_next = Some(message.into());
    }
}

impl TextSink for FakeSink {
    fn type_text(&mut self, t: &str) {
        let mut s = self.0.borrow_mut();
        if s.fault.is_some() {
            return;
        }
        if let Some(m) = s.reject_next.take() {
            s.fault = Some(m);
            return;
        }
        s.text.push_str(t);
    }
}

impl TypingFaults for FakeSink {
    fn take(&mut self) -> Option<String> {
        self.0.borrow_mut().fault.take()
    }
}

#[derive(Clone, Default)]
pub struct FakeStartGate(Rc<RefCell<Option<String>>>);

impl FakeStartGate {
    pub fn set(&self, message: Option<&str>) {
        *self.0.borrow_mut() = message.map(Into::into);
    }
}

impl StartGate for FakeStartGate {
    fn check(&mut self) -> Option<String> {
        self.0.borrow().clone()
    }
}
