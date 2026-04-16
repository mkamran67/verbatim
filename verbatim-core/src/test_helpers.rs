use crate::config::Config;
use crate::db::Transcription;

/// Create a Config with deterministic, non-filesystem-dependent values.
#[allow(dead_code)]
pub fn make_test_config() -> Config {
    let mut config = Config::default();
    config.whisper.model_dir = "/tmp/verbatim-test-models".into();
    config.openai.api_key = String::new();
    config.deepgram.api_key = String::new();
    config
}

/// Create a sample Transcription for testing.
#[allow(dead_code)]
pub fn sample_transcription(id: &str, text: &str) -> Transcription {
    Transcription {
        id: id.to_string(),
        text: text.to_string(),
        word_count: text.split_whitespace().count() as i64,
        char_count: text.len() as i64,
        duration_secs: 1.0,
        backend: "test".to_string(),
        language: Some("en".to_string()),
        created_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        prompt_tokens: 0,
        completion_tokens: 0,
        post_processing_error: None,
        raw_text: None,
        stt_model: None,
        pp_model: None,
    }
}
