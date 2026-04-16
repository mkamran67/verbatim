use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, SampleRate, Stream, StreamConfig};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::watch;

use super::resampler::Resampler;
use crate::errors::AudioError;

/// Target sample rate for whisper.cpp.
pub const TARGET_SAMPLE_RATE: u32 = 16_000;

/// Pre-allocated capacity for ~60 seconds of 16kHz mono audio.
const BUFFER_CAPACITY: usize = TARGET_SAMPLE_RATE as usize * 60;

/// Shared audio buffer that the cpal callback writes to.
pub struct AudioBuffer {
    inner: Arc<Mutex<Vec<f32>>>,
}

impl AudioBuffer {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Vec::with_capacity(BUFFER_CAPACITY))),
        }
    }

    /// Get a clone of the Arc for sharing with the cpal callback.
    pub fn shared(&self) -> Arc<Mutex<Vec<f32>>> {
        Arc::clone(&self.inner)
    }

    /// Take all accumulated samples, leaving an empty pre-allocated buffer.
    pub fn take(&self) -> Vec<f32> {
        let mut buf = self.inner.lock().unwrap();
        let mut taken = Vec::with_capacity(BUFFER_CAPACITY);
        std::mem::swap(&mut *buf, &mut taken);
        tracing::debug!(samples = taken.len(), duration_secs = format_args!("{:.2}", taken.len() as f32 / 16000.0), "took samples from audio buffer");
        taken
    }

    /// Clear the buffer without taking ownership.
    pub fn clear(&self) {
        tracing::trace!("clearing audio buffer");
        self.inner.lock().unwrap().clear();
    }
}

/// Find the preferred input device.
pub fn get_input_device(device_name: &str) -> Result<Device, AudioError> {
    tracing::debug!(device_name, "looking for input device");
    let host = cpal::default_host();

    if device_name.is_empty() {
        tracing::debug!("no device name specified, using default input device");
        let device = host.default_input_device()
            .ok_or(AudioError::NoInputDevice)?;
        if let Ok(name) = device.name() {
            tracing::debug!(device = %name, "using default input device");
        }
        Ok(device)
    } else {
        let devices = host
            .input_devices()
            .map_err(|e| AudioError::DeviceError(e.to_string()))?;

        for device in devices {
            if let Ok(name) = device.name() {
                tracing::trace!(name = %name, "checking device");
                if name.contains(device_name) {
                    tracing::debug!(device = %name, "found matching input device");
                    return Ok(device);
                }
            }
        }

        tracing::warn!(device_name, "input device not found");
        Err(AudioError::DeviceError(format!(
            "Input device '{}' not found",
            device_name
        )))
    }
}

/// List available input device names.
pub fn list_input_devices() -> Vec<String> {
    let host = cpal::default_host();
    let result = match host.input_devices() {
        Ok(devices) => devices.filter_map(|d| d.name().ok()).collect(),
        Err(e) => {
            tracing::warn!("failed to enumerate input devices: {}", e);
            Vec::new()
        }
    };
    tracing::debug!(count = result.len(), "found input devices");
    result
}

/// Start a stream that only monitors input level (no buffering/resampling).
/// Returns the Stream (must be kept alive) and an Arc<AtomicU32> where
/// the current RMS level is stored as f32 bits (0.0..1.0 range, clamped).
pub fn start_level_monitor(device: &Device) -> Result<(Stream, Arc<AtomicU32>)> {
    let supported = device
        .default_input_config()
        .map_err(|e| AudioError::DeviceError(e.to_string()))?;

    tracing::debug!(
        sample_rate = supported.sample_rate().0,
        channels = supported.channels(),
        "starting level monitor"
    );

    let config = StreamConfig {
        channels: supported.channels(),
        sample_rate: supported.sample_rate(),
        buffer_size: cpal::BufferSize::Default,
    };

    let level = Arc::new(AtomicU32::new(0));
    let level_writer = Arc::clone(&level);

    let stream = device
        .build_input_stream(
            &config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                if data.is_empty() {
                    return;
                }
                let sum_sq: f32 = data.iter().map(|s| s * s).sum();
                let rms = (sum_sq / data.len() as f32).sqrt().min(1.0);
                level_writer.store(rms.to_bits(), Ordering::Relaxed);
            },
            |err| {
                tracing::error!("Level monitor stream error: {}", err);
            },
            None,
        )
        .context("Failed to build level monitor stream")?;

    stream.play().context("Failed to start level monitor")?;

    Ok((stream, level))
}

