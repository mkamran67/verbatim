/// Simple linear-interpolation resampler that converts multi-channel audio
/// to 16kHz mono f32.
pub struct Resampler {
    from_rate: u32,
    to_rate: u32,
    channels: usize,
    /// Fractional sample position carried between calls.
    fractional_pos: f64,
}

impl Resampler {
    pub fn new(from_rate: u32, to_rate: u32, channels: usize) -> Self {
        Self {
            from_rate,
            to_rate,
            channels,
            fractional_pos: 0.0,
        }
    }

    /// Process a buffer of interleaved f32 samples.
    /// Returns mono 16kHz f32 samples.
    pub fn process(&mut self, input: &[f32]) -> Vec<f32> {
        // First, mix down to mono
        let mono = self.to_mono(input);

        // If rates match, return mono directly
        if self.from_rate == self.to_rate {
            return mono;
        }

        // Linear interpolation resampling
        let ratio = self.from_rate as f64 / self.to_rate as f64;
        let input_len = mono.len();
        let estimated_output = (input_len as f64 / ratio) as usize + 2;
        let mut output = Vec::with_capacity(estimated_output);

        let mut pos = self.fractional_pos;
        while (pos as usize) < input_len.saturating_sub(1) {
            let idx = pos as usize;
            let frac = (pos - idx as f64) as f32;
            let sample = mono[idx] * (1.0 - frac) + mono[idx + 1] * frac;
            output.push(sample);
            pos += ratio;
        }

        // Carry over the fractional position for the next call
        self.fractional_pos = pos - input_len as f64;
        if self.fractional_pos < 0.0 {
            self.fractional_pos = 0.0;
        }

        output
    }

    fn to_mono(&self, input: &[f32]) -> Vec<f32> {
        if self.channels == 1 {
            return input.to_vec();
        }

        let frames = input.len() / self.channels;
        let mut mono = Vec::with_capacity(frames);

        for i in 0..frames {
            let start = i * self.channels;
            let sum: f32 = input[start..start + self.channels].iter().sum();
            mono.push(sum / self.channels as f32);
        }

        mono
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mono_passthrough() {
        let mut r = Resampler::new(16000, 16000, 1);
        let input: Vec<f32> = (0..100).map(|i| i as f32 / 100.0).collect();
        let output = r.process(&input);
        assert_eq!(output.len(), input.len());
    }

    #[test]
    fn test_stereo_to_mono() {
        let mut r = Resampler::new(16000, 16000, 2);
        // Stereo: L=1.0 R=0.0, L=0.5 R=0.5
        let input = vec![1.0, 0.0, 0.5, 0.5];
        let output = r.process(&input);
        assert_eq!(output.len(), 2);
        assert!((output[0] - 0.5).abs() < 1e-6);
        assert!((output[1] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn test_downsample() {
        let mut r = Resampler::new(48000, 16000, 1);
        let input: Vec<f32> = (0..4800).map(|i| (i as f32 / 4800.0).sin()).collect();
        let output = r.process(&input);
        // 48000->16000 is 3:1, so ~1600 samples from 4800
        assert!((output.len() as i32 - 1600).abs() <= 2);
    }
}
