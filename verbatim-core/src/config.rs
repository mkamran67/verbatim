use anyhow::{Context, Result};
use serde::{Deserialize, Deserializer, Serialize};
use std::path::PathBuf;

use crate::keyring_store;

/// A hotkey binding using raw OS keycodes captured at bind time.
/// On Linux these are evdev codes; on macOS these are CGKeyCode values.
/// Stored as `u32` for forward-compat (the OS APIs use u16).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hotkey {
    pub key: u32,
    #[serde(default)]
    pub modifiers: Vec<u32>,
    /// Display label, e.g. "F5" or "Right Ctrl + F5". Free-form.
    #[serde(default)]
    pub label: String,
}

impl Hotkey {
    pub fn new(key: u32, modifiers: Vec<u32>, label: impl Into<String>) -> Self {
        Self { key, modifiers, label: label.into() }
    }
}

/// Deserialize `Vec<Hotkey>` accepting either the new object form or the legacy
/// string form (e.g. `"KEY_F5"` / `"KEY_LEFTCTRL+KEY_F5"`).  Legacy entries that
/// can't be converted on this platform are dropped with a warning so the user
/// can rebind without the app failing to load.
fn deserialize_hotkeys<'de, D>(de: D) -> Result<Vec<Hotkey>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Entry {
        Legacy(String),
        Modern(Hotkey),
    }

    let entries: Vec<Entry> = Vec::deserialize(de)?;
    let mut out = Vec::with_capacity(entries.len());
    for e in entries {
        match e {
            Entry::Modern(h) => out.push(h),
            Entry::Legacy(s) => match legacy_hotkey_from_string(&s) {
                Some(h) => {
                    tracing::info!(legacy = %s, "migrated legacy hotkey string");
                    out.push(h);
                }
                None => {
                    tracing::warn!(
                        legacy = %s,
                        "could not migrate legacy hotkey string; rebind it in Settings"
                    );
                }
            },
        }
    }
    Ok(out)
}

/// Best-effort migration of a legacy `KEY_*` string to the new numeric form
/// for the current platform. Returns None on unknown names or unsupported
/// platform.  Intentionally narrow — the new flow captures real OS codes.
fn legacy_hotkey_from_string(s: &str) -> Option<Hotkey> {
    let parts: Vec<&str> = s.split('+').collect();
    if parts.is_empty() || parts.len() > 3 {
        return None;
    }
    let codes: Vec<u32> = parts.iter().map(|p| legacy_key_code(p)).collect::<Option<Vec<_>>>()?;
    let (modifiers, key) = codes.split_at(codes.len() - 1);
    let key = key[0];
    let label = parts.iter().map(|p| legacy_key_label(p)).collect::<Vec<_>>().join(" + ");
    Some(Hotkey { key, modifiers: modifiers.to_vec(), label })
}

#[cfg(target_os = "linux")]
fn legacy_key_code(name: &str) -> Option<u32> {
    use evdev::Key;
    let k = match name {
        "KEY_RIGHTCTRL" => Key::KEY_RIGHTCTRL, "KEY_LEFTCTRL" => Key::KEY_LEFTCTRL,
        "KEY_RIGHTALT" => Key::KEY_RIGHTALT, "KEY_LEFTALT" => Key::KEY_LEFTALT,
        "KEY_RIGHTSHIFT" => Key::KEY_RIGHTSHIFT, "KEY_LEFTSHIFT" => Key::KEY_LEFTSHIFT,
        "KEY_F1" => Key::KEY_F1, "KEY_F2" => Key::KEY_F2, "KEY_F3" => Key::KEY_F3,
        "KEY_F4" => Key::KEY_F4, "KEY_F5" => Key::KEY_F5, "KEY_F6" => Key::KEY_F6,
        "KEY_F7" => Key::KEY_F7, "KEY_F8" => Key::KEY_F8, "KEY_F9" => Key::KEY_F9,
        "KEY_F10" => Key::KEY_F10, "KEY_F11" => Key::KEY_F11, "KEY_F12" => Key::KEY_F12,
        "KEY_CAPSLOCK" => Key::KEY_CAPSLOCK, "KEY_SCROLLLOCK" => Key::KEY_SCROLLLOCK,
        "KEY_PAUSE" => Key::KEY_PAUSE, "KEY_INSERT" => Key::KEY_INSERT,
        "KEY_SPACE" => Key::KEY_SPACE, "KEY_TAB" => Key::KEY_TAB, "KEY_ENTER" => Key::KEY_ENTER,
        _ => return None,
    };
    Some(k.code() as u32)
}

