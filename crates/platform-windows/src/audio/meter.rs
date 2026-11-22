//! Loudness metering on the 16 kHz mono stream: RMS per 25 ms hop, mapped to
//! 0..1 with fast attack and slow release.

pub const HOP_SAMPLES: usize = 400; // 25 ms at 16 kHz
const FLOOR_DB: f32 = -60.0;
const CEIL_DB: f32 = -10.0;
const RELEASE: f32 = 0.75; // per-hop decay factor

#[derive(Default)]
pub struct LevelMeter {
    buf: Vec<f32>,
    level: f32,
}

impl LevelMeter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Latest smoothed level, 0..1.
    pub fn level(&self) -> f32 {
        self.level
    }

    /// Feed samples; returns one smoothed level per completed hop.
    pub fn push(&mut self, samples: &[f32]) -> Vec<f32> {
        self.buf.extend_from_slice(samples);
        let mut out = Vec::new();
        while self.buf.len() >= HOP_SAMPLES {
            let raw = hop_level(&self.buf[..HOP_SAMPLES]);
            self.buf.drain(..HOP_SAMPLES);
            self.level =
                if raw > self.level { raw } else { self.level * RELEASE + raw * (1.0 - RELEASE) };
            out.push(self.level);
        }
        out
    }

    pub fn reset(&mut self) {
        self.buf.clear();
        self.level = 0.0;
    }
}

fn hop_level(hop: &[f32]) -> f32 {
    let ms = hop.iter().map(|s| s * s).sum::<f32>() / hop.len() as f32;
    if ms <= 0.0 {
        return 0.0;
    }
    let db = 10.0 * ms.log10();
    ((db - FLOOR_DB) / (CEIL_DB - FLOOR_DB)).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digital_silence_reads_zero() {
        let mut m = LevelMeter::new();
        assert_eq!(m.push(&vec![0.0; HOP_SAMPLES]), vec![0.0]);
        assert_eq!(m.level(), 0.0);
    }

    #[test]
    fn loud_signal_reads_high_and_quiet_reads_low() {
        let mut loud = LevelMeter::new();
        let l = loud.push(&vec![0.5; HOP_SAMPLES])[0]; // -6 dBFS
        assert_eq!(l, 1.0);
        let mut quiet = LevelMeter::new();
        let q = quiet.push(&vec![0.01; HOP_SAMPLES])[0]; // -40 dBFS
        assert!((q - 0.4).abs() < 0.01, "{q}");
    }

    #[test]
    fn emits_one_value_per_25ms_hop_across_pushes() {
        let mut m = LevelMeter::new();
        assert!(m.push(&vec![0.1; 300]).is_empty());
        assert_eq!(m.push(&vec![0.1; 600]).len(), 2);
    }

    #[test]
    fn attack_is_instant_and_release_is_gradual() {
        let mut m = LevelMeter::new();
        m.push(&vec![0.5; HOP_SAMPLES]);
        let after = m.push(&vec![0.0; HOP_SAMPLES * 2]);
        assert!(after[0] < 1.0 && after[0] > 0.5);
        assert!(after[1] < after[0]);
    }
}
