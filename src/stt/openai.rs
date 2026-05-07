use async_trait::async_trait;
use reqwest::multipart;

use super::SttBackend;
use crate::errors::SttError;

pub struct OpenAiWhisper {
    client: reqwest::Client,
    api_key: String,
    model: String,
}

impl OpenAiWhisper {
    pub fn new(api_key: String, model: String) -> Result<Self, SttError> {
        if api_key.is_empty() {
            return Err(SttError::ApiError(
                "OpenAI API key not configured. Set openai.api_key in config or OPENAI_API_KEY env var.".into(),
            ));
        }

        Ok(Self {
            client: reqwest::Client::new(),
            api_key,
            model,
        })
    }
}

#[async_trait]
impl SttBackend for OpenAiWhisper {
    fn name(&self) -> &str {
        "openai"
    }

    async fn transcribe(&self, audio: &[f32], language: Option<&str>) -> Result<String, SttError> {
        // Encode audio as WAV in memory
        let wav_data = encode_wav(audio)?;

        let file_part = multipart::Part::bytes(wav_data)
            .file_name("audio.wav")
            .mime_str("audio/wav")
            .map_err(|e| SttError::ApiError(e.to_string()))?;

        let mut form = multipart::Form::new()
            .part("file", file_part)
            .text("model", self.model.clone())
            .text("response_format", "text");

        if let Some(lang) = language {
            form = form.text("language", lang.to_string());
        }

        let response = self
            .client
            .post("https://api.openai.com/v1/audio/transcriptions")
            .bearer_auth(&self.api_key)
            .multipart(form)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(SttError::ApiError(format!(
                "OpenAI API error {}: {}",
                status, body
            )));
        }

        let text = response.text().await?;
        let text = text.trim().to_string();

        tracing::debug!("OpenAI transcribed {} samples -> '{}'", audio.len(), text);
        Ok(text)
    }
}

/// Encode f32 PCM samples (16kHz mono) to WAV format in memory.
fn encode_wav(samples: &[f32]) -> Result<Vec<u8>, SttError> {
    let mut cursor = std::io::Cursor::new(Vec::new());

    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 16_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut writer = hound::WavWriter::new(&mut cursor, spec)
        .map_err(|e| SttError::InvalidAudio(e.to_string()))?;

    for &sample in samples {
        let s = (sample * 32767.0).clamp(-32768.0, 32767.0) as i16;
        writer
            .write_sample(s)
            .map_err(|e| SttError::InvalidAudio(e.to_string()))?;
    }

    writer
        .finalize()
        .map_err(|e| SttError::InvalidAudio(e.to_string()))?;

    Ok(cursor.into_inner())
}
