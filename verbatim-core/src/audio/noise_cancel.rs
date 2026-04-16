use nnnoiseless::DenoiseState;

/// Denoise 16kHz mono f32 audio using RNNoise.
///
/// Internally upsamples to 48kHz (RNNoise's native rate), processes
/// in 480-sample frames, then downsamples back to 16kHz.
pub fn denoise(samples: &[f32]) -> Vec<f32> {
    if samples.is_empty() {
        return Vec::new();
    }

    let start = std::time::Instant::now();

    // Upsample 16kHz → 48kHz (3x)
    let upsampled = upsample_3x(samples);

    // RNNoise expects f32 in i16 range (-32768..32767)
    let scaled: Vec<f32> = upsampled.iter().map(|&s| s * i16::MAX as f32).collect();

    // Process through RNNoise in FRAME_SIZE (480) sample chunks
    let mut denoiser = DenoiseState::new();
    let frame_size = DenoiseState::FRAME_SIZE;
    let mut output = vec![0.0f32; scaled.len()];
    let mut out_frame = vec![0.0f32; frame_size];

    let full_frames = scaled.len() / frame_size;
    for i in 0..full_frames {
        let offset = i * frame_size;
        denoiser.process_frame(&mut out_frame, &scaled[offset..offset + frame_size]);
        output[offset..offset + frame_size].copy_from_slice(&out_frame);
    }

    // Handle remaining samples by zero-padding
    let remainder = scaled.len() % frame_size;
    if remainder > 0 {
        let offset = full_frames * frame_size;
        let mut padded = vec![0.0f32; frame_size];
        padded[..remainder].copy_from_slice(&scaled[offset..]);
        denoiser.process_frame(&mut out_frame, &padded);
        output[offset..offset + remainder].copy_from_slice(&out_frame[..remainder]);
    }

    // Scale back to normalized [-1.0, 1.0]
    for s in output.iter_mut() {
        *s /= i16::MAX as f32;
    }

    // Downsample 48kHz → 16kHz (pick every 3rd sample)
    let result = downsample_3x(&output);

    tracing::info!(
        input_samples = samples.len(),
        output_samples = result.len(),
        elapsed_ms = start.elapsed().as_millis(),
        "noise cancellation complete"
    );

    result
}

/// Upsample by factor of 3 using linear interpolation.
fn upsample_3x(input: &[f32]) -> Vec<f32> {
    if input.len() < 2 {
        return vec![input.first().copied().unwrap_or(0.0); input.len() * 3];
    }
    let mut out = Vec::with_capacity(input.len() * 3);
    for i in 0..input.len() - 1 {
        let a = input[i];
        let b = input[i + 1];
        out.push(a);
        out.push(a + (b - a) / 3.0);
        out.push(a + (b - a) * 2.0 / 3.0);
    }
    // Last sample
    let last = *input.last().unwrap();
    out.push(last);
    out.push(last);
    out.push(last);
    out
}

/// Downsample by factor of 3 (pick every 3rd sample).
fn downsample_3x(input: &[f32]) -> Vec<f32> {
    input.iter().step_by(3).copied().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_denoise_empty() {
        let result = denoise(&[]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_denoise_preserves_length() {
        // 1 second of silence at 16kHz
        let silence = vec![0.0f32; 16_000];
        let result = denoise(&silence);
        // Should be approximately the same length (minor rounding from 3x up/down)
        assert!((result.len() as i64 - 16_000i64).abs() < 10);
    }

    #[test]
    fn test_upsample_downsample_roundtrip() {
        let input = vec![0.0, 0.5, 1.0, 0.5, 0.0];
        let up = upsample_3x(&input);
        assert_eq!(up.len(), input.len() * 3);
        let down = downsample_3x(&up);
        assert_eq!(down.len(), input.len());
        // First samples should match exactly
        for (a, b) in input.iter().zip(down.iter()) {
            assert!((a - b).abs() < 0.01, "expected {}, got {}", a, b);
        }
    }
}