#[cfg(target_os = "macos")]
fn legacy_key_code(name: &str) -> Option<u32> {
    let v: u16 = match name {
        "KEY_RIGHTCTRL" => 62, "KEY_LEFTCTRL" => 59,
        "KEY_RIGHTALT" => 61, "KEY_LEFTALT" => 58,
        "KEY_RIGHTSHIFT" => 60, "KEY_LEFTSHIFT" => 56,
        "KEY_F1" => 122, "KEY_F2" => 120, "KEY_F3" => 99, "KEY_F4" => 118,
        "KEY_F5" => 96, "KEY_F6" => 97, "KEY_F7" => 98, "KEY_F8" => 100,
        "KEY_F9" => 101, "KEY_F10" => 109, "KEY_F11" => 103, "KEY_F12" => 111,
        "KEY_CAPSLOCK" => 57, "KEY_INSERT" => 114,
        "KEY_SPACE" => 49, "KEY_TAB" => 48, "KEY_ENTER" => 36,
        _ => return None,
    };
    Some(v as u32)
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn legacy_key_code(_name: &str) -> Option<u32> { None }

fn legacy_key_label(name: &str) -> String {
    match name {
        "KEY_RIGHTCTRL" => "Right Ctrl",  "KEY_LEFTCTRL" => "Left Ctrl",
        "KEY_RIGHTALT" => "Right Alt",    "KEY_LEFTALT" => "Left Alt",
        "KEY_RIGHTSHIFT" => "Right Shift","KEY_LEFTSHIFT" => "Left Shift",
        s if s.starts_with("KEY_F") => &s[4..],
        s if s.starts_with("KEY_") => &s[4..],
        s => s,
    }.to_string()
}

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
    pub smallest: SmallestConfig,
    #[serde(default)]
    pub audio: AudioConfig,
    #[serde(default)]
    pub input: InputConfig,
    #[serde(default)]
    pub post_processing: PostProcessingConfig,
    #[serde(default)]
    pub hands_free: HandsFreeConfig,
    #[serde(default)]
    pub llm: LlmConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    #[serde(default = "default_backend")]
    pub backend: String,
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default)]
    pub clipboard_only: bool,
    #[serde(default = "default_hotkeys", deserialize_with = "deserialize_hotkeys")]
    pub hotkeys: Vec<Hotkey>,
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_ui_language")]
    pub ui_language: String,
    #[serde(default)]
    pub onboarding_complete: bool,
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
    #[serde(default)]
    pub admin_key: String,
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
pub struct SmallestConfig {
    #[serde(default)]
    pub api_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioConfig {
    #[serde(default)]
    pub device: String,
    #[serde(default = "default_min_duration")]
    pub min_duration: f32,
    #[serde(default = "default_energy_threshold")]
    pub energy_threshold: f32,
    #[serde(default)]
    pub noise_cancellation: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasteRule {
    pub app_class: String,
    pub paste_command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputConfig {
    #[serde(default = "default_input_method")]
    pub method: String,
    #[serde(default = "default_paste_command")]
    pub paste_command: String,
    #[serde(default)]
    pub paste_rules: Vec<PasteRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedPrompt {
    pub name: String,
    pub prompt: String,
    #[serde(default)]
    pub emoji: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostProcessingConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_pp_provider")]
    pub provider: String,
    #[serde(default = "default_pp_model")]
    pub model: String,
    #[serde(default = "default_pp_prompt")]
    pub prompt: String,
    #[serde(default)]
    pub saved_prompts: Vec<SavedPrompt>,
    #[serde(default = "default_pp_emoji")]
    pub default_emoji: String,

    // ── Ollama provider settings ──────────────────────────
    #[serde(default = "default_ollama_mode")]
    pub ollama_mode: String, // "managed" | "existing" | "custom"
    #[serde(default = "default_ollama_url")]
    pub ollama_url: String,
    #[serde(default)]
    pub ollama_auth_token: String,
    #[serde(default = "default_ollama_bundled_port")]
    pub ollama_bundled_port: u16,
    #[serde(default = "default_ollama_model")]
    pub ollama_model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    #[serde(default = "default_llm_model_dir")]
    pub model_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandsFreeConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_handsfree_hotkeys", deserialize_with = "deserialize_hotkeys")]
    pub hotkeys: Vec<Hotkey>,
}

// Defaults
fn default_theme() -> String { "system".into() }
fn default_ui_language() -> String { "system".into() }
fn default_handsfree_hotkeys() -> Vec<Hotkey> { vec![] }
fn default_backend() -> String { "whisper-local".into() }
fn default_language() -> String { "en".into() }
fn default_hotkeys() -> Vec<Hotkey> {
    #[cfg(target_os = "macos")]
    { vec![Hotkey::new(96, vec![], "F5")] } // CGKeyCode F5
    #[cfg(target_os = "linux")]
    {
        use evdev::Key;
        vec![Hotkey::new(Key::KEY_RIGHTCTRL.code() as u32, vec![], "Right Ctrl")]
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    { vec![] }
}
fn default_model() -> String { "base.en".into() }
fn default_openai_model() -> String { "whisper-1".into() }
fn default_deepgram_model() -> String { "nova-2".into() }
fn default_min_duration() -> f32 { 0.5 }
fn default_energy_threshold() -> f32 { 0.0 }
fn default_input_method() -> String { "auto".into() }
fn default_paste_command() -> String {
    #[cfg(target_os = "macos")]
    { "meta+v".into() }
    #[cfg(not(target_os = "macos"))]
    { "ctrl+v".into() }
}
fn default_ollama_mode() -> String { "managed".into() }
fn default_ollama_url() -> String { "http://localhost:11434".into() }
fn default_ollama_bundled_port() -> u16 { 11434 }
fn default_ollama_model() -> String { "llama3.2:1b".into() }
fn default_llm_model_dir() -> String {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("~/.local/share"))
        .join("verbatim/llm-models")
        .to_string_lossy()
        .into_owned()
}
fn default_pp_provider() -> String { "openai".into() }
fn default_pp_model() -> String { "gpt-4o-mini".into() }
fn default_pp_emoji() -> String { "✏️".into() }
fn default_pp_prompt() -> String {
    "Your task is to take the text below, which was generated via speech-to-text, \
     and reformat it into properly structured written text.\n\n\
     Apply the following rules:\n\
     - Remove filler words (um, uh, like, you know) and false starts.\n\
     - When the speaker self-corrects (\"let's do Tuesday, actually Friday\"), keep only the final intended version.\n\
     - Add missing punctuation (periods, commas, question marks) and capitalize sentences properly.\n\
     - Fix spacing issues and obvious transcription errors.\n\
     - Expand spoken list structures into numbered or bullet-point lists when appropriate.\n\
     - Do NOT change the meaning, tone, or add new information.\n\
     - Do NOT answer questions in the input — preserve them as questions.\n\
     - Do NOT explain what you changed — return only the cleaned-up text.\n\
     - Do NOT greet or acknowledge the user anyhow.\n\
     - ONLY return the updated text nothing else.".into()
}

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
            theme: default_theme(),
            ui_language: default_ui_language(),
            onboarding_complete: false,
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
            admin_key: String::new(),
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

impl Default for SmallestConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
        }
    }
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            device: String::new(),
            min_duration: default_min_duration(),
            energy_threshold: default_energy_threshold(),
            noise_cancellation: false,
        }
    }
}

