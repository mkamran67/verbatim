//! Fast voiced-content gate.
//!
//! The previous silence check compared global peak/RMS against fixed floors.
//! That misses the common failure mode of a short clip containing one stray
//! click, breath pop, or keyboard tap — a single high-amplitude sample
//! satisfies the gate, so the buffer goes to STT and the model hallucinates
//! ("Thanks for watching!", "Bye.").
//!
//! Instead, we walk the buffer in fixed-size frames, count how many frames
//! are above a voiced-energy floor, and require both an absolute amount of
//! voiced audio *and* a minimum fraction of the clip — with the fraction
//! tightening for shorter clips. Single-pass, no allocations, no deps.

/// Frame size in milliseconds. 20 ms is the standard VAD frame length.
const FRAME_MS: u32 = 20;

/// Per-frame RMS floor that counts as "voiced".
///
/// 0.005 is ~-46 dBFS. Comfortably above typical room/mic noise on consumer
/// hardware, comfortably below quiet speech. A single sample click cannot
/// hold a *frame's* worth of RMS above this floor, which is the point.
const VOICED_RMS_FLOOR: f32 = 0.005;

/// Require at least this much cumulative voiced audio. Shorter than the
/// shortest plausible monosyllable, but long enough to reject isolated
/// transients (clicks, taps, pops) that span only a frame or two.
const MIN_VOICED_MS: u32 = 120;

/// Floor on the voiced fraction of the clip.
const MIN_VOICED_FRACTION: f32 = 0.08;

/// For very short clips we need a higher voiced fraction. Expressed as
/// "must have at least this many ms voiced relative to total length".
const SHORT_CLIP_VOICED_MS_FLOOR: f32 = 100.0;

/// Returns true if the buffer contains a plausible amount of voiced audio.
///
/// Designed to be cheap (single linear pass over the samples) and to catch
/// the cases the global peak/RMS gate misses: short clips with one stray
/// click, mostly-silent buffers with a small noise spike, etc.
pub fn has_voiced_content(samples: &[f32], sample_rate: u32) -> bool {
    if samples.is_empty() || sample_rate == 0 {
        return false;
    }

    let frame_len = ((sample_rate * FRAME_MS) / 1000) as usize;
    if frame_len == 0 || samples.len() < frame_len {
        // Clip shorter than one frame — too short to make a confident call,
        // but check global RMS as a fallback so we don't accept dead silence.
        let sum_sq: f32 = samples.iter().map(|s| s * s).sum();
        let rms = (sum_sq / samples.len() as f32).sqrt();
        return rms >= VOICED_RMS_FLOOR;
    }

    let floor_sq = VOICED_RMS_FLOOR * VOICED_RMS_FLOOR;
    let inv_frame_len = 1.0 / frame_len as f32;

    let mut voiced_frames: u32 = 0;
    let mut total_frames: u32 = 0;

    for frame in samples.chunks_exact(frame_len) {
        let sum_sq: f32 = frame.iter().map(|s| s * s).sum();
        let mean_sq = sum_sq * inv_frame_len;
        if mean_sq >= floor_sq {
            voiced_frames += 1;
        }
        total_frames += 1;
    }

    if total_frames == 0 {
        return false;
    }

    let voiced_ms = voiced_frames * FRAME_MS;
    let total_ms = total_frames * FRAME_MS;
    let voiced_fraction = voiced_frames as f32 / total_frames as f32;

    // For short clips, tighten the fraction requirement so a single voiced
    // frame in an otherwise silent 300 ms buffer doesn't pass.
    let required_fraction =
        MIN_VOICED_FRACTION.max(SHORT_CLIP_VOICED_MS_FLOOR / total_ms as f32);

    voiced_ms >= MIN_VOICED_MS && voiced_fraction >= required_fraction
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: u32 = 16_000;

    fn silence(secs: f32) -> Vec<f32> {
        vec![0.0; (SR as f32 * secs) as usize]
    }

    fn tone(secs: f32, freq: f32, amp: f32) -> Vec<f32> {
        let n = (SR as f32 * secs) as usize;
        (0..n)
            .map(|i| amp * (2.0 * std::f32::consts::PI * freq * i as f32 / SR as f32).sin())
            .collect()
    }

    #[test]
    fn rejects_all_zero() {
        assert!(!has_voiced_content(&silence(1.0), SR));
    }

    #[test]
    fn rejects_empty() {
        assert!(!has_voiced_content(&[], SR));
    }

    #[test]
    fn rejects_single_click_in_silence() {
        let mut buf = silence(1.0);
        // One-sample spike at amplitude 1.0 inside a second of silence.
        buf[SR as usize / 2] = 1.0;
        assert!(!has_voiced_content(&buf, SR));
    }

    #[test]
    fn rejects_short_burst_of_noise() {
        // ~5 ms of loud noise inside 1 s of silence — under MIN_VOICED_MS.
        let mut buf = silence(1.0);
        let start = SR as usize / 2;
        let burst_len = (SR as f32 * 0.005) as usize;
        for s in &mut buf[start..start + burst_len] {
            *s = 0.5;
        }
        assert!(!has_voiced_content(&buf, SR));
    }

    #[test]
    fn accepts_200ms_tone() {
        assert!(has_voiced_content(&tone(0.2, 440.0, 0.1), SR));
    }

    #[test]
    fn accepts_speech_like_low_amplitude_tone() {
        // -40 dBFS sustained tone — quiet but well above ambient.
        assert!(has_voiced_content(&tone(0.5, 220.0, 0.01), SR));
    }

    #[test]
    fn rejects_100ms_tone_in_longer_silent_clip() {
        // 100 ms of voiced inside a 1 s buffer — voiced_ms < MIN_VOICED_MS.
        let mut buf = silence(1.0);
        let voiced = tone(0.1, 440.0, 0.1);
        buf[..voiced.len()].copy_from_slice(&voiced);
        assert!(!has_voiced_content(&buf, SR));
    }

    #[test]
    fn accepts_200ms_tone_in_500ms_clip() {
        // Voiced fraction = 0.4, voiced_ms = 200 — passes both gates.
        let mut buf = silence(0.5);
        let voiced = tone(0.2, 440.0, 0.1);
        buf[..voiced.len()].copy_from_slice(&voiced);
        assert!(has_voiced_content(&buf, SR));
    }
}
