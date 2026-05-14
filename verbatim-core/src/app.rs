use anyhow::{bail, Context, Result};
use std::sync::Arc;
use tokio::sync::{mpsc, watch};

use crate::audio::capture::{self, AudioBuffer};
use crate::clipboard;
use crate::config::Config;
use crate::db::{SharedDatabase, Transcription};
use crate::input::enigo_backend::EnigoBackend;
use crate::input::InputMethod;
use crate::model_manager;
use crate::post_processing::PostProcessor;
use crate::stt::deepgram::DeepgramStt;
use crate::stt::openai::OpenAiWhisper;
use crate::stt::smallest::SmallestStt;
use crate::stt::whisper_local::WhisperLocal;
use crate::stt::SttBackend;

#[cfg(target_os = "linux")]
use crate::hotkey::evdev_listener;
#[cfg(target_os = "macos")]
use crate::hotkey::macos_listener;

use crate::hotkey::{CaptureSlot, HotkeyEvent};

/// Determines if an STT error should trigger fallback to local whisper.
/// Cloud errors (API, network) are eligible; local errors (model not found, bad audio) are not.
fn is_fallback_eligible(err: &crate::errors::SttError) -> bool {
    matches!(err, crate::errors::SttError::ApiError(_) | crate::errors::SttError::NetworkError(_))
}

/// Parameters needed for transcription processing, to avoid passing many individual args.
struct ProcessContext<'a> {
    audio_buffer: &'a AudioBuffer,
    recording_tx: &'a watch::Sender<bool>,
    backend: &'a Arc<dyn SttBackend>,
    backend_name: &'a str,
    backend_model: &'a str,
    /// Local whisper fallback — used when a cloud backend fails (auth, network, credits).
    fallback_backend: &'a Option<Arc<dyn SttBackend>>,
    post_processor: &'a Option<Arc<PostProcessor>>,
    gui_tx: &'a mpsc::UnboundedSender<SttEvent>,
    db: &'a Option<SharedDatabase>,
    clipboard_only: bool,
    input_method: &'a str,
    paste_command: &'a str,
    paste_rules: &'a [crate::config::PasteRule],
    min_duration: f32,
    energy_threshold: f32,
    noise_cancellation: bool,
    language: &'a Option<String>,
}

