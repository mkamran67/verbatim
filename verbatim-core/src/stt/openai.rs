use async_trait::async_trait;
use reqwest::multipart;
use serde::Deserialize;

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

// ── OpenAI Costs API ────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OpenAiCosts {
    pub total_cost_usd: f64,
    pub days_queried: i64,
}

#[derive(Debug, Deserialize)]
struct CostsPage {
    data: Vec<CostsBucket>,
}

#[derive(Debug, Deserialize)]
struct CostsBucket {
    results: Vec<CostsResult>,
}

#[derive(Debug, Deserialize)]
struct CostsResult {
    amount: CostsAmount,
}

#[derive(Debug, Deserialize)]
struct CostsAmount {
    #[serde(deserialize_with = "deserialize_string_or_f64")]
    value: f64,
}

fn deserialize_string_or_f64<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct StringOrF64;
    impl<'de> serde::de::Visitor<'de> for StringOrF64 {
        type Value = f64;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("a number or numeric string")
        }
        fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<f64, E> {
            Ok(v)
        }
        fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<f64, E> {
            Ok(v as f64)
        }
        fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<f64, E> {
            Ok(v as f64)
        }
        fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<f64, E> {
            v.parse::<f64>().map_err(serde::de::Error::custom)
        }
    }
    deserializer.deserialize_any(StringOrF64)
}

pub async fn check_costs(admin_key: &str) -> Result<OpenAiCosts, SttError> {
    if admin_key.is_empty() {
        return Err(SttError::ApiError(
            "OpenAI Admin key not configured. Add it in API Keys settings.".into(),
        ));
    }

    let client = reqwest::Client::new();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let thirty_days_ago = now - 30 * 24 * 60 * 60;

    let resp = client
        .get("https://api.openai.com/v1/organization/costs")
        .bearer_auth(admin_key)
        .query(&[
            ("start_time", thirty_days_ago.to_string()),
            ("limit", "31".to_string()),
        ])
        .send()
        .await?;

    if resp.status() == reqwest::StatusCode::FORBIDDEN {
        return Err(SttError::ApiError(
            "Cost check unavailable — your Admin API key lacks billing permissions. Check your usage at platform.openai.com/usage.".into(),
        ));
    }

    let body = resp
        .error_for_status()
        .map_err(|e| SttError::ApiError(format!("Failed to get OpenAI costs: {}", e)))?
        .text()
        .await
        .map_err(|e| SttError::ApiError(format!("Failed to read OpenAI costs response: {}", e)))?;

    tracing::debug!("OpenAI costs raw response: {}", body);

    let page: CostsPage = serde_json::from_str(&body)
        .map_err(|e| SttError::ApiError(format!("Failed to parse OpenAI costs response: {} — body: {}", e, body)))?;

    let total: f64 = page
        .data
        .iter()
        .flat_map(|bucket| &bucket.results)
        .map(|r| r.amount.value)
        .sum();

    Ok(OpenAiCosts {
        total_cost_usd: total,
        days_queried: 30,
    })
}

// ── OpenAI Credit Grants (legacy /dashboard endpoint) ──────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OpenAiCreditGrants {
    pub total_granted: f64,
    pub total_used: f64,
    pub total_available: f64,
}

#[derive(Debug, Deserialize)]
struct CreditGrantsResponse {
    #[serde(deserialize_with = "deserialize_string_or_f64")]
    total_granted: f64,
    #[serde(deserialize_with = "deserialize_string_or_f64")]
    total_used: f64,
    #[serde(deserialize_with = "deserialize_string_or_f64")]
    total_available: f64,
}

