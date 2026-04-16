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
        tracing::debug!(from_rate, to_rate, channels, "resampler created");
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

        tracing::trace!(input_samples = input.len(), output_samples = output.len(), "resampler processed");
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

    #[test]
    fn test_upsample_8k_to_16k() {
        let mut r = Resampler::new(8000, 16000, 1);
        let input: Vec<f32> = (0..800).map(|i| (i as f32 / 800.0).sin()).collect();
        let output = r.process(&input);
        // 8kHz->16kHz is 1:2, so ~1600 samples from 800
        assert!((output.len() as i32 - 1600).abs() <= 2);
    }

    #[test]
    fn test_four_channel_to_mono() {
        let mut r = Resampler::new(16000, 16000, 4);
        // 4 channels: [1.0, 0.0, 0.0, 0.0] -> mono 0.25
        let input = vec![1.0, 0.0, 0.0, 0.0, 0.4, 0.4, 0.4, 0.4];
        let output = r.process(&input);
        assert_eq!(output.len(), 2);
        assert!((output[0] - 0.25).abs() < 1e-6);
        assert!((output[1] - 0.4).abs() < 1e-6);
    }

    #[test]
    fn test_fractional_position_continuity() {
        let mut r = Resampler::new(48000, 16000, 1);
        let input1: Vec<f32> = (0..4800).map(|i| (i as f32 / 4800.0).sin()).collect();
        let input2: Vec<f32> = (4800..9600).map(|i| (i as f32 / 4800.0).sin()).collect();
        let out1 = r.process(&input1);
        let out2 = r.process(&input2);
        // Total output should be ~3200 samples (9600 / 3)
        let total = out1.len() + out2.len();
        assert!((total as i32 - 3200).abs() <= 4);
    }

    #[test]
    fn test_empty_input_returns_empty() {
        let mut r = Resampler::new(48000, 16000, 1);
        let output = r.process(&[]);
        assert!(output.is_empty());
    }

    #[test]
    fn test_single_sample_passthrough() {
        let mut r = Resampler::new(16000, 16000, 1);
        let output = r.process(&[0.5]);
        assert_eq!(output.len(), 1);
        assert!((output[0] - 0.5).abs() < 1e-6);
    }

    // ── Edge case & boundary tests ──────────────────────────────────

    #[test]
    fn test_single_sample_with_actual_resampling() {
        // When from_rate != to_rate, a single sample produces zero output
        // because the loop condition `pos < input_len.saturating_sub(1)` requires at least 2 samples
        let mut r = Resampler::new(48000, 16000, 1);
        let output = r.process(&[0.5]);
        assert!(output.is_empty(), "single sample with rate conversion produces no output");
    }

    #[test]
    fn test_nan_samples_propagate() {
        let mut r = Resampler::new(48000, 16000, 1);
        let input = vec![f32::NAN, 0.0, 0.0, 0.0, 0.0, 0.0];
        let output = r.process(&input);
        // Should not panic; NaN propagates through interpolation
        assert!(!output.is_empty());
    }

    #[test]
    fn test_infinity_samples_propagate() {
        let mut r = Resampler::new(48000, 16000, 1);
        let input = vec![f32::INFINITY, 0.0, 0.0, 0.0, 0.0, 0.0];
        let output = r.process(&input);
        // Should not panic
        assert!(!output.is_empty());
    }

    #[test]
    fn test_extreme_downsample_192k_to_16k() {
        let mut r = Resampler::new(192000, 16000, 1);
        let input: Vec<f32> = (0..19200).map(|i| (i as f32 / 19200.0).sin()).collect();
        let output = r.process(&input);
        // 192kHz -> 16kHz is 12:1 ratio, so ~1600 from 19200
        assert!((output.len() as i32 - 1600).abs() <= 2);
    }

    #[test]
    fn test_extreme_upsample_1k_to_16k() {
        let mut r = Resampler::new(1000, 16000, 1);
        let input: Vec<f32> = (0..100).map(|i| (i as f32 / 100.0).sin()).collect();
        let output = r.process(&input);
        // 1kHz -> 16kHz is 1:16 ratio, so ~1584 from 99 usable intervals (100-1 samples * 16)
        assert!(output.len() > 1500 && output.len() < 1700,
            "expected ~1584 samples, got {}", output.len());
    }

    #[test]
    #[should_panic]
    fn test_zero_channels_panics() {
        // Division by zero in to_mono when channels == 0
        let mut r = Resampler::new(16000, 16000, 0);
        r.process(&[1.0]);
    }

    #[test]
    fn test_large_buffer_no_panic() {
        // 10 seconds of 48kHz stereo = 960,000 samples
        let mut r = Resampler::new(48000, 16000, 2);
        let input: Vec<f32> = (0..960_000).map(|i| (i as f32 / 48000.0).sin()).collect();
        let output = r.process(&input);
        // 480k mono frames, 3:1 ratio -> ~160k output
        assert!((output.len() as i32 - 160_000).abs() < 100);
    }

    #[test]
    fn test_interpolation_within_bounds() {
        // All input samples within [-1, 1] => output should stay within [-1, 1]
        let mut r = Resampler::new(48000, 16000, 1);
        let input: Vec<f32> = (0..4800)
            .map(|i| (i as f32 * 2.0 * std::f32::consts::PI / 4800.0).sin())
            .collect();
        let output = r.process(&input);
        for &s in &output {
            assert!(s >= -1.0 && s <= 1.0, "sample {} out of bounds", s);
        }
    }
}
