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
use crate::stt::openai::OpenAiWhisper;
use crate::stt::whisper_local::WhisperLocal;
use crate::stt::SttBackend;

#[cfg(target_os = "linux")]
use crate::hotkey::evdev_listener;

use crate::hotkey::HotkeyEvent;

/// Application state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppState {
    Idle,
    Recording,
    Processing,
}

/// Events sent from the STT service to the GUI.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum SttEvent {
    StateChanged(AppState),
    TranscriptionComplete {
        text: String,
        duration_secs: f32,
        word_count: usize,
    },
    TranscriptionError(String),
    BackendReady(String),
}

/// Commands sent from the GUI to the STT service.
#[derive(Debug, Clone)]
pub enum SttCommand {
    UpdateConfig(Config),
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
}

impl SttService {
    pub fn new(
        config: Config,
        gui_tx: mpsc::UnboundedSender<SttEvent>,
        cmd_rx: mpsc::UnboundedReceiver<SttCommand>,
        db: Option<SharedDatabase>,
    ) -> Self {
        Self {
            config,
            gui_tx,
            cmd_rx,
            db,
        }
    }

    pub async fn run(mut self) -> Result<()> {
        // Create the STT backend
        let backend = self.create_backend().await?;
        let backend: Arc<dyn SttBackend> = Arc::from(backend);
        let backend_name = backend.name().to_string();
        tracing::info!("Using STT backend: {}", backend_name);
        let _ = self.gui_tx.send(SttEvent::BackendReady(backend_name.clone()));

        // Parse hotkeys and start listener
        #[cfg(target_os = "linux")]
        let hotkeys = {
            let mut keys = Vec::new();
            for hk in &self.config.general.hotkeys {
                keys.push(evdev_listener::parse_key(hk)
                    .with_context(|| format!("Invalid hotkey: {}", hk))?);
            }
            keys
        };

        // Set up audio capture
        let device = capture::get_input_device(&self.config.audio.device)
            .context("Failed to find audio input device")?;

        let audio_buffer = AudioBuffer::new();
        let (recording_tx, recording_rx) = watch::channel(false);

        let _stream = capture::start_capture(&device, &audio_buffer, recording_rx)
            .context("Failed to start audio capture")?;

        // Set up hotkey listener
        let (hotkey_tx, mut hotkey_rx) = mpsc::unbounded_channel();

        #[cfg(target_os = "linux")]
        let _hotkey_handle = evdev_listener::start_listener(hotkeys, hotkey_tx)
            .context("Failed to start hotkey listener")?;

        #[cfg(not(target_os = "linux"))]
        {
            let _ = hotkey_tx;
            tracing::warn!("Hotkey listener not implemented for this platform");
        }

        let clipboard_only = self.config.general.clipboard_only;
        let min_duration = self.config.audio.min_duration;
        let input_method = self.config.input.method.clone();
        let language = if self.config.general.language.is_empty() {
            None
        } else {
            Some(self.config.general.language.clone())
        };

        tracing::info!(
            "Verbatim ready! Hold {:?} to record, release to transcribe.",
            self.config.general.hotkeys
        );

        let _ = self.gui_tx.send(SttEvent::StateChanged(AppState::Idle));
        let mut state = AppState::Idle;

        loop {
            tokio::select! {
                // Handle hotkey events
                Some(event) = hotkey_rx.recv() => {
                    match (state, event) {
                        (AppState::Idle, HotkeyEvent::Pressed) => {
                            state = AppState::Recording;
                            audio_buffer.clear();
                            let _ = recording_tx.send(true);
                            let _ = self.gui_tx.send(SttEvent::StateChanged(AppState::Recording));
                            tracing::info!("Recording...");
                        }
                        (AppState::Recording, HotkeyEvent::Released) => {
                            let _ = recording_tx.send(false);
                            let _ = self.gui_tx.send(SttEvent::StateChanged(AppState::Processing));

                            let samples = audio_buffer.take();
                            let sample_count = samples.len();
                            let duration_secs = sample_count as f32 / 16_000.0;

                            if samples.is_empty() || duration_secs < min_duration {
                                if samples.is_empty() {
                                    tracing::warn!("No audio captured");
                                } else {
                                    tracing::info!("Recording too short ({:.1}s < {:.1}s threshold), ignoring", duration_secs, min_duration);
                                }
                                state = AppState::Idle;
                                let _ = self.gui_tx.send(SttEvent::StateChanged(AppState::Idle));
                                continue;
                            }

                            tracing::info!("Processing {:.1}s of audio...", duration_secs);

                            let lang = language.clone();
                            let backend_clone = Arc::clone(&backend);

                            let result = tokio::task::spawn_blocking(move || {
                                tokio::runtime::Handle::current().block_on(async {
                                    backend_clone.transcribe(&samples, lang.as_deref()).await
                                })
                            })
                            .await
                            .context("Transcription task panicked")?;

                            match result {
                                Ok(text) if text.is_empty() => {
                                    tracing::info!("(no speech detected)");
                                }
                                Ok(text) => {
                                    let word_count = text.split_whitespace().count();
                                    tracing::info!("Transcribed: {}", text);

                                    // Copy to clipboard
                                    if let Err(e) = clipboard::copy_to_clipboard(&text) {
                                        tracing::error!("Clipboard error: {}", e);
                                    }

                                    // Type into focused window (unless clipboard-only)
                                    if !clipboard_only {
                                        match EnigoBackend::new(&input_method) {
                                            Ok(mut input) => {
                                                if let Err(e) = input.type_text(&text) {
                                                    tracing::error!("Typing error: {}", e);
                                                }
                                            }
                                            Err(e) => {
                                                tracing::error!("Failed to init keyboard input: {}", e);
                                            }
                                        }
                                    }

                                    // Store in database
                                    if let Some(ref db) = self.db {
                                        let t = Transcription {
                                            id: uuid::Uuid::new_v4().to_string(),
                                            text: text.clone(),
                                            word_count: word_count as i64,
                                            char_count: text.len() as i64,
                                            duration_secs: duration_secs as f64,
                                            backend: backend_name.clone(),
                                            language: language.clone(),
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

                                    // Notify GUI
                                    let _ = self.gui_tx.send(SttEvent::TranscriptionComplete {
                                        text,
                                        duration_secs,
                                        word_count,
                                    });
                                }
                                Err(e) => {
                                    let msg = e.to_string();
                                    tracing::error!("Transcription error: {}", msg);
                                    let _ = self.gui_tx.send(SttEvent::TranscriptionError(msg));
                                }
                            }

                            state = AppState::Idle;
                            let _ = self.gui_tx.send(SttEvent::StateChanged(AppState::Idle));
                        }
                        _ => {}
                    }
                }

                // Handle commands from GUI
                Some(cmd) = self.cmd_rx.recv() => {
                    match cmd {
                        SttCommand::Shutdown => {
                            tracing::info!("STT service shutting down");
                            break;
                        }
                        SttCommand::UpdateConfig(new_config) => {
                            tracing::info!("Config update received (restart required for backend changes)");
                            self.config = new_config;
                        }
                    }
                }

                else => break,
            }
        }

        Ok(())
    }

    async fn create_backend(&self) -> Result<Box<dyn SttBackend>> {
        match self.config.general.backend.as_str() {
            "whisper-local" => {
                let model_dir = self.config.resolved_model_dir();
                let model_name = &self.config.whisper.model;

                if !model_manager::model_exists(&model_dir, model_name) {
                    tracing::info!("Model '{}' not found, downloading...", model_name);
                    model_manager::download_model(&model_dir, model_name).await?;
                }

                let model_path = model_manager::model_path(&model_dir, model_name);
                let backend = WhisperLocal::new(&model_path, self.config.whisper.threads)?;
                Ok(Box::new(backend))
            }
            "openai" => {
                let backend = OpenAiWhisper::new(
                    self.config.openai.api_key.clone(),
                    self.config.openai.model.clone(),
                )?;
                Ok(Box::new(backend))
            }
            other => bail!(
                "Unknown backend: '{}'. Use 'whisper-local' or 'openai'.",
                other
            ),
        }
    }
}
