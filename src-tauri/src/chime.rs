//! Soft audio chimes played on recording start and stop.
//!
//! Replaces the on-screen overlay indicator (which couldn't reliably appear
//! over other apps' fullscreen Spaces on macOS). Tones are generated in code
//! and played via the default `cpal` output device on a detached thread so
//! the STT pipeline never blocks on audio playback.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

#[derive(Clone, Copy, Debug)]
pub enum Chime {
    /// Played on Idle → Recording. Short, rising, dry.
    Start,
    /// Played on Recording → Processing/Idle. Falling tone with a soft
    /// comb-filter reverb tail to give a "trailing off" feel.
    Stop,
}

/// Fire-and-forget. Spawns a thread that opens the output device, plays the
/// chime, and tears the stream down. Errors are logged, never propagated.
pub fn play(kind: Chime) {
    thread::spawn(move || {
        if let Err(e) = play_blocking(kind) {
            tracing::warn!(error = %e, ?kind, "chime playback failed");
        }
    });
}

fn play_blocking(kind: Chime) -> Result<(), Box<dyn std::error::Error>> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or("no default audio output device")?;
    let supported = device.default_output_config()?;
    let sample_rate = supported.sample_rate().0 as f32;
    let channels = supported.channels() as usize;
    let sample_format = supported.sample_format();
    let stream_config: cpal::StreamConfig = supported.into();

    let samples = match kind {
        Chime::Start => generate_start(sample_rate),
        Chime::Stop => generate_stop(sample_rate),
    };
    let total = samples.len();
    let samples = Arc::new(samples);
    let pos = Arc::new(Mutex::new(0_usize));

    let err_fn = |e| tracing::warn!("chime stream error: {}", e);

    // Build an output stream typed for whichever sample format the device
    // wants. We synthesize as f32 and convert per-format on write.
    let stream = match sample_format {
        cpal::SampleFormat::F32 => {
            let s = samples.clone();
            let p = pos.clone();
            device.build_output_stream(
                &stream_config,
                move |data: &mut [f32], _| write_frames(data, channels, &s, &p, total, |v| v),
                err_fn,
                None,
            )?
        }
        cpal::SampleFormat::I16 => {
            let s = samples.clone();
            let p = pos.clone();
            device.build_output_stream(
                &stream_config,
                move |data: &mut [i16], _| {
                    write_frames(data, channels, &s, &p, total, |v| {
                        (v.clamp(-1.0, 1.0) * i16::MAX as f32) as i16
                    })
                },
                err_fn,
                None,
            )?
        }
        cpal::SampleFormat::U16 => {
            let s = samples.clone();
            let p = pos.clone();
            device.build_output_stream(
                &stream_config,
                move |data: &mut [u16], _| {
                    write_frames(data, channels, &s, &p, total, |v| {
                        let v = (v.clamp(-1.0, 1.0) + 1.0) * 0.5;
                        (v * u16::MAX as f32) as u16
                    })
                },
                err_fn,
                None,
            )?
        }
        other => return Err(format!("unsupported sample format: {:?}", other).into()),
    };

    stream.play()?;

    // Hold the stream open until playback finishes (+small tail).
    let dur_ms = ((total as f32 / sample_rate) * 1000.0) as u64 + 80;
    thread::sleep(Duration::from_millis(dur_ms));
    Ok(())
}

fn write_frames<T: Copy>(
    data: &mut [T],
    channels: usize,
    samples: &Arc<Vec<f32>>,
    pos: &Arc<Mutex<usize>>,
    total: usize,
    convert: impl Fn(f32) -> T,
) {
    let mut p = pos.lock().unwrap();
    for frame in data.chunks_mut(channels) {
        let s = if *p < total { samples[*p] } else { 0.0 };
        let out = convert(s);
        for ch in frame.iter_mut() {
            *ch = out;
        }
        *p += 1;
    }
}

/// Pleasant rising two-tone chime: A4 → E5, ~350ms, exponential decay.
fn generate_start(sr: f32) -> Vec<f32> {
    let dur = 0.35_f32;
    let n = (sr * dur) as usize;
    let f1 = 440.0_f32; // A4
    let f2 = 659.25_f32; // E5
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / sr;
        let env = (-3.0 * t / dur).exp();
        let mix = (t / dur).clamp(0.0, 1.0);
        let s1 = (2.0 * std::f32::consts::PI * f1 * t).sin();
        let s2 = (2.0 * std::f32::consts::PI * f2 * t).sin();
        // Soft attack to avoid a click on the first sample.
        let attack = (i as f32 / (sr * 0.005)).min(1.0);
        out.push((s1 * (1.0 - mix) + s2 * mix) * env * attack * 0.16);
    }
    out
}

/// Falling chime with comb-filter reverb tail. ~600ms total.
fn generate_stop(sr: f32) -> Vec<f32> {
    let total_dur = 0.6_f32;
    let attack_dur = 0.25_f32;
    let n = (sr * total_dur) as usize;
    let attack_n = (sr * attack_dur) as usize;
    let f1 = 659.25_f32; // E5
    let f2 = 440.0_f32; // A4

    let mut dry = vec![0.0_f32; n];
    for i in 0..attack_n.min(n) {
        let t = i as f32 / sr;
        let env = (-4.0 * t / attack_dur).exp();
        let mix = (t / attack_dur).clamp(0.0, 1.0);
        let s1 = (2.0 * std::f32::consts::PI * f1 * t).sin();
        let s2 = (2.0 * std::f32::consts::PI * f2 * t).sin();
        let attack = (i as f32 / (sr * 0.005)).min(1.0);
        dry[i] = (s1 * (1.0 - mix) + s2 * mix) * env * attack * 0.16;
    }

    // Coprime delays + decreasing gains → diffuse, bell-like reverb tail
    // without a dedicated allpass network.
    let mut out = dry.clone();
    let taps: [(f32, f32); 5] = [
        (37.0, 0.45),
        (71.0, 0.32),
        (113.0, 0.22),
        (167.0, 0.15),
        (211.0, 0.10),
    ];
    for (delay_ms, gain) in taps.iter() {
        let d = (delay_ms * sr / 1000.0) as usize;
        if d >= n {
            continue;
        }
        for i in 0..(n - d) {
            out[i + d] += dry[i] * gain;
        }
    }

    // Ensure no clipping after summing taps.
    let peak = out.iter().fold(0.0_f32, |acc, &v| acc.max(v.abs()));
    if peak > 1.0 {
        let scale = 0.95 / peak;
        for v in &mut out {
            *v *= scale;
        }
    }
    out
}
