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
        Self {
            vertical: Vertical::Bottom,
            horizontal: Horizontal::Center,
        }
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

/// The three Listening indicator styles (Settings values `bar-pill`, `orb`,
/// `trace-strip`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Style {
    BarPill,
    Orb,
    #[default]
    TraceStrip,
}
impl Style {
    /// From the Settings value; `None` for anything unknown.
    pub fn from_setting(s: &str) -> Option<Self> {
        match s {
            "bar-pill" => Some(Self::BarPill),
            "orb" => Some(Self::Orb),
            "trace-strip" => Some(Self::TraceStrip),
            _ => None,
        }
    }
}
impl Placement {
    /// From the Settings values (`top`/`bottom`, `left`/`center`/`right`).
    pub fn from_settings(vertical: &str, horizontal: &str) -> Option<Self> {
        let vertical = match vertical {
            "top" => Vertical::Top,
            "bottom" => Vertical::Bottom,
            _ => return None,
        };
        let horizontal = match horizontal {
            "left" => Horizontal::Left,
            "center" => Horizontal::Center,
            "right" => Horizontal::Right,
            _ => return None,
        };
        Some(Self {
            vertical,
            horizontal,
        })
    }
}

/// The loudness graphic a style draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Glyph {
    None,
    /// Scrolling envelope (Trace strip).
    Trace,
    /// Seven bars, the centre reacting first (Bar pill).
    Bars,
    /// A halo that grows with loudness around a steady disc (Orb).
    Orb,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Shape {
    /// Corner radius in logical pixels.
    Rounded(f32),
    /// Fully round ends (half the shorter side).
    Pill,
}

/// What the overlay should look like for one indicator state.
#[derive(Debug, Clone, PartialEq)]
pub struct Look {
    pub style: Style,
    pub tone: Tone,
    pub title: String,
    pub hint: String,
    pub glyph: Glyph,
    pub shape: Shape,
    /// No background body (the Orb while listening is just the orb).
    pub bodyless: bool,
    /// Logical left and right text insets.
    pub inset: (f32, f32),
    /// The title fits on one line.
    pub one_line: bool,
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
pub const PILL_W: f32 = 76.0;
pub const PILL_H: f32 = 26.0;
pub const PILL_FAULT_W: f32 = 300.0;
pub const PILL_FAULT_H: f32 = 52.0;
pub const PILL_FAULT_H_NO_HINT: f32 = 40.0;
pub const ORB_D: f32 = 40.0;
pub const ORB_FAULT_D: f32 = 44.0;
pub const ORB_FAULT_W: f32 = 310.0;
pub const ORB_FAULT_H: f32 = 56.0;
/// Approximate width of one character of the 13 px bold UI font.
const CHAR_W: f32 = 7.15;

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

fn base(style: Style, tone: Tone, title: &str, hint: &str, w: f32, h: f32) -> Look {
    let (shape, inset) = match style {
        Style::TraceStrip => (Shape::Rounded(8.0), (14.0, 14.0)),
        Style::BarPill => (Shape::Pill, (20.0, 20.0)),
        Style::Orb => (Shape::Rounded(14.0), (14.0, 14.0)),
    };
    let chars = ((w - inset.0 - inset.1) / CHAR_W).floor() as usize;
    Look {
        style,
        tone,
        title: title.to_string(),
        hint: hint.to_string(),
        glyph: Glyph::None,
        shape,
        bodyless: false,
        inset,
        one_line: title.chars().count() <= chars,
        w,
        h,
    }
}

fn listening_look(style: Style) -> Look {
    let (w, h) = match style {
        Style::TraceStrip => (STRIP_W, STRIP_H),
        Style::BarPill => (PILL_W, PILL_H),
        Style::Orb => (ORB_D, ORB_D),
    };
    let mut l = base(style, Tone::Neutral, "", "", w, h);
    match style {
        Style::TraceStrip => l.glyph = Glyph::Trace,
        Style::BarPill => l.glyph = Glyph::Bars,
        Style::Orb => {
            l.glyph = Glyph::Orb;
            l.shape = Shape::Pill;
            l.bodyless = true;
        }
    }
    l
}

/// Size of the indicator while listening normally; faults grow from it.
pub fn idle_size(style: Style) -> (f32, f32) {
    let l = listening_look(style);
    (l.w, l.h)
}

/// The look for an indicator state; `None` when the overlay must be hidden.
pub fn look(ind: &Indicator, style: Style) -> Option<Look> {
    match ind {
        Indicator::Hidden => None,
        Indicator::Listening { note: None, .. } => Some(listening_look(style)),
        Indicator::Listening { note: Some(n), .. } => {
            let mut l = base(style, Tone::Neutral, n, "", NOTE_W, NOTE_H);
            if style == Style::Orb {
                l.shape = Shape::Pill;
            }
            Some(l)
        }
        Indicator::ListeningFault {
            reason, message, ..
        } => {
            let hint = hint(*reason);
            let (w, h) = match (style, hint.is_empty()) {
                (Style::TraceStrip, false) => (RED_W, RED_H),
                (Style::TraceStrip, true) => (RED_W, RED_H_NO_HINT),
                (Style::BarPill, false) => (PILL_FAULT_W, PILL_FAULT_H),
                (Style::BarPill, true) => (PILL_FAULT_W, PILL_FAULT_H_NO_HINT),
                (Style::Orb, _) => (ORB_FAULT_W, ORB_FAULT_H),
            };
            let mut l = base(style, Tone::Red, message, hint, w, h);
            if style == Style::Orb {
                l.glyph = Glyph::Orb;
                l.inset = (6.0 + ORB_FAULT_D + 10.0, 16.0);
                l.one_line = l.title.chars().count()
                    <= ((w - l.inset.0 - l.inset.1) / CHAR_W).floor() as usize;
            }
            Some(l)
        }
        Indicator::TypingFault { message } => {
            let w = if style == Style::Orb {
                ORB_FAULT_W
            } else {
                AMBER_W
            };
            let mut l = base(style, Tone::Amber, message, "", w, 0.0);
            l.h = match (l.one_line, style) {
                (true, Style::Orb) => ORB_FAULT_H,
                (true, _) => AMBER_H_ONE_LINE,
                (false, _) => AMBER_H_TWO_LINES,
            };
            Some(l)
        }
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
    /// Smoothed current loudness, 0..=1.
    pub level: f32,
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
    style: Style,
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
            style: Style::TraceStrip,
        }
    }

