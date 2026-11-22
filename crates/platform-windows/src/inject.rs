//! Text injection logic (OS-free, unit-tested): batching per word, UTF-16 /
//! surrogate encoding, and the Typing fault rules. The Win32 backend lives in
//! `inject_win`.
use aloud_type_core::streaming::TextSink;
use std::sync::{Arc, Mutex};

pub const MSG_ELEVATED: &str = "Can't type into an administrator window";
pub const MSG_REJECTED: &str = "Couldn't type into this window";
/// Max UTF-16 code units per `SendInput` call (pacing cap for long words).
pub const MAX_UNITS_PER_CALL: usize = 48;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypingFault {
    /// Foreground window is elevated at key press: no Dictation starts.
    Elevated,
    /// Typing was rejected or only partly delivered: the Dictation ends.
    Rejected,
}

impl TypingFault {
    pub fn message(self) -> &'static str {
        match self {
            TypingFault::Elevated => MSG_ELEVATED,
            TypingFault::Rejected => MSG_REJECTED,
        }
    }
}

/// One keyboard event: a UTF-16 code unit sent as Unicode input, or Enter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Unit(u16),
    Return,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyEv {
    pub key: Key,
    pub up: bool,
}

/// Platform seam: deliver events in one atomic call; returns how many were accepted.
pub trait InputBackend {
    fn send(&mut self, events: &[KeyEv]) -> usize;
}

/// Platform seam: is the foreground window running at a higher integrity than us?
pub trait ElevationProbe {
    fn foreground_elevated(&mut self) -> bool;
}

