use std::sync::atomic::{AtomicBool, AtomicU32};
use std::sync::Arc;
use tokio::sync::{mpsc, watch, Mutex};
use verbatim_core::app::{SttCommand, SttEvent};
use verbatim_core::config::Config;
use verbatim_core::db::SharedDatabase;

pub struct CachedBalance {
    pub balance: f64,
    pub currency: String,
    pub checked_at: chrono::DateTime<chrono::Local>,
    pub checked_at_instant: std::time::Instant,
}

pub struct BalanceCache {
    pub deepgram: Option<CachedBalance>,
    pub openai: Option<CachedBalance>,
}

impl BalanceCache {
    pub fn new() -> Self {
        Self {
            deepgram: None,
            openai: None,
        }
    }
}

pub struct AppState {
    pub stt_cmd_tx: mpsc::UnboundedSender<SttCommand>,
    pub stt_event_rx: Arc<Mutex<mpsc::UnboundedReceiver<SttEvent>>>,
    pub config: Arc<Mutex<Config>>,
    pub db: SharedDatabase,
    pub download_cancel_tx: Arc<Mutex<Option<watch::Sender<bool>>>>,
    pub llm_download_cancel_tx: Arc<Mutex<Option<watch::Sender<bool>>>>,
    pub mic_monitor_stop: Arc<Mutex<Option<tokio::sync::oneshot::Sender<()>>>>,
    pub mic_monitor_level: Arc<Mutex<Option<Arc<AtomicU32>>>>,
    pub balance_cache: Arc<std::sync::Mutex<BalanceCache>>,
    /// Set to true when the LLM model falls back to CPU-only mode.
    pub llm_gpu_fallback: Arc<AtomicBool>,
    /// Set to true once the STT backend is ready (tracks whether STT is active).
    pub stt_backend_ready: Arc<AtomicBool>,
}
