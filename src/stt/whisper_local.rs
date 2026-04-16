use async_trait::async_trait;
use std::path::Path;
use std::sync::Mutex;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use super::SttBackend;
use crate::errors::SttError;

pub struct WhisperLocal {
    ctx: Mutex<WhisperContext>,
    threads: i32,
}

impl WhisperLocal {
    /// Create a new WhisperLocal backend from a model file path.
    pub fn new(model_path: &Path, threads: u32) -> Result<Self, SttError> {
        if !model_path.exists() {
            return Err(SttError::ModelNotFound(
                model_path.to_string_lossy().into_owned(),
            ));
        }

        let ctx = WhisperContext::new_with_params(
            model_path.to_str().unwrap_or_default(),
            WhisperContextParameters::default(),
        )
        .map_err(|e| SttError::InferenceFailed(format!("Failed to load model: {}", e)))?;

        let threads = if threads == 0 {
            num_cpus() as i32
        } else {
            threads as i32
        };

        tracing::info!(
            "Whisper model loaded from {}, using {} threads",
            model_path.display(),
            threads
        );

        Ok(Self {
            ctx: Mutex::new(ctx),
            threads,
        })
    }
}

#[async_trait]
impl SttBackend for WhisperLocal {
    fn name(&self) -> &str {
        "whisper-local"
    }

    async fn transcribe(&self, audio: &[f32], language: Option<&str>) -> Result<String, SttError> {
        let threads = self.threads;

        // whisper-rs is blocking, so we do the work in the current context
        // (caller should use spawn_blocking if needed)
        let ctx = self
            .ctx
            .lock()
            .map_err(|e| SttError::InferenceFailed(e.to_string()))?;

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_n_threads(threads);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_suppress_blank(true);
        params.set_suppress_non_speech_tokens(true);

        if let Some(lang) = language {
            params.set_language(Some(lang));
        }

        let mut state = ctx
            .create_state()
            .map_err(|e| SttError::InferenceFailed(format!("Failed to create state: {}", e)))?;

        state
            .full(params, audio)
            .map_err(|e| SttError::InferenceFailed(format!("Inference failed: {}", e)))?;

        let num_segments = state.full_n_segments()
            .map_err(|e| SttError::InferenceFailed(format!("Failed to get segments: {}", e)))?;

        let mut text = String::new();
        for i in 0..num_segments {
            if let Ok(segment) = state.full_get_segment_text(i) {
                text.push_str(&segment);
            }
        }

        let text = text.trim().to_string();
        tracing::debug!("Transcribed {} samples -> '{}' ({} segments)", audio.len(), text, num_segments);

        Ok(text)
    }
}

fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .min(8) // Cap at 8 threads for whisper
}