/// Start capturing audio from the given device.
/// Samples are resampled to 16kHz mono and appended to the buffer
/// only while `recording_rx` is `true`.
pub fn start_capture(
    device: &Device,
    buffer: &AudioBuffer,
    recording_rx: watch::Receiver<bool>,
) -> Result<Stream> {
    let supported = device
        .default_input_config()
        .map_err(|e| AudioError::DeviceError(e.to_string()))?;

    let sample_rate = supported.sample_rate().0;
    let channels = supported.channels() as usize;

    tracing::info!(
        "Audio input: {}Hz, {} channels, format: {:?}",
        sample_rate,
        channels,
        supported.sample_format()
    );

    let config = StreamConfig {
        channels: supported.channels(),
        sample_rate: SampleRate(sample_rate),
        buffer_size: cpal::BufferSize::Default,
    };

    let shared_buf = buffer.shared();
    let resampler = Arc::new(Mutex::new(Resampler::new(sample_rate, TARGET_SAMPLE_RATE, channels)));

    let stream = device
        .build_input_stream(
            &config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                if !*recording_rx.borrow() {
                    return;
                }

                let resampled = resampler.lock().unwrap().process(data);

                if let Ok(mut buf) = shared_buf.lock() {
                    buf.extend_from_slice(&resampled);
                }
            },
            |err| {
                tracing::error!("Audio stream error: {}", err);
            },
            None,
        )
        .context("Failed to build input stream")?;
    tracing::debug!("audio capture stream built");

    stream.play().context("Failed to start audio stream")?;
    tracing::debug!("audio capture stream playing");

    Ok(stream)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_buffer_new_is_empty() {
        let buf = AudioBuffer::new();
        let taken = buf.take();
        assert!(taken.is_empty());
    }

    #[test]
    fn test_audio_buffer_write_and_take() {
        let buf = AudioBuffer::new();
        let shared = buf.shared();
        {
            let mut inner = shared.lock().unwrap();
            inner.extend_from_slice(&[0.1, 0.2, 0.3]);
        }
        let taken = buf.take();
        assert_eq!(taken.len(), 3);
        assert!((taken[0] - 0.1).abs() < 1e-6);
        assert!((taken[1] - 0.2).abs() < 1e-6);
        assert!((taken[2] - 0.3).abs() < 1e-6);
    }

    #[test]
    fn test_audio_buffer_take_clears() {
        let buf = AudioBuffer::new();
        let shared = buf.shared();
        {
            let mut inner = shared.lock().unwrap();
            inner.extend_from_slice(&[1.0, 2.0]);
        }
        let _ = buf.take();
        let taken_again = buf.take();
        assert!(taken_again.is_empty());
    }

    #[test]
    fn test_audio_buffer_clear() {
        let buf = AudioBuffer::new();
        let shared = buf.shared();
        {
            let mut inner = shared.lock().unwrap();
            inner.extend_from_slice(&[1.0, 2.0]);
        }
        buf.clear();
        let taken = buf.take();
        assert!(taken.is_empty());
    }

    // ── Edge case tests ──────────────────────────────────

    #[test]
    fn test_audio_buffer_concurrent_write_and_take() {
        let buf = AudioBuffer::new();
        let shared = buf.shared();

        let writer_shared = shared.clone();
        let handle = std::thread::spawn(move || {
            for _ in 0..100 {
                let mut inner = writer_shared.lock().unwrap();
                inner.extend_from_slice(&[0.5; 100]);
            }
        });

        let mut total_samples = 0;
        // Take while writer is running
        for _ in 0..50 {
            let taken = buf.take();
            total_samples += taken.len();
            std::thread::yield_now();
        }

        handle.join().unwrap();
        // Drain remaining
        total_samples += buf.take().len();
        assert_eq!(total_samples, 10_000, "all 100*100 samples should be accounted for");
    }

    #[test]
    fn test_audio_buffer_large_write() {
        let buf = AudioBuffer::new();
        let shared = buf.shared();
        // 2 minutes of 16kHz audio
        let samples: Vec<f32> = vec![0.1; 16000 * 120];
        {
            let mut inner = shared.lock().unwrap();
            inner.extend_from_slice(&samples);
        }
        let taken = buf.take();
        assert_eq!(taken.len(), 16000 * 120);
    }

    #[test]
    fn test_audio_buffer_multiple_takes_interleaved() {
        let buf = AudioBuffer::new();
        let shared = buf.shared();

        // Write first batch
        {
            let mut inner = shared.lock().unwrap();
            inner.extend_from_slice(&[1.0, 2.0, 3.0]);
        }
        let first = buf.take();
        assert_eq!(first.len(), 3);

        // Write second batch
        {
            let mut inner = shared.lock().unwrap();
            inner.extend_from_slice(&[4.0, 5.0]);
        }
        let second = buf.take();
        assert_eq!(second.len(), 2);
        assert!((second[0] - 4.0).abs() < 1e-6);
    }

    #[test]
    fn test_audio_buffer_take_preserves_capacity() {
        let buf = AudioBuffer::new();
        let shared = buf.shared();

        // Write and take
        {
            let mut inner = shared.lock().unwrap();
            inner.extend_from_slice(&[0.0; 1000]);
        }
        let _ = buf.take();

        // Writing after take should still work efficiently
        {
            let mut inner = shared.lock().unwrap();
            inner.extend_from_slice(&[0.0; 500]);
        }
        let taken = buf.take();
        assert_eq!(taken.len(), 500);
    }
}
