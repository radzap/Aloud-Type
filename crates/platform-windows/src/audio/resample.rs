//! Streaming downmix to mono and resample to 16 kHz (area averaging).

pub const TARGET_RATE: u32 = 16_000;

pub struct Resampler {
    channels: usize,
    input_rate: u32,
    /// Input frames per output sample.
    step: f64,
    /// Start (in mono input frames, relative to `pending`) of the next output window.
    pos: f64,
    pending: Vec<f32>,
}

impl Resampler {
    pub fn new(input_rate: u32, channels: u16) -> Self {
        Self {
            channels: channels.max(1) as usize,
            input_rate,
            step: input_rate as f64 / TARGET_RATE as f64,
            pos: 0.0,
            pending: Vec::new(),
        }
    }

    pub fn input_rate(&self) -> u32 {
        self.input_rate
    }

    pub fn channels(&self) -> u16 {
        self.channels as u16
    }

    /// Feed interleaved frames; returns the newly available 16 kHz mono samples.
    pub fn process(&mut self, interleaved: &[f32]) -> Vec<f32> {
        for frame in interleaved.chunks_exact(self.channels) {
            self.pending.push(frame.iter().sum::<f32>() / self.channels as f32);
        }
        let mut out = Vec::new();
        loop {
            let end = self.pos + self.step;
            if end > self.pending.len() as f64 + 1e-9 {
                break;
            }
            out.push(self.window_mean(self.pos, end));
            self.pos = end;
        }
        let drop = (self.pos.floor() as usize).min(self.pending.len());
        self.pending.drain(..drop);
        self.pos -= drop as f64;
        out
    }

    fn window_mean(&self, start: f64, end: f64) -> f32 {
        let first = start.floor() as usize;
        let last = (end.ceil() as usize).min(self.pending.len());
        let mut acc = 0.0f64;
        for (i, &sample) in self.pending.iter().enumerate().take(last).skip(first) {
            let lo = (i as f64).max(start);
            let hi = ((i + 1) as f64).min(end);
            acc += sample as f64 * (hi - lo).max(0.0);
        }
        (acc / (end - start)) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(rate: u32, hz: f32, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| (2.0 * std::f32::consts::PI * hz * i as f32 / rate as f32).sin() * 0.5)
            .collect()
    }

    #[test]
    fn sixteen_k_mono_passes_through() {
        let mut r = Resampler::new(16_000, 1);
        let input = [0.1, -0.2, 0.3, 0.4];
        assert_eq!(r.process(&input), input.to_vec());
    }

    #[test]
    fn forty_eight_k_yields_one_third_the_samples() {
        let mut r = Resampler::new(48_000, 1);
        let out = r.process(&vec![0.25; 4800]);
        assert_eq!(out.len(), 1600);
        assert!(out.iter().all(|s| (s - 0.25).abs() < 1e-6));
    }

    #[test]
    fn stereo_is_averaged_to_mono() {
        let mut r = Resampler::new(16_000, 2);
        assert_eq!(r.process(&[1.0, 0.0, 0.5, 0.5]), vec![0.5, 0.5]);
    }

    #[test]
    fn non_integer_ratio_44100_produces_expected_count() {
        let mut r = Resampler::new(44_100, 1);
        let out = r.process(&vec![0.1; 44_100]);
        assert!((out.len() as i32 - 16_000).abs() <= 1, "got {}", out.len());
    }

    #[test]
    fn chunking_does_not_change_the_output() {
        let input = sine(44_100, 440.0, 8820);
        let whole = Resampler::new(44_100, 1).process(&input);
        let mut r = Resampler::new(44_100, 1);
        let mut parts = Vec::new();
        for c in input.chunks(441) {
            parts.extend(r.process(c));
        }
        assert_eq!(whole.len(), parts.len());
        for (a, b) in whole.iter().zip(&parts) {
            assert!((a - b).abs() < 1e-5);
        }
    }

    #[test]
    fn in_band_tone_keeps_its_amplitude_and_out_of_band_is_attenuated() {
        let mut r = Resampler::new(48_000, 1);
        let out = r.process(&sine(48_000, 1000.0, 48_000));
        let peak = out.iter().cloned().fold(0.0f32, f32::max);
        assert!(peak > 0.4 && peak <= 0.51, "peak {peak}");
        let mut r = Resampler::new(48_000, 1);
        let hi = r.process(&sine(48_000, 12_000.0, 48_000));
        let hpeak = hi.iter().skip(10).cloned().fold(0.0f32, f32::max);
        assert!(hpeak < 0.2, "aliased peak {hpeak}");
    }
}
