use std::sync::atomic::Ordering;
use tauri::{Emitter, State};
use verbatim_core::app::SttCommand;
use verbatim_core::audio::capture;
use verbatim_core::config::Config;
use verbatim_core::db::{DailyCostSummary, DailyTokenUsage, DailyWordStats, ModelTokenUsage, ProviderCostSummary, Stats, Transcription};
use verbatim_core::input::window_detect;
use verbatim_core::llm_model_manager;
use verbatim_core::model_manager;

use crate::state::AppState;

#[derive(serde::Serialize)]
pub struct UpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub update_available: bool,
    pub release_url: String,
    pub release_notes: String,
}

#[derive(serde::Serialize)]
pub struct MacPermissions {
    pub accessibility: bool,
    pub microphone: bool,
}

#[derive(serde::Serialize)]
pub struct SystemInfo {
    pub total_ram_mb: u64,
    pub cpu_cores: usize,
}

#[tauri::command]
pub async fn get_system_info() -> SystemInfo {
    tracing::debug!("IPC: get_system_info");
    use sysinfo::System;
    let mut sys = System::new();
    sys.refresh_memory();
    let cpu_cores = std::thread::available_parallelism().map(|p| p.get()).unwrap_or(4);
    let info = SystemInfo {
        total_ram_mb: sys.total_memory() / (1024 * 1024),
        cpu_cores,
    };
    tracing::debug!(ram_mb = info.total_ram_mb, cpu_cores = info.cpu_cores, "system info");
    info
}

// ── Debug Info ───────────────────────────────────────────────────────

#[derive(serde::Serialize)]
pub struct LogFileInfo {
    pub name: String,
    pub size_bytes: u64,
}

#[derive(serde::Serialize)]
pub struct VramInfo {
    pub used_mb: u64,
    pub total_mb: u64,
    pub gpu_name: String,
}

#[derive(serde::Serialize)]
pub struct DebugInfo {
    pub log_dir: String,
    pub log_files: Vec<LogFileInfo>,
    pub whisper_models_bytes: u64,
    pub llm_models_bytes: u64,
    pub database_bytes: u64,
    pub logs_bytes: u64,
    pub config_bytes: u64,
    pub process_rss_mb: u64,
    pub total_ram_mb: u64,
    pub vram_info: Option<VramInfo>,
    pub amd_vram_info: Option<VramInfo>,
    pub gpu_backend: String,
    pub stt_using_gpu: bool,
    pub llm_using_gpu: bool,
    pub app_vram_mb: Option<u64>,
}

/// Report which GPU backend this binary was compiled with.
fn compiled_gpu_backend() -> &'static str {
    if cfg!(feature = "cuda") && cfg!(target_os = "linux") {
        // Release builds: CUDA for NVIDIA + Vulkan for AMD/Intel fallback
        "cuda+vulkan"
    } else if cfg!(feature = "cuda") {
        "cuda"
    } else if cfg!(feature = "rocm") {
        "rocm"
    } else if cfg!(target_os = "linux") {
        // Dev builds: Vulkan only (no CUDA toolkit required)
        "vulkan"
    } else if cfg!(target_os = "macos") {
        "metal"
    } else {
        "cpu"
    }
}

/// Sum all file sizes in a directory (non-recursive).
fn dir_size_bytes(path: &std::path::Path) -> u64 {
    std::fs::read_dir(path)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .filter_map(|e| e.metadata().ok().map(|m| m.len()))
        .sum()
}