    /// True while something is visible or fading (the timer must run).
    pub fn active(&self) -> bool {
        self.target.is_some() || self.opacity > 0.0
    }

    /// `set_styled` with the default Trace strip style.
    pub fn set(&mut self, ind: &Indicator, now: Millis) {
        self.set_styled(ind, Style::TraceStrip, now);
    }

    /// Reports the indicator state. `style` is read only when the overlay
    /// appears: a style change applies to the next Dictation.
    pub fn set_styled(&mut self, ind: &Indicator, style: Style, now: Millis) {
        let style = if self.active() { self.style } else { style };
        self.style = style;
        let new = look(ind, style);
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
                    self.size = idle_size(style);
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
        let dt = self
            .last_tick
            .map(|t| now.saturating_sub(t).min(100) as f32)
            .unwrap_or(0.0);
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
        let tau = if self.level_target > self.level {
            ATTACK_TAU_MS
        } else {
            RELEASE_TAU_MS
        };
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
            level: self.level,
        })
    }
}

/// Pixel size of a frame at the given scale (DPI / 96).
pub fn pixel_size(f: &Frame, scale: f32) -> (usize, usize) {
    (
        (f.w * scale).round().max(1.0) as usize,
        (f.h * scale).round().max(1.0) as usize,
    )
}

/// Anti-aliased coverage of pixel (x, y) inside a rounded rectangle at
/// (rx, ry) of size rw x rh with corner radius r (all in pixels).
fn rrect_coverage(x: usize, y: usize, rx: f32, ry: f32, rw: f32, rh: f32, r: f32) -> f32 {
    let r = r.min(rw / 2.0).min(rh / 2.0).max(0.0);
    let (px, py) = (
        x as f32 + 0.5 - rx - rw / 2.0,
        y as f32 + 0.5 - ry - rh / 2.0,
    );
    let (hx, hy) = (rw / 2.0 - r, rh / 2.0 - r);
    let (qx, qy) = ((px.abs() - hx).max(0.0), (py.abs() - hy).max(0.0));
    let d = (qx * qx + qy * qy).sqrt() - r;
    (0.5 - d).clamp(0.0, 1.0)
}