/// Stop recording, transcribe, post-process, output, and store in DB.
/// Returns the new AppState (always Idle).
async fn process_and_output(ctx: &ProcessContext<'_>) -> Result<AppState> {
    tracing::debug!(
        backend = ctx.backend_name,
        clipboard_only = ctx.clipboard_only,
        min_duration = ctx.min_duration,
        energy_threshold = ctx.energy_threshold,
        language = ?ctx.language,
        "process_and_output started"
    );
    let process_start = std::time::Instant::now();

    let _ = ctx.recording_tx.send(false);

    let samples = ctx.audio_buffer.take();
    let sample_count = samples.len();
    let duration_secs = sample_count as f32 / 16_000.0;
    tracing::debug!(sample_count, duration_secs = format_args!("{:.1}", duration_secs), "audio samples collected");

    // Gate silent / too-short clips BEFORE announcing Processing so the UI
    // doesn't flicker into a processing state when there's nothing to do.
    if samples.is_empty() || duration_secs < ctx.min_duration {
        if samples.is_empty() {
            tracing::warn!("No audio captured");
        } else {
            tracing::info!("Recording too short ({:.1}s < {:.1}s threshold), ignoring", duration_secs, ctx.min_duration);
        }
        let _ = ctx.gui_tx.send(SttEvent::StateChanged(AppState::Idle));
        return Ok(AppState::Idle);
    }

    if !crate::audio::silence::has_voiced_content(&samples, 16_000) {
        tracing::info!(
            duration_secs = format_args!("{:.2}", duration_secs),
            "no voiced content detected, skipping STT"
        );
        let _ = ctx.gui_tx.send(SttEvent::StateChanged(AppState::Idle));
        return Ok(AppState::Idle);
    }

    // User-configurable energy threshold (applies on top of the voiced gate).
    if ctx.energy_threshold > 0.0 {
        let sum_sq: f32 = samples.iter().map(|s| s * s).sum();
        let rms = (sum_sq / samples.len() as f32).sqrt();
        if rms < ctx.energy_threshold {
            tracing::info!(
                "Audio RMS {:.4} below threshold {:.4}, treating as silence",
                rms, ctx.energy_threshold
            );
            let _ = ctx.gui_tx.send(SttEvent::StateChanged(AppState::Idle));
            return Ok(AppState::Idle);
        }
    }

    let _ = ctx.gui_tx.send(SttEvent::StateChanged(AppState::Processing));
    tracing::info!("Processing {:.1}s of audio...", duration_secs);

    // Apply noise cancellation if enabled
    let samples = if ctx.noise_cancellation {
        tracing::info!("Applying noise cancellation...");
        crate::audio::noise_cancel::denoise(&samples)
    } else {
        samples
    };

    // Keep a copy of the samples in case we need to fallback to local whisper
    let has_fallback = ctx.fallback_backend.is_some();
    let samples_for_fallback = if has_fallback { Some(samples.clone()) } else { None };

    let lang = ctx.language.clone();
    let backend_clone = Arc::clone(ctx.backend);

    let transcription_start = std::time::Instant::now();
    let result = tokio::task::spawn_blocking(move || {
        tokio::runtime::Handle::current().block_on(async {
            backend_clone.transcribe(&samples, lang.as_deref()).await
        })
    })
    .await
    .context("Transcription task panicked")?;

    tracing::debug!(elapsed_ms = transcription_start.elapsed().as_millis(), "transcription task completed");

    // If the primary (cloud) backend failed with a retriable error and we have a
    // local whisper fallback available, retry with local and notify the user.
    let result = match (&result, ctx.fallback_backend, samples_for_fallback) {
        (Err(e), Some(fallback), Some(samples)) if is_fallback_eligible(e) => {
            let primary_err = e.to_string();
            tracing::warn!(
                primary_backend = ctx.backend_name,
                error = %primary_err,
                "Primary STT backend failed, falling back to local whisper"
            );
            let _ = ctx.gui_tx.send(SttEvent::TranscriptionError(
                format!("{} failed ({}), using local whisper instead", ctx.backend_name, primary_err)
            ));

            let lang = ctx.language.clone();
            let fb = Arc::clone(fallback);
            let fb_result = tokio::task::spawn_blocking(move || {
                tokio::runtime::Handle::current().block_on(async {
                    fb.transcribe(&samples, lang.as_deref()).await
                })
            })
            .await
            .context("Fallback transcription task panicked")?;

            match &fb_result {
                Ok(_) => tracing::info!("Fallback transcription succeeded"),
                Err(e) => tracing::error!("Fallback transcription also failed: {}", e),
            }
            fb_result
        }
        _ => result,
    };

    match result {
        Ok(text) if text.is_empty() => {
            tracing::info!("(no speech detected)");
        }
        Ok(text) => {
            let transcription_id = uuid::Uuid::new_v4().to_string();

            // Record STT API cost for cloud backends
            if ctx.backend_name == "openai" || ctx.backend_name == "deepgram" || ctx.backend_name == "smallest" {
                let provider = match ctx.backend_name {
                    "deepgram" => "deepgram",
                    "smallest" => "smallest",
                    _ => "openai-stt",
                };
                let model = ctx.backend_model;
                if let Some(ref db) = ctx.db {
                    if let Ok(db) = db.lock() {
                        if let Err(e) = db.insert_api_cost(
                            &transcription_id,
                            provider,
                            model,
                            duration_secs as f64,
                            0, 0,
                        ) {
                            tracing::error!("Failed to record STT API cost: {}", e);
                        }
                    }
                }
            }

            // Post-process if enabled. Deepgram already runs smart_format, so an
            // LLM pass on top is usually redundant, but we still let the user
            // opt in — the toggle in the PP page decides, not the STT backend.
            let mut post_processing_error: Option<String> = None;
            let raw_text_before_pp = text.clone();
            let text = if let Some(ref pp) = ctx.post_processor {
                tracing::info!(text_len = text.len(), model = %pp.model(), "about to call post_processor.process");
                let pp = Arc::clone(pp);
                let raw = text.clone();
                let pp_start = std::time::Instant::now();
                let result = pp.process(&raw).await;
                tracing::info!(
                    elapsed_ms = pp_start.elapsed().as_millis(),
                    err = result.error.is_some(),
                    "post_processor.process returned"
                );
                if result.text != raw {
                    tracing::info!("Post-processed: {} -> {}", raw, result.text);
                }
                if result.usage.total_tokens > 0 {
                    tracing::info!("Token usage: {} prompt + {} completion = {} total",
                        result.usage.prompt_tokens, result.usage.completion_tokens, result.usage.total_tokens);
                    if let Some(ref db) = ctx.db {
                        if let Ok(db) = db.lock() {
                            if let Err(e) = db.insert_token_usage(
                                &transcription_id,
                                pp.model(),
                                result.usage.prompt_tokens,
                                result.usage.completion_tokens,
                                result.usage.total_tokens,
                            ) {
                                tracing::error!("Failed to record token usage: {}", e);
                            }
                            // Record post-processing API cost
                            if let Err(e) = db.insert_api_cost(
                                &transcription_id,
                                "openai-postproc",
                                pp.model(),
                                0.0,
                                result.usage.prompt_tokens,
                                result.usage.completion_tokens,
                            ) {
                                tracing::error!("Failed to record post-processing API cost: {}", e);
                            }
                        }
                    }
                }
                post_processing_error = result.error;
                result.text
            } else {
                text
            };

            let word_count = text.split_whitespace().count();
            tracing::info!("Transcribed: {}", text);

            if ctx.clipboard_only {
                tracing::debug!("clipboard_only mode, copying to clipboard");
                if let Err(e) = clipboard::copy_to_clipboard(&text) {
                    tracing::error!("Clipboard error: {}", e);
                }
            } else {
                tracing::debug!(input_method = %ctx.input_method, paste_command = %ctx.paste_command, "paste mode: copying to clipboard then pasting");
                let previous_clipboard = clipboard::get_clipboard_text();

                // Auto-detect: if the focused app is a known Linux terminal
                // and the user has no rule covering it, synthesize one for
                // this paste and emit an event so it gets persisted.
                let augmented_rules: Vec<crate::config::PasteRule> = {
                    #[cfg(target_os = "linux")]
                    {
                        let mut rules = ctx.paste_rules.to_vec();
                        if let Some(active) = crate::input::window_detect::get_active_window_class() {
                            let active_trim = active.trim();
                            if !active_trim.is_empty()
                                && crate::input::terminal_detect::is_known_linux_terminal(active_trim)
                            {
                                let active_lower = active_trim.to_ascii_lowercase();
                                let already_covered = rules.iter().any(|r| {
                                    let r_lower = r.app_class.to_ascii_lowercase();
                                    active_lower.contains(&r_lower) || r_lower == active_lower
                                });
                                if !already_covered {
                                    let new_rule = crate::config::PasteRule {
                                        app_class: active_trim.to_string(),
                                        paste_command: crate::input::terminal_detect::DEFAULT_TERMINAL_PASTE_COMMAND.to_string(),
                                    };
                                    tracing::info!(
                                        app_class = %new_rule.app_class,
                                        paste_command = %new_rule.paste_command,
                                        "auto-detected unconfigured Linux terminal; adding paste rule"
                                    );
                                    let _ = ctx.gui_tx.send(SttEvent::AutoPasteRuleAdded {
                                        app_class: new_rule.app_class.clone(),
                                        paste_command: new_rule.paste_command.clone(),
                                    });
                                    rules.push(new_rule);
                                }
                            }
                        }
                        rules
                    }
                    #[cfg(not(target_os = "linux"))]
                    { ctx.paste_rules.to_vec() }
                };

                if let Err(e) = clipboard::copy_to_clipboard(&text) {
                    tracing::error!("Clipboard error: {}", e);
                } else {
                    match EnigoBackend::new(ctx.input_method, ctx.paste_command, &augmented_rules) {
                        Ok(mut input) => {
                            if let Err(e) = input.type_text(&text) {
                                tracing::error!("Typing error: {}", e);
                            }
                        }
                        Err(e) => {
                            tracing::error!("Failed to init keyboard input: {}", e);
                        }
                    }

                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

                    if let Err(e) = clipboard::restore_clipboard(previous_clipboard.as_deref()) {
                        tracing::error!("Failed to restore clipboard: {}", e);
                    }
                }
            }

            // Store in database
            tracing::debug!(id = %transcription_id, word_count, "storing transcription in database");
            if let Some(ref db) = ctx.db {
                let backend_name = ctx.backend_name.to_string();
                let language = ctx.language.clone();
                // Store raw STT text only when post-processing changed it
                let raw_text = if raw_text_before_pp != text {
                    Some(raw_text_before_pp.clone())
                } else {
                    None
                };
                let stt_model = if ctx.backend_model.is_empty() {
                    None
                } else {
                    Some(ctx.backend_model.to_string())
                };
                let pp_model = ctx.post_processor.as_ref().map(|pp| pp.model().to_string());
                let t = Transcription {
                    id: transcription_id.clone(),
                    text: text.clone(),
                    word_count: word_count as i64,
                    char_count: text.len() as i64,
                    duration_secs: duration_secs as f64,
                    backend: backend_name,
                    language,
                    prompt_tokens: 0,
                    completion_tokens: 0,
                    post_processing_error: post_processing_error.clone(),
                    raw_text,
                    stt_model,
                    pp_model,
                    created_at: chrono::Utc::now()
                        .format("%Y-%m-%d %H:%M:%S")
                        .to_string(),
                };
                if let Ok(db) = db.lock() {
                    if let Err(e) = db.insert_transcription(&t) {
                        tracing::error!("DB insert error: {}", e);
                    }
                }
            }

            let _ = ctx.gui_tx.send(SttEvent::TranscriptionComplete {
                text,
                duration_secs,
                word_count,
            });
        }
        Err(e) => {
            let msg = e.to_string();
            tracing::error!("Transcription error: {}", msg);
            let _ = ctx.gui_tx.send(SttEvent::TranscriptionError(msg));
        }
    }

    let _ = ctx.gui_tx.send(SttEvent::StateChanged(AppState::Idle));
    tracing::debug!(total_elapsed_ms = process_start.elapsed().as_millis(), "process_and_output complete, returning Idle");
    Ok(AppState::Idle)
}