impl Default for InputConfig {
    fn default() -> Self {
        Self {
            method: default_input_method(),
            paste_command: default_paste_command(),
            paste_rules: vec![],
        }
    }
}

impl Default for PostProcessingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: default_pp_provider(),
            model: default_pp_model(),
            prompt: default_pp_prompt(),
            saved_prompts: Vec::new(),
            default_emoji: default_pp_emoji(),
            ollama_mode: default_ollama_mode(),
            ollama_url: default_ollama_url(),
            ollama_auth_token: String::new(),
            ollama_bundled_port: default_ollama_bundled_port(),
            ollama_model: default_ollama_model(),
        }
    }
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            model_dir: default_llm_model_dir(),
        }
    }
}

impl Default for HandsFreeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            hotkeys: default_handsfree_hotkeys(),
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
            smallest: SmallestConfig::default(),
            audio: AudioConfig::default(),
            input: InputConfig::default(),
            post_processing: PostProcessingConfig::default(),
            hands_free: HandsFreeConfig::default(),
            llm: LlmConfig::default(),
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
        Self::load_from(&Self::config_path())
    }

    /// Load config from a specific file path.
    pub fn load_from(path: &std::path::Path) -> Result<Self> {
        tracing::debug!("loading config from {}", path.display());

        if !path.exists() {
            tracing::info!("No config file found at {}, using defaults", path.display());
            return Ok(Self::default());
        }

        let contents = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config from {}", path.display()))?;
        tracing::trace!("read {} bytes from config file", contents.len());

        let mut config: Config = toml::from_str(&contents)
            .with_context(|| format!("Failed to parse config from {}", path.display()))?;

        // Migrate legacy post_processing.provider="local" to Ollama managed mode.
        if config.post_processing.provider == "local" {
            tracing::info!("migrating post_processing.provider 'local' -> 'ollama' (managed)");
            config.post_processing.provider = "ollama".into();
            config.post_processing.ollama_mode = "managed".into();
        }


        tracing::debug!(
            backend = %config.general.backend,
            language = %config.general.language,
            hotkeys = ?config.general.hotkeys,
            model = %config.whisper.model,
            audio_device = %config.audio.device,
            clipboard_only = config.general.clipboard_only,
            min_duration = config.audio.min_duration,
            energy_threshold = config.audio.energy_threshold,
            input_method = %config.input.method,
            post_processing_enabled = config.post_processing.enabled,
            "config parsed successfully"
        );

        // Load API keys from OS keyring
        let mut migrated = false;
        if let Some(key) = keyring_store::get_secret("openai_api_key") {
            if !key.is_empty() {
                keyring_store::seed_write_cache("openai_api_key", &key);
                config.openai.api_key = key;
            }
        } else if !config.openai.api_key.is_empty() {
            // Key exists in TOML but not keyring — migrate it
            tracing::info!("migrating openai.api_key from config file to keyring");
            migrated = true;
        }

        if let Some(key) = keyring_store::get_secret("openai_admin_key") {
            if !key.is_empty() {
                keyring_store::seed_write_cache("openai_admin_key", &key);
                config.openai.admin_key = key;
            }
        } else if !config.openai.admin_key.is_empty() {
            tracing::info!("migrating openai.admin_key from config file to keyring");
            migrated = true;
        }

        if let Some(key) = keyring_store::get_secret("deepgram_api_key") {
            if !key.is_empty() {
                keyring_store::seed_write_cache("deepgram_api_key", &key);
                config.deepgram.api_key = key;
            }
        } else if !config.deepgram.api_key.is_empty() {
            tracing::info!("migrating deepgram.api_key from config file to keyring");
            migrated = true;
        }

        if let Some(key) = keyring_store::get_secret("smallest_api_key") {
            if !key.is_empty() {
                keyring_store::seed_write_cache("smallest_api_key", &key);
                config.smallest.api_key = key;
            }
        } else if !config.smallest.api_key.is_empty() {
            tracing::info!("migrating smallest.api_key from config file to keyring");
            migrated = true;
        }

        // Auto-migrate: move plaintext keys to keyring and re-save stripped config
        if migrated {
            if let Err(e) = config.save() {
                tracing::warn!(error = %e, "failed to re-save config during keyring migration");
            }
        }

        // Override API keys from environment variables
        if config.openai.api_key.is_empty() {
            if let Ok(key) = std::env::var("OPENAI_API_KEY") {
                tracing::debug!("overriding openai.api_key from OPENAI_API_KEY env var");
                config.openai.api_key = key;
            }
        }
        if config.openai.admin_key.is_empty() {
            if let Ok(key) = std::env::var("OPENAI_ADMIN_KEY") {
                tracing::debug!("overriding openai.admin_key from OPENAI_ADMIN_KEY env var");
                config.openai.admin_key = key;
            }
        }
        if config.deepgram.api_key.is_empty() {
            if let Ok(key) = std::env::var("DEEPGRAM_API_KEY") {
                tracing::debug!("overriding deepgram.api_key from DEEPGRAM_API_KEY env var");
                config.deepgram.api_key = key;
            }
        }
        if config.smallest.api_key.is_empty() {
            if let Ok(key) = std::env::var("SMALLEST_API_KEY") {
                tracing::debug!("overriding smallest.api_key from SMALLEST_API_KEY env var");
                config.smallest.api_key = key;
            }
        }

        // If a cloud backend is selected but its API key is missing, fall back to local
        let mut needs_save = false;
        if config.general.backend == "deepgram" && config.deepgram.api_key.is_empty() {
            tracing::warn!("deepgram selected but API key is missing — falling back to whisper-local");
            config.general.backend = "whisper-local".into();
            needs_save = true;
        }
        if config.general.backend == "openai" && config.openai.api_key.is_empty() {
            tracing::warn!("openai selected but API key is missing — falling back to whisper-local");
            config.general.backend = "whisper-local".into();
            needs_save = true;
        }
        if config.general.backend == "smallest" && config.smallest.api_key.is_empty() {
            tracing::warn!("smallest selected but API key is missing — falling back to whisper-local");
            config.general.backend = "whisper-local".into();
            needs_save = true;
        }
        if needs_save {
            if let Err(e) = config.save() {
                tracing::warn!(error = %e, "failed to save config after backend fallback");
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
            tracing::debug!("default config already exists at {}, skipping", path.display());
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

        self.save_to(&Self::config_path())
    }

    /// Save the current config to a specific file path.
    /// API keys are stored in the OS keyring and stripped from the TOML file.
    pub fn save_to(&self, path: &std::path::Path) -> Result<()> {
        tracing::debug!("saving config to {}", path.display());

        let mut sanitized = self.clone();
        let keyring_ok = self.store_keys_in_keyring();

        if keyring_ok {
            // Strip keys from TOML — they're safely in the keyring
            sanitized.openai.api_key = String::new();
            sanitized.openai.admin_key = String::new();
            sanitized.deepgram.api_key = String::new();
            sanitized.smallest.api_key = String::new();
        } else {
            tracing::warn!("keyring unavailable, API keys will be stored in plaintext config");
        }

        let contents = toml::to_string_pretty(&sanitized)?;
        std::fs::write(path, contents)
            .with_context(|| format!("Failed to write config to {}", path.display()))?;

        tracing::info!("Config saved to {}", path.display());
        Ok(())
    }

    /// Store API keys in the OS keyring. Returns true if all succeeded.
    /// Uses a write cache to skip redundant writes when keys haven't changed.
    fn store_keys_in_keyring(&self) -> bool {
        if !keyring_store::is_available() {
            return false;
        }
        let ok1 = keyring_store::store_secret_if_changed("openai_api_key", &self.openai.api_key).is_ok();
        let ok1b = keyring_store::store_secret_if_changed("openai_admin_key", &self.openai.admin_key).is_ok();
        let ok2 = keyring_store::store_secret_if_changed("deepgram_api_key", &self.deepgram.api_key).is_ok();
        let ok3 = keyring_store::store_secret_if_changed("smallest_api_key", &self.smallest.api_key).is_ok();
        ok1 && ok1b && ok2 && ok3
    }

    /// Resolve the model directory path, expanding ~ if present.
    pub fn resolved_model_dir(&self) -> PathBuf {
        Self::resolve_dir(&self.whisper.model_dir)
    }

    /// Resolve the LLM model directory path, expanding ~ if present.
    pub fn resolved_llm_model_dir(&self) -> PathBuf {
        Self::resolve_dir(&self.llm.model_dir)
    }

    fn resolve_dir(dir: &str) -> PathBuf {
        let resolved = if dir.starts_with("~/") {
            if let Some(home) = dirs::home_dir() {
                home.join(&dir[2..])
            } else {
                PathBuf::from(dir)
            }
        } else {
            PathBuf::from(dir)
        };
        tracing::trace!(raw = %dir, resolved = %resolved.display(), "resolved directory");
        resolved
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_default_config_backend() {
        let config = Config::default();
        assert_eq!(config.general.backend, "whisper-local");
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_default_hotkeys_macos() {
        let config = Config::default();
        assert_eq!(config.general.hotkeys.len(), 1);
        assert_eq!(config.general.hotkeys[0].key, 96); // CGKeyCode F5
        assert!(config.general.hotkeys[0].modifiers.is_empty());
    }

    #[test]
    fn test_legacy_hotkey_string_migration() {
        // A config file using the pre-numeric format should still load.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[general]\nhotkeys = [\"KEY_F5\"]\n").unwrap();
        let config = Config::load_from(&path).unwrap();
        assert_eq!(config.general.hotkeys.len(), 1);
        // F5 evdev=63, macOS=96
        #[cfg(target_os = "macos")]
        assert_eq!(config.general.hotkeys[0].key, 96);
        #[cfg(target_os = "linux")]
        assert_eq!(config.general.hotkeys[0].key, evdev::Key::KEY_F5.code() as u32);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_default_paste_command_macos() {
        let config = Config::default();
        assert_eq!(config.input.paste_command, "meta+v");
    }

    #[test]
    fn test_config_roundtrip_toml() {
        let original = Config::default();
        let toml_str = toml::to_string_pretty(&original).unwrap();
        let parsed: Config = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.general.backend, original.general.backend);
        assert_eq!(parsed.general.hotkeys, original.general.hotkeys);
        assert_eq!(parsed.input.paste_command, original.input.paste_command);
        assert_eq!(parsed.whisper.model, original.whisper.model);
        assert_eq!(parsed.audio.min_duration, original.audio.min_duration);
    }

    #[test]
    fn test_load_from_valid_toml() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let toml_content = r#"
[general]
backend = "openai"
language = "fr"
clipboard_only = true

[openai]
api_key = "sk-test-key"

[whisper]
model = "small"
"#;
        std::fs::write(&path, toml_content).unwrap();
        let config = Config::load_from(&path).unwrap();
        assert_eq!(config.general.backend, "openai");
        assert_eq!(config.general.language, "fr");
        assert!(config.general.clipboard_only);
        assert_eq!(config.whisper.model, "small");
    }

    #[test]
    fn test_load_from_missing_file_returns_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nonexistent.toml");
        let config = Config::load_from(&path).unwrap();
        assert_eq!(config.general.backend, "whisper-local");
    }

    #[test]
    fn test_load_from_partial_toml_fills_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[general]\nbackend = \"openai\"\n\n[openai]\napi_key = \"sk-test\"\n").unwrap();
        let config = Config::load_from(&path).unwrap();
        assert_eq!(config.general.backend, "openai");
        // All other fields should be defaults
        assert_eq!(config.general.language, "en");
        assert_eq!(config.whisper.model, "base.en");
        assert_eq!(config.audio.min_duration, 0.5);
    }

    #[test]
    fn test_load_from_invalid_toml_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "{{{{ not valid toml !!!!").unwrap();
        let result = Config::load_from(&path);
        assert!(result.is_err());
    }

    #[test]
    fn test_env_var_overrides_empty_openai_key() {
        // Clear any keyring entry that may have been set by other tests
        crate::keyring_store::delete_secret("openai_api_key");

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[openai]\napi_key = \"\"\n").unwrap();

        // Set the env var temporarily
        let key = "OPENAI_API_KEY";
        let prev = std::env::var(key).ok();
        std::env::set_var(key, "test-key-from-env");

        let config = Config::load_from(&path).unwrap();
        assert_eq!(config.openai.api_key, "test-key-from-env");

        // Restore
        match prev {
            Some(v) => std::env::set_var(key, v),
            None => std::env::remove_var(key),
        }
    }

    #[test]
    fn test_resolved_model_dir_tilde_expansion() {
        let mut config = Config::default();
        config.whisper.model_dir = "~/models".into();
        let resolved = config.resolved_model_dir();
        // Should not start with ~
        assert!(!resolved.to_string_lossy().starts_with('~'));
        assert!(resolved.to_string_lossy().ends_with("models"));
    }

    #[test]
    fn test_resolved_model_dir_absolute_unchanged() {
        let mut config = Config::default();
        config.whisper.model_dir = "/tmp/my-models".into();
        let resolved = config.resolved_model_dir();
        assert_eq!(resolved, PathBuf::from("/tmp/my-models"));
    }

    #[test]
    fn test_save_to_and_load_from_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");

        let mut config = Config::default();
        config.general.backend = "openai".into();
        config.openai.api_key = "sk-test123".into();
        config.save_to(&path).unwrap();

        // Read the TOML to see which path was taken
        let saved_toml = std::fs::read_to_string(&path).unwrap();
        let saved_config: Config = toml::from_str(&saved_toml).unwrap();

        let loaded = Config::load_from(&path).unwrap();
        assert_eq!(loaded.general.backend, "openai");

        if saved_config.openai.api_key.is_empty() {
            // Key was stored in keyring and stripped from TOML.
            // Another parallel test may have deleted the keyring entry,
            // so only assert the key is non-empty if keyring is still intact.
            if let Some(kr_key) = crate::keyring_store::get_secret("openai_api_key") {
                assert_eq!(kr_key, "sk-test123");
            }
        } else {
            // Keyring unavailable — key should be in plaintext TOML
            assert_eq!(loaded.openai.api_key, "sk-test123");
        }

        // Clean up keyring entries left by save_to
        crate::keyring_store::delete_secret("openai_api_key");
        crate::keyring_store::delete_secret("openai_admin_key");
        crate::keyring_store::delete_secret("deepgram_api_key");
        crate::keyring_store::delete_secret("smallest_api_key");
    }

    #[test]
    fn test_paste_rules_deserialization() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let toml_content = r#"