fn pack(a: f32, r: f32, g: f32, b: f32) -> u32 {
    ((a.round() as u32) << 24)
        | ((r.round() as u32) << 16)
        | ((g.round() as u32) << 8)
        | b.round() as u32
}

/// Source-over blend of colour `c` at opacity `a` onto a premultiplied pixel.
fn blend(dst: u32, c: [f32; 3], a: f32) -> u32 {
    let k = 1.0 - a;
    pack(
        (dst >> 24) as f32 * k + 255.0 * a,
        (dst >> 16 & 255) as f32 * k + c[0] * a,
        (dst >> 8 & 255) as f32 * k + c[1] * a,
        (dst & 255) as f32 * k + c[2] * a,
    )
}

/// Fills an anti-aliased rounded rectangle (a circle when r = w/2 = h/2).
#[allow(clippy::too_many_arguments)]
fn fill_rrect(
    px: &mut [u32],
    w: usize,
    h: usize,
    rect: (f32, f32, f32, f32),
    r: f32,
    c: [f32; 3],
    alpha: f32,
) {
    let (rx, ry, rw, rh) = rect;
    let x0 = rx.floor().max(0.0) as usize;
    let y0 = ry.floor().max(0.0) as usize;
    let x1 = ((rx + rw).ceil().max(0.0) as usize).min(w);
    let y1 = ((ry + rh).ceil().max(0.0) as usize).min(h);
    for y in y0..y1 {
        for x in x0..x1 {
            let cov = rrect_coverage(x, y, rx, ry, rw, rh, r) * alpha;
            if cov > 0.0 {
                px[y * w + x] = blend(px[y * w + x], c, cov);
            }
        }
    }
}

fn fill_circle(
    px: &mut [u32],
    w: usize,
    h: usize,
    (cx, cy, r): (f32, f32, f32),
    c: [f32; 3],
    alpha: f32,
) {
    fill_rrect(px, w, h, (cx - r, cy - r, 2.0 * r, 2.0 * r), r, c, alpha);
}

/// Centre and diameter (pixels) of the orb of a frame.
fn orb_geometry(f: &Frame, w: usize, h: usize, scale: f32) -> (f32, f32, f32) {
    if f.look.tone == Tone::Neutral {
        (w as f32 / 2.0, h as f32 / 2.0, w.min(h) as f32)
    } else {
        (
            (6.0 + ORB_FAULT_D / 2.0) * scale,
            h as f32 / 2.0,
            ORB_FAULT_D * scale,
        )
    }
}

/// Renders the background shape and the loudness glyph (not the text) into a
/// premultiplied `0xAARRGGBB` buffer of `pixel_size(f, scale)`.
pub fn render(f: &Frame, scale: f32) -> (usize, usize, Vec<u32>) {
    let (w, h) = pixel_size(f, scale);
    let (bg, _) = f.look.tone.colors();
    let radius = match f.look.shape {
        Shape::Rounded(r) => r * scale,
        Shape::Pill => w.min(h) as f32 / 2.0,
    };
    let mut px = vec![0u32; w * h];
    if !f.look.bodyless {
        for y in 0..h {
            for x in 0..w {
                let c = rrect_coverage(x, y, 0.0, 0.0, w as f32, h as f32, radius) * bg[3] as f32
                    / 255.0;
                px[y * w + x] = pack(
                    c * 255.0,
                    bg[0] as f32 * c,
                    bg[1] as f32 * c,
                    bg[2] as f32 * c,
                );
            }
        }
    }
    match f.look.glyph {
        Glyph::None => {}
        Glyph::Trace => render_trace(&mut px, w, h, f, scale),
        Glyph::Bars => {
            // Seven bars in a 52 x 16 canvas, the centre one reacting first and
            // the edges echoing recent history.
            let (cw, ch) = (52.0 * scale, 16.0 * scale);
            let (ox, oy) = ((w as f32 - cw) / 2.0, (h as f32 - ch) / 2.0);
            let n = 7usize;
            let gap = cw / n as f32 * 0.38;
            let bw = cw / n as f32 - gap;
            for i in 0..n {
                let d = (i as i32 - 3).unsigned_abs() as usize;
                let v = f.trace[TRACE_LEN - 1 - d * 2] * (1.0 - d as f32 * 0.12);
                let bh = (v.clamp(0.0, 1.0) * ch).max(bw);
                let x = ox + i as f32 * (bw + gap) + gap / 2.0;
                fill_rrect(
                    &mut px,
                    w,
                    h,
                    (x, oy + (ch - bh) / 2.0, bw, bh),
                    bw / 2.0,
                    [255.0; 3],
                    1.0,
                );
            }
        }
        Glyph::Orb => {
            let (cx, cy, d) = orb_geometry(f, w, h, scale);
            let r0 = d * 0.3;
            if f.look.tone == Tone::Neutral {
                // Halo grows with loudness; the inner disc stays steady.
                let v = f.level.clamp(0.0, 1.0);
                fill_circle(
                    &mut px,
                    w,
                    h,
                    (cx, cy, r0 + v * (d / 2.0 - r0)),
                    [80.0, 140.0, 255.0],
                    0.25 + v * 0.35,
                );
                fill_circle(&mut px, w, h, (cx, cy, r0 * 1.15), [20.0, 20.0, 24.0], 0.9);
            } else {
                fill_circle(&mut px, w, h, (cx, cy, r0 * 1.15), [0.0; 3], 0.18);
            }
        }
    }
    (w, h, px)
}

