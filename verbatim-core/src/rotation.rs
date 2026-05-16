//! Provider rotation engine.
//!
//! Holds in-process state about which providers are currently cooling down
//! (after a failure) and which provider the app has stuck to after a
//! failover. Pure logic — emitting Tauri events is the caller's job.

use crate::config::Config;
use crate::provider_error::ProviderFailure;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Provider id ↔ when its cooldown ends (`None` until restart for AuthError).
type Cooldowns = HashMap<String, CooldownEntry>;

#[derive(Debug, Clone)]
struct CooldownEntry {
    /// `None` = until restart (auth errors). `Some` = absolute deadline.
    until: Option<Instant>,
    last_failure: ProviderFailure,
}

#[derive(Debug, Default)]
pub struct RotationState {
    sticky_stt: Option<String>,
    sticky_pp: Option<String>,
    cooldown: Cooldowns,
}

impl RotationState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Reset rotation state (e.g. when config changes drastically).
    pub fn reset(&mut self) {
        self.sticky_stt = None;
        self.sticky_pp = None;
        self.cooldown.clear();
    }

    /// True when the provider is currently cooling down.
    pub fn is_cooling(&self, provider: &str) -> bool {
        match self.cooldown.get(provider) {
            None => false,
            Some(entry) => match entry.until {
                None => true, // auth error: locked until restart
                Some(deadline) => Instant::now() < deadline,
            },
        }
    }

    /// Status snapshot for a given provider (`active` / `cooling` / `exhausted`).
    pub fn status_label(&self, provider: &str) -> &'static str {
        match self.cooldown.get(provider) {
            None => "active",
            Some(e) => match e.last_failure {
                ProviderFailure::Exhausted => "exhausted",
                ProviderFailure::AuthError => "auth_error",
                _ if self.is_cooling(provider) => "cooling",
                _ => "active",
            },
        }
    }

    /// Pick the active STT backend. When rotation is disabled, always returns
    /// the user-configured `general.backend`. When enabled, walks `stt_order`
    /// skipping providers that are currently cooling down, preferring the
    /// sticky failover choice if it is still healthy.
    pub fn pick_stt(&self, cfg: &Config) -> String {
        if !cfg.rotation.enabled {
            return cfg.general.backend.clone();
        }
        // Prefer sticky if still healthy.
        if let Some(s) = &self.sticky_stt {
            if !self.is_cooling(s) && stt_has_credentials(cfg, s) {
                return s.clone();
            }
        }
        // Otherwise walk the configured order.
        for id in &cfg.rotation.stt_order {
            if !self.is_cooling(id) && stt_has_credentials(cfg, id) {
                return id.clone();
            }
        }
        // Hard fallback: whisper-local is always available.
        "whisper-local".to_string()
    }

    /// Pick the active post-processing provider. When rotation is disabled,
    /// returns the user-configured `post_processing.provider`.
    pub fn pick_pp(&self, cfg: &Config) -> String {
        if !cfg.rotation.enabled {
            return cfg.post_processing.provider.clone();
        }
        if let Some(s) = &self.sticky_pp {
            if !self.is_cooling(s) && pp_has_credentials(cfg, s) {
                return s.clone();
            }
        }
        for id in &cfg.rotation.pp_order {
            if !self.is_cooling(id) && pp_has_credentials(cfg, id) {
                return id.clone();
            }
        }
        // Hard fallback: ollama runs locally without credentials.
        "ollama".to_string()
    }

    /// Record a provider failure and update cooldown + sticky state.
    /// Returns `true` if the caller should advance to the next provider.
    pub fn record_failure(
        &mut self,
        kind: ProviderKind,
        provider: &str,
        failure: ProviderFailure,
    ) -> bool {
        let until = cooldown_for(failure);
        self.cooldown.insert(
            provider.to_string(),
            CooldownEntry { until, last_failure: failure },
        );
        match kind {
            ProviderKind::Stt => {
                if self.sticky_stt.as_deref() == Some(provider) {
                    self.sticky_stt = None;
                }
            }
            ProviderKind::PostProcessing => {
                if self.sticky_pp.as_deref() == Some(provider) {
                    self.sticky_pp = None;
                }
            }
        }
        matches!(
            failure,
            ProviderFailure::Exhausted
                | ProviderFailure::AuthError
                | ProviderFailure::RateLimited
                | ProviderFailure::Transient
        )
    }

    /// Mark a successful call: clears cooldown and updates sticky pointer.
    pub fn record_success(&mut self, kind: ProviderKind, provider: &str) {
        self.cooldown.remove(provider);
        match kind {
            ProviderKind::Stt => self.sticky_stt = Some(provider.to_string()),
            ProviderKind::PostProcessing => self.sticky_pp = Some(provider.to_string()),
        }
    }

    /// Force a provider into the exhausted state (used by the UI when a
    /// manual balance crosses zero). Treats it as if the provider had
    /// returned an out-of-quota response.
    pub fn force_exhaust(&mut self, provider: &str) {
        self.cooldown.insert(
            provider.to_string(),
            CooldownEntry {
                until: cooldown_for(ProviderFailure::Exhausted),
                last_failure: ProviderFailure::Exhausted,
            },
        );
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ProviderKind {
    Stt,
    PostProcessing,
}

fn cooldown_for(failure: ProviderFailure) -> Option<Instant> {
    match failure {
        // Until end of UTC day (best-effort: 12h cap as a simple heuristic).
        ProviderFailure::Exhausted => Some(Instant::now() + Duration::from_secs(12 * 3600)),
        ProviderFailure::AuthError => None, // until restart
        ProviderFailure::RateLimited => Some(Instant::now() + Duration::from_secs(60)),
        ProviderFailure::Transient => Some(Instant::now() + Duration::from_secs(30)),
        ProviderFailure::Other => Some(Instant::now() + Duration::from_secs(30)),
    }
}

fn stt_has_credentials(cfg: &Config, id: &str) -> bool {
    match id {
        "whisper-local" => true,
        "openai" => !cfg.openai.api_key.is_empty(),
        "deepgram" => !cfg.deepgram.api_key.is_empty(),
        "smallest" => !cfg.smallest.api_key.is_empty(),
        _ => false,
    }
}

fn pp_has_credentials(cfg: &Config, id: &str) -> bool {
    match id {
        "openai" => !cfg.openai.api_key.is_empty(),
        "ollama" => true, // local
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg_with_rotation(enabled: bool) -> Config {
        let mut c = Config::default();
        c.rotation.enabled = enabled;
        c.openai.api_key = "sk-x".into();
        c.deepgram.api_key = "dg-x".into();
        c.smallest.api_key = "sm-x".into();
        c
    }

    #[test]
    fn pick_stt_respects_disabled_rotation() {
        let mut c = cfg_with_rotation(false);
        c.general.backend = "openai".into();
        let state = RotationState::new();
        assert_eq!(state.pick_stt(&c), "openai");
    }

    #[test]
    fn pick_stt_skips_cooling_providers() {
        let c = cfg_with_rotation(true);
        let mut state = RotationState::new();
        state.record_failure(ProviderKind::Stt, "whisper-local", ProviderFailure::Transient);
        // Next in default order is openai.
        assert_eq!(state.pick_stt(&c), "openai");
    }

    #[test]
    fn pick_stt_falls_back_to_whisper_when_all_cooling() {
        let c = cfg_with_rotation(true);
        let mut state = RotationState::new();
        for id in ["openai", "deepgram", "smallest"] {
            state.record_failure(ProviderKind::Stt, id, ProviderFailure::Exhausted);
        }
        // whisper-local is first in default order and not cooling.
        assert_eq!(state.pick_stt(&c), "whisper-local");
    }

    #[test]
    fn record_success_clears_cooldown_and_sets_sticky() {
        let c = cfg_with_rotation(true);
        let mut state = RotationState::new();
        state.record_failure(ProviderKind::Stt, "openai", ProviderFailure::Transient);
        assert!(state.is_cooling("openai"));
        state.record_success(ProviderKind::Stt, "openai");
        assert!(!state.is_cooling("openai"));
        assert_eq!(state.pick_stt(&c), "openai");
    }

    #[test]
    fn auth_error_locks_until_restart() {
        let mut state = RotationState::new();
        state.record_failure(ProviderKind::Stt, "openai", ProviderFailure::AuthError);
        // No deadline → always cooling.
        assert!(state.is_cooling("openai"));
        assert_eq!(state.status_label("openai"), "auth_error");
    }

    #[test]
    fn force_exhaust_marks_exhausted() {
        let mut state = RotationState::new();
        state.force_exhaust("deepgram");
        assert_eq!(state.status_label("deepgram"), "exhausted");
    }

    #[test]
    fn pick_pp_skips_missing_credentials() {
        let mut c = cfg_with_rotation(true);
        c.openai.api_key = "".into();
        // pp_order is [openai, ollama], openai has no key → ollama.
        let state = RotationState::new();
        assert_eq!(state.pick_pp(&c), "ollama");
    }
}
