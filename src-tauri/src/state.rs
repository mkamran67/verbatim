use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8};
use std::sync::Arc;
use tokio::sync::{mpsc, watch, Mutex};
use verbatim_core::app::{SttCommand, SttEvent};
use verbatim_core::config::Config;
use verbatim_core::db::SharedDatabase;
use verbatim_core::hotkey::CaptureSlot;
use verbatim_core::rotation::RotationState;

pub struct CachedBalance {
    pub balance: f64,
    pub currency: String,
    /// "balance" (real remaining credits) or "estimated_cost" (fallback spend).
    pub kind: String,
    pub checked_at: chrono::DateTime<chrono::Local>,
    pub checked_at_instant: std::time::Instant,
}

pub struct BalanceCache {
    pub deepgram: Option<CachedBalance>,
}

impl BalanceCache {
    pub fn new() -> Self {
        Self {
            deepgram: None,
        }
    }
}

pub struct AppState {
    pub stt_cmd_tx: mpsc::UnboundedSender<SttCommand>,
    pub stt_event_rx: Arc<Mutex<mpsc::UnboundedReceiver<SttEvent>>>,
    pub config: Arc<Mutex<Config>>,
    pub db: SharedDatabase,
    pub download_cancel_tx: Arc<Mutex<Option<watch::Sender<bool>>>>,
    pub mic_monitor_stop: Arc<Mutex<Option<tokio::sync::oneshot::Sender<()>>>>,
    pub mic_monitor_level: Arc<Mutex<Option<Arc<AtomicU32>>>>,
    pub balance_cache: Arc<std::sync::Mutex<BalanceCache>>,
    /// Set to true once the STT backend is ready (tracks whether STT is active).
    pub stt_backend_ready: Arc<AtomicBool>,
    /// Handle to a managed `ollama serve` process, if Verbatim is managing it.
    pub ollama_child: Arc<Mutex<Option<tokio::process::Child>>>,
    /// Capture slot for the push-to-talk listener. Armed by `capture_hotkey`
    /// when the user is rebinding a PTT key.
    pub ptt_capture: CaptureSlot,
    /// Capture slot for the hands-free listener.
    pub handsfree_capture: CaptureSlot,
    /// Live RMS mic level (f32 bits, 0.0..1.0) populated by the audio capture
    /// callback while a recording is active. Read by the macOS tray
    /// waveform animation.
    pub recording_level: Arc<AtomicU32>,
    /// Latest STT app state, mirrored from the event stream. 0=Idle, 1=Recording, 2=Processing.
    /// Read by the macOS tray icon to drive its status row + recording animation.
    pub current_app_state: Arc<AtomicU8>,
    /// In-process rotation state. Mutated by `record_provider_failure` /
    /// `record_provider_success` and read by `get_rotation_status`.
    pub rotation: Arc<std::sync::Mutex<RotationState>>,
}

pub fn encode_app_state(s: verbatim_core::app::AppState) -> u8 {
    match s {
        verbatim_core::app::AppState::Idle => 0,
        verbatim_core::app::AppState::Recording => 1,
        verbatim_core::app::AppState::Processing => 2,
    }
}

pub fn decode_app_state(v: u8) -> verbatim_core::app::AppState {
    match v {
        1 => verbatim_core::app::AppState::Recording,
        2 => verbatim_core::app::AppState::Processing,
        _ => verbatim_core::app::AppState::Idle,
    }
}