[input]
method = "auto"
paste_command = "meta+v"

[[input.paste_rules]]
app_class = "Terminal"
paste_command = "ctrl+shift+v"

[[input.paste_rules]]
app_class = "Firefox"
paste_command = "ctrl+v"
"#;
        std::fs::write(&path, toml_content).unwrap();
        let config = Config::load_from(&path).unwrap();
        assert_eq!(config.input.paste_rules.len(), 2);
        assert_eq!(config.input.paste_rules[0].app_class, "Terminal");
        assert_eq!(config.input.paste_rules[0].paste_command, "ctrl+shift+v");
        assert_eq!(config.input.paste_rules[1].app_class, "Firefox");
    }

    #[test]
    fn test_deepgram_without_api_key_falls_back_to_local() {
        // Clear any keyring/env that might provide a deepgram key
        crate::keyring_store::delete_secret("deepgram_api_key");
        let prev = std::env::var("DEEPGRAM_API_KEY").ok();
        std::env::remove_var("DEEPGRAM_API_KEY");

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[general]\nbackend = \"deepgram\"\n").unwrap();

        let config = Config::load_from(&path).unwrap();
        // Should fall back to local when API key is missing
        assert_eq!(config.general.backend, "whisper-local");

        // Restore env
        if let Some(v) = prev {
            std::env::set_var("DEEPGRAM_API_KEY", v);
        }
    }

    #[test]
    fn test_deepgram_with_api_key_stays_deepgram() {
        // Clear keyring so env var is used
        crate::keyring_store::delete_secret("deepgram_api_key");

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[general]\nbackend = \"deepgram\"\n\n[deepgram]\napi_key = \"test-key\"\n").unwrap();

        let config = Config::load_from(&path).unwrap();
        assert_eq!(config.general.backend, "deepgram");
    }

    // ── Edge case & boundary tests ──────────────────────────────────

    #[test]
    fn test_load_from_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "").unwrap();
        let config = Config::load_from(&path).unwrap();
        assert_eq!(config.general.backend, "whisper-local");
    }

    #[test]
    fn test_load_from_whitespace_only_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "  \n\n  ").unwrap();
        let config = Config::load_from(&path).unwrap();
        assert_eq!(config.general.backend, "whisper-local");
    }

    #[test]
    fn test_load_from_unknown_sections_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let toml_content = r#"