fn render_trace(px: &mut [u32], w: usize, h: usize, f: &Frame, scale: f32) {
    // Canvas inset 8 x 3 logical pixels, bars grow from the middle row.
    let (padx, pady) = (
        (8.0 * scale).round() as usize,
        (3.0 * scale).round() as usize,
    );
    let (cw, ch) = (w.saturating_sub(2 * padx), h.saturating_sub(2 * pady));
    for (i, &level) in f.trace.iter().enumerate().take(TRACE_LEN) {
        let x0 = padx + i * cw / TRACE_LEN;
        let x1 = padx + (i + 1) * cw / TRACE_LEN;
        let x1 = if x1 - x0 > 1 { x1 - 1 } else { x1 };
        let bh = (level.clamp(0.0, 1.0) * ch as f32).max(MIN_BAR * scale);
        let top = (pady as f32 + (ch as f32 - bh) / 2.0).round() as usize;
        let bot = ((pady as f32 + (ch as f32 + bh) / 2.0).round() as usize).min(h);
        let a = 0.35 + 0.65 * (i as f32 / TRACE_LEN as f32);
        for y in top..bot {
            for x in x0..x1.min(w) {
                px[y * w + x] = blend(px[y * w + x], [255.0; 3], a);
            }
        }
    }
}

/// GDI text drawing may clear alpha. Keeps the RGB GDI drew and puts back the
/// alpha of the rendered shape, premultiplying the colour.
pub fn restore_alpha(rendered: &[u32], drawn: &mut [u32]) {
    for (d, r) in drawn.iter_mut().zip(rendered) {
        let a = r >> 24;
        let k = a as f32 / 255.0;
        let (rr, gg, bb) = (
            (*d >> 16 & 255) as f32 * k,
            (*d >> 8 & 255) as f32 * k,
            (*d & 255) as f32 * k,
        );
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
    let (left, right) = (
        (f.look.inset.0 * scale).round() as i32,
        (f.look.inset.1 * scale).round() as i32,
    );
    let r = |top: f32, bottom: f32| Rect {
        left,
        top: (top * scale).round() as i32,
        right: w as i32 - right,
        bottom: ((bottom * scale).round() as i32).min(h as i32),
    };
    let mut out = vec![];
    if f.look.hint.is_empty() {
        let lines = if f.look.one_line { 1.0 } else { 2.0 };
        let top = (f.look.h - lines * 17.0) / 2.0;
        out.push(TextLine {
            text: f.look.title.clone(),
            rect: r(top, top + lines * 17.0),
            font_px: 13.0 * scale,
            bold: f.look.tone != Tone::Neutral,
            color: fg,
        });
    } else {
        // Title (19) and hint (17) with a 2 px gap, centred vertically.
        let top = (f.look.h - 38.0) / 2.0;
        out.push(TextLine {
            text: f.look.title.clone(),
            rect: r(top, top + 19.0),
            font_px: 14.0 * scale,
            bold: true,
            color: fg,
        });
        out.push(TextLine {
            text: f.look.hint.clone(),
            rect: r(top + 21.0, top + 38.0),
            font_px: 12.0 * scale,
            bold: false,
            color: fg,
        });
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
        Indicator::ListeningFault {
            reason,
            message: reason.message(),
            recoverable: reason.is_recoverable(),
        }
    }
    fn amber(m: &str) -> Indicator {
        Indicator::TypingFault {
            message: m.to_string(),
        }
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
        let l = look(&red(FaultReason::Blocked), Style::TraceStrip).unwrap();
        assert_eq!(l.title, "Windows is blocking the microphone");
        assert_eq!(l.hint, "Settings > Privacy > Microphone");
        assert_eq!((l.w, l.h), (RED_W, RED_H));
        assert_eq!(l.glyph, Glyph::None);
    }

    #[test]
    fn red_fault_without_a_hint_is_a_single_block() {
        let l = look(&red(FaultReason::MicrophoneError), Style::TraceStrip).unwrap();
        assert_eq!(l.h, RED_H_NO_HINT);
    }

    #[test]
    fn amber_typing_fault_is_amber_and_sized_to_its_message() {
        let short = look(&amber("Couldn't type into this window"), Style::TraceStrip).unwrap();
        assert_eq!((short.tone, short.h), (Tone::Amber, AMBER_H_ONE_LINE));
        let long = look(
            &amber("Speech model is too slow for this PC. Choose a smaller model in Settings."),
            Style::TraceStrip,
        )
        .unwrap();
        assert_eq!((long.tone, long.h), (Tone::Amber, AMBER_H_TWO_LINES));
        assert_eq!(
            look(
                &amber("Can't type into an administrator window"),
                Style::TraceStrip
            )
            .unwrap()
            .h,
            AMBER_H_TWO_LINES
        );
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
            assert_eq!(look(&red(r), Style::TraceStrip).unwrap().tone, Tone::Red);
        }
    }

    #[test]
    fn normal_listening_is_the_small_dark_trace_strip_and_a_note_shows_text() {
        let l = look(&listening(0.2), Style::TraceStrip).unwrap();
        assert_eq!(
            (l.tone, l.glyph, l.w, l.h),
            (Tone::Neutral, Glyph::Trace, STRIP_W, STRIP_H)
        );
        let n = look(
            &Indicator::Listening {
                level: 0.2,
                note: Some("Using default microphone"),
            },
            Style::TraceStrip,
        )
        .unwrap();
        assert_eq!(n.glyph, Glyph::None);
        assert_eq!(n.title, "Using default microphone");
    }

    #[test]
    fn bottom_center_sits_centered_above_the_taskbar() {
        let work = Rect {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1040,
        };
        assert_eq!(place(work, 150, 22, Placement::default(), 1.0), (885, 1002));
    }

    #[test]
    fn placement_follows_the_monitor_work_area_and_dpi() {
        let second = Rect {
            left: 1920,
            top: 100,
            right: 3840,
            bottom: 1140,
        };
        let (x, y) = place(second, 300, 44, Placement::default(), 2.0);
        assert_eq!((x, y), (1920 + 810, 1140 - 32 - 44));
        let top_left = Placement {
            vertical: Vertical::Top,
            horizontal: Horizontal::Left,
        };
        assert_eq!(place(second, 300, 44, top_left, 1.0), (1936, 116));
        let right = Placement {
            vertical: Vertical::Bottom,
            horizontal: Horizontal::Right,
        };
        assert_eq!(
            place(second, 300, 44, right, 1.0),
            (3840 - 16 - 300, 1140 - 16 - 44)
        );
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
        (0..h)
            .filter(|&y| (buf[y * w + x] >> 8 & 255) > 200)
            .count()
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
            assert!(
                l.rect.left >= 0
                    && l.rect.right <= 280
                    && l.rect.bottom <= 66
                    && l.rect.top < l.rect.bottom
            );
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

    // ---- ticket 14: Bar pill, Orb, positions ----

    fn styled(ind: &Indicator, style: Style, settle_ms: u64) -> Frame {
        let mut a = Animator::new();
        a.set_styled(ind, style, 0);
        a.tick(0);
        let mut f = a.tick(33).unwrap();
        let mut t = 33;
        while t < settle_ms {
            t += 33;
            a.set_styled(ind, style, t);
            f = a.tick(t).unwrap();
        }
        f
    }

    #[test]
    fn settings_values_map_to_styles_and_positions() {
        assert_eq!(Style::from_setting("bar-pill"), Some(Style::BarPill));
        assert_eq!(Style::from_setting("orb"), Some(Style::Orb));
        assert_eq!(Style::from_setting("trace-strip"), Some(Style::TraceStrip));
        assert_eq!(Style::from_setting("nope"), None);
        assert_eq!(
            Placement::from_settings("top", "right"),
            Some(Placement {
                vertical: Vertical::Top,
                horizontal: Horizontal::Right
            })
        );
        assert_eq!(Placement::from_settings("middle", "left"), None);
    }

    #[test]
    fn each_style_has_its_own_listening_size() {
        let size = |s| {
            let l = look(&listening(0.2), s).unwrap();
            (l.w, l.h)
        };
        assert_eq!(size(Style::TraceStrip), (150.0, 22.0));
        assert_eq!(size(Style::BarPill), (76.0, 26.0));
        assert_eq!(size(Style::Orb), (40.0, 40.0));
    }

    #[test]
    fn pill_fault_is_a_wide_pill_with_reason_and_hint_and_fits_its_text() {
        let l = look(&red(FaultReason::Blocked), Style::BarPill).unwrap();
        assert_eq!(
            (l.tone, l.shape, l.w, l.h),
            (Tone::Red, Shape::Pill, 300.0, 52.0)
        );
        assert!(l.one_line);
        assert_eq!(
            look(&red(FaultReason::MicrophoneError), Style::BarPill)
                .unwrap()
                .h,
            40.0
        );
        let f = styled(&red(FaultReason::Blocked), Style::BarPill, 400);
        for t in text_lines(&f, 1.0) {
            assert!(
                t.rect.left >= 20 && t.rect.right <= 280 && t.rect.top >= 0 && t.rect.bottom <= 52
            );
        }
    }

    #[test]
    fn orb_fault_keeps_the_orb_on_the_left_and_text_to_its_right() {
        let l = look(&red(FaultReason::NoMicrophone), Style::Orb).unwrap();
        assert_eq!((l.glyph, l.w, l.h), (Glyph::Orb, 310.0, 56.0));
        let f = styled(&red(FaultReason::NoMicrophone), Style::Orb, 400);
        let lines = text_lines(&f, 1.0);
        assert_eq!(lines.len(), 2);
        for t in &lines {
            assert!(t.rect.left >= 60 && t.rect.right <= 294 && t.rect.bottom <= 56);
        }
        // The darker disc of the orb sits at the left, plain red body elsewhere.
        let (w, h, px) = render(&f, 1.0);
        let disc = px[(h / 2) * w + 28];
        assert!((disc >> 16 & 255) < 220, "disc not drawn: {disc:#x}");
        assert_eq!(px[(h / 2) * w + 200] >> 16 & 255, 220);
    }

    #[test]
    fn long_messages_get_two_lines_in_every_style() {
        let long = "Speech model is too slow for this PC. Choose a smaller model in Settings.";
        for s in [Style::TraceStrip, Style::BarPill, Style::Orb] {
            let l = look(&amber(long), s).unwrap();
            assert!(!l.one_line, "{s:?}");
            assert_eq!(l.h, AMBER_H_TWO_LINES, "{s:?}");
            let short = look(&amber("Couldn't type into this window"), s).unwrap();
            assert!(short.one_line, "{s:?}");
        }
    }

    #[test]
    fn faults_grow_from_each_styles_own_listening_size() {
        for s in [Style::BarPill, Style::Orb] {
            let (w0, h0) = idle_size(s);
            let mut a = Animator::new();
            a.set_styled(&red(FaultReason::NoMicrophone), s, 0);
            let f = a.tick(0).unwrap();
            assert_eq!((f.w, f.h), (w0, h0), "{s:?}");
            let end = a.tick(400).unwrap();
            let target = look(&red(FaultReason::NoMicrophone), s).unwrap();
            assert_eq!((end.w, end.h), (target.w, target.h), "{s:?}");
        }
    }

    #[test]
    fn style_change_applies_to_the_next_dictation_not_the_running_one() {
        let mut a = Animator::new();
        a.set_styled(&listening(0.2), Style::BarPill, 0);
        a.tick(0);
        a.set_styled(&listening(0.2), Style::Orb, 40);
        assert_eq!(a.tick(40).unwrap().look.style, Style::BarPill);
        a.set_styled(&Indicator::Hidden, Style::Orb, 100);
        a.tick(100);
        a.tick(300);
        assert!(!a.active());
        a.set_styled(&listening(0.2), Style::Orb, 400);
        assert_eq!(a.tick(400).unwrap().look.style, Style::Orb);
    }

    #[test]
    fn pill_bars_react_in_the_centre_and_stay_in_a_pill_body() {
        let quiet = styled(&listening(0.0), Style::BarPill, 600);
        let loud = styled(&listening(1.0), Style::BarPill, 600);
        let (w, h, q) = render(&quiet, 1.0);
        let (_, _, l) = render(&loud, 1.0);
        assert_eq!((w, h), (76, 26));
        let cx = w / 2;
        let lit = |b: &[u32], x: usize| (0..h).filter(|&y| (b[y * w + x] >> 8 & 255) > 200).count();
        assert!(lit(&l, cx) >= 14, "{}", lit(&l, cx));
        assert!(lit(&q, cx) <= 6, "{}", lit(&q, cx));
        assert_eq!(l[0] >> 24, 0, "corner of a pill is transparent");
        assert_eq!(l[(h / 2) * w + 5] >> 24, 191);
    }

    #[test]
    fn orb_halo_grows_with_loudness_and_has_no_body() {
        let quiet = styled(&listening(0.0), Style::Orb, 800);
        let loud = styled(&listening(1.0), Style::Orb, 800);
        let (w, h, q) = render(&quiet, 1.0);
        let (_, _, l) = render(&loud, 1.0);
        assert_eq!((w, h), (40, 40));
        assert_eq!(q[0] >> 24, 0);
        let row = h / 2;
        // x = 3 is outside the quiet orb (disc radius about 14) but inside a loud halo.
        assert_eq!(q[row * w + 3] >> 24, 0);
        assert!(l[row * w + 3] >> 24 > 40, "{:#x}", l[row * w + 3]);
        assert!(q[row * w + w / 2] >> 24 > 200, "inner disc steady");
    }

    #[test]
    fn dpi_scales_every_style() {
        for (s, w, h) in [
            (Style::BarPill, 114, 39),
            (Style::Orb, 60, 60),
            (Style::TraceStrip, 225, 33),
        ] {
            let (rw, rh, px) = render(&styled(&listening(0.5), s, 400), 1.5);
            assert_eq!((rw, rh), (w, h), "{s:?}");
            assert_eq!(px.len(), w * h);
        }
        let f = styled(&red(FaultReason::NoMicrophone), Style::Orb, 400);
        assert_eq!(pixel_size(&f, 2.0), (620, 112));
    }

    #[test]
    fn all_six_positions_sit_inside_the_work_area_beside_the_taskbar() {
        // Taskbar on the left (work area starts at x = 48) of a 1920x1080 monitor.
        let work = Rect {
            left: 48,
            top: 0,
            right: 1920,
            bottom: 1080,
        };
        let (w, h) = (300, 56);
        let cases = [
            (Vertical::Top, Horizontal::Left, (64, 16)),
            (
                Vertical::Top,
                Horizontal::Center,
                (48 + (1872 - 300) / 2, 16),
            ),
            (Vertical::Top, Horizontal::Right, (1920 - 16 - 300, 16)),
            (Vertical::Bottom, Horizontal::Left, (64, 1080 - 16 - 56)),
            (
                Vertical::Bottom,
                Horizontal::Center,
                (48 + (1872 - 300) / 2, 1008),
            ),
            (Vertical::Bottom, Horizontal::Right, (1604, 1008)),
        ];
        for (vertical, horizontal, want) in cases {
            assert_eq!(
                place(
                    work,
                    w,
                    h,
                    Placement {
                        vertical,
                        horizontal
                    },
                    1.0
                ),
                want
            );
        }
    }

    #[test]
    fn margin_scales_with_dpi_in_every_corner() {
        let work = Rect {
            left: 0,
            top: 0,
            right: 3000,
            bottom: 2000,
        };
        let tl = Placement {
            vertical: Vertical::Top,
            horizontal: Horizontal::Left,
        };
        let br = Placement {
            vertical: Vertical::Bottom,
            horizontal: Horizontal::Right,
        };
        assert_eq!(place(work, 100, 60, tl, 1.5), (24, 24));
        assert_eq!(
            place(work, 100, 60, br, 1.5),
            (3000 - 24 - 100, 2000 - 24 - 60)
        );
    }
}
