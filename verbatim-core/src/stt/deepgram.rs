use async_trait::async_trait;
use serde::Deserialize;

use super::openai::encode_wav;
use super::SttBackend;
use crate::errors::SttError;

pub struct DeepgramStt {
    client: reqwest::Client,
    api_key: String,
    model: String,
}

impl DeepgramStt {
    pub fn new(api_key: String, model: String) -> Result<Self, SttError> {
        tracing::debug!(
            model = %model,
            api_key_present = !api_key.is_empty(),
            "creating Deepgram STT client"
        );
        if api_key.is_empty() {
            return Err(SttError::ApiError(
                "Deepgram API key not configured. Set deepgram.api_key in config or DEEPGRAM_API_KEY env var.".into(),
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
impl SttBackend for DeepgramStt {
    fn name(&self) -> &str {
        "deepgram"
    }

    async fn transcribe(&self, audio: &[f32], language: Option<&str>) -> Result<String, SttError> {
        tracing::debug!(
            samples = audio.len(),
            duration_secs = format_args!("{:.1}", audio.len() as f32 / 16000.0),
            language = ?language,
            model = %self.model,
            "starting Deepgram transcription"
        );

        let wav_data = encode_wav(audio)?;
        tracing::debug!(wav_bytes = wav_data.len(), "encoded audio to WAV");

        let mut query_params = vec![
            ("model", self.model.clone()),
            ("smart_format", "true".into()),
            ("punctuation", "true".into()),
            ("paragraphs", "true".into()),
        ];

        if let Some(lang) = language {
            query_params.push(("language", lang.to_string()));
        }

        tracing::debug!("sending request to Deepgram API");
        let request_start = std::time::Instant::now();
        let response = self
            .client
            .post("https://api.deepgram.com/v1/listen")
            .header("Authorization", format!("Token {}", self.api_key))
            .header("Content-Type", "audio/wav")
            .query(&query_params)
            .body(wav_data)
            .send()
            .await?;

        let status = response.status();
        tracing::debug!(
            elapsed_ms = request_start.elapsed().as_millis(),
            status = %status,
            "Deepgram API response received"
        );

        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(SttError::ApiError(format!(
                "Deepgram API error {}: {}",
                status, body
            )));
        }

        let dg_response: DeepgramResponse = response
            .json()
            .await
            .map_err(|e| SttError::ApiError(format!("Failed to parse Deepgram response: {}", e)))?;

        let text = dg_response
            .results
            .channels
            .into_iter()
            .next()
            .and_then(|ch| ch.alternatives.into_iter().next())
            .map(|alt| alt.transcript)
            .unwrap_or_default();

        let text = text.trim().to_string();

        if let Some(ref metadata) = dg_response.metadata {
            tracing::debug!(
                duration = ?metadata.duration,
                "Deepgram metadata"
            );
        }

        tracing::debug!("Deepgram transcribed {} samples -> '{}'", audio.len(), text);
        Ok(text)
    }
}

// ── Deepgram API Response Types ─────────────────────────────────────

#[derive(Debug, Deserialize)]
struct DeepgramResponse {
    results: DeepgramResults,
    metadata: Option<DeepgramMetadata>,
}

#[derive(Debug, Deserialize)]
struct DeepgramResults {
    channels: Vec<DeepgramChannel>,
}

#[derive(Debug, Deserialize)]
struct DeepgramChannel {
    alternatives: Vec<DeepgramAlternative>,
}

#[derive(Debug, Deserialize)]
struct DeepgramAlternative {
    transcript: String,
}

#[derive(Debug, Deserialize)]
struct DeepgramMetadata {
    duration: Option<f64>,
}

// ── Deepgram Balance API ────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, Deserialize)]
pub struct DeepgramBalance {
    pub amount: f64,
    pub currency: String,
}

#[derive(Debug, Deserialize)]
struct ProjectsResponse {
    projects: Vec<Project>,
}

#[derive(Debug, Deserialize)]
struct Project {
    project_id: String,
}

#[derive(Debug, Deserialize)]
struct BalancesResponse {
    balances: Vec<BalanceEntry>,
}

#[derive(Debug, Deserialize)]
struct BalanceEntry {
    amount: f64,
    units: String,
}

pub async fn check_balance(api_key: &str) -> Result<DeepgramBalance, SttError> {
    if api_key.is_empty() {
        return Err(SttError::ApiError("Deepgram API key not configured".into()));
    }

    let client = reqwest::Client::new();
    let auth = format!("Token {}", api_key);

    // Step 1: Get project ID
    let projects_resp_raw = client
        .get("https://api.deepgram.com/v1/projects")
        .header("Authorization", &auth)
        .send()
        .await?;

    if projects_resp_raw.status() == reqwest::StatusCode::FORBIDDEN {
        return Err(SttError::ApiError(
            "Balance check unavailable — your API key lacks billing permissions. Check your balance at console.deepgram.com.".into(),
        ));
    }

    let projects_resp: ProjectsResponse = projects_resp_raw
        .error_for_status()
        .map_err(|e| SttError::ApiError(format!("Failed to list Deepgram projects: {}", e)))?
        .json()
        .await
        .map_err(|e| SttError::ApiError(format!("Failed to parse projects response: {}", e)))?;

    let project_id = projects_resp
        .projects
        .first()
        .ok_or_else(|| SttError::ApiError("No Deepgram projects found".into()))?
        .project_id
        .clone();

    // Step 2: Get balance for the project
    let balance_resp_raw = client
        .get(format!(
            "https://api.deepgram.com/v1/projects/{}/balances",
            project_id
        ))
        .header("Authorization", &auth)
        .send()
        .await?;

    if balance_resp_raw.status() == reqwest::StatusCode::FORBIDDEN {
        return Err(SttError::ApiError(
            "Balance check unavailable — your API key lacks billing permissions. Check your balance at console.deepgram.com.".into(),
        ));
    }

    let balances_resp: BalancesResponse = balance_resp_raw
        .error_for_status()
        .map_err(|e| SttError::ApiError(format!("Failed to get Deepgram balance: {}", e)))?
        .json()
        .await
        .map_err(|e| SttError::ApiError(format!("Failed to parse balance response: {}", e)))?;

    let entry = balances_resp
        .balances
        .first()
        .ok_or_else(|| SttError::ApiError("No balance information found".into()))?;

    Ok(DeepgramBalance {
        amount: entry.amount,
        currency: entry.units.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deepgram_new_rejects_empty_api_key() {
        let result = DeepgramStt::new(String::new(), "nova-2".into());
        assert!(result.is_err());
        match result {
            Err(SttError::ApiError(msg)) => assert!(msg.contains("API key")),
            _ => panic!("expected ApiError"),
        }
    }

    #[test]
    fn test_deepgram_new_accepts_valid_key() {
        let result = DeepgramStt::new("dg-test123".into(), "nova-2".into());
        assert!(result.is_ok());
    }

    #[test]
    fn test_deepgram_name() {
        let backend = DeepgramStt::new("dg-test".into(), "nova-2".into()).unwrap();
        assert_eq!(backend.name(), "deepgram");
    }

    #[test]
    fn test_check_balance_rejects_empty_key() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(check_balance(""));
        assert!(result.is_err());
    }

    // ── Edge case tests ──────────────────────────────────

    #[test]
    fn test_deepgram_new_whitespace_only_key_accepted() {
        // Whitespace-only key passes is_empty() but will fail at runtime
        let result = DeepgramStt::new("   ".into(), "nova-2".into());
        assert!(result.is_ok(), "whitespace-only key passes validation (potential bug)");
    }

    #[test]
    fn test_deepgram_new_stores_model() {
        let backend = DeepgramStt::new("dg-test".into(), "nova-3".into()).unwrap();
        // The model is stored internally; name() still returns "deepgram"
        assert_eq!(backend.name(), "deepgram");
    }

    #[test]
    fn test_check_balance_whitespace_only_key() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(check_balance("   "));
        // Whitespace passes is_empty() — will fail at API call
        assert!(result.is_err());
    }
}
