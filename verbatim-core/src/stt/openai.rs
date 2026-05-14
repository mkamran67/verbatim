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
        tracing::debug!(
            model = %model,
            api_key_present = !api_key.is_empty(),
            "creating OpenAI whisper client"
        );
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
        tracing::debug!(
            samples = audio.len(),
            duration_secs = format_args!("{:.1}", audio.len() as f32 / 16000.0),
            language = ?language,
            model = %self.model,
            "starting OpenAI transcription"
        );

        // Encode audio as WAV in memory
        let wav_data = encode_wav(audio)?;
        tracing::debug!(wav_bytes = wav_data.len(), "encoded audio to WAV");

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

        tracing::debug!("sending request to OpenAI API");
        let request_start = std::time::Instant::now();
        let response = self
            .client
            .post("https://api.openai.com/v1/audio/transcriptions")
            .bearer_auth(&self.api_key)
            .multipart(form)
            .send()
            .await?;

        let status = response.status();
        tracing::debug!(
            elapsed_ms = request_start.elapsed().as_millis(),
            status = %status,
            "OpenAI API response received"
        );

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
pub(crate) fn encode_wav(samples: &[f32]) -> Result<Vec<u8>, SttError> {
    tracing::trace!(samples = samples.len(), "encoding f32 samples to 16-bit WAV");
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openai_new_rejects_empty_api_key() {
        let result = OpenAiWhisper::new(String::new(), "whisper-1".into());
        assert!(result.is_err());
        match result {
            Err(SttError::ApiError(msg)) => assert!(msg.contains("API key")),
            _ => panic!("expected ApiError"),
        }
    }

    #[test]
    fn test_openai_new_accepts_valid_key() {
        let result = OpenAiWhisper::new("sk-test123".into(), "whisper-1".into());
        assert!(result.is_ok());
    }

    #[test]
    fn test_openai_name() {
        let backend = OpenAiWhisper::new("sk-test".into(), "whisper-1".into()).unwrap();
        assert_eq!(backend.name(), "openai");
    }

    #[test]
    fn test_encode_wav_valid_header() {
        let samples: Vec<f32> = vec![0.0; 16000]; // 1 second of silence
        let wav = encode_wav(&samples).unwrap();
        // WAV files start with "RIFF"
        assert_eq!(&wav[0..4], b"RIFF");
        // Contains "WAVE" format marker
        assert_eq!(&wav[8..12], b"WAVE");
    }

    #[test]
    fn test_encode_wav_correct_sample_count() {
        let samples: Vec<f32> = vec![0.5; 100];
        let wav = encode_wav(&samples).unwrap();
        // WAV header is 44 bytes, each sample is 2 bytes (16-bit)
        let expected_data_size = 100 * 2;
        let expected_total = 44 + expected_data_size;
        assert_eq!(wav.len(), expected_total);
    }

    #[test]
    fn test_encode_wav_empty_input() {
        let wav = encode_wav(&[]).unwrap();
        // Should still produce a valid WAV header
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(wav.len(), 44); // header only
    }

    // ── Edge case tests ──────────────────────────────────

    #[test]
    fn test_openai_new_whitespace_only_key_accepted() {
        // Whitespace-only key passes is_empty() but will fail at runtime with HTTP 401
        let result = OpenAiWhisper::new("   ".into(), "whisper-1".into());
        assert!(result.is_ok(), "whitespace-only key passes validation (potential bug)");
    }

    #[test]
    fn test_encode_wav_clamps_large_samples() {
        // Samples > 1.0 should be clamped to 32767
        let samples = vec![2.0_f32; 10];
        let wav = encode_wav(&samples).unwrap();
        // Data starts at offset 44, each sample is 2 bytes (little-endian i16)
        let first_sample = i16::from_le_bytes([wav[44], wav[45]]);
        assert_eq!(first_sample, 32767);
    }

    #[test]
    fn test_encode_wav_clamps_negative_samples() {
        let samples = vec![-2.0_f32; 10];
        let wav = encode_wav(&samples).unwrap();
        let first_sample = i16::from_le_bytes([wav[44], wav[45]]);
        assert_eq!(first_sample, -32768);
    }

    #[test]
    fn test_encode_wav_large_input() {
        // 5 minutes of 16kHz audio = 4,800,000 samples
        let samples: Vec<f32> = vec![0.0; 4_800_000];
        let wav = encode_wav(&samples).unwrap();
        assert_eq!(wav.len(), 44 + 4_800_000 * 2);
        assert_eq!(&wav[0..4], b"RIFF");
    }

}
