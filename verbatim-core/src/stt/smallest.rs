use async_trait::async_trait;
use serde::Deserialize;

use super::openai::encode_wav;
use super::SttBackend;
use crate::errors::SttError;

pub struct SmallestStt {
    client: reqwest::Client,
    api_key: String,
}

impl SmallestStt {
    pub fn new(api_key: String) -> Result<Self, SttError> {
        tracing::debug!(
            api_key_present = !api_key.is_empty(),
            "creating Smallest STT client"
        );
        if api_key.is_empty() {
            return Err(SttError::ApiError(
                "Smallest API key not configured. Set smallest.api_key in config or via the API Keys page.".into(),
            ));
        }

        Ok(Self {
            client: reqwest::Client::new(),
            api_key,
        })
    }
}

/// Map our internal language codes (BCP-47-ish, plus "auto") to a language
/// value accepted by the Smallest Pulse API. Codes outside Smallest's
/// supported set fall back to `multi` (full multilingual auto-detect).
fn map_language(lang: Option<&str>) -> &'static str {
    let Some(lang) = lang else { return "multi"; };
    match lang {
        "auto" | "" => "multi",
        "it" => "it",
        "es" => "es",
        "en" => "en",
        "pt" => "pt",
        "hi" => "hi",
        "de" => "de",
        "fr" => "fr",
        "uk" => "uk",
        "ru" => "ru",
        "kn" => "kn",
        "ml" => "ml",
        "pl" => "pl",
        "mr" => "mr",
        "gu" => "gu",
        "cs" => "cs",
        "sk" => "sk",
        "te" => "te",
        "or" => "or",
        "nl" => "nl",
        "bn" => "bn",
        "lv" => "lv",
        "et" => "et",
        "ro" => "ro",
        "pa" => "pa",
        "fi" => "fi",
        "sv" => "sv",
        "bg" => "bg",
        "ta" => "ta",
        "hu" => "hu",
        "da" => "da",
        "lt" => "lt",
        "mt" => "mt",
        _ => "multi",
    }
}

#[async_trait]
impl SttBackend for SmallestStt {
    fn name(&self) -> &str {
        "smallest"
    }

    async fn transcribe(&self, audio: &[f32], language: Option<&str>) -> Result<String, SttError> {
        tracing::debug!(
            samples = audio.len(),
            duration_secs = format_args!("{:.1}", audio.len() as f32 / 16000.0),
            language = ?language,
            "starting Smallest transcription"
        );

        let wav_data = encode_wav(audio)?;
        tracing::debug!(wav_bytes = wav_data.len(), "encoded audio to WAV");

        let lang_param = map_language(language);
        let query_params = vec![("language", lang_param.to_string())];

        tracing::debug!(language = %lang_param, "sending request to Smallest API");
        let request_start = std::time::Instant::now();
        let response = self
            .client
            .post("https://api.smallest.ai/waves/v1/pulse/get_text")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/octet-stream")
            .query(&query_params)
            .body(wav_data)
            .send()
            .await?;

        let status = response.status();
        tracing::debug!(
            elapsed_ms = request_start.elapsed().as_millis(),
            status = %status,
            "Smallest API response received"
        );

        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(SttError::ApiError(format!(
                "Smallest API error {}: {}",
                status, body
            )));
        }

        let sm_response: SmallestResponse = response
            .json()
            .await
            .map_err(|e| SttError::ApiError(format!("Failed to parse Smallest response: {}", e)))?;

        let text = sm_response.transcription.trim().to_string();

        tracing::debug!(
            audio_length = ?sm_response.audio_length,
            "Smallest transcribed {} samples -> '{}'",
            audio.len(),
            text
        );
        Ok(text)
    }
}

// ── Smallest API Response Types ─────────────────────────────────────

#[derive(Debug, Deserialize)]
struct SmallestResponse {
    #[serde(default)]
    transcription: String,
    #[serde(default)]
    audio_length: Option<f64>,
}

// Smallest does not publish a public REST endpoint for credit balance —
// `/atoms/v1/user` returns only profile fields, and the Atoms API reference
// has no billing endpoints. Balance tracking lives in the UI as a manual
// balance + estimated USD spend (Pulse STT priced at $0.005/min).

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_smallest_new_rejects_empty_api_key() {
        let result = SmallestStt::new(String::new());
        assert!(result.is_err());
        match result {
            Err(SttError::ApiError(msg)) => assert!(msg.contains("API key")),
            _ => panic!("expected ApiError"),
        }
    }

    #[test]
    fn test_smallest_new_accepts_valid_key() {
        let result = SmallestStt::new("sm-test123".into());
        assert!(result.is_ok());
    }

    #[test]
    fn test_smallest_name() {
        let backend = SmallestStt::new("sm-test".into()).unwrap();
        assert_eq!(backend.name(), "smallest");
    }

    #[test]
    fn test_map_language_none_to_multi() {
        assert_eq!(map_language(None), "multi");
    }

    #[test]
    fn test_map_language_auto_to_multi() {
        assert_eq!(map_language(Some("auto")), "multi");
    }

    #[test]
    fn test_map_language_passthrough_supported() {
        assert_eq!(map_language(Some("en")), "en");
        assert_eq!(map_language(Some("de")), "de");
        assert_eq!(map_language(Some("fr")), "fr");
    }

    #[test]
    fn test_map_language_unsupported_falls_back_to_multi() {
        // Smallest doesn't support these in its enum — must fall back to multi.
        assert_eq!(map_language(Some("ja")), "multi");
        assert_eq!(map_language(Some("zh")), "multi");
        assert_eq!(map_language(Some("ko")), "multi");
        assert_eq!(map_language(Some("ar")), "multi");
    }

    #[test]
    fn test_smallest_response_parsing() {
        let json = r#"{"status":"ok","transcription":"hello world","audio_length":1.5}"#;
        let parsed: SmallestResponse = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.transcription, "hello world");
        assert_eq!(parsed.audio_length, Some(1.5));
    }
}