/// Split into per-word batches (a word keeps its leading whitespace), each at
/// most `MAX_UNITS_PER_CALL` UTF-16 units, never splitting a surrogate pair.
pub fn batches(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut units = 0usize;
    let mut prev_ws = false;
    let mut has_word = false;
    for ch in text.chars() {
        let ws = ch.is_whitespace();
        let n = ch.len_utf16();
        // a new word starts when non-space follows whitespace, or the cap is hit
        if !cur.is_empty() && ((!ws && prev_ws && has_word) || units + n > MAX_UNITS_PER_CALL) {
            out.push(std::mem::take(&mut cur));
            units = 0;
            has_word = false;
        }
        cur.push(ch);
        units += n;
        has_word |= !ws;
        prev_ws = ws;
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Down+up pair per UTF-16 code unit (surrogates stay adjacent in one batch).
pub fn encode(batch: &str) -> Vec<KeyEv> {
    let mut ev = Vec::new();
    for ch in batch.chars() {
        if ch == '\n' {
            ev.push(KeyEv { key: Key::Return, up: false });
            ev.push(KeyEv { key: Key::Return, up: true });
            continue;
        }
        if ch == '\r' {
            continue;
        }
        let mut buf = [0u16; 2];
        for &u in ch.encode_utf16(&mut buf).iter() {
            ev.push(KeyEv { key: Key::Unit(u), up: false });
            ev.push(KeyEv { key: Key::Unit(u), up: true });
        }
    }
    ev
}

pub struct TextInjector<B: InputBackend> {
    backend: B,
}

impl<B: InputBackend> TextInjector<B> {
    pub fn new(backend: B) -> Self {
        Self { backend }
    }

    /// Type `text` batch by batch; a short send is a rejection.
    pub fn type_text(&mut self, text: &str) -> Result<(), TypingFault> {
        for b in batches(text) {
            let ev = encode(&b);
            if ev.is_empty() {
                continue;
            }
            if self.backend.send(&ev) < ev.len() {
                return Err(TypingFault::Rejected);
            }
        }
        Ok(())
    }
}

/// Gate run when the Dictation key goes down: refuse to start on an elevated window.
pub fn check_can_start(probe: &mut impl ElevationProbe) -> Result<(), TypingFault> {
    if probe.foreground_elevated() {
        Err(TypingFault::Elevated)
    } else {
        Ok(())
    }
}

/// Shared slot where a sink records the fault that ended typing.
pub type FaultSlot = Arc<Mutex<Option<TypingFault>>>;

/// `TextSink` over a `TextInjector`. After the first failure it types nothing
/// more and records the fault for the controller to report to the engine.
pub struct InjectingSink<B: InputBackend> {
    injector: TextInjector<B>,
    fault: FaultSlot,
}

impl<B: InputBackend> InjectingSink<B> {
    pub fn new(backend: B) -> (Self, FaultSlot) {
        let fault: FaultSlot = Arc::default();
        (Self { injector: TextInjector::new(backend), fault: fault.clone() }, fault)
    }
}

impl<B: InputBackend> TextSink for InjectingSink<B> {
    fn type_text(&mut self, text: &str) {
        if self.fault.lock().unwrap().is_some() {
            return;
        }
        if let Err(f) = self.injector.type_text(text) {
            *self.fault.lock().unwrap() = Some(f);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Fake {
        calls: Vec<Vec<KeyEv>>,
        accept: Option<usize>, // max events accepted per call
    }
    impl InputBackend for Fake {
        fn send(&mut self, e: &[KeyEv]) -> usize {
            self.calls.push(e.to_vec());
            self.accept.map_or(e.len(), |m| m.min(e.len()))
        }
    }
    fn units(calls: &[Vec<KeyEv>]) -> Vec<u16> {
        calls
            .iter()
            .flatten()
            .filter(|e| !e.up)
            .filter_map(|e| if let Key::Unit(u) = e.key { Some(u) } else { None })
            .collect()
    }

    #[test]
    fn text_is_batched_per_word_keeping_adjacent_spaces() {
        assert_eq!(batches("hello big world"), vec!["hello ", "big ", "world"]);
        assert_eq!(batches(" hi"), vec![" hi"]);
        assert!(batches("").is_empty());
    }

    #[test]
    fn long_word_is_capped_without_splitting_a_surrogate_pair() {
        let s = "\u{1F600}".repeat(40); // 80 units
        let b = batches(&s);
        assert!(b.len() >= 2);
        for x in &b {
            assert!(x.encode_utf16().count() <= MAX_UNITS_PER_CALL);
        }
        assert_eq!(b.concat(), s);
    }

    #[test]
    fn each_unit_is_a_down_up_pair_and_astral_chars_send_both_surrogates() {
        let ev = encode("a\u{1F600}");
        let want = [0x61u16, 0xD83D, 0xDE00];
        assert_eq!(ev.len(), 6);
        for (i, u) in want.iter().enumerate() {
            assert_eq!(ev[2 * i], KeyEv { key: Key::Unit(*u), up: false });
            assert_eq!(ev[2 * i + 1], KeyEv { key: Key::Unit(*u), up: true });
        }
    }

    #[test]
    fn special_characters_round_trip_through_utf16() {
        let s = "na\u{ef}ve caf\u{e9} \u{2014} \u{201c}quoted\u{201d} \u{65e5}\u{672c}\u{8a9e} \u{f1} \t";
        let mut inj = TextInjector::new(Fake::default());
        inj.type_text(s).unwrap();
        let back = String::from_utf16(&units(&inj.backend.calls)).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn a_surrogate_pair_is_never_split_across_calls() {
        let mut inj = TextInjector::new(Fake::default());
        inj.type_text(&format!("x{}y {}", "\u{1F600}".repeat(30), "\u{1F680}")).unwrap();
        for c in &inj.backend.calls {
            let u = units(std::slice::from_ref(c));
            assert!(String::from_utf16(&u).is_ok(), "call split a pair");
        }
    }

    #[test]
    fn newline_becomes_enter_and_cr_is_dropped() {
        let ev = encode("a\r\nb");
        assert_eq!(ev[2], KeyEv { key: Key::Return, up: false });
        assert_eq!(ev.len(), 6);
    }

    #[test]
    fn short_send_is_a_rejection_and_stops_typing() {
        let mut inj = TextInjector::new(Fake { accept: Some(3), ..Default::default() });
        assert_eq!(inj.type_text("hello world"), Err(TypingFault::Rejected));
        assert_eq!(inj.backend.calls.len(), 1);
    }

    #[test]
    fn zero_accepted_is_a_rejection() {
        let mut inj = TextInjector::new(Fake { accept: Some(0), ..Default::default() });
        assert_eq!(inj.type_text("a"), Err(TypingFault::Rejected));
    }

    struct Probe(bool);
    impl ElevationProbe for Probe {
        fn foreground_elevated(&mut self) -> bool {
            self.0
        }
    }

    #[test]
    fn elevated_foreground_refuses_to_start_with_the_amber_message() {
        assert_eq!(check_can_start(&mut Probe(true)), Err(TypingFault::Elevated));
        assert_eq!(TypingFault::Elevated.message(), "Can't type into an administrator window");
        assert_eq!(check_can_start(&mut Probe(false)), Ok(()));
        assert_eq!(TypingFault::Rejected.message(), "Couldn't type into this window");
    }

    #[test]
    fn sink_records_the_fault_and_types_nothing_afterwards() {
        let (mut sink, slot) = InjectingSink::new(Fake { accept: Some(1), ..Default::default() });
        sink.type_text("one");
        assert_eq!(*slot.lock().unwrap(), Some(TypingFault::Rejected));
        sink.type_text("two");
        assert_eq!(sink.injector.backend.calls.len(), 1);
    }

    #[test]
    fn sink_types_scripted_phrase_when_accepted() {
        let (mut sink, slot) = InjectingSink::new(Fake::default());
        sink.type_text("hello");
        sink.type_text(" world");
        assert!(slot.lock().unwrap().is_none());
        assert_eq!(String::from_utf16(&units(&sink.injector.backend.calls)).unwrap(), "hello world");
    }
}
