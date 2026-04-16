use crate::config::PostProcessingConfig;
use crate::llm_inference::LlmEngine;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Default)]
pub struct TokenUsage {
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
}

pub struct PostProcessResult {
    pub text: String,
    pub usage: TokenUsage,
    pub error: Option<String>,
}

/// Post-processor that can use either OpenAI API or a local LLM.
pub enum PostProcessor {
    OpenAi(OpenAiPostProcessor),
    Local(LocalPostProcessor),
}

impl PostProcessor {
    pub fn new_openai(config: &PostProcessingConfig, api_key: String) -> Self {
        PostProcessor::OpenAi(OpenAiPostProcessor::new(config, api_key))
    }

    pub fn new_local(config: &PostProcessingConfig, engine: Arc<LlmEngine>) -> Self {
        PostProcessor::Local(LocalPostProcessor::new(config, engine))
    }

    pub fn model(&self) -> &str {
        match self {
            PostProcessor::OpenAi(p) => &p.model,
            PostProcessor::Local(p) => &p.model_name,
        }
    }

    pub async fn process(&self, text: &str) -> PostProcessResult {
        match self {
            PostProcessor::OpenAi(p) => p.process(text).await,
            PostProcessor::Local(p) => p.process(text).await,
        }
    }
}

// ── OpenAI Provider ──────────────────────────────────────────────────

pub struct OpenAiPostProcessor {
    client: Client,
    api_key: String,
    model: String,
    system_prompt: String,
}

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<Message>,
    temperature: f32,
}

#[derive(Serialize, Deserialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
    usage: Option<ApiUsage>,
}

#[derive(Deserialize)]
struct Choice {
    message: Message,
}

#[derive(Deserialize)]
struct ApiUsage {
    prompt_tokens: i64,
    completion_tokens: i64,
    total_tokens: i64,
}

impl OpenAiPostProcessor {
    pub fn new(config: &PostProcessingConfig, api_key: String) -> Self {
        tracing::debug!(
            model = %config.model,
            prompt_len = config.prompt.len(),
            api_key_present = !api_key.is_empty(),
            "creating OpenAiPostProcessor"
        );
        Self {
            client: Client::new(),
            api_key,
            model: config.model.clone(),
            system_prompt: config.prompt.clone(),
        }
    }

    pub async fn process(&self, text: &str) -> PostProcessResult {
        tracing::debug!(text_len = text.len(), model = %self.model, "starting OpenAI post-processing");
        match self.call_api(text).await {
            Ok(result) => result,
            Err(e) => {
                tracing::error!("Post-processing failed, returning original text: {}", e);
                PostProcessResult {
                    text: text.to_string(),
                    usage: TokenUsage::default(),
                    error: Some(format!("OpenAI post-processing failed: {}", e)),
                }
            }
        }
    }

    async fn call_api(&self, text: &str) -> Result<PostProcessResult, reqwest::Error> {
        tracing::debug!(
            model = %self.model,
            system_prompt_len = self.system_prompt.len(),
            text_len = text.len(),
            "calling chat API for post-processing"
        );
        let request_start = std::time::Instant::now();
        let request = ChatRequest {
            model: self.model.clone(),
            messages: vec![
                Message {
                    role: "system".into(),
                    content: self.system_prompt.clone(),
                },
                Message {
                    role: "user".into(),
                    content: format!("Text:\n{}", text),
                },
            ],
            temperature: 0.1,
        };

        let response = self
            .client
            .post("https://api.openai.com/v1/chat/completions")
            .bearer_auth(&self.api_key)
            .json(&request)
            .send()
            .await?
            .error_for_status()?
            .json::<ChatResponse>()
            .await?;

        tracing::debug!(
            elapsed_ms = request_start.elapsed().as_millis(),
            "chat API response received"
        );

        let usage = response.usage.map(|u| {
            tracing::debug!(
                prompt_tokens = u.prompt_tokens,
                completion_tokens = u.completion_tokens,
                total_tokens = u.total_tokens,
                "post-processing token usage"
            );
            TokenUsage {
                prompt_tokens: u.prompt_tokens,
                completion_tokens: u.completion_tokens,
                total_tokens: u.total_tokens,
            }
        }).unwrap_or_default();

        let text = response
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content.trim().to_string())
            .unwrap_or_else(|| text.to_string());

        tracing::trace!(text_len = text.len(), "post-processed text ready");
        Ok(PostProcessResult { text, usage, error: None })
    }
}

// ── Local LLM Provider ───────────────────────────────────────────────

pub struct LocalPostProcessor {
    engine: Arc<LlmEngine>,
    system_prompt: String,
    model_name: String,
}

