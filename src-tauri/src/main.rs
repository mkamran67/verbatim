// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod chime;
mod commands;
mod state;
#[cfg(target_os = "macos")]
mod tray;

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};
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

    tracing::info!("tracing subscriber initialized (logs: {})", log_dir.display());

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
    // Capture slots: shared between the STT listener threads and the
    // `capture_hotkey` IPC command so the UI can ask "what key did the user
    // just press?" without a static keycode whitelist.
    let ptt_capture = verbatim_core::hotkey::CaptureSlot::new();
    let handsfree_capture = verbatim_core::hotkey::CaptureSlot::new();
    let ptt_capture_for_state = ptt_capture.clone();
    let handsfree_capture_for_state = handsfree_capture.clone();
    // Shared live RMS mic level. The audio capture callback writes here while
    // recording; consumers like the macOS tray waveform animation read it.
    let recording_level = Arc::new(AtomicU32::new(0));
    let recording_level_for_stt = recording_level.clone();
    std::thread::Builder::new()
        .name("stt-runtime".into())
        .spawn(move || {
            let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
            let service = verbatim_core::app::SttService::new_with_level(
                stt_config,
                stt_event_tx,
                stt_cmd_rx,
                Some(stt_db),
                ptt_capture,
                handsfree_capture,
                recording_level_for_stt,
            );
            if let Err(e) = rt.block_on(service.run()) {
                tracing::error!("STT service error: {}", e);
            }
        })
        .expect("Failed to spawn STT thread");

    let stt_backend_ready = Arc::new(AtomicBool::new(false));

    let app_state = state::AppState {
        stt_cmd_tx,
        stt_event_rx: Arc::new(Mutex::new(stt_event_rx)),
        config: Arc::new(Mutex::new(config)),
        db: database,
        download_cancel_tx: Arc::new(Mutex::new(None)),
        mic_monitor_stop: Arc::new(Mutex::new(None)),
        mic_monitor_level: Arc::new(Mutex::new(None)),
        balance_cache: Arc::new(std::sync::Mutex::new(state::BalanceCache::new())),
        stt_backend_ready: stt_backend_ready.clone(),
        ollama_child: Arc::new(Mutex::new(None)),
        ptt_capture: ptt_capture_for_state,
        handsfree_capture: handsfree_capture_for_state,
        recording_level: recording_level.clone(),
        current_app_state: Arc::new(AtomicU8::new(0)),
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
            commands::get_daily_provider_usage,
            commands::get_token_usage_by_model,
            commands::get_recent,
            commands::search_history,
            commands::delete_transcription,
            commands::pause_hotkey,
            commands::resume_hotkey,
            commands::capture_hotkey,
            commands::list_audio_devices,
            commands::list_open_windows,
            commands::list_models,
            commands::download_model,
            commands::cancel_model_download,
            commands::delete_model,
            commands::ollama_detect,
            commands::ollama_install,
            commands::ollama_managed_installed,
            commands::ollama_start,
            commands::ollama_restart,
            commands::ollama_uninstall,
            commands::ollama_pull_model,
            commands::ollama_list_local,
            commands::ollama_delete_model,
            commands::ollama_search_registry,
            commands::get_system_info,
            commands::start_mic_monitor,
            commands::stop_mic_monitor,
            commands::get_mic_level,
            commands::check_macos_permissions,
            commands::check_linux_input_permission,
            commands::open_macos_settings,
            commands::check_deepgram_balance,
            commands::get_daily_cost_summary,
            commands::get_cost_by_provider,
            commands::check_for_update,
            commands::get_debug_info,
            commands::open_path,
        ])
        .setup(move |app| {
            tracing::debug!("Tauri setup: starting STT event forwarding loop");

            // macOS menu-bar tray icon. Returns handles the STT event loop
            // uses to drive the recording animation + status text.
            #[cfg(target_os = "macos")]
            let tray_handles = match tray::install(
                app.handle(),
                app.state::<state::AppState>().recording_level.clone(),
            ) {
                Ok(h) => Some(h),
                Err(e) => {
                    tracing::warn!("failed to install macOS tray: {}", e);
                    None
                }
            };

            // Forward STT events to the frontend, intercepting backend-ready signal
            let handle = app.handle().clone();
            let rx = app.state::<state::AppState>().stt_event_rx.clone();
            let stt_ready = stt_backend_ready.clone();
            #[cfg(target_os = "macos")]
            let tray_handles_for_loop = tray_handles;
            tauri::async_runtime::spawn(async move {
                loop {
                    let event = {
                        let mut guard = rx.lock().await;
                        guard.recv().await
                    };
                    match event {
                        Some(evt) => {
                            if matches!(&evt, verbatim_core::app::SttEvent::BackendReady(_)) {
                                stt_ready.store(true, Ordering::Relaxed);
                            }
                            // Mirror state changes into AppState so other code can react,
                            // and play start/stop chimes on the relevant transitions.
                            if let verbatim_core::app::SttEvent::StateChanged(app_state) = &evt {
                                use verbatim_core::app::AppState as CoreAppState;
                                let cfg_state = handle.state::<state::AppState>();
                                let prev_encoded = cfg_state
                                    .current_app_state
                                    .swap(state::encode_app_state(*app_state), Ordering::Relaxed);
                                let prev = state::decode_app_state(prev_encoded);
                                match (prev, *app_state) {
                                    (CoreAppState::Idle, CoreAppState::Recording) => {
                                        chime::play(chime::Chime::Start);
                                    }
                                    (CoreAppState::Recording, CoreAppState::Processing)
                                    | (CoreAppState::Recording, CoreAppState::Idle) => {
                                        chime::play(chime::Chime::Stop);
                                    }
                                    _ => {}
                                }
                            }

                            // Drive the macOS tray status row + recording
                            // animation off the same event stream.
                            #[cfg(target_os = "macos")]
                            if let (Some(handles), verbatim_core::app::SttEvent::StateChanged(app_state)) =
                                (tray_handles_for_loop.as_ref(), &evt)
                            {
                                use verbatim_core::app::AppState as CoreAppState;
                                handles
                                    .is_recording
                                    .store(*app_state == CoreAppState::Recording, Ordering::Relaxed);
                                handles
                                    .is_processing
                                    .store(*app_state == CoreAppState::Processing, Ordering::Relaxed);
                                let label = match app_state {
                                    CoreAppState::Idle => "Idle",
                                    CoreAppState::Recording => "Recording…",
                                    CoreAppState::Processing => "Processing…",
                                };
                                let status = handles.status_item.clone();
                                let label = label.to_string();
                                tauri::async_runtime::spawn(async move {
                                    let item = status.lock().await;
                                    if let Err(e) = item.set_text(&label) {
                                        tracing::warn!("tray status set_text failed: {}", e);
                                    }
                                });

                                // "Start recording" while idle/processing,
                                // "Stop recording" while actively recording.
                                let toggle_label = match app_state {
                                    CoreAppState::Recording => "Stop recording",
                                    _ => "Start recording",
                                };
                                let toggle = handles.toggle_item.clone();
                                let toggle_label = toggle_label.to_string();
                                tauri::async_runtime::spawn(async move {
                                    let item = toggle.lock().await;
                                    if let Err(e) = item.set_text(&toggle_label) {
                                        tracing::warn!("tray toggle set_text failed: {}", e);
                                    }
                                });
                            }
                            // Auto-rule persistence: when the STT thread detects an
                            // unconfigured Linux terminal, it has already augmented its
                            // own in-memory rules. We mirror that into AppState.config
                            // and disk so the rule survives restarts.
                            if let verbatim_core::app::SttEvent::AutoPasteRuleAdded { app_class, paste_command } = &evt {
                                let state = handle.state::<state::AppState>();
                                let mut cfg = state.config.lock().await;
                                let already = cfg.input.paste_rules.iter().any(|r| {
                                    r.app_class.eq_ignore_ascii_case(app_class)
                                });
                                if !already {
                                    tracing::info!(%app_class, %paste_command, "persisting auto-detected terminal paste rule");
                                    cfg.input.paste_rules.push(verbatim_core::config::PasteRule {
                                        app_class: app_class.clone(),
                                        paste_command: paste_command.clone(),
                                    });
                                    if let Err(e) = cfg.save() {
                                        tracing::warn!(error = %e, "failed to persist auto-detected paste rule");
                                    }
                                    let _ = handle.emit("config-changed", ());
                                }
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

            // Auto-spawn managed Ollama on startup if configured + binary exists.
            let auto_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let state = auto_handle.state::<state::AppState>();
                let (provider, mode) = {
                    let cfg = state.config.lock().await;
                    (cfg.post_processing.provider.clone(), cfg.post_processing.ollama_mode.clone())
                };
                if provider != "ollama" || mode != "managed" {
                    return;
                }
                let data_dir = match dirs::data_dir() {
                    Some(d) => d,
                    None => return,
                };
                let bin = verbatim_core::ollama_manager::managed_binary(&data_dir);
                if !bin.exists() {
                    tracing::debug!("managed Ollama not yet installed; skipping startup spawn");
                    return;
                }
                tracing::info!("auto-spawning managed Ollama on startup");
                if let Err(e) = commands::spawn_managed_ollama(&auto_handle, &state).await {
                    tracing::error!("managed Ollama startup spawn failed: {}", e);
                }
            });

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            // macOS: pressing the window's red close button hides the window
            // instead of quitting the app — Verbatim keeps running in the
            // menu bar. Only the tray "Quit Verbatim" item (or Cmd+Q via the
            // app menu, which calls `app.exit(0)`) actually exits.
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::WindowEvent {
                label,
                event: tauri::WindowEvent::CloseRequested { api, .. },
                ..
            } = &event
            {
                if label == "main" {
                    if let Some(window) = app_handle.get_webview_window("main") {
                        let _ = window.hide();
                    }
                    api.prevent_close();
                    return;
                }
            }

            if let tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit = event {
                let state = app_handle.state::<state::AppState>();
                let child_arc = state.ollama_child.clone();
                // Best-effort: synchronously block on shutdown so the child
                // doesn't outlive the app process.
                tauri::async_runtime::block_on(async move {
                    let mut guard = child_arc.lock().await;
                    if let Some(mut child) = guard.take() {
                        tracing::info!("shutting down managed Ollama");
                        verbatim_core::ollama_manager::shutdown(&mut child).await;
                    }
                });
            }
        });
}
