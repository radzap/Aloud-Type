//! Pure model of the Listening indicator overlay (Trace strip style): the
//! loudness smoothing, fade and grow animation, layout, placement and CPU
//! rendering into a premultiplied BGRA pixel buffer. No OS calls: the Win32
//! window in `overlay` only feeds it state and time and presents pixels.
use aloud_type_core::listening::{FaultReason, Indicator};
use aloud_type_core::Millis;

/// Frame period of the animation timer (about 30 fps).
pub const FRAME_MS: u32 = 33;
pub const FADE_IN_MS: f32 = 120.0;
pub const FADE_OUT_MS: f32 = 180.0;
pub const GROW_MS: f32 = 220.0;
/// Number of loudness samples in the trace (about 2 seconds at 30 fps).
pub const TRACE_LEN: usize = 64;
const ATTACK_TAU_MS: f32 = 36.0;
const RELEASE_TAU_MS: f32 = 205.0;
const MARGIN: f32 = 16.0;
const MIN_BAR: f32 = 1.5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Vertical {
    Top,
    Bottom,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Horizontal {
    Left,
    Center,
    Right,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placement {
    pub vertical: Vertical,
    pub horizontal: Horizontal,
}
impl Default for Placement {
    fn default() -> Self {
        Self { vertical: Vertical::Bottom, horizontal: Horizontal::Center }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

/// Top-left pixel position of an overlay of `w` x `h` pixels inside the
/// monitor work area (which excludes the taskbar), `scale` being DPI / 96.
pub fn place(work: Rect, w: i32, h: i32, p: Placement, scale: f32) -> (i32, i32) {
    let m = (MARGIN * scale).round() as i32;
    let x = match p.horizontal {
        Horizontal::Left => work.left + m,
        Horizontal::Center => work.left + (work.right - work.left - w) / 2,
        Horizontal::Right => work.right - m - w,
    };
    let y = match p.vertical {
        Vertical::Top => work.top + m,
        Vertical::Bottom => work.bottom - m - h,
    };
    (x, y)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    /// Normal listening: dark translucent.
    Neutral,
    /// Listening fault.
    Red,
    /// Typing fault.
    Amber,
}
impl Tone {
    /// Background (r, g, b, a) and text colour (r, g, b).
    fn colors(self) -> ([u8; 4], [u8; 3]) {
        match self {
            Tone::Neutral => ([20, 20, 24, 191], [255, 255, 255]),
            Tone::Red => ([220, 38, 38, 255], [255, 255, 255]),
            Tone::Amber => ([245, 166, 35, 255], [32, 22, 0]),
        }
    }
}

/// What the overlay should look like for one indicator state.
#[derive(Debug, Clone, PartialEq)]
pub struct Look {
    pub tone: Tone,
    pub title: String,
    pub hint: String,
    /// Draws the loudness trace (normal listening only).
    pub trace: bool,
    /// Logical size in pixels at 96 dpi.
    pub w: f32,
    pub h: f32,
}

pub const STRIP_W: f32 = 150.0;
pub const STRIP_H: f32 = 22.0;
pub const RED_W: f32 = 280.0;
pub const RED_H: f32 = 66.0;
pub const RED_H_NO_HINT: f32 = 44.0;
pub const AMBER_W: f32 = 300.0;
pub const AMBER_H_ONE_LINE: f32 = 44.0;
pub const AMBER_H_TWO_LINES: f32 = 62.0;
pub const NOTE_W: f32 = 300.0;
pub const NOTE_H: f32 = 36.0;
const ONE_LINE_CHARS: usize = 38;

/// Second line shown under a Listening fault's reason.
pub fn hint(reason: FaultReason) -> &'static str {
    match reason {
        FaultReason::NoMicrophone => "Plug one in or pick one in Settings",
        FaultReason::Disconnected => "Dictation stopped",
        FaultReason::InUse => "Close it and try again",
        FaultReason::Blocked => "Settings > Privacy > Microphone",
        FaultReason::MutedOrSilent => "Check the mute switch",
        FaultReason::NoSound => "Is the right microphone selected?",
        FaultReason::MicrophoneError => "",
    }
}

fn strip() -> Look {
    Look { tone: Tone::Neutral, title: String::new(), hint: String::new(), trace: true, w: STRIP_W, h: STRIP_H }
}

/// The look for an indicator state; `None` when the overlay must be hidden.
pub fn look(ind: &Indicator) -> Option<Look> {
    match ind {
        Indicator::Hidden => None,
        Indicator::Listening { note: None, .. } => Some(strip()),
        Indicator::Listening { note: Some(n), .. } => Some(Look {
            tone: Tone::Neutral,
            title: (*n).to_string(),
            hint: String::new(),
            trace: false,
            w: NOTE_W,
            h: NOTE_H,
        }),
        Indicator::ListeningFault { reason, message, .. } => {
            let hint = hint(*reason);
            Some(Look {
                tone: Tone::Red,
                title: (*message).to_string(),
                hint: hint.to_string(),
                trace: false,
                w: RED_W,
                h: if hint.is_empty() { RED_H_NO_HINT } else { RED_H },
            })
        }
        Indicator::TypingFault { message } => Some(Look {
            tone: Tone::Amber,
            title: message.clone(),
            hint: String::new(),
            trace: false,
            w: AMBER_W,
            h: if message.chars().count() <= ONE_LINE_CHARS { AMBER_H_ONE_LINE } else { AMBER_H_TWO_LINES },
        }),
    }
}

/// One animation frame to present.
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    /// 0..=1, applied to the whole window.
    pub opacity: f32,
    /// Current logical size (animates between looks).
    pub w: f32,
    pub h: f32,
    pub look: Look,
    /// Smoothed loudness history, oldest first, each 0..=1.
    pub trace: [f32; TRACE_LEN],
    /// The text is shown once the overlay has mostly grown.
    pub show_text: bool,
}

fn ease_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

/// Time-driven animation state. Feed it the indicator state with `set` and
/// call `tick` once per frame; `tick` returns `None` when nothing is on
/// screen, which is the signal to stop the timer.
pub struct Animator {
    target: Option<Look>,
    /// Look kept while fading out.
    shown: Option<Look>,
    opacity: f32,
    level_target: f32,
    level: f32,
    trace: [f32; TRACE_LEN],
    size: (f32, f32),
    grow_from: (f32, f32),
    grow_start: Millis,
    grow_to: (f32, f32),
    last_tick: Option<Millis>,
}

impl Default for Animator {
    fn default() -> Self {
        Self::new()
    }
}

impl Animator {
    pub fn new() -> Self {
        Self {
            target: None,
            shown: None,
            opacity: 0.0,
            level_target: 0.0,
            level: 0.0,
            trace: [0.0; TRACE_LEN],
            size: (STRIP_W, STRIP_H),
            grow_from: (STRIP_W, STRIP_H),
            grow_start: 0,
            grow_to: (STRIP_W, STRIP_H),
            last_tick: None,
        }
    }

    /// True while something is visible or fading (the timer must run).
    pub fn active(&self) -> bool {
        self.target.is_some() || self.opacity > 0.0
    }

    pub fn set(&mut self, ind: &Indicator, now: Millis) {
        let new = look(ind);
        self.level_target = match ind {
            Indicator::Listening { level, .. } => level.clamp(0.0, 1.0),
            _ => 0.0,
        };
        match new {
            None => {
                self.target = None;
            }
            Some(l) => {
                if !self.active() {
                    // Fresh appearance: start as the small strip with no history.
                    self.opacity = 0.0;
                    self.level = 0.0;
                    self.trace = [0.0; TRACE_LEN];
                    self.size = (STRIP_W, STRIP_H);
                    self.grow_to = self.size;
                    self.last_tick = None;
                }
                let dims = (l.w, l.h);
                if dims != self.grow_to {
                    self.grow_from = self.size;
                    self.grow_to = dims;
                    self.grow_start = now;
                }
                self.shown = Some(l.clone());
                self.target = Some(l);
            }
        }
    }

    pub fn tick(&mut self, now: Millis) -> Option<Frame> {
        // A stalled timer must not skip the animation.
        let dt = self.last_tick.map(|t| now.saturating_sub(t).min(100) as f32).unwrap_or(0.0);
        self.last_tick = Some(now);
        if self.target.is_some() {
            self.opacity = (self.opacity + dt / FADE_IN_MS).min(1.0);
        } else {
            self.opacity = (self.opacity - dt / FADE_OUT_MS).max(0.0);
            if self.opacity <= 1e-4 {
                self.opacity = 0.0;
                self.shown = None;
                return None;
            }
        }
        // Fast attack, slow release.
        let tau = if self.level_target > self.level { ATTACK_TAU_MS } else { RELEASE_TAU_MS };
        self.level += (self.level_target - self.level) * (1.0 - (-dt / tau).exp());
        self.trace.copy_within(1.., 0);
        self.trace[TRACE_LEN - 1] = self.level;
        // Grow.
        let progress = if self.size == self.grow_to {
            1.0
        } else {
            (now.saturating_sub(self.grow_start) as f32 / GROW_MS).clamp(0.0, 1.0)
        };
        let e = ease_out(progress);
        self.size = if progress >= 1.0 {
            self.grow_to
        } else {
            (
                self.grow_from.0 + (self.grow_to.0 - self.grow_from.0) * e,
                self.grow_from.1 + (self.grow_to.1 - self.grow_from.1) * e,
            )
        };
        let look = self.shown.clone()?;
        Some(Frame {
            opacity: self.opacity,
            w: self.size.0,
            h: self.size.1,
            show_text: !look.title.is_empty() && progress >= 0.5,
            look,
            trace: self.trace,
        })
    }
}

/// Pixel size of a frame at the given scale (DPI / 96).
pub fn pixel_size(f: &Frame, scale: f32) -> (usize, usize) {
    ((f.w * scale).round().max(1.0) as usize, (f.h * scale).round().max(1.0) as usize)
}

fn coverage(x: usize, y: usize, w: usize, h: usize, r: f32) -> f32 {
    let (px, py) = (x as f32 + 0.5 - w as f32 / 2.0, y as f32 + 0.5 - h as f32 / 2.0);
    let (hx, hy) = (w as f32 / 2.0 - r, h as f32 / 2.0 - r);
    let (qx, qy) = ((px.abs() - hx).max(0.0), (py.abs() - hy).max(0.0));
    let d = (qx * qx + qy * qy).sqrt() - r;
    (0.5 - d).clamp(0.0, 1.0)
}

fn pack(a: f32, r: f32, g: f32, b: f32) -> u32 {
    ((a.round() as u32) << 24) | ((r.round() as u32) << 16) | ((g.round() as u32) << 8) | b.round() as u32
}

/// Renders the background shape and the trace (not the text) into a
/// premultiplied `0xAARRGGBB` buffer of `pixel_size(f, scale)`.
pub fn render(f: &Frame, scale: f32) -> (usize, usize, Vec<u32>) {
    let (w, h) = pixel_size(f, scale);
    let (bg, _) = f.look.tone.colors();
    let radius = 8.0 * scale;
    let mut px = vec![0u32; w * h];
    for y in 0..h {
        for x in 0..w {
            let c = coverage(x, y, w, h, radius) * bg[3] as f32 / 255.0;
            px[y * w + x] = pack(c * 255.0, bg[0] as f32 * c, bg[1] as f32 * c, bg[2] as f32 * c);
        }
    }
    if f.look.trace {
        // Canvas inset 8 x 3 logical pixels, bars grow from the middle row.
        let (padx, pady) = ((8.0 * scale).round() as usize, (3.0 * scale).round() as usize);
        let (cw, ch) = (w.saturating_sub(2 * padx), h.saturating_sub(2 * pady));
        for i in 0..TRACE_LEN {
            let x0 = padx + i * cw / TRACE_LEN;
            let x1 = padx + (i + 1) * cw / TRACE_LEN;
            let x1 = if x1 - x0 > 1 { x1 - 1 } else { x1 };
            let bh = (f.trace[i].clamp(0.0, 1.0) * ch as f32).max(MIN_BAR * scale);
            let top = (pady as f32 + (ch as f32 - bh) / 2.0).round() as usize;
            let bot = ((pady as f32 + (ch as f32 + bh) / 2.0).round() as usize).min(h);
            let a = 0.35 + 0.65 * (i as f32 / TRACE_LEN as f32);
            for y in top..bot {
                for x in x0..x1.min(w) {
                    let p = px[y * w + x];
                    let (pa, pr, pg, pb) =
                        ((p >> 24) as f32, (p >> 16 & 255) as f32, (p >> 8 & 255) as f32, (p & 255) as f32);
                    let k = 1.0 - a;
                    px[y * w + x] = pack(pa * k + 255.0 * a, pr * k + 255.0 * a, pg * k + 255.0 * a, pb * k + 255.0 * a);
                }
            }
        }
    }
    (w, h, px)
}

/// GDI text drawing may clear alpha. Keeps the RGB GDI drew and puts back the
/// alpha of the rendered shape, premultiplying the colour.
pub fn restore_alpha(rendered: &[u32], drawn: &mut [u32]) {
    for (d, r) in drawn.iter_mut().zip(rendered) {
        let a = r >> 24;
        let k = a as f32 / 255.0;
        let (rr, gg, bb) = ((*d >> 16 & 255) as f32 * k, (*d >> 8 & 255) as f32 * k, (*d & 255) as f32 * k);
        *d = pack(a as f32, rr, gg, bb);
    }
}

/// A line of text to draw with the OS font, in pixels inside the window.
#[derive(Debug, Clone, PartialEq)]
pub struct TextLine {
    pub text: String,
    pub rect: Rect,
    /// Font height in pixels.
    pub font_px: f32,
    pub bold: bool,
    pub color: [u8; 3],
}

/// The text lines for a frame (empty until the overlay has mostly grown).
pub fn text_lines(f: &Frame, scale: f32) -> Vec<TextLine> {
    if !f.show_text {
        return vec![];
    }
    let (w, h) = pixel_size(f, scale);
    let (_, fg) = f.look.tone.colors();
    let padx = (14.0 * scale).round() as i32;
    let r = |top: f32, bottom: f32| Rect {
        left: padx,
        top: (top * scale).round() as i32,
        right: w as i32 - padx,
        bottom: ((bottom * scale).round() as i32).min(h as i32),
    };
    let mut out = vec![];
    if f.look.hint.is_empty() {
        let lines = if f.look.title.chars().count() <= ONE_LINE_CHARS { 1.0 } else { 2.0 };
        let top = (f.look.h - lines * 17.0) / 2.0;
        out.push(TextLine {
            text: f.look.title.clone(),
            rect: r(top, top + lines * 17.0),
            font_px: 13.0 * scale,
            bold: f.look.tone != Tone::Neutral,
            color: fg,
        });
    } else {
        out.push(TextLine { text: f.look.title.clone(), rect: r(10.0, 29.0), font_px: 14.0 * scale, bold: true, color: fg });
        out.push(TextLine { text: f.look.hint.clone(), rect: r(31.0, 52.0), font_px: 12.0 * scale, bold: false, color: fg });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listening(level: f32) -> Indicator {
        Indicator::Listening { level, note: None }
    }
    fn red(reason: FaultReason) -> Indicator {
        Indicator::ListeningFault { reason, message: reason.message(), recoverable: reason.is_recoverable() }
    }
    fn amber(m: &str) -> Indicator {
        Indicator::TypingFault { message: m.to_string() }
    }

    #[test]
    fn nothing_is_drawn_and_no_timer_is_needed_before_a_dictation() {
        let mut a = Animator::new();
        assert!(!a.active());
        assert_eq!(a.tick(0), None);
        a.set(&Indicator::Hidden, 10);
        assert!(!a.active());
        assert_eq!(a.tick(40), None);
    }

    #[test]
    fn fades_in_over_120_ms() {
        let mut a = Animator::new();
        a.set(&listening(0.0), 0);
        assert_eq!(a.tick(0).unwrap().opacity, 0.0);
        let half = a.tick(60).unwrap().opacity;
        assert!((half - 0.5).abs() < 0.01, "{half}");
        assert_eq!(a.tick(120).unwrap().opacity, 1.0);
    }

    #[test]
    fn fades_out_over_180_ms_then_stops_ticking() {
        let mut a = Animator::new();
        a.set(&listening(0.5), 0);
        a.tick(0);
        a.tick(200);
        a.tick(990);
        a.tick(1000);
        a.set(&Indicator::Hidden, 1000);
        let half = a.tick(1090).unwrap().opacity;
        assert!((half - 0.5).abs() < 0.01, "{half}");
        assert!(a.active());
        assert_eq!(a.tick(1180), None);
        assert!(!a.active());
    }

    #[test]
    fn reappearing_during_fade_out_returns_without_restart() {
        let mut a = Animator::new();
        a.set(&listening(0.5), 0);
        a.tick(0);
        a.tick(200);
        a.tick(990);
        a.tick(1000);
        a.set(&Indicator::Hidden, 1000);
        a.tick(1090);
        a.set(&listening(0.5), 1090);
        let f = a.tick(1120).unwrap();
        assert!(f.opacity > 0.5);
    }

    #[test]
    fn loudness_has_fast_attack_and_slow_release() {
        let mut a = Animator::new();
        a.set(&listening(0.0), 0);
        a.tick(0);
        a.tick(33);
        a.set(&listening(1.0), 66);
        let up = a.tick(99).unwrap().trace[TRACE_LEN - 1];
        assert!(up > 0.55, "attack too slow: {up}");
        for t in 2..12 {
            a.set(&listening(1.0), 66 + t * 33);
            a.tick(66 + t * 33 + 33);
        }
        let top = a.tick(600).unwrap().trace[TRACE_LEN - 1];
        assert!(top > 0.95);
        a.set(&listening(0.0), 600);
        let down = a.tick(633).unwrap().trace[TRACE_LEN - 1];
        assert!(down > 0.8, "release too fast: {down}");
        let mut later = 1.0;
        for i in 2..=21 {
            later = a.tick(600 + 33 * i).unwrap().trace[TRACE_LEN - 1];
        }
        assert!(later < 0.1, "release never completes: {later}");
    }

    #[test]
    fn trace_scrolls_with_newest_on_the_right() {
        let mut a = Animator::new();
        a.set(&listening(1.0), 0);
        a.tick(0);
        let f = a.tick(33).unwrap();
        assert!(f.trace[TRACE_LEN - 1] > f.trace[TRACE_LEN - 2]);
        assert_eq!(f.trace[0], 0.0);
    }

    #[test]
    fn a_fault_grows_from_the_strip_to_the_red_size_in_220_ms() {
        let mut a = Animator::new();
        a.set(&listening(0.3), 0);
        a.tick(0);
        a.tick(200);
        a.set(&red(FaultReason::NoMicrophone), 300);
        let start = a.tick(300).unwrap();
        assert_eq!((start.w, start.h), (STRIP_W, STRIP_H));
        let mid = a.tick(410).unwrap();
        assert!(mid.h > STRIP_H && mid.h < RED_H && mid.w > STRIP_W && mid.w < RED_W);
        let end = a.tick(520).unwrap();
        assert_eq!((end.w, end.h), (RED_W, RED_H));
        assert_eq!(end.look.tone, Tone::Red);
        assert!(end.show_text);
    }

    #[test]
    fn text_waits_until_the_overlay_has_mostly_grown() {
        let mut a = Animator::new();
        a.set(&red(FaultReason::Blocked), 0);
        assert!(!a.tick(0).unwrap().show_text);
        assert!(!a.tick(30).unwrap().show_text);
        assert!(a.tick(300).unwrap().show_text);
    }

    #[test]
    fn red_fault_shows_the_reason_and_its_hint() {
        let l = look(&red(FaultReason::Blocked)).unwrap();
        assert_eq!(l.title, "Windows is blocking the microphone");
        assert_eq!(l.hint, "Settings > Privacy > Microphone");
        assert_eq!((l.w, l.h), (RED_W, RED_H));
        assert!(!l.trace);
    }

    #[test]
    fn red_fault_without_a_hint_is_a_single_block() {
        let l = look(&red(FaultReason::MicrophoneError)).unwrap();
        assert_eq!(l.h, RED_H_NO_HINT);
    }

    #[test]
    fn amber_typing_fault_is_amber_and_sized_to_its_message() {
        let short = look(&amber("Couldn't type into this window")).unwrap();
        assert_eq!((short.tone, short.h), (Tone::Amber, AMBER_H_ONE_LINE));
        let long = look(&amber("Speech model is too slow for this PC. Choose a smaller model in Settings.")).unwrap();
        assert_eq!((long.tone, long.h), (Tone::Amber, AMBER_H_TWO_LINES));
        assert_eq!(look(&amber("Can't type into an administrator window")).unwrap().h, AMBER_H_TWO_LINES);
    }

    #[test]
    fn every_listening_fault_reason_is_red_and_never_amber() {
        for r in [
            FaultReason::NoMicrophone,
            FaultReason::Disconnected,
            FaultReason::InUse,
            FaultReason::Blocked,
            FaultReason::MutedOrSilent,
            FaultReason::NoSound,
            FaultReason::MicrophoneError,
        ] {
            assert_eq!(look(&red(r)).unwrap().tone, Tone::Red);
        }
    }

    #[test]
    fn normal_listening_is_the_small_dark_trace_strip_and_a_note_shows_text() {
        let l = look(&listening(0.2)).unwrap();
        assert_eq!((l.tone, l.trace, l.w, l.h), (Tone::Neutral, true, STRIP_W, STRIP_H));
        let n = look(&Indicator::Listening { level: 0.2, note: Some("Microphone not found, using the system default") })
            .unwrap();
        assert!(!n.trace);
        assert_eq!(n.title, "Microphone not found, using the system default");
    }

    #[test]
    fn bottom_center_sits_centered_above_the_taskbar() {
        let work = Rect { left: 0, top: 0, right: 1920, bottom: 1040 };
        assert_eq!(place(work, 150, 22, Placement::default(), 1.0), (885, 1002));
    }

    #[test]
    fn placement_follows_the_monitor_work_area_and_dpi() {
        let second = Rect { left: 1920, top: 100, right: 3840, bottom: 1140 };
        let (x, y) = place(second, 300, 44, Placement::default(), 2.0);
        assert_eq!((x, y), (1920 + 810, 1140 - 32 - 44));
        let top_left = Placement { vertical: Vertical::Top, horizontal: Horizontal::Left };
        assert_eq!(place(second, 300, 44, top_left, 1.0), (1936, 116));
        let right = Placement { vertical: Vertical::Bottom, horizontal: Horizontal::Right };
        assert_eq!(place(second, 300, 44, right, 1.0), (3840 - 16 - 300, 1140 - 16 - 44));
    }

    fn frame_with(level: f32) -> Frame {
        let mut a = Animator::new();
        a.set(&listening(level), 0);
        a.tick(0);
        let mut f = a.tick(120).unwrap();
        for t in 1..80 {
            a.set(&listening(level), 120 + t * 33);
            f = a.tick(120 + t * 33).unwrap();
        }
        f
    }

    fn lit_in_column(buf: &[u32], w: usize, h: usize, x: usize) -> usize {
        (0..h).filter(|&y| (buf[y * w + x] >> 8 & 255) > 200).count()
    }

    #[test]
    fn loud_trace_draws_taller_bars_than_silence() {
        let quiet = frame_with(0.0);
        let loud = frame_with(1.0);
        let (w, h, q) = render(&quiet, 1.0);
        let (_, _, l) = render(&loud, 1.0);
        assert_eq!((w, h), (150, 22));
        let x = w - 10;
        let (cq, cl) = (lit_in_column(&q, w, h, x), lit_in_column(&l, w, h, x));
        assert!(cq <= 2, "{cq}");
        assert!(cl >= 12, "{cl}");
    }

    #[test]
    fn corners_are_transparent_and_the_body_is_translucent_dark() {
        let (w, h, px) = render(&frame_with(0.0), 1.0);
        assert_eq!(px[0] >> 24, 0);
        let mid = px[(h / 2) * w + 4];
        assert_eq!(mid >> 24, 191);
        assert!((mid >> 8 & 255) < 40);
    }

    #[test]
    fn red_body_is_opaque_red() {
        let mut a = Animator::new();
        a.set(&red(FaultReason::NoMicrophone), 0);
        a.tick(0);
        let f = a.tick(400).unwrap();
        let (w, h, px) = render(&f, 1.0);
        assert_eq!((w, h), (280, 66));
        let p = px[(h / 2) * w + 5];
        assert_eq!((p >> 24, p >> 16 & 255, p >> 8 & 255), (255, 220, 38));
    }

    #[test]
    fn size_scales_with_dpi() {
        let (w, h, _) = render(&frame_with(0.0), 1.5);
        assert_eq!((w, h), (225, 33));
    }

    #[test]
    fn text_lines_are_placed_inside_the_overlay_and_use_the_tone_colour() {
        let mut a = Animator::new();
        a.set(&red(FaultReason::NoMicrophone), 0);
        a.tick(0);
        let f = a.tick(400).unwrap();
        let lines = text_lines(&f, 1.0);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].text, "No microphone found");
        assert_eq!(lines[1].text, "Plug one in or pick one in Settings");
        for l in &lines {
            assert!(l.rect.left >= 0 && l.rect.right <= 280 && l.rect.bottom <= 66 && l.rect.top < l.rect.bottom);
        }
        let mut b = Animator::new();
        b.set(&amber("Couldn't type into this window"), 0);
        b.tick(0);
        let f = b.tick(400).unwrap();
        let t = text_lines(&f, 1.0);
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].color, [32, 22, 0]);
    }

    #[test]
    fn strip_has_no_text() {
        assert!(text_lines(&frame_with(0.5), 1.0).is_empty());
    }

    #[test]
    fn restore_alpha_puts_back_shape_alpha_and_premultiplies() {
        let rendered = [0x00000000u32, 0xFF_DC_26_26, 0x80_6E_13_13];
        let mut drawn = [0x00_FF_FF_FFu32, 0x00_FF_FF_FF, 0x00_FF_FF_FF];
        restore_alpha(&rendered, &mut drawn);
        assert_eq!(drawn[0], 0);
        assert_eq!(drawn[1], 0xFF_FF_FF_FF);
        assert_eq!(drawn[2] >> 24, 0x80);
        assert_eq!(drawn[2] >> 16 & 255, 128);
    }
}