impl LocalPostProcessor {
    pub fn new(config: &PostProcessingConfig, engine: Arc<LlmEngine>) -> Self {
        tracing::debug!(
            llm_model = %config.llm_model,
            prompt_len = config.prompt.len(),
            "creating LocalPostProcessor"
        );
        Self {
            engine,
            system_prompt: config.prompt.clone(),
            model_name: config.llm_model.clone(),
        }
    }

    pub async fn process(&self, text: &str) -> PostProcessResult {
        tracing::debug!(text_len = text.len(), model = %self.model_name, "starting local LLM post-processing");

        let engine = self.engine.clone();
        let prompt = self.system_prompt.clone();
        let input = format!("Text:\n{}", text);

        let result = tokio::task::spawn_blocking(move || {
            engine.chat_complete(&prompt, &input)
        })
        .await;

        match result {
            Ok(Ok(output)) => {
                tracing::debug!(output_len = output.len(), "local post-processing completed");
                PostProcessResult {
                    text: output,
                    usage: TokenUsage::default(),
                    error: None,
                }
            }
            Ok(Err(e)) => {
                tracing::error!("Local post-processing failed: {}, returning original text", e);
                PostProcessResult {
                    text: text.to_string(),
                    usage: TokenUsage::default(),
                    error: Some(format!("Local LLM failed: {}", e)),
                }
            }
            Err(e) => {
                tracing::error!("Local post-processing task panicked: {}, returning original text", e);
                PostProcessResult {
                    text: text.to_string(),
                    usage: TokenUsage::default(),
                    error: Some(format!("Local LLM task panicked: {}", e)),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::PostProcessingConfig;

    #[test]
    fn test_openai_postprocessor_model_returns_configured() {
        let config = PostProcessingConfig {
            enabled: true,
            provider: "openai".into(),
            model: "gpt-4o-mini".into(),
            prompt: "test prompt".into(),
            llm_model: "llama-3.2-1b".into(),
            saved_prompts: vec![],
        };
        let pp = PostProcessor::new_openai(&config, "sk-test".into());
        assert_eq!(pp.model(), "gpt-4o-mini");
    }

    #[test]
    fn test_openai_postprocessor_new_sets_fields() {
        let config = PostProcessingConfig {
            enabled: true,
            provider: "openai".into(),
            model: "gpt-4".into(),
            prompt: "Format this".into(),
            llm_model: "llama-3.2-1b".into(),
            saved_prompts: vec![],
        };
        let pp = OpenAiPostProcessor::new(&config, "sk-key".into());
        assert_eq!(pp.model, "gpt-4");
        assert_eq!(pp.api_key, "sk-key");
        assert_eq!(pp.system_prompt, "Format this");
    }

    #[test]
    fn test_token_usage_default() {
        let usage = TokenUsage::default();
        assert_eq!(usage.prompt_tokens, 0);
        assert_eq!(usage.completion_tokens, 0);
        assert_eq!(usage.total_tokens, 0);
    }

    // ── Edge case tests ──────────────────────────────────

    #[test]
    fn test_openai_postprocessor_empty_api_key_constructs() {
        let config = PostProcessingConfig {
            enabled: true,
            provider: "openai".into(),
            model: "gpt-4o-mini".into(),
            prompt: "test".into(),
            llm_model: "llama-3.2-1b".into(),
            saved_prompts: vec![],
        };
        // Empty API key is accepted at construction time (will fail at runtime)
        let pp = OpenAiPostProcessor::new(&config, String::new());
        assert_eq!(pp.api_key, "");
    }

    #[test]
    fn test_openai_postprocessor_empty_prompt_constructs() {
        let config = PostProcessingConfig {
            enabled: true,
            provider: "openai".into(),
            model: "gpt-4o-mini".into(),
            prompt: String::new(),
            llm_model: "llama-3.2-1b".into(),
            saved_prompts: vec![],
        };
        let pp = OpenAiPostProcessor::new(&config, "sk-test".into());
        assert_eq!(pp.system_prompt, "");
    }

    #[test]
    fn test_postprocessor_different_models() {
        for model_name in &["gpt-4o", "gpt-4o-mini", "gpt-3.5-turbo"] {
            let config = PostProcessingConfig {
                enabled: true,
                provider: "openai".into(),
                model: model_name.to_string(),
                prompt: "test".into(),
                llm_model: "llama-3.2-1b".into(),
                saved_prompts: vec![],
            };
            let pp = PostProcessor::new_openai(&config, "sk-test".into());
            assert_eq!(pp.model(), *model_name);
        }
    }
}