/// Application state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum AppState {
    Idle,
    Recording,
    Processing,
}

/// Events sent from the STT service to the GUI.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum SttEvent {
    StateChanged(AppState),
    TranscriptionComplete {
        text: String,
        duration_secs: f32,
        word_count: usize,
    },
    TranscriptionError(String),
    BackendReady(String),
    PostProcessorLoading,
    PostProcessorReady,
    PostProcessorError(String),
    GpuFallback(String),
    /// Emitted when the active window during a paste matches a known Linux
    /// terminal that the user hasn't configured a paste rule for. The Tauri
    /// layer persists this rule into `config.input.paste_rules`.
    AutoPasteRuleAdded { app_class: String, paste_command: String },
}

/// Commands sent from the GUI to the STT service.
#[derive(Debug, Clone)]
pub enum SttCommand {
    UpdateConfig(Config),
    PauseHotkey,
    ResumeHotkey,
    /// Toggle recording on/off, regardless of hands-free configuration.
    /// Used by the macOS tray "Toggle recording" menu item and the
    /// double-click-tray-icon shortcut.
    ToggleRecording,
    #[allow(dead_code)]
    Shutdown,
}

/// The STT service runs in the background, handling hotkeys, audio capture,
/// and transcription. It communicates with the GUI via channels.
pub struct SttService {
    config: Config,
    gui_tx: mpsc::UnboundedSender<SttEvent>,
    cmd_rx: mpsc::UnboundedReceiver<SttCommand>,
    db: Option<SharedDatabase>,
    ptt_capture: CaptureSlot,
    handsfree_capture: CaptureSlot,
    /// Live RMS mic level (f32 bits, 0.0..1.0). Cloned to the AudioBuffer at
    /// run() startup so the cpal callback writes here. Held outside the
    /// service for consumers like the macOS tray waveform animation.
    mic_level: std::sync::Arc<std::sync::atomic::AtomicU32>,
}

impl SttService {
    pub fn new(
        config: Config,
        gui_tx: mpsc::UnboundedSender<SttEvent>,
        cmd_rx: mpsc::UnboundedReceiver<SttCommand>,
        db: Option<SharedDatabase>,
        ptt_capture: CaptureSlot,
        handsfree_capture: CaptureSlot,
    ) -> Self {
        Self::new_with_level(
            config,
            gui_tx,
            cmd_rx,
            db,
            ptt_capture,
            handsfree_capture,
            std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
        )
    }

    /// Like `new`, but accepts an externally-held mic level handle so the
    /// embedder (Tauri tray, mic-test UI, etc.) can read live RMS levels
    /// while a recording is in progress.
    pub fn new_with_level(
        config: Config,
        gui_tx: mpsc::UnboundedSender<SttEvent>,
        cmd_rx: mpsc::UnboundedReceiver<SttCommand>,
        db: Option<SharedDatabase>,
        ptt_capture: CaptureSlot,
        handsfree_capture: CaptureSlot,
        mic_level: std::sync::Arc<std::sync::atomic::AtomicU32>,
    ) -> Self {
        tracing::debug!(
            backend = %config.general.backend,
            hotkeys = ?config.general.hotkeys,
            audio_device = %config.audio.device,
            clipboard_only = config.general.clipboard_only,
            language = %config.general.language,
            has_db = db.is_some(),
            "creating SttService"
        );
        Self {
            config,
            gui_tx,
            cmd_rx,
            db,
            ptt_capture,
            handsfree_capture,
            mic_level,
        }
    }

