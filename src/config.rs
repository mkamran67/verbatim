use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub general: GeneralConfig,
    #[serde(default)]
    pub whisper: WhisperConfig,
    #[serde(default)]
    pub openai: OpenAiConfig,
    #[serde(default)]
    pub deepgram: DeepgramConfig,
    #[serde(default)]
    pub audio: AudioConfig,
    #[serde(default)]
    pub input: InputConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    #[serde(default = "default_backend")]
    pub backend: String,
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default)]
    pub clipboard_only: bool,
    #[serde(default = "default_hotkeys")]
    pub hotkeys: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhisperConfig {
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_model_dir")]
    pub model_dir: String,
    #[serde(default)]
    pub threads: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiConfig {
    #[serde(default)]
    pub api_key: String,
    #[serde(default = "default_openai_model")]
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeepgramConfig {
    #[serde(default)]
    pub api_key: String,
    #[serde(default = "default_deepgram_model")]
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioConfig {
    #[serde(default)]
    pub device: String,
    #[serde(default = "default_min_duration")]
    pub min_duration: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputConfig {
    #[serde(default = "default_input_method")]
    pub method: String,
}

// Defaults
fn default_backend() -> String { "whisper-local".into() }
fn default_language() -> String { "en".into() }
fn default_hotkeys() -> Vec<String> { vec!["KEY_RIGHTCTRL".into()] }
fn default_model() -> String { "base.en".into() }
fn default_openai_model() -> String { "whisper-1".into() }
fn default_deepgram_model() -> String { "nova-2".into() }
fn default_min_duration() -> f32 { 0.5 }
fn default_input_method() -> String { "auto".into() }

fn default_model_dir() -> String {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("~/.local/share"))
        .join("verbatim/models")
        .to_string_lossy()
        .into_owned()
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            backend: default_backend(),
            language: default_language(),
            clipboard_only: false,
            hotkeys: default_hotkeys(),
        }
    }
}

impl Default for WhisperConfig {
    fn default() -> Self {
        Self {
            model: default_model(),
            model_dir: default_model_dir(),
            threads: 0,
        }
    }
}

impl Default for OpenAiConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            model: default_openai_model(),
        }
    }
}

impl Default for DeepgramConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            model: default_deepgram_model(),
        }
    }
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            device: String::new(),
            min_duration: default_min_duration(),
        }
    }
}

impl Default for InputConfig {
    fn default() -> Self {
        Self {
            method: default_input_method(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            general: GeneralConfig::default(),
            whisper: WhisperConfig::default(),
            openai: OpenAiConfig::default(),
            deepgram: DeepgramConfig::default(),
            audio: AudioConfig::default(),
            input: InputConfig::default(),
        }
    }
}

impl Config {
    pub fn config_dir() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("~/.config"))
            .join("verbatim")
    }

    pub fn config_path() -> PathBuf {
        Self::config_dir().join("config.toml")
    }

    pub fn load() -> Result<Self> {
        let path = Self::config_path();

        if !path.exists() {
            tracing::info!("No config file found at {}, using defaults", path.display());
            return Ok(Self::default());
        }

        let contents = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read config from {}", path.display()))?;

        let mut config: Config = toml::from_str(&contents)
            .with_context(|| format!("Failed to parse config from {}", path.display()))?;

        // Override API keys from environment variables
        if config.openai.api_key.is_empty() {
            if let Ok(key) = std::env::var("OPENAI_API_KEY") {
                config.openai.api_key = key;
            }
        }
        if config.deepgram.api_key.is_empty() {
            if let Ok(key) = std::env::var("DEEPGRAM_API_KEY") {
                config.deepgram.api_key = key;
            }
        }
        Ok(config)
    }

    pub fn save_default_config() -> Result<()> {
        let dir = Self::config_dir();
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("Failed to create config directory {}", dir.display()))?;

        let path = Self::config_path();
        if path.exists() {
            return Ok(());
        }

        let default = Self::default();
        let contents = toml::to_string_pretty(&default)?;
        std::fs::write(&path, contents)
            .with_context(|| format!("Failed to write default config to {}", path.display()))?;

        tracing::info!("Created default config at {}", path.display());
        Ok(())
    }

    /// Save the current config to the config file.
    pub fn save(&self) -> Result<()> {
        let dir = Self::config_dir();
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("Failed to create config directory {}", dir.display()))?;

        let path = Self::config_path();
        let contents = toml::to_string_pretty(self)?;
        std::fs::write(&path, contents)
            .with_context(|| format!("Failed to write config to {}", path.display()))?;

        tracing::info!("Config saved to {}", path.display());
        Ok(())
    }

    /// Resolve the model directory path, expanding ~ if present.
    pub fn resolved_model_dir(&self) -> PathBuf {
        let dir = &self.whisper.model_dir;
        if dir.starts_with("~/") {
            if let Some(home) = dirs::home_dir() {
                return home.join(&dir[2..]);
            }
        }
        PathBuf::from(dir)
    }
}
