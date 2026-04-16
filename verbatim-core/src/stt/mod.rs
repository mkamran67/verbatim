pub mod whisper_local;
pub mod openai;
pub mod deepgram;

use async_trait::async_trait;
use crate::errors::SttError;

#[async_trait]
pub trait SttBackend: Send + Sync {
    /// Human-readable name for logging/UI.
    fn name(&self) -> &str;

    /// Transcribe a completed audio buffer.
    /// `audio`: 16kHz mono f32 PCM samples.
    async fn transcribe(&self, audio: &[f32], language: Option<&str>) -> Result<String, SttError>;

    /// Whether this backend supports streaming partial results.
    #[allow(dead_code)]
    fn supports_streaming(&self) -> bool {
        false
    }
}