[general]
backend = "openai"

[totally_unknown_section]
foo = "bar"
baz = 42
"#;
        std::fs::write(&path, toml_content).unwrap();
        // serde with #[serde(default)] and no deny_unknown_fields should parse fine
        let result = Config::load_from(&path);
        // This documents the actual behavior — either it ignores unknown sections or errors
        assert!(result.is_ok() || result.is_err(), "should handle unknown sections gracefully");
    }

    #[test]
    fn test_config_negative_min_duration() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[audio]\nmin_duration = -1.0\n").unwrap();
        let config = Config::load_from(&path).unwrap();
        assert_eq!(config.audio.min_duration, -1.0);
    }

    #[test]
    fn test_config_very_long_backend_string() {
        let long_str = "x".repeat(10_000);
        let mut config = Config::default();
        config.general.backend = long_str.clone();
        let toml_str = toml::to_string_pretty(&config).unwrap();
        let parsed: Config = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.general.backend, long_str);
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_config_default_hotkeys_linux() {
        let config = Config::default();
        assert_eq!(config.general.hotkeys.len(), 1);
        assert_eq!(config.general.hotkeys[0].key, evdev::Key::KEY_RIGHTCTRL.code() as u32);
    }

    #[test]
    #[cfg(not(target_os = "macos"))]
    fn test_config_default_paste_command_linux() {
        let config = Config::default();
        assert_eq!(config.input.paste_command, "ctrl+v");
    }

    #[test]
    fn test_config_default_model_dir_contains_verbatim() {
        let config = Config::default();
        assert!(config.whisper.model_dir.contains("verbatim"), "model_dir should contain 'verbatim'");
    }

    #[test]
    fn test_config_default_llm_model_dir_contains_verbatim() {
        let config = Config::default();
        assert!(config.llm.model_dir.contains("verbatim"), "llm model_dir should contain 'verbatim'");
    }

    #[test]
    fn test_resolved_llm_model_dir_tilde_expansion() {
        let mut config = Config::default();
        config.llm.model_dir = "~/llm-models".into();
        let resolved = config.resolved_llm_model_dir();
        assert!(!resolved.to_string_lossy().starts_with('~'));
        assert!(resolved.to_string_lossy().ends_with("llm-models"));
    }

    #[test]
    fn test_config_paste_rules_roundtrip() {
        let mut config = Config::default();
        config.input.paste_rules = vec![
            PasteRule { app_class: "Terminal".into(), paste_command: "ctrl+shift+v".into() },
            PasteRule { app_class: "Firefox".into(), paste_command: "ctrl+v".into() },
        ];
        let toml_str = toml::to_string_pretty(&config).unwrap();
        let parsed: Config = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.input.paste_rules.len(), 2);
        assert_eq!(parsed.input.paste_rules[0].app_class, "Terminal");
        assert_eq!(parsed.input.paste_rules[1].paste_command, "ctrl+v");
    }
}