/// Try to fetch the remaining credit balance via OpenAI's legacy
/// `/dashboard/billing/credit_grants` endpoint. This is undocumented and only
/// works for prepaid-credit accounts; pay-as-you-go accounts typically get
/// 401/403/404. Errors are prefixed with "credit_grants_unavailable:" so callers
/// can fall back to a different signal (e.g. last-30-days spend).
pub async fn check_credit_grants(admin_key: &str) -> Result<OpenAiCreditGrants, SttError> {
    if admin_key.is_empty() {
        return Err(SttError::ApiError(
            "credit_grants_unavailable: OpenAI Admin key not configured.".into(),
        ));
    }

    let resp = reqwest::Client::new()
        .get("https://api.openai.com/dashboard/billing/credit_grants")
        .bearer_auth(admin_key)
        .send()
        .await
        .map_err(|e| SttError::ApiError(format!("credit_grants_unavailable: {}", e)))?;

    let status = resp.status();
    if !status.is_success() {
        return Err(SttError::ApiError(format!(
            "credit_grants_unavailable: HTTP {}",
            status.as_u16()
        )));
    }

    let body = resp
        .text()
        .await
        .map_err(|e| SttError::ApiError(format!("credit_grants_unavailable: {}", e)))?;

    let parsed: CreditGrantsResponse = serde_json::from_str(&body).map_err(|e| {
        SttError::ApiError(format!(
            "credit_grants_unavailable: parse error {} — body: {}",
            e, body
        ))
    })?;

    Ok(OpenAiCreditGrants {
        total_granted: parsed.total_granted,
        total_used: parsed.total_used,
        total_available: parsed.total_available,
    })
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

    #[test]
    fn test_check_costs_rejects_empty_key() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(check_costs(""));
        assert!(result.is_err());
        match result {
            Err(SttError::ApiError(msg)) => assert!(msg.contains("Admin key not configured")),
            _ => panic!("expected ApiError"),
        }
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

    #[test]
    fn test_costs_amount_deserialize_string_value() {
        let json = r#"{"value": "0.0001690500000000000000000000000", "currency": "usd"}"#;
        let amount: CostsAmount = serde_json::from_str(json).unwrap();
        assert!((amount.value - 0.00016905).abs() < 1e-10);
    }

    #[test]
    fn test_costs_amount_deserialize_numeric_value() {
        let json = r#"{"value": 1.25, "currency": "usd"}"#;
        let amount: CostsAmount = serde_json::from_str(json).unwrap();
        assert!((amount.value - 1.25).abs() < f64::EPSILON);
    }

    #[test]
    fn test_check_costs_whitespace_only_key() {
        // Whitespace-only key passes is_empty() check, documents this behavior
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(check_costs("   "));
        // It doesn't reject whitespace-only keys at validation — it will try the API call
        assert!(result.is_err(), "whitespace key should fail at API level");
    }

    #[test]
    fn test_check_credit_grants_rejects_empty_key() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(check_credit_grants(""));
        assert!(result.is_err());
        match result {
            Err(SttError::ApiError(msg)) => {
                assert!(msg.starts_with("credit_grants_unavailable:"));
            }
            _ => panic!("expected ApiError"),
        }
    }

    #[test]
    fn test_credit_grants_response_parses_numeric_fields() {
        let json = r#"{
            "object": "credit_summary",
            "total_granted": 100.0,
            "total_used": 25.5,
            "total_available": 74.5,
            "grants": {"data": []}
        }"#;
        let parsed: CreditGrantsResponse = serde_json::from_str(json).unwrap();
        assert!((parsed.total_granted - 100.0).abs() < f64::EPSILON);
        assert!((parsed.total_used - 25.5).abs() < f64::EPSILON);
        assert!((parsed.total_available - 74.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_credit_grants_response_parses_string_fields() {
        // Defend against OpenAI returning numeric values as strings (as costs API does)
        let json = r#"{
            "total_granted": "100.0",
            "total_used": "25.5",
            "total_available": "74.5"
        }"#;
        let parsed: CreditGrantsResponse = serde_json::from_str(json).unwrap();
        assert!((parsed.total_available - 74.5).abs() < f64::EPSILON);
    }
}