    pub async fn run(mut self) -> Result<()> {
        tracing::debug!("SttService::run starting");

        // Install a panic hook that logs via tracing before the default hook
        // runs. Native FFI code paths (whisper/llama.cpp) can panic on
        // background threads; without this, the panic output races with the
        // process exit and the tail of the story is lost.
        let default_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let payload = info
                .payload()
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| info.payload().downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "<non-string panic payload>".to_string());
            let location = info
                .location()
                .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
                .unwrap_or_else(|| "<unknown location>".to_string());
            tracing::error!(payload, location, "PANIC captured by tracing hook");
            default_hook(info);
        }));

        // Create the STT backend
        let backend = self.create_backend().await?;
        let mut backend: Arc<dyn SttBackend> = Arc::from(backend);
        let mut backend_name = backend.name().to_string();
        let mut backend_model = match backend_name.as_str() {
            "openai" => self.config.openai.model.clone(),
            "deepgram" => self.config.deepgram.model.clone(),
            "whisper-local" => self.config.whisper.model.clone(),
            _ => String::new(),
        };
        tracing::info!("Using STT backend: {}", backend_name);
        let _ = self.gui_tx.send(SttEvent::BackendReady(backend_name.clone()));

        // Create local whisper fallback for cloud backends
        let mut fallback_backend: Option<Arc<dyn SttBackend>> = if backend_name != "whisper-local" {
            self.create_fallback_whisper()
        } else {
            None
        };

        // Surface whisper GPU fallback once if it happened during init.
        if crate::stt::whisper_local::gpu_fallback_occurred() {
            let _ = self.gui_tx.send(SttEvent::GpuFallback(
                "Whisper loaded in CPU-only mode (GPU unavailable)".into(),
            ));
        }

        // Create post-processor if enabled
        let mut post_processor: Option<Arc<PostProcessor>> = if self.config.post_processing.enabled {
            match self.create_post_processor().await {
                Ok(pp) => Some(Arc::new(pp)),
                Err(e) => {
                    tracing::error!("Failed to create post-processor: {}", e);
                    let _ = self.gui_tx.send(SttEvent::PostProcessorError(e.to_string()));
                    None
                }
            }
        } else {
            None
        };

        // Build shared hotkey configs from the (already numeric) Config hotkeys.
        #[cfg(target_os = "linux")]
        let ptt_config = {
            let combos = evdev_listener::combos_from_hotkeys(&self.config.general.hotkeys);
            evdev_listener::SharedHotkeyConfig::new(combos)
        };
        #[cfg(target_os = "macos")]
        let ptt_config = {
            let combos = macos_listener::combos_from_hotkeys(&self.config.general.hotkeys);
            macos_listener::SharedHotkeyConfig::new(combos)
        };

        // Set up audio capture. We probe the device upfront for early failure,
        // but the cpal input stream itself is only opened on demand (while
        // `recording_rx` is true) so the OS doesn't keep the mic — and the
        // macOS mic indicator — active when we're idle.
        tracing::debug!(device = %self.config.audio.device, "setting up audio capture");
        let _ = capture::get_input_device(&self.config.audio.device)
            .context("Failed to find audio input device")?;

        let audio_buffer = AudioBuffer::with_level(self.mic_level.clone());
        let (recording_tx, recording_rx) = watch::channel(false);

        let _capture = capture::start_capture(
            self.config.audio.device.clone(),
            &audio_buffer,
            recording_rx,
        )
        .context("Failed to start audio capture")?;
        tracing::debug!("audio capture controller started successfully");

        // Start persistent push-to-talk hotkey listener (never restarted)
        let (hotkey_tx, mut hotkey_rx) = mpsc::unbounded_channel();
        // Capture slot is shared between both listeners and the GUI's
        // `capture_hotkey` command — whichever listener sees a key first wins.
        let ptt_capture = self.ptt_capture.clone();
        let handsfree_capture = self.handsfree_capture.clone();

        #[cfg(target_os = "linux")]
        let _hotkey_handle = evdev_listener::start_listener(ptt_config.clone(), ptt_capture.clone(), hotkey_tx)
            .context("Failed to start hotkey listener")?;

        #[cfg(target_os = "macos")]
        let _hotkey_handle = macos_listener::start_listener(ptt_config.clone(), ptt_capture.clone(), hotkey_tx)
            .context("Failed to start hotkey listener")?;

        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            let _ = hotkey_tx;
            tracing::warn!("Hotkey listener not implemented for this platform");
        }

        // Set up persistent hands-free hotkey listener (empty config = disabled)
        let mut current_handsfree_hotkeys = self.config.hands_free.hotkeys.clone();
        let mut handsfree_enabled = self.config.hands_free.enabled;

        #[cfg(target_os = "linux")]
        let handsfree_config = {
            let combos = if handsfree_enabled && !current_handsfree_hotkeys.is_empty() {
                evdev_listener::combos_from_hotkeys(&current_handsfree_hotkeys)
            } else {
                Vec::new()
            };
            evdev_listener::SharedHotkeyConfig::new(combos)
        };
        #[cfg(target_os = "macos")]
        let handsfree_config = {
            let combos = if handsfree_enabled && !current_handsfree_hotkeys.is_empty() {
                macos_listener::combos_from_hotkeys(&current_handsfree_hotkeys)
            } else {
                Vec::new()
            };
            macos_listener::SharedHotkeyConfig::new(combos)
        };

        let (handsfree_tx, mut handsfree_rx) = mpsc::unbounded_channel::<HotkeyEvent>();

        #[cfg(target_os = "linux")]
        let _handsfree_handle = evdev_listener::start_listener(handsfree_config.clone(), handsfree_capture.clone(), handsfree_tx)
            .context("Failed to start hands-free listener")?;

        #[cfg(target_os = "macos")]
        let _handsfree_handle = macos_listener::start_listener(handsfree_config.clone(), handsfree_capture.clone(), handsfree_tx)
            .context("Failed to start hands-free listener")?;

        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            let _ = handsfree_tx;
        }

        if handsfree_enabled && !current_handsfree_hotkeys.is_empty() {
            tracing::info!("Hands-free listener active for: {:?}", current_handsfree_hotkeys);
        }

        let mut clipboard_only = self.config.general.clipboard_only;
        let mut input_method = self.config.input.method.clone();
        let mut paste_command = self.config.input.paste_command.clone();
        let mut paste_rules = self.config.input.paste_rules.clone();
        let mut min_duration = self.config.audio.min_duration;
        let mut energy_threshold = self.config.audio.energy_threshold;
        let mut noise_cancellation = self.config.audio.noise_cancellation;
        let mut language = if self.config.general.language.is_empty() {
            None
        } else {
            Some(self.config.general.language.clone())
        };
        let mut current_hotkeys = self.config.general.hotkeys.clone();

        tracing::info!(
            "Verbatim ready! Hold {:?} to record, release to transcribe.",
            self.config.general.hotkeys
        );

        let _ = self.gui_tx.send(SttEvent::StateChanged(AppState::Idle));
        let mut state = AppState::Idle;
        let mut hotkey_paused = false;
        let mut handsfree_active = false;

        tracing::debug!("entering main event loop");
        loop {
            tokio::select! {
                // Handle push-to-talk hotkey events
                Some(event) = hotkey_rx.recv() => {
                    if hotkey_paused {
                        tracing::trace!("hotkey event ignored (paused)");
                        continue;
                    }
                    match (state, event) {
                        (AppState::Idle, HotkeyEvent::Pressed) => {
                            state = AppState::Recording;
                            audio_buffer.clear();
                            let _ = recording_tx.send(true);
                            let _ = self.gui_tx.send(SttEvent::StateChanged(AppState::Recording));
                            tracing::info!("Recording (push-to-talk)...");
                        }
                        (AppState::Recording, HotkeyEvent::Released) if !handsfree_active => {
                            let ctx = ProcessContext {
                                audio_buffer: &audio_buffer,
                                recording_tx: &recording_tx,
                                backend: &backend,
                                backend_name: &backend_name,
                                backend_model: &backend_model,
                                fallback_backend: &fallback_backend,
                                post_processor: &post_processor,
                                gui_tx: &self.gui_tx,
                                db: &self.db,
                                clipboard_only,
                                input_method: &input_method,
                                paste_command: &paste_command,
                                paste_rules: &paste_rules,
                                min_duration,
                                energy_threshold,
                                noise_cancellation,
                                language: &language,
                            };
                            state = process_and_output(&ctx).await?;
                        }
                        _ => {
                            tracing::trace!(?state, ?event, "ignoring push-to-talk event in current state");
                        }
                    }
                }

                // Handle hands-free hotkey events (toggle mode)
                Some(event) = handsfree_rx.recv() => {
                    if hotkey_paused || !handsfree_enabled {
                        continue;
                    }
                    match (state, event) {
                        (AppState::Idle, HotkeyEvent::Pressed) => {
                            state = AppState::Recording;
                            handsfree_active = true;
                            audio_buffer.clear();
                            let _ = recording_tx.send(true);
                            let _ = self.gui_tx.send(SttEvent::StateChanged(AppState::Recording));
                            tracing::info!("Recording (hands-free)...");
                        }
                        (AppState::Recording, HotkeyEvent::Pressed) if handsfree_active => {
                            handsfree_active = false;
                            let ctx = ProcessContext {
                                audio_buffer: &audio_buffer,
                                recording_tx: &recording_tx,
                                backend: &backend,
                                backend_name: &backend_name,
                                backend_model: &backend_model,
                                fallback_backend: &fallback_backend,
                                post_processor: &post_processor,
                                gui_tx: &self.gui_tx,
                                db: &self.db,
                                clipboard_only,
                                input_method: &input_method,
                                paste_command: &paste_command,
                                paste_rules: &paste_rules,
                                min_duration,
                                energy_threshold,
                                noise_cancellation,
                                language: &language,
                            };
                            state = process_and_output(&ctx).await?;
                        }
                        _ => {
                            tracing::trace!(?state, ?event, handsfree_active, "ignoring hands-free event in current state");
                        }
                    }
                }

                // Handle commands from GUI
                Some(cmd) = self.cmd_rx.recv() => {
                    match cmd {
                        SttCommand::Shutdown => {
                            tracing::info!("STT service shutting down");
                            break;
                        }
                        SttCommand::PauseHotkey => {
                            tracing::debug!("Hotkey listener paused");
                            hotkey_paused = true;
                        }
                        SttCommand::ResumeHotkey => {
                            tracing::debug!("Hotkey listener resumed");
                            hotkey_paused = false;
                        }
                        SttCommand::ToggleRecording => {
                            if hotkey_paused {
                                tracing::debug!("ToggleRecording ignored: hotkey paused");
                            } else {
                                match state {
                                    AppState::Idle => {
                                        state = AppState::Recording;
                                        handsfree_active = true;
                                        audio_buffer.clear();
                                        let _ = recording_tx.send(true);
                                        let _ = self.gui_tx.send(SttEvent::StateChanged(AppState::Recording));
                                        tracing::info!("Recording (toggle)...");
                                    }
                                    AppState::Recording if handsfree_active => {
                                        handsfree_active = false;
                                        let ctx = ProcessContext {
                                            audio_buffer: &audio_buffer,
                                            recording_tx: &recording_tx,
                                            backend: &backend,
                                            backend_name: &backend_name,
                                            backend_model: &backend_model,
                                            fallback_backend: &fallback_backend,
                                            post_processor: &post_processor,
                                            gui_tx: &self.gui_tx,
                                            db: &self.db,
                                            clipboard_only,
                                            input_method: &input_method,
                                            paste_command: &paste_command,
                                            paste_rules: &paste_rules,
                                            min_duration,
                                            energy_threshold,
                                            noise_cancellation,
                                            language: &language,
                                        };
                                        state = process_and_output(&ctx).await?;
                                    }
                                    _ => {
                                        tracing::debug!(?state, handsfree_active, "ToggleRecording ignored in current state (likely a hold-to-talk in progress)");
                                    }
                                }
                            }
                        }
                        SttCommand::UpdateConfig(new_config) => {
                            tracing::info!("Config update received");
                            tracing::debug!(
                                clipboard_only = new_config.general.clipboard_only,
                                input_method = %new_config.input.method,
                                language = %new_config.general.language,
                                min_duration = new_config.audio.min_duration,
                                energy_threshold = new_config.audio.energy_threshold,
                                post_processing = new_config.post_processing.enabled,
                                handsfree = new_config.hands_free.enabled,
                                "config update details"
                            );

                            // Update runtime settings
                            clipboard_only = new_config.general.clipboard_only;
                            input_method = new_config.input.method.clone();
                            paste_command = new_config.input.paste_command.clone();
                            paste_rules = new_config.input.paste_rules.clone();
                            min_duration = new_config.audio.min_duration;
                            energy_threshold = new_config.audio.energy_threshold;
                            noise_cancellation = new_config.audio.noise_cancellation;
                            language = if new_config.general.language.is_empty() {
                                None
                            } else {
                                Some(new_config.general.language.clone())
                            };

                            // Update post-processor
                            if new_config.post_processing.enabled {
                                let _ = self.gui_tx.send(SttEvent::PostProcessorLoading);
                                // Rebuild config temporarily to call create_post_processor
                                let prev_config = std::mem::replace(&mut self.config, new_config.clone());
                                match self.create_post_processor().await {
                                    Ok(pp) => {
                                        post_processor = Some(Arc::new(pp));
                                    }
                                    Err(e) => {
                                        tracing::error!("Failed to create post-processor: {}", e);
                                        let _ = self.gui_tx.send(SttEvent::PostProcessorError(e.to_string()));
                                        post_processor = None;
                                    }
                                }
                                let _ = self.gui_tx.send(SttEvent::PostProcessorReady);
                                self.config = prev_config;
                            } else {
                                if post_processor.is_some() {
                                    tracing::info!("Post-processing disabled");
                                }
                                post_processor = None;
                            }

                            // Update push-to-talk hotkeys via shared config (no thread restart)
                            if new_config.general.hotkeys != current_hotkeys {
                                #[cfg(target_os = "linux")]
                                {
                                    let new_combos = evdev_listener::combos_from_hotkeys(&new_config.general.hotkeys);
                                    ptt_config.update(new_combos);
                                }
                                #[cfg(target_os = "macos")]
                                {
                                    let new_combos = macos_listener::combos_from_hotkeys(&new_config.general.hotkeys);
                                    ptt_config.update(new_combos);
                                }
                                current_hotkeys = new_config.general.hotkeys.clone();
                                tracing::info!("Hotkeys updated ({} bindings)", current_hotkeys.len());
                            }

                            // Update hands-free settings
                            handsfree_enabled = new_config.hands_free.enabled;

                            // Update hands-free hotkeys via shared config (no thread restart)
                            if new_config.hands_free.hotkeys != current_handsfree_hotkeys {
                                current_handsfree_hotkeys = new_config.hands_free.hotkeys.clone();

                                if handsfree_enabled && !current_handsfree_hotkeys.is_empty() {
                                    #[cfg(target_os = "linux")]
                                    {
                                        let new_combos = evdev_listener::combos_from_hotkeys(&current_handsfree_hotkeys);
                                        handsfree_config.update(new_combos);
                                    }
                                    #[cfg(target_os = "macos")]
                                    {
                                        let new_combos = macos_listener::combos_from_hotkeys(&current_handsfree_hotkeys);
                                        handsfree_config.update(new_combos);
                                    }
                                    tracing::info!("Hands-free hotkeys updated ({} bindings)", current_handsfree_hotkeys.len());
                                } else {
                                    // Disable: set empty config (listener stays alive but matches nothing)
                                    handsfree_config.update(Vec::new());
                                    tracing::info!("Hands-free hotkeys cleared");
                                }
                            }

                            // If hands-free was disabled while recording, stop gracefully
                            if !handsfree_enabled && handsfree_active && state == AppState::Recording {
                                handsfree_active = false;
                                let ctx = ProcessContext {
                                    audio_buffer: &audio_buffer,
                                    recording_tx: &recording_tx,
                                    backend: &backend,
                                    backend_name: &backend_name,
                                    backend_model: &backend_model,
                                    fallback_backend: &fallback_backend,
                                    post_processor: &post_processor,
                                    gui_tx: &self.gui_tx,
                                    db: &self.db,
                                    clipboard_only,
                                    input_method: &input_method,
                                    paste_command: &paste_command,
                                    paste_rules: &paste_rules,
                                    min_duration,
                                    energy_threshold,
                                    noise_cancellation,
                                    language: &language,
                                };
                                state = process_and_output(&ctx).await?;
                            }

                            // Recreate STT backend if provider changed
                            if new_config.general.backend != backend_name {
                                self.config = new_config.clone();
                                match self.create_backend().await {
                                    Ok(new_backend) => {
                                        let new_arc: Arc<dyn SttBackend> = Arc::from(new_backend);
                                        backend_name = new_arc.name().to_string();
                                        backend_model = match backend_name.as_str() {
                                            "openai" => self.config.openai.model.clone(),
                                            "deepgram" => self.config.deepgram.model.clone(),
                                            "whisper-local" => self.config.whisper.model.clone(),
                                            _ => String::new(),
                                        };
                                        backend = new_arc;
                                        tracing::info!("STT backend switched to: {}", backend_name);
                                        let _ = self.gui_tx.send(SttEvent::BackendReady(backend_name.clone()));

                                        // Update fallback: cloud backends get local fallback, local doesn't need one
                                        fallback_backend = if backend_name != "whisper-local" {
                                            self.create_fallback_whisper()
                                        } else {
                                            None
                                        };
                                    }
                                    Err(e) => {
                                        tracing::error!("Failed to switch STT backend: {}", e);
                                    }
                                }
                            }

                            self.config = new_config;
                        }
                    }
                }

                else => break,
            }
        }

        Ok(())
    }

    async fn create_post_processor(&self) -> Result<PostProcessor> {
        let pp_config = &self.config.post_processing;
        match pp_config.provider.as_str() {
            "openai" => {
                tracing::info!("Post-processing enabled (provider: openai, model: {})", pp_config.model);
                Ok(PostProcessor::new_openai(pp_config, self.config.openai.api_key.clone()))
            }
            "ollama" => {
                let base_url = match pp_config.ollama_mode.as_str() {
                    "managed" => format!("http://127.0.0.1:{}", pp_config.ollama_bundled_port),
                    _ => pp_config.ollama_url.clone(),
                };
                tracing::info!(
                    mode = %pp_config.ollama_mode,
                    model = %pp_config.ollama_model,
                    %base_url,
                    "Post-processing enabled (provider: ollama)"
                );
                Ok(PostProcessor::new_ollama(pp_config, base_url))
            }
            other => {
                anyhow::bail!("Unknown post-processing provider: '{}'", other);
            }
        }
    }

    /// Try to create a local whisper backend as a fallback for cloud providers.
    /// Returns None (with a log) if no whisper model is downloaded.
    fn create_fallback_whisper(&self) -> Option<Arc<dyn SttBackend>> {
        let model_dir = self.config.resolved_model_dir();
        // Find any downloaded whisper model — prefer the configured one, else first available
        let preferred = &self.config.whisper.model;
        let model_name = if model_manager::model_exists(&model_dir, preferred) {
            preferred.clone()
        } else {
            // Find any downloaded model
            match model_manager::available_models()
                .iter()
                .find(|name| model_manager::model_exists(&model_dir, name))
            {
                Some(name) => name.to_string(),
                None => {
                    tracing::info!("No local whisper model available for fallback");
                    return None;
                }
            }
        };

        let model_path = model_manager::model_path(&model_dir, &model_name);
        let threads = if self.config.whisper.threads > 0 {
            self.config.whisper.threads
        } else {
            std::thread::available_parallelism()
                .map(|p| p.get() as u32)
                .unwrap_or(4)
                .min(8)
        };

        match WhisperLocal::new(&model_path, threads) {
            Ok(backend) => {
                tracing::info!(model = %model_name, "Local whisper fallback ready");
                Some(Arc::new(backend))
            }
            Err(e) => {
                tracing::warn!("Failed to create whisper fallback: {}", e);
                None
            }
        }
    }

    async fn create_backend(&self) -> Result<Box<dyn SttBackend>> {
        tracing::debug!(backend = %self.config.general.backend, "creating STT backend");
        match self.config.general.backend.as_str() {
            "whisper-local" => {
                let model_dir = self.config.resolved_model_dir();
                let model_name = &self.config.whisper.model;
                tracing::debug!(
                    model_dir = %model_dir.display(),
                    model_name,
                    threads = self.config.whisper.threads,
                    "creating whisper-local backend"
                );

                if !model_manager::model_exists(&model_dir, model_name) {
                    tracing::info!("Model '{}' not found, downloading...", model_name);
                    let (_cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
                    model_manager::download_model(&model_dir, model_name, |_, _| {}, || {}, cancel_rx).await?;
                }

                let model_path = model_manager::model_path(&model_dir, model_name);
                let backend = WhisperLocal::new(&model_path, self.config.whisper.threads)?;
                Ok(Box::new(backend))
            }
            "openai" => {
                tracing::debug!(
                    model = %self.config.openai.model,
                    api_key_present = !self.config.openai.api_key.is_empty(),
                    "creating openai backend"
                );
                let backend = OpenAiWhisper::new(
                    self.config.openai.api_key.clone(),
                    self.config.openai.model.clone(),
                )?;
                Ok(Box::new(backend))
            }
            "deepgram" => {
                tracing::debug!(
                    model = %self.config.deepgram.model,
                    api_key_present = !self.config.deepgram.api_key.is_empty(),
                    "creating deepgram backend"
                );
                let backend = DeepgramStt::new(
                    self.config.deepgram.api_key.clone(),
                    self.config.deepgram.model.clone(),
                )?;
                Ok(Box::new(backend))
            }
            "smallest" => {
                tracing::debug!(
                    api_key_present = !self.config.smallest.api_key.is_empty(),
                    "creating smallest backend"
                );
                let backend = SmallestStt::new(self.config.smallest.api_key.clone())?;
                Ok(Box::new(backend))
            }
            other => bail!(
                "Unknown backend: '{}'. Use 'whisper-local', 'openai', 'deepgram', or 'smallest'.",
                other
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_state_variants() {
        assert_ne!(AppState::Idle, AppState::Recording);
        assert_ne!(AppState::Recording, AppState::Processing);
        assert_ne!(AppState::Idle, AppState::Processing);
    }

    #[test]
    fn test_toggle_recording_command_variant() {
        // Guards against accidental removal of the variant the macOS tray
        // "Toggle recording" menu item depends on.
        let cmd = SttCommand::ToggleRecording;
        assert!(matches!(cmd, SttCommand::ToggleRecording));
    }

    #[test]
    fn test_stt_event_serialization() {
        let event = SttEvent::TranscriptionComplete {
            text: "hello world".into(),
            duration_secs: 1.5,
            word_count: 2,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("hello world"));

        let state_event = SttEvent::StateChanged(AppState::Idle);
        let json = serde_json::to_string(&state_event).unwrap();
        assert!(json.contains("Idle"));
    }

    #[test]
    fn test_app_state_serialization_roundtrip() {
        let state = AppState::Recording;
        let json = serde_json::to_string(&state).unwrap();
        let parsed: AppState = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, state);
    }

    #[tokio::test]
    async fn test_create_backend_unknown_errors() {
        let (gui_tx, _gui_rx) = mpsc::unbounded_channel();
        let (_cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let mut config = Config::default();
        config.general.backend = "bogus-backend".into();

        let service = SttService::new(config, gui_tx, cmd_rx, None, CaptureSlot::new(), CaptureSlot::new());
        let result = service.create_backend().await;
        assert!(result.is_err());
        let err = result.err().expect("should be an error");
        assert!(err.to_string().contains("Unknown backend"));
    }

    #[tokio::test]
    async fn test_process_short_audio_returns_idle() {
        let (gui_tx, mut gui_rx) = mpsc::unbounded_channel();
        let (recording_tx, _recording_rx) = watch::channel(true);

        // Create a mock backend
        struct MockBackend;
        #[async_trait::async_trait]
        impl SttBackend for MockBackend {
            fn name(&self) -> &str { "mock" }
            async fn transcribe(&self, _audio: &[f32], _language: Option<&str>) -> Result<String, crate::errors::SttError> {
                Ok("should not be called".into())
            }
        }

        let backend: Arc<dyn SttBackend> = Arc::new(MockBackend);
        let audio_buffer = AudioBuffer::new();
        // Put only a few samples (less than min_duration)
        {
            let shared = audio_buffer.shared();
            let mut buf = shared.lock().unwrap();
            buf.extend_from_slice(&[0.1; 100]); // ~6ms at 16kHz
        }

        let ctx = ProcessContext {
            audio_buffer: &audio_buffer,
            recording_tx: &recording_tx,
            backend: &backend,
            backend_name: "mock",
            backend_model: "",
            fallback_backend: &None,
            post_processor: &None,
            gui_tx: &gui_tx,
            db: &None,
            clipboard_only: true,
            input_method: "auto",
            paste_command: "meta+v",
            paste_rules: &[],
            min_duration: 0.5,
            energy_threshold: 0.0,
            noise_cancellation: false,
            language: &None,
        };

        let state = process_and_output(&ctx).await.unwrap();
        assert_eq!(state, AppState::Idle);

        // Short clip is gated before Processing is announced — UI sees only
        // a transition back to Idle (no Processing flicker).
        let event = gui_rx.recv().await.unwrap();
        assert!(matches!(event, SttEvent::StateChanged(AppState::Idle)));
        assert!(gui_rx.try_recv().is_err(), "no further state events expected");
    }

    #[tokio::test]
    async fn test_process_silent_audio_skips_processing_event() {
        let (gui_tx, mut gui_rx) = mpsc::unbounded_channel();
        let (recording_tx, _recording_rx) = watch::channel(true);

        struct MockBackend;
        #[async_trait::async_trait]
        impl SttBackend for MockBackend {
            fn name(&self) -> &str { "mock" }
            async fn transcribe(&self, _audio: &[f32], _language: Option<&str>) -> Result<String, crate::errors::SttError> {
                panic!("transcribe must not be called for a silent buffer");
            }
        }

        let backend: Arc<dyn SttBackend> = Arc::new(MockBackend);
        let audio_buffer = AudioBuffer::new();
        // 1.5 s of dead silence plus a single stray click — passes the old
        // peak/RMS gate but must be rejected by the voiced-content check.
        {
            let shared = audio_buffer.shared();
            let mut buf = shared.lock().unwrap();
            buf.resize(24_000, 0.0);
            buf[12_000] = 1.0;
        }

        let ctx = ProcessContext {
            audio_buffer: &audio_buffer,
            recording_tx: &recording_tx,
            backend: &backend,
            backend_name: "mock",
            backend_model: "",
            fallback_backend: &None,
            post_processor: &None,
            gui_tx: &gui_tx,
            db: &None,
            clipboard_only: true,
            input_method: "auto",
            paste_command: "meta+v",
            paste_rules: &[],
            min_duration: 0.5,
            energy_threshold: 0.0,
            noise_cancellation: false,
            language: &None,
        };

        let state = process_and_output(&ctx).await.unwrap();
        assert_eq!(state, AppState::Idle);

        let event = gui_rx.recv().await.unwrap();
        assert!(matches!(event, SttEvent::StateChanged(AppState::Idle)));
        assert!(
            gui_rx.try_recv().is_err(),
            "silent buffer must not emit a Processing event"
        );
    }

    // ── Edge case tests ──────────────────────────────────

    #[test]
    fn test_all_app_state_variants_serialize_uniquely() {
        let states = [AppState::Idle, AppState::Recording, AppState::Processing];
        let jsons: std::collections::HashSet<String> = states
            .iter()
            .map(|s| serde_json::to_string(s).unwrap())
            .collect();
        assert_eq!(jsons.len(), 3, "all AppState variants should serialize to unique strings");
    }

    #[test]
    fn test_all_stt_event_variants_serialize() {
        let events: Vec<SttEvent> = vec![
            SttEvent::StateChanged(AppState::Idle),
            SttEvent::TranscriptionComplete {
                text: "test".into(),
                duration_secs: 1.0,
                word_count: 1,
            },
            SttEvent::TranscriptionError("err".into()),
            SttEvent::BackendReady("mock".into()),
        ];
        for event in &events {
            let json = serde_json::to_string(event);
            assert!(json.is_ok(), "failed to serialize {:?}", event);
        }
    }

    #[test]
    fn test_stt_event_deserialization_roundtrip() {
        let events: Vec<SttEvent> = vec![
            SttEvent::StateChanged(AppState::Recording),
            SttEvent::TranscriptionComplete {
                text: "hello".into(),
                duration_secs: 2.5,
                word_count: 1,
            },
            SttEvent::TranscriptionError("fail".into()),
            SttEvent::BackendReady("openai".into()),
        ];
        for event in &events {
            let json = serde_json::to_string(event).unwrap();
            let parsed: SttEvent = serde_json::from_str(&json).unwrap();
            let json2 = serde_json::to_string(&parsed).unwrap();
            assert_eq!(json, json2, "roundtrip should preserve JSON");
        }
    }
}
