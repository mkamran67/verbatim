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
        taken
    }

    /// Clear the buffer without taking ownership.
    pub fn clear(&self) {
        self.inner.lock().unwrap().clear();
    }
}

/// Find the preferred input device.
pub fn get_input_device(device_name: &str) -> Result<Device, AudioError> {
    let host = cpal::default_host();

    if device_name.is_empty() {
        host.default_input_device()
            .ok_or(AudioError::NoInputDevice)
    } else {
        let devices = host
            .input_devices()
            .map_err(|e| AudioError::DeviceError(e.to_string()))?;

        for device in devices {
            if let Ok(name) = device.name() {
                if name.contains(device_name) {
                    return Ok(device);
                }
            }
        }

        Err(AudioError::DeviceError(format!(
            "Input device '{}' not found",
            device_name
        )))
    }
}

/// List available input device names.
pub fn list_input_devices() -> Vec<String> {
    let host = cpal::default_host();
    match host.input_devices() {
        Ok(devices) => devices.filter_map(|d| d.name().ok()).collect(),
        Err(_) => Vec::new(),
    }
}

/// Start a stream that only monitors input level (no buffering/resampling).
/// Returns the Stream (must be kept alive) and an Arc<AtomicU32> where
/// the current RMS level is stored as f32 bits (0.0..1.0 range, clamped).
pub fn start_level_monitor(device: &Device) -> Result<(Stream, Arc<AtomicU32>)> {
    let supported = device
        .default_input_config()
        .map_err(|e| AudioError::DeviceError(e.to_string()))?;

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

    stream.play().context("Failed to start audio stream")?;

    Ok(stream)
}