/// Try to query NVIDIA GPU VRAM via nvidia-smi.
fn query_vram() -> Option<VramInfo> {
    let output = std::process::Command::new("nvidia-smi")
        .args([
            "--query-gpu=memory.used,memory.total,name",
            "--format=csv,noheader,nounits",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout.lines().next()?;
    let parts: Vec<&str> = line.splitn(3, ',').map(|s| s.trim()).collect();
    if parts.len() < 3 {
        return None;
    }
    Some(VramInfo {
        used_mb: parts[0].parse().ok()?,
        total_mb: parts[1].parse().ok()?,
        gpu_name: parts[2].to_string(),
    })
}

/// Try to query AMD GPU VRAM via rocm-smi.
fn query_amd_vram() -> Option<VramInfo> {
    // Try rocm-smi --showmeminfo vram --json for structured output
    let output = std::process::Command::new("rocm-smi")
        .args(["--showmeminfo", "vram", "--showproductname", "--csv"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Parse CSV: look for VRAM Total and VRAM Used lines
    let mut total_mb: Option<u64> = None;
    let mut used_mb: Option<u64> = None;
    let mut gpu_name = String::from("AMD GPU");

    for line in stdout.lines() {
        let line_lower = line.to_lowercase();
        if line_lower.contains("total") && line_lower.contains("vram") {
            // Extract numeric value (in bytes, convert to MB)
            if let Some(val) = extract_numeric_value(line) {
                total_mb = Some(val / (1024 * 1024));
            }
        } else if line_lower.contains("used") && line_lower.contains("vram") {
            if let Some(val) = extract_numeric_value(line) {
                used_mb = Some(val / (1024 * 1024));
            }
        } else if line_lower.contains("card series") || line_lower.contains("product name") {
            // Try to extract GPU name from product info
            if let Some(name) = line.split(',').nth(1) {
                let name = name.trim();
                if !name.is_empty() {
                    gpu_name = name.to_string();
                }
            }
        }
    }

    Some(VramInfo {
        used_mb: used_mb.unwrap_or(0),
        total_mb: total_mb?,
        gpu_name,
    })
}

/// Extract the first numeric value from a CSV line.
fn extract_numeric_value(line: &str) -> Option<u64> {
    for part in line.split(',') {
        if let Ok(val) = part.trim().parse::<u64>() {
            return Some(val);
        }
    }
    None
}

/// Query VRAM used by this process via nvidia-smi.
fn query_process_vram() -> Option<u64> {
    let pid = std::process::id();
    let output = std::process::Command::new("nvidia-smi")
        .args([
            "--query-compute-apps=pid,used_memory",
            "--format=csv,noheader,nounits",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut total_mb = 0u64;
    for line in stdout.lines() {
        let parts: Vec<&str> = line.splitn(2, ',').map(|s| s.trim()).collect();
        if parts.len() == 2 {
            if let Ok(line_pid) = parts[0].parse::<u32>() {
                if line_pid == pid {
                    if let Ok(mb) = parts[1].parse::<u64>() {
                        total_mb += mb;
                    }
                }
            }
        }
    }
    if total_mb > 0 { Some(total_mb) } else { None }
}

#[tauri::command]
pub async fn get_debug_info(state: State<'_, AppState>) -> Result<DebugInfo, String> {
    tracing::debug!("IPC: get_debug_info");

    let config = state.config.lock().await;
    let whisper_dir = config.resolved_model_dir();
    let llm_dir = config.resolved_llm_model_dir();
    drop(config);

    let llm_gpu_fallback = state.llm_gpu_fallback.load(std::sync::atomic::Ordering::Relaxed);
    let stt_backend_ready = state.stt_backend_ready.load(std::sync::atomic::Ordering::Relaxed);

    let info = tokio::task::spawn_blocking(move || {
        use sysinfo::{Pid, System};

        let data_dir = dirs::data_dir()
            .unwrap_or_default()
            .join("verbatim");
        let config_dir = dirs::config_dir()
            .unwrap_or_default()
            .join("verbatim");

        let log_dir = data_dir.join("logs");
        let log_dir_str = log_dir.to_string_lossy().to_string();

        // Collect log files (most recent first, cap at 30)
        let mut log_files: Vec<LogFileInfo> = std::fs::read_dir(&log_dir)
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                let meta = e.metadata().ok()?;
                if meta.is_file() {
                    Some(LogFileInfo {
                        name: e.file_name().to_string_lossy().to_string(),
                        size_bytes: meta.len(),
                    })
                } else {
                    None
                }
            })
            .collect();
        log_files.sort_by(|a, b| b.name.cmp(&a.name));
        log_files.truncate(30);

        let logs_bytes: u64 = log_files.iter().map(|f| f.size_bytes).sum();

        // Storage sizes
        let whisper_models_bytes = dir_size_bytes(&whisper_dir);
        let llm_models_bytes = dir_size_bytes(&llm_dir);

        // Database: main file + WAL + SHM
        let db_path = data_dir.join("verbatim.db");
        let database_bytes = [
            db_path.clone(),
            db_path.with_extension("db-wal"),
            db_path.with_extension("db-shm"),
        ]
        .iter()
        .filter_map(|p| std::fs::metadata(p).ok().map(|m| m.len()))
        .sum();

        // Config file size
        let config_bytes = std::fs::metadata(config_dir.join("config.toml"))
            .map(|m| m.len())
            .unwrap_or(0);

        // Process memory (RSS)
        let mut sys = System::new();
        let pid = Pid::from_u32(std::process::id());
        sys.refresh_processes_specifics(
            sysinfo::ProcessesToUpdate::Some(&[pid]),
            true,
            sysinfo::ProcessRefreshKind::nothing().with_memory(),
        );
        let process_rss_mb = sys
            .process(pid)
            .map(|p| p.memory() / (1024 * 1024))
            .unwrap_or(0);

        // Total RAM
        sys.refresh_memory();
        let total_ram_mb = sys.total_memory() / (1024 * 1024);

        // VRAM (NVIDIA + AMD)
        let vram_info = query_vram();
        let amd_vram_info = query_amd_vram();
        let app_vram_mb = query_process_vram();

        // Determine runtime GPU usage:
        // STT (whisper) uses GPU if compiled with a GPU backend and the backend is ready
        let gpu_backend = compiled_gpu_backend();
        let stt_has_gpu_backend = gpu_backend != "cpu";
        let stt_using_gpu = stt_backend_ready && stt_has_gpu_backend;

        // LLM uses GPU only with CUDA and only if it didn't fall back to CPU
        let llm_has_cuda = cfg!(feature = "cuda");
        let llm_using_gpu = llm_has_cuda && !llm_gpu_fallback;

        DebugInfo {
            log_dir: log_dir_str,
            log_files,
            whisper_models_bytes,
            llm_models_bytes,
            database_bytes,
            logs_bytes,
            config_bytes,
            process_rss_mb,
            total_ram_mb,
            vram_info,
            amd_vram_info,
            gpu_backend: gpu_backend.to_string(),
            stt_using_gpu,
            llm_using_gpu,
            app_vram_mb,
        }
    })
    .await
    .map_err(|e| e.to_string())?;

    Ok(info)
}

#[tauri::command]
pub async fn get_config(state: State<'_, AppState>) -> Result<Config, String> {
    tracing::debug!("IPC: get_config");
    let config = state.config.lock().await;
    Ok(config.clone())
}

#[tauri::command]
pub async fn save_config(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
    config: Config,
) -> Result<(), String> {
    tracing::debug!(
        backend = %config.general.backend,
        language = %config.general.language,
        clipboard_only = config.general.clipboard_only,
        "IPC: save_config"
    );
    config.save().map_err(|e| e.to_string())?;
    // Update the STT service with new config
    let _ = state.stt_cmd_tx.send(SttCommand::UpdateConfig(config.clone()));
    *state.config.lock().await = config;
    // Invalidate balance cache so key changes trigger fresh checks
    {
        let mut cache = state.balance_cache.lock().map_err(|e| e.to_string())?;
        cache.deepgram = None;
        cache.openai = None;
    }
    let _ = app_handle.emit("config-changed", ());
    tracing::debug!("config saved and STT service notified");
    Ok(())
}

#[tauri::command]
pub async fn get_stats(state: State<'_, AppState>) -> Result<Stats, String> {
    tracing::debug!("IPC: get_stats");
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.get_stats().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_recent(state: State<'_, AppState>, limit: usize) -> Result<Vec<Transcription>, String> {
    tracing::debug!(limit, "IPC: get_recent");
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.get_recent(limit).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn search_history(
    state: State<'_, AppState>,
    query: String,
    limit: usize,
    offset: usize,
) -> Result<Vec<Transcription>, String> {
    tracing::debug!(query = %query, limit, offset, "IPC: search_history");
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.search(&query, limit, offset).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_transcriptions_for_date(state: State<'_, AppState>, date: String) -> Result<Vec<Transcription>, String> {
    tracing::debug!(date = %date, "IPC: get_transcriptions_for_date");
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.get_transcriptions_for_date(&date).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_daily_word_stats(state: State<'_, AppState>, days: i64) -> Result<Vec<DailyWordStats>, String> {
    tracing::debug!(days, "IPC: get_daily_word_stats");
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.get_daily_word_stats(days).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_daily_token_usage(state: State<'_, AppState>, days: i64) -> Result<Vec<DailyTokenUsage>, String> {
    tracing::debug!(days, "IPC: get_daily_token_usage");
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.get_daily_token_usage(days).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_token_usage_by_model(state: State<'_, AppState>) -> Result<Vec<ModelTokenUsage>, String> {
    tracing::debug!("IPC: get_token_usage_by_model");
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.get_token_usage_by_model().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_transcription(state: State<'_, AppState>, id: String) -> Result<(), String> {
    tracing::debug!(id = %id, "IPC: delete_transcription");
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.delete(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn pause_hotkey(state: State<'_, AppState>) -> Result<(), String> {
    tracing::debug!("IPC: pause_hotkey");
    let _ = state.stt_cmd_tx.send(SttCommand::PauseHotkey);
    Ok(())
}

#[tauri::command]
pub async fn resume_hotkey(state: State<'_, AppState>) -> Result<(), String> {
    tracing::debug!("IPC: resume_hotkey");
    let _ = state.stt_cmd_tx.send(SttCommand::ResumeHotkey);
    Ok(())
}

#[tauri::command]
pub async fn list_open_windows() -> Vec<String> {
    tracing::debug!("IPC: list_open_windows");
    let result = tokio::task::spawn_blocking(window_detect::list_open_windows)
        .await
        .unwrap_or_default();
    tracing::debug!(count = result.len(), "IPC: list_open_windows result");
    result
}

#[tauri::command]
pub async fn list_audio_devices() -> Vec<String> {
    tracing::debug!("IPC: list_audio_devices");
    let result = tokio::task::spawn_blocking(|| capture::list_input_devices())
        .await
        .unwrap_or_default();
    tracing::debug!(count = result.len(), "IPC: list_audio_devices result");
    result
}

#[derive(serde::Serialize)]
pub struct ModelInfo {
    pub name: String,
    pub size_bytes: u64,
    pub downloaded: bool,
}

#[tauri::command]
pub async fn list_models(state: State<'_, AppState>) -> Result<Vec<ModelInfo>, String> {
    tracing::debug!("IPC: list_models");
    let config = state.config.lock().await;
    let model_dir = config.resolved_model_dir();
    let models = model_manager::available_models();

    Ok(models
        .into_iter()
        .map(|name| {
            let downloaded = model_manager::model_exists(&model_dir, name);
            ModelInfo {
                name: name.to_string(),
                size_bytes: model_manager::model_size(name),
                downloaded,
            }
        })
        .collect())
}

#[derive(Clone, serde::Serialize)]
struct ModelDownloadProgress {
    model: String,
    downloaded: u64,
    total: u64,
    done: bool,
    error: Option<String>,
    cancelled: bool,
    verifying: bool,
}

#[tauri::command]
pub async fn download_model(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    name: String,
) -> Result<(), String> {
    tracing::debug!(name = %name, "IPC: download_model");
    let config = state.config.lock().await;
    let model_dir = config.resolved_model_dir();
    drop(config);

    let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
    *state.download_cancel_tx.lock().await = Some(cancel_tx);

    let model_name = name.clone();
    let handle = app.clone();

    tracing::debug!(name = %name, "model download task spawned");

    tauri::async_runtime::spawn(async move {
        let emit_name = model_name.clone();
        let emit_handle = handle.clone();
        let verify_name = model_name.clone();
        let verify_handle = handle.clone();

        let result = model_manager::download_model(
            &model_dir,
            &model_name,
            move |downloaded, total| {
                let _ = emit_handle.emit(
                    "model-download-progress",
                    ModelDownloadProgress {
                        model: emit_name.clone(),
                        downloaded,
                        total,
                        done: false,
                        error: None,
                        cancelled: false,
                        verifying: false,
                    },
                );
            },
            move || {
                let _ = verify_handle.emit(
                    "model-download-progress",
                    ModelDownloadProgress {
                        model: verify_name.clone(),
                        downloaded: 0,
                        total: 0,
                        done: false,
                        error: None,
                        cancelled: false,
                        verifying: true,
                    },
                );
            },
            cancel_rx,
        )
        .await;

        match result {
            Ok(_) => {
                tracing::debug!(model = %model_name, "model download completed");
                let _ = handle.emit(
                    "model-download-progress",
                    ModelDownloadProgress {
                        model: model_name,
                        downloaded: 0,
                        total: 0,
                        done: true,
                        error: None,
                        cancelled: false,
                        verifying: false,
                    },
                );
            }
            Err(e) => {
                let msg = e.to_string();
                tracing::warn!(model = %model_name, error = %msg, "model download failed");
                let cancelled = msg.contains("cancelled");
                let _ = handle.emit(
                    "model-download-progress",
                    ModelDownloadProgress {
                        model: model_name,
                        downloaded: 0,
                        total: 0,
                        done: true,
                        error: if cancelled { None } else { Some(msg) },
                        cancelled,
                        verifying: false,
                    },
                );
            }
        }
    });

    Ok(())
}

#[tauri::command]
pub async fn cancel_model_download(state: State<'_, AppState>) -> Result<(), String> {
    tracing::debug!("IPC: cancel_model_download");
    let guard = state.download_cancel_tx.lock().await;
    if let Some(tx) = guard.as_ref() {
        tracing::debug!("cancelling active model download");
        let _ = tx.send(true);
    } else {
        tracing::debug!("no active download to cancel");
    }
    Ok(())
}

#[tauri::command]
pub async fn delete_model(state: State<'_, AppState>, name: String) -> Result<(), String> {
    tracing::debug!(name = %name, "IPC: delete_model");
    let config = state.config.lock().await;
    let model_dir = config.resolved_model_dir();
    let path = model_manager::model_path(&model_dir, &name);
    drop(config);
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| e.to_string())?;
        tracing::debug!(path = %path.display(), "deleted model file");
    } else {
        tracing::debug!(path = %path.display(), "model file does not exist, nothing to delete");
    }
    Ok(())
}

// ── LLM Model Management ─────────────────────────────────────────────

#[derive(serde::Serialize)]
pub struct LlmModelInfo {
    pub id: String,
    pub display_name: String,
    pub size_bytes: u64,
    pub downloaded: bool,
    pub context_length: u32,
}

#[tauri::command]
pub async fn list_llm_models(state: State<'_, AppState>) -> Result<Vec<LlmModelInfo>, String> {
    tracing::debug!("IPC: list_llm_models");
    let config = state.config.lock().await;
    let model_dir = config.resolved_llm_model_dir();
    let models = llm_model_manager::available_llm_models();

    Ok(models
        .iter()
        .map(|def| {
            let downloaded = llm_model_manager::llm_model_exists(&model_dir, def.id);
            LlmModelInfo {
                id: def.id.to_string(),
                display_name: def.display_name.to_string(),
                size_bytes: def.size,
                downloaded,
                context_length: def.context_length,
            }
        })
        .collect())
}

#[derive(Clone, serde::Serialize)]
struct LlmModelDownloadProgress {
    model: String,
    downloaded: u64,
    total: u64,
    done: bool,
    error: Option<String>,
    cancelled: bool,
    verifying: bool,
}

#[tauri::command]
pub async fn download_llm_model(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    id: String,
) -> Result<(), String> {
    tracing::debug!(id = %id, "IPC: download_llm_model");
    let config = state.config.lock().await;
    let model_dir = config.resolved_llm_model_dir();
    drop(config);

    let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
    *state.llm_download_cancel_tx.lock().await = Some(cancel_tx);

    let model_id = id.clone();
    let handle = app.clone();

    tracing::debug!(id = %id, "LLM model download task spawned");

    tauri::async_runtime::spawn(async move {
        let emit_id = model_id.clone();
        let emit_handle = handle.clone();
        let verify_id = model_id.clone();
        let verify_handle = handle.clone();

        let result = llm_model_manager::download_llm_model(
            &model_dir,
            &model_id,
            move |downloaded, total| {
                let _ = emit_handle.emit(
                    "llm-model-download-progress",
                    LlmModelDownloadProgress {
                        model: emit_id.clone(),
                        downloaded,
                        total,
                        done: false,
                        error: None,
                        cancelled: false,
                        verifying: false,
                    },
                );
            },
            move || {
                let _ = verify_handle.emit(
                    "llm-model-download-progress",
                    LlmModelDownloadProgress {
                        model: verify_id.clone(),
                        downloaded: 0,
                        total: 0,
                        done: false,
                        error: None,
                        cancelled: false,
                        verifying: true,
                    },
                );
            },
            cancel_rx,
        )
        .await;

        match result {
            Ok(_) => {
                tracing::debug!(model = %model_id, "LLM model download completed");
                let _ = handle.emit(
                    "llm-model-download-progress",
                    LlmModelDownloadProgress {
                        model: model_id,
                        downloaded: 0,
                        total: 0,
                        done: true,
                        error: None,
                        cancelled: false,
                        verifying: false,
                    },
                );
            }
            Err(e) => {
                let msg = e.to_string();
                tracing::warn!(model = %model_id, error = %msg, "LLM model download failed");
                let cancelled = msg.contains("cancelled");
                let _ = handle.emit(
                    "llm-model-download-progress",
                    LlmModelDownloadProgress {
                        model: model_id,
                        downloaded: 0,
                        total: 0,
                        done: true,
                        error: if cancelled { None } else { Some(msg) },
                        cancelled,
                        verifying: false,
                    },
                );
            }
        }
    });

    Ok(())
}

#[tauri::command]
pub async fn cancel_llm_model_download(state: State<'_, AppState>) -> Result<(), String> {
    tracing::debug!("IPC: cancel_llm_model_download");
    let guard = state.llm_download_cancel_tx.lock().await;
    if let Some(tx) = guard.as_ref() {
        tracing::debug!("cancelling active LLM model download");
        let _ = tx.send(true);
    } else {
        tracing::debug!("no active LLM download to cancel");
    }
    Ok(())
}

#[tauri::command]
pub async fn delete_llm_model(state: State<'_, AppState>, id: String) -> Result<(), String> {
    tracing::debug!(id = %id, "IPC: delete_llm_model");
    let config = state.config.lock().await;
    let model_dir = config.resolved_llm_model_dir();
    let path = llm_model_manager::llm_model_path(&model_dir, &id);
    drop(config);
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| e.to_string())?;
        tracing::debug!(path = %path.display(), "deleted LLM model file");
    } else {
        tracing::debug!(path = %path.display(), "LLM model file does not exist, nothing to delete");
    }
    Ok(())
}

#[tauri::command]
pub async fn start_mic_monitor(state: State<'_, AppState>) -> Result<(), String> {
    tracing::debug!("IPC: start_mic_monitor");
    // Stop any existing monitor
    if let Some(tx) = state.mic_monitor_stop.lock().await.take() {
        let _ = tx.send(());
    }

    let config = state.config.lock().await;
    let device_name = config.audio.device.clone();
    drop(config);

    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();
    let level = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
    let level_clone = level.clone();

    // Spawn a dedicated thread to own the non-Send cpal::Stream
    std::thread::Builder::new()
        .name("mic-monitor".into())
        .spawn(move || {
            let device = match capture::get_input_device(&device_name) {
                Ok(d) => d,
                Err(e) => {
                    tracing::error!("Mic monitor: failed to get device: {}", e);
                    return;
                }
            };
            let (stream, monitor_level) = match capture::start_level_monitor(&device) {
                Ok(r) => r,
                Err(e) => {
                    tracing::error!("Mic monitor: failed to start: {}", e);
                    return;
                }
            };

            // Forward level from monitor to shared atomic
            let forward_level = monitor_level.clone();
            let forward_target = level_clone;
            let forward_thread = std::thread::spawn(move || {
                loop {
                    let bits = forward_level.load(Ordering::Relaxed);
                    forward_target.store(bits, Ordering::Relaxed);
                    std::thread::sleep(std::time::Duration::from_millis(16));
                }
            });

            // Block until stop signal
            let _ = stop_rx.blocking_recv();
            drop(stream);
            drop(forward_thread);
        })
        .map_err(|e| e.to_string())?;

    *state.mic_monitor_stop.lock().await = Some(stop_tx);
    *state.mic_monitor_level.lock().await = Some(level);
    tracing::debug!("mic monitor started");
    Ok(())
}

#[tauri::command]
pub async fn stop_mic_monitor(state: State<'_, AppState>) -> Result<(), String> {
    tracing::debug!("IPC: stop_mic_monitor");
    if let Some(tx) = state.mic_monitor_stop.lock().await.take() {
        let _ = tx.send(());
    }
    *state.mic_monitor_level.lock().await = None;
    Ok(())
}

#[tauri::command]
pub async fn get_mic_level(state: State<'_, AppState>) -> Result<f32, String> {
    // trace-only: this is called every frame
    let guard = state.mic_monitor_level.lock().await;
    match guard.as_ref() {
        Some(atomic) => {
            let bits = atomic.load(Ordering::Relaxed);
            Ok(f32::from_bits(bits))
        }
        None => Ok(0.0),
    }
}

/// Check macOS Accessibility and Microphone permissions.
/// Returns None on non-macOS platforms.
#[tauri::command]
pub async fn check_macos_permissions() -> Option<MacPermissions> {
    #[cfg(not(target_os = "macos"))]
    {
        None
    }

    #[cfg(target_os = "macos")]
    {
        tracing::debug!("IPC: check_macos_permissions");

        // Check Accessibility via AXIsProcessTrusted
        extern "C" {
            fn AXIsProcessTrusted() -> bool;
        }
        let accessibility = unsafe { AXIsProcessTrusted() };

        // Check Microphone by attempting to list and briefly open an input device
        let microphone = tokio::task::spawn_blocking(|| {
            let devices = capture::list_input_devices();
            if devices.is_empty() {
                return false;
            }
            // Try to get the default device and start a level monitor briefly
            match capture::get_input_device("") {
                Ok(device) => capture::start_level_monitor(&device).is_ok(),
                Err(_) => false,
            }
        })
        .await
        .unwrap_or(false);

        tracing::debug!(accessibility, microphone, "macOS permission check result");
        Some(MacPermissions {
            accessibility,
            microphone,
        })
    }
}

// ── API Cost & Balance Commands ──────────────────────────────────────

const BALANCE_CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(4 * 60 * 60);

#[derive(serde::Serialize, Clone)]
pub struct CreditBalance {
    pub provider: String,
    pub amount: f64,
    pub currency: String,
    pub checked_at: String,
    pub estimated_usage_since: f64,
    pub from_cache: bool,
}

#[tauri::command]
pub async fn check_deepgram_balance(state: State<'_, AppState>, force: bool) -> Result<CreditBalance, String> {
    tracing::debug!(force, "IPC: check_deepgram_balance");

    // Early check: bail if no API key configured
    {
        let config = state.config.lock().await;
        if config.deepgram.api_key.is_empty() {
            return Err("Deepgram API key not configured".into());
        }
    }

    // Check cache first (if not forcing)
    if !force {
        let cached_data = {
            let cache = state.balance_cache.lock().map_err(|e| e.to_string())?;
            cache.deepgram.as_ref().and_then(|c| {
                if c.checked_at_instant.elapsed() < BALANCE_CACHE_TTL {
                    Some((c.balance, c.currency.clone(), c.checked_at))
                } else {
                    None
                }
            })
        };

        if let Some((balance, currency, checked_at)) = cached_data {
            let checked_at_str = checked_at.format("%Y-%m-%d %H:%M:%S").to_string();
            let estimated = {
                let db = state.db.lock().map_err(|e| e.to_string())?;
                db.get_estimated_costs_since(&checked_at_str, Some("deepgram")).unwrap_or(0.0)
            };
            return Ok(CreditBalance {
                provider: "deepgram".into(),
                amount: balance,
                currency,
                checked_at: checked_at.to_rfc3339(),
                estimated_usage_since: estimated,
                from_cache: true,
            });
        }
    }

    let config = state.config.lock().await;
    let api_key = config.deepgram.api_key.clone();
    drop(config);

    let balance = verbatim_core::stt::deepgram::check_balance(&api_key)
        .await
        .map_err(|e| e.to_string())?;

    let now = chrono::Local::now();

    {
        let mut cache = state.balance_cache.lock().map_err(|e| e.to_string())?;
        cache.deepgram = Some(crate::state::CachedBalance {
            balance: balance.amount,
            currency: balance.currency.clone(),
            checked_at: now,
            checked_at_instant: std::time::Instant::now(),
        });
    }

    Ok(CreditBalance {
        provider: "deepgram".into(),
        amount: balance.amount,
        currency: balance.currency,
        checked_at: now.to_rfc3339(),
        estimated_usage_since: 0.0,
        from_cache: false,
    })
}

#[tauri::command]
pub async fn check_openai_costs(state: State<'_, AppState>, force: bool) -> Result<CreditBalance, String> {
    tracing::debug!(force, "IPC: check_openai_costs");

    // Early check: bail if no admin key configured
    {
        let config = state.config.lock().await;
        if config.openai.admin_key.is_empty() {
            return Err("OpenAI Admin key not configured. Add it in API Keys settings.".into());
        }
    }

    if !force {
        let cached_data = {
            let cache = state.balance_cache.lock().map_err(|e| e.to_string())?;
            cache.openai.as_ref().and_then(|c| {
                if c.checked_at_instant.elapsed() < BALANCE_CACHE_TTL {
                    Some((c.balance, c.currency.clone(), c.checked_at))
                } else {
                    None
                }
            })
        };

        if let Some((balance, currency, checked_at)) = cached_data {
            let checked_at_str = checked_at.format("%Y-%m-%d %H:%M:%S").to_string();
            let estimated = {
                let db = state.db.lock().map_err(|e| e.to_string())?;
                db.get_estimated_costs_since(&checked_at_str, Some("openai")).unwrap_or(0.0)
            };
            return Ok(CreditBalance {
                provider: "openai".into(),
                amount: balance,
                currency,
                checked_at: checked_at.to_rfc3339(),
                estimated_usage_since: estimated,
                from_cache: true,
            });
        }
    }

    let config = state.config.lock().await;
    let admin_key = config.openai.admin_key.clone();
    drop(config);

    let costs = verbatim_core::stt::openai::check_costs(&admin_key)
        .await
        .map_err(|e| e.to_string())?;

    let now = chrono::Local::now();

    {
        let mut cache = state.balance_cache.lock().map_err(|e| e.to_string())?;
        cache.openai = Some(crate::state::CachedBalance {
            balance: costs.total_cost_usd,
            currency: "usd".into(),
            checked_at: now,
            checked_at_instant: std::time::Instant::now(),
        });
    }

    Ok(CreditBalance {
        provider: "openai".into(),
        amount: costs.total_cost_usd,
        currency: "usd".into(),
        checked_at: now.to_rfc3339(),
        estimated_usage_since: 0.0,
        from_cache: false,
    })
}

#[tauri::command]
pub async fn get_daily_cost_summary(state: State<'_, AppState>, days: i64) -> Result<Vec<DailyCostSummary>, String> {
    tracing::debug!(days, "IPC: get_daily_cost_summary");
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.get_daily_cost_summary(days).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_cost_by_provider(state: State<'_, AppState>) -> Result<Vec<ProviderCostSummary>, String> {
    tracing::debug!("IPC: get_cost_by_provider");
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.get_cost_by_provider().map_err(|e| e.to_string())
}

/// Open a specific macOS System Settings privacy pane.
#[tauri::command]
pub async fn open_macos_settings(pane: String) -> Result<(), String> {
    tracing::debug!(pane = %pane, "IPC: open_macos_settings");

    #[cfg(not(target_os = "macos"))]
    {
        let _ = pane;
        Ok(())
    }

    #[cfg(target_os = "macos")]
    {
        let url = match pane.as_str() {
            "accessibility" => "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
            "microphone" => "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone",
            _ => return Err(format!("Unknown settings pane: {}", pane)),
        };
        std::process::Command::new("open")
            .arg(url)
            .spawn()
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

#[tauri::command]
pub async fn check_for_update(app: tauri::AppHandle) -> Result<UpdateInfo, String> {
    tracing::debug!("IPC: check_for_update");

    let current_version = app.package_info().version.to_string();

    let client = reqwest::Client::new();
    let resp = client
        .get("https://api.github.com/repos/mkamran67/verbatim-desktop/releases/latest")
        .header("User-Agent", format!("Verbatim/{}", current_version))
        .header("Accept", "application/vnd.github.v3+json")
        .send()
        .await
        .map_err(|e| format!("Failed to check for updates: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("GitHub API returned status {}", resp.status()));
    }

    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    let tag = json["tag_name"]
        .as_str()
        .unwrap_or("")
        .trim_start_matches('v');
    let release_url = json["html_url"].as_str().unwrap_or("").to_string();
    let release_notes = json["body"].as_str().unwrap_or("").to_string();

    let update_available = version_newer_than(tag, &current_version);

    Ok(UpdateInfo {
        current_version,
        latest_version: tag.to_string(),
        update_available,
        release_url,
        release_notes,
    })
}

/// Simple semver comparison: returns true if `latest` is newer than `current`.
fn version_newer_than(latest: &str, current: &str) -> bool {
    let parse = |s: &str| -> (u32, u32, u32) {
        let parts: Vec<u32> = s.split('.').filter_map(|p| p.parse().ok()).collect();
        (
            parts.first().copied().unwrap_or(0),
            parts.get(1).copied().unwrap_or(0),
            parts.get(2).copied().unwrap_or(0),
        )
    };
    parse(latest) > parse(current)
}
