use async_trait::async_trait;
use std::path::Path;
use std::sync::Mutex;
use whisper_rs::{
    FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState,
};

use super::SttBackend;
use crate::errors::SttError;

pub struct WhisperLocal {
    /// Long-lived state reused across transcriptions.
    /// Kept alive to avoid CUDA resource cleanup conflicts with llama-cpp
    /// when both share the same GPU — dropping WhisperState calls
    /// whisper_free_state() which can segfault during CUDA teardown.
    state: Mutex<WhisperState>,
    threads: i32,
}

impl WhisperLocal {
    /// Create a new WhisperLocal backend from a model file path.
    pub fn new(model_path: &Path, threads: u32) -> Result<Self, SttError> {
        tracing::debug!(
            model_path = %model_path.display(),
            requested_threads = threads,
            "loading whisper model"
        );

        if !model_path.exists() {
            tracing::error!(model_path = %model_path.display(), "model file not found");
            return Err(SttError::ModelNotFound(
                model_path.to_string_lossy().into_owned(),
            ));
        }

        let start = std::time::Instant::now();
        let ctx_params = WhisperContextParameters::default();
        tracing::info!(use_gpu = ctx_params.use_gpu, "creating whisper context");
        let ctx = WhisperContext::new_with_params(
            model_path.to_str().unwrap_or_default(),
            ctx_params,
        )
        .map_err(|e| SttError::InferenceFailed(format!("Failed to load model: {}", e)))?;
        tracing::debug!(elapsed_ms = start.elapsed().as_millis(), "whisper context created");

        let state = ctx
            .create_state()
            .map_err(|e| SttError::InferenceFailed(format!("Failed to create state: {}", e)))?;

        let threads = if threads == 0 {
            let auto = num_cpus() as i32;
            tracing::debug!(auto_detected_threads = auto, "thread count auto-detected (capped at 8)");
            auto
        } else {
            threads as i32
        };

        tracing::info!(
            "Whisper model loaded from {}, using {} threads",
            model_path.display(),
            threads
        );

        Ok(Self {
            state: Mutex::new(state),
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
        tracing::debug!(
            samples = audio.len(),
            duration_secs = format_args!("{:.1}", audio.len() as f32 / 16000.0),
            language = ?language,
            threads = self.threads,
            "starting local whisper transcription"
        );
        let threads = self.threads;

        // whisper-rs is blocking, so we do the work in the current context
        // (caller should use spawn_blocking if needed)
        let mut state = self
            .state
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

        let inference_start = std::time::Instant::now();
        state
            .full(params, audio)
            .map_err(|e| SttError::InferenceFailed(format!("Inference failed: {}", e)))?;

        let num_segments = state.full_n_segments()
            .map_err(|e| SttError::InferenceFailed(format!("Failed to get segments: {}", e)))?;

        tracing::debug!(
            elapsed_ms = inference_start.elapsed().as_millis(),
            segments = num_segments,
            "whisper inference complete"
        );

        let mut text = String::new();
        for i in 0..num_segments {
            if let Ok(segment) = state.full_get_segment_text(i) {
                text.push_str(&segment);
            }
        }

        let text = text.trim().to_string();
        tracing::debug!(
            samples = audio.len(),
            text_len = text.len(),
            segments = num_segments,
            "transcription result: '{}'", text
        );

        Ok(text)
    }
}

fn num_cpus() -> usize {
    let count = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .min(8); // Cap at 8 threads for whisper
    tracing::trace!(count, "detected available CPU parallelism (capped at 8)");
    count
}
