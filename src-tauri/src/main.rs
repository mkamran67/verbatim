// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod state;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{Emitter, Manager};
use tokio::sync::Mutex;

fn main() {
    // Workaround for webkit2gtk compositing crashes on Linux
    // ("Error flushing display: Broken pipe")
    #[cfg(target_os = "linux")]
    {
        if std::env::var("WEBKIT_DISABLE_COMPOSITING_MODE").is_err() {
            std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
        }
        if std::env::var("WEBKIT_DISABLE_DMABUF_RENDERER").is_err() {
            std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        }
    }

    // Set up logging to both stderr and a rotating log file
    let log_dir = dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("~/.local/share"))
        .join("verbatim/logs");
    let file_appender = tracing_appender::rolling::daily(&log_dir, "verbatim.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;

    tracing_subscriber::registry()
        .with(env_filter)
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stderr),
        )
        .with(
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(non_blocking),
        )
        .init();

    tracing::debug!("tracing subscriber initialized (logs: {})", log_dir.display());

    let config = verbatim_core::config::Config::load().expect("Failed to load config");
    tracing::debug!(
        backend = %config.general.backend,
        hotkeys = ?config.general.hotkeys,
        language = %config.general.language,
        "config loaded"
    );

    // Platform checks
    let display_server = verbatim_core::platform::detect_display_server();
    tracing::info!("Detected display server: {:?}", display_server);
    for warning in verbatim_core::platform::check_input_requirements(&display_server) {
        tracing::warn!("{}", warning);
    }

    // Open database
    let database = verbatim_core::db::Database::open_shared().expect("Failed to open database");
    tracing::debug!("database opened");

    // Create channels for STT service <-> GUI communication
    tracing::debug!("creating STT service channels");
    let (stt_event_tx, stt_event_rx) = tokio::sync::mpsc::unbounded_channel();
    let (stt_cmd_tx, stt_cmd_rx) = tokio::sync::mpsc::unbounded_channel();

    // Spawn the STT service in a background thread
    tracing::debug!("spawning STT service thread");
    let stt_config = config.clone();
    let stt_db = database.clone();
    std::thread::Builder::new()
        .name("stt-runtime".into())
        .spawn(move || {
            let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
            let service = verbatim_core::app::SttService::new(
                stt_config,
                stt_event_tx,
                stt_cmd_rx,
                Some(stt_db),
            );
            if let Err(e) = rt.block_on(service.run()) {
                tracing::error!("STT service error: {}", e);
            }
        })
        .expect("Failed to spawn STT thread");

    let llm_gpu_fallback = Arc::new(AtomicBool::new(false));
    let stt_backend_ready = Arc::new(AtomicBool::new(false));

    let app_state = state::AppState {
        stt_cmd_tx,
        stt_event_rx: Arc::new(Mutex::new(stt_event_rx)),
        config: Arc::new(Mutex::new(config)),
        db: database,
        download_cancel_tx: Arc::new(Mutex::new(None)),
        llm_download_cancel_tx: Arc::new(Mutex::new(None)),
        mic_monitor_stop: Arc::new(Mutex::new(None)),
        mic_monitor_level: Arc::new(Mutex::new(None)),
        balance_cache: Arc::new(std::sync::Mutex::new(state::BalanceCache::new())),
        llm_gpu_fallback: llm_gpu_fallback.clone(),
        stt_backend_ready: stt_backend_ready.clone(),
    };

    tracing::debug!("building Tauri application");
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::save_config,
            commands::get_stats,
            commands::get_daily_word_stats,
            commands::get_transcriptions_for_date,
            commands::get_daily_token_usage,
            commands::get_token_usage_by_model,
            commands::get_recent,
            commands::search_history,
            commands::delete_transcription,
            commands::pause_hotkey,
            commands::resume_hotkey,
            commands::list_audio_devices,
            commands::list_open_windows,
            commands::list_models,
            commands::download_model,
            commands::cancel_model_download,
            commands::delete_model,
            commands::list_llm_models,
            commands::download_llm_model,
            commands::cancel_llm_model_download,
            commands::delete_llm_model,
            commands::get_system_info,
            commands::start_mic_monitor,
            commands::stop_mic_monitor,
            commands::get_mic_level,
            commands::check_macos_permissions,
            commands::open_macos_settings,
            commands::check_deepgram_balance,
            commands::check_openai_costs,
            commands::get_daily_cost_summary,
            commands::get_cost_by_provider,
            commands::check_for_update,
            commands::get_debug_info,
        ])
        .setup(move |app| {
            tracing::debug!("Tauri setup: starting STT event forwarding loop");
            // Forward STT events to the frontend, intercepting GPU status events
            let handle = app.handle().clone();
            let rx = app.state::<state::AppState>().stt_event_rx.clone();
            let llm_fallback = llm_gpu_fallback.clone();
            let stt_ready = stt_backend_ready.clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    let event = {
                        let mut guard = rx.lock().await;
                        guard.recv().await
                    };
                    match event {
                        Some(evt) => {
                            // Track GPU status from events
                            match &evt {
                                verbatim_core::app::SttEvent::GpuFallback(_) => {
                                    llm_fallback.store(true, Ordering::Relaxed);
                                }
                                verbatim_core::app::SttEvent::BackendReady(_) => {
                                    stt_ready.store(true, Ordering::Relaxed);
                                }
                                _ => {}
                            }
                            tracing::trace!("forwarding STT event to frontend: {:?}", evt);
                            if let Err(e) = handle.emit("stt-event", &evt) {
                                tracing::error!("Failed to emit stt-event: {}", e);
                            }
                        }
                        None => break,
                    }
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
