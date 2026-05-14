use std::sync::atomic::Ordering;
use tauri::{Emitter, State};
use verbatim_core::app::SttCommand;
use verbatim_core::audio::capture;
use verbatim_core::config::Config;
use verbatim_core::db::{DailyCostSummary, DailyProviderUsage, DailyTokenUsage, DailyWordStats, ModelTokenUsage, ProviderCostSummary, Stats, Transcription};
use verbatim_core::input::window_detect;
use verbatim_core::model_manager;
use verbatim_core::ollama_manager;

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
    pub input_monitoring: bool,
    pub automation: bool,
}

#[derive(serde::Serialize)]
pub struct SystemInfo {
    pub total_ram_mb: u64,
    pub cpu_cores: usize,
    /// "apple_silicon" (macOS aarch64, unified memory + Metal), "linux" (CPU
    /// inference), or "other". Drives throughput and compatibility scoring.
    pub platform: String,
}

fn detect_platform() -> &'static str {
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        "apple_silicon"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        "other"
    }
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
        platform: detect_platform().to_string(),
    };
    tracing::debug!(ram_mb = info.total_ram_mb, cpu_cores = info.cpu_cores, platform = %info.platform, "system info");
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
pub struct SttRuntime {
    /// Backend identifier from config: "whisper-local" | "openai" | "deepgram" | …
    pub backend: String,
    /// Display name of the selected model for the active backend.
    pub model: String,
    /// True only for `whisper-local` — other backends run in the cloud.
    pub is_local: bool,
    /// True once the STT backend has finished initialising.
    pub backend_ready: bool,
    /// True when the local STT backend is using GPU acceleration. Always false
    /// for cloud backends.
    pub using_gpu: bool,
}

#[derive(serde::Serialize)]
pub struct OllamaStatus {
    pub reachable: bool,
    pub model_loaded: bool,
    pub using_gpu: bool,
    pub vram_bytes: u64,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PpKind {
    Disabled,
    Cloud,
    OllamaManaged,
    OllamaRemote,
}

#[derive(serde::Serialize)]
pub struct PpRuntime {
    pub enabled: bool,
    pub kind: PpKind,
    pub provider: String,
    pub model: String,
    pub ollama_status: Option<OllamaStatus>,
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
    /// Compile-time GPU backend (capability label), e.g. "vulkan", "cuda+vulkan",
    /// "metal", "cpu". Not the live runtime state.
    pub gpu_backend: String,
    pub stt: SttRuntime,
    pub pp: PpRuntime,
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
/// Sum NVIDIA VRAM used by Verbatim's own PID. When `include_ollama` is set,
/// also sum any compute-app whose process name is "ollama" — Ollama runs in a
/// separate process (whether we manage it or not), so the only way to attribute
/// its VRAM to Verbatim's footprint is by name match.
fn query_process_vram(include_ollama: bool) -> Option<u64> {
    let self_pid = std::process::id();
    let output = std::process::Command::new("nvidia-smi")
        .args([
            "--query-compute-apps=pid,process_name,used_memory",
            "--format=csv,noheader,nounits",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut total_mb = 0u64;
    let mut saw_any_row = false;
    for line in stdout.lines() {
        let parts: Vec<&str> = line.splitn(3, ',').map(|s| s.trim()).collect();
        if parts.len() != 3 { continue; }
        saw_any_row = true;
        let line_pid: u32 = match parts[0].parse() { Ok(p) => p, Err(_) => continue };
        let proc_name = parts[1].to_lowercase();
        let mb: u64 = match parts[2].parse() { Ok(m) => m, Err(_) => continue };

        if line_pid == self_pid {
            total_mb += mb;
        } else if include_ollama && (proc_name.contains("ollama") || proc_name.ends_with("/ollama")) {
            total_mb += mb;
        }
    }
    // Distinguish "no NVIDIA / nvidia-smi failed" (return None) from "ran fine
    // but nothing of ours is on the GPU" (return Some(0) so the UI still ticks).
    if saw_any_row || total_mb > 0 { Some(total_mb) } else { Some(0) }
}

#[tauri::command]
pub async fn get_debug_info(state: State<'_, AppState>) -> Result<DebugInfo, String> {
    tracing::debug!("IPC: get_debug_info");

    let config = state.config.lock().await;
    let whisper_dir = config.resolved_model_dir();
    let llm_dir = {
        let data = dirs::data_dir().unwrap_or_default();
        ollama_manager::managed_models_dir(&data)
    };

    // Snapshot the bits of config needed for the runtime block so we can drop
    // the lock before doing any I/O.
    let stt_backend = config.general.backend.clone();
    let stt_model = match stt_backend.as_str() {
        "whisper-local" => config.whisper.model.clone(),
        "openai" => config.openai.model.clone(),
        "deepgram" => config.deepgram.model.clone(),
        _ => String::new(),
    };
    let pp_enabled = config.post_processing.enabled;
    let pp_provider = config.post_processing.provider.clone();
    let pp_ollama_mode = config.post_processing.ollama_mode.clone();
    let pp_ollama_url = config.post_processing.ollama_url.clone();
    let pp_ollama_token = config.post_processing.ollama_auth_token.clone();
    let pp_ollama_port = config.post_processing.ollama_bundled_port;
    let pp_model_display = if pp_provider == "ollama" {
        config.post_processing.ollama_model.clone()
    } else {
        config.post_processing.model.clone()
    };
    drop(config);

    let stt_backend_ready = state.stt_backend_ready.load(std::sync::atomic::Ordering::Relaxed);

    // Resolve the post-processing runtime kind and probe Ollama if applicable.
    let pp_kind = if !pp_enabled {
        PpKind::Disabled
    } else if pp_provider == "ollama" {
        if pp_ollama_mode == "managed" { PpKind::OllamaManaged } else { PpKind::OllamaRemote }
    } else {
        PpKind::Cloud
    };

    let ollama_status = match &pp_kind {
        PpKind::OllamaManaged | PpKind::OllamaRemote => {
            let url = if matches!(pp_kind, PpKind::OllamaManaged) {
                format!("http://127.0.0.1:{}", pp_ollama_port)
            } else {
                pp_ollama_url.clone()
            };
            let token = if pp_ollama_token.is_empty() { None } else { Some(pp_ollama_token.as_str()) };
            match ollama_manager::query_running_models(&url, token).await {
                Ok(models) => {
                    let target = pp_model_display.clone();
                    let hit = models.iter().find(|m| m.name == target);
                    Some(OllamaStatus {
                        reachable: true,
                        model_loaded: hit.is_some(),
                        using_gpu: hit.map(|m| m.size_vram > 0).unwrap_or(false),
                        vram_bytes: hit.map(|m| m.size_vram).unwrap_or(0),
                    })
                }
                Err(_) => Some(OllamaStatus {
                    reachable: false,
                    model_loaded: false,
                    using_gpu: false,
                    vram_bytes: 0,
                }),
            }
        }
        _ => None,
    };

    let stt_is_local = stt_backend == "whisper-local";
    let include_ollama_in_vram = matches!(pp_kind, PpKind::OllamaManaged | PpKind::OllamaRemote);

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
        let app_vram_mb = query_process_vram(include_ollama_in_vram);

        // STT runtime: only the local Whisper backend can be GPU-accelerated.
        // Cloud backends (openai/deepgram/etc.) always report using_gpu=false.
        let gpu_backend = compiled_gpu_backend();
        let stt_has_gpu_backend = gpu_backend != "cpu";
        let stt_using_gpu = stt_is_local && stt_backend_ready && stt_has_gpu_backend;

        let stt = SttRuntime {
            backend: stt_backend,
            model: stt_model,
            is_local: stt_is_local,
            backend_ready: stt_backend_ready,
            using_gpu: stt_using_gpu,
        };

        let pp = PpRuntime {
            enabled: pp_enabled,
            kind: pp_kind,
            provider: pp_provider,
            model: pp_model_display,
            ollama_status,
        };

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
            stt,
            pp,
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

#[derive(serde::Serialize)]
pub struct CapturedHotkeyDto {
    pub key: u32,
    pub modifiers: Vec<u32>,
    pub label: String,
}

/// Arm the listener's capture slot and await the next keypress. Times out
/// after 30 seconds so a forgotten capture session doesn't wedge the
/// listener forever. `target` selects which listener to capture from
/// ("ptt" or "handsfree").
#[tauri::command]
pub async fn capture_hotkey(
    state: State<'_, AppState>,
    target: String,
) -> Result<CapturedHotkeyDto, String> {
    let slot = match target.as_str() {
        "ptt" => state.ptt_capture.clone(),
        "handsfree" => state.handsfree_capture.clone(),
        other => return Err(format!("unknown capture target: {}", other)),
    };
    tracing::debug!(target = %target, "IPC: capture_hotkey armed");
    let rx = slot.arm();
    let captured = tokio::time::timeout(std::time::Duration::from_secs(30), rx)
        .await
        .map_err(|_| {
            // Disarm on timeout so the next press isn't accidentally captured.
            let _ = slot.take();
            "capture timed out".to_string()
        })?
        .map_err(|_| "capture cancelled".to_string())?;
    Ok(CapturedHotkeyDto {
        key: captured.key,
        modifiers: captured.modifiers,
        label: captured.label,
    })
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

    // Snapshot the previous post-processing settings BEFORE the config swap
    // so we can detect an Ollama model switch and proactively unload the old
    // model. Otherwise it sits in RAM/VRAM until Ollama's 5-min idle timeout.
    let prev_pp = state.config.lock().await.post_processing.clone();

    // Update the STT service with new config
    let _ = state.stt_cmd_tx.send(SttCommand::UpdateConfig(config.clone()));
    *state.config.lock().await = config;

    // Fire-and-forget unload of the previously-loaded Ollama model when the
    // user has effectively switched away from it.
    {
        let new_pp = &state.config.lock().await.post_processing;
        let was_ollama = prev_pp.enabled && prev_pp.provider == "ollama";
        let still_same = new_pp.enabled
            && new_pp.provider == "ollama"
            && new_pp.ollama_model == prev_pp.ollama_model
            && new_pp.ollama_mode == prev_pp.ollama_mode
            && new_pp.ollama_url == prev_pp.ollama_url
            && new_pp.ollama_bundled_port == prev_pp.ollama_bundled_port;
        if was_ollama && !still_same && !prev_pp.ollama_model.is_empty() {
            let url = match prev_pp.ollama_mode.as_str() {
                "managed" => format!("http://127.0.0.1:{}", prev_pp.ollama_bundled_port),
                _ => prev_pp.ollama_url.clone(),
            };
            let token = if prev_pp.ollama_auth_token.is_empty() {
                None
            } else {
                Some(prev_pp.ollama_auth_token.clone())
            };
            let model = prev_pp.ollama_model.clone();
            tauri::async_runtime::spawn(async move {
                tracing::info!(model = %model, url = %url, "unloading previous Ollama model");
                if let Err(e) = ollama_manager::unload_model(&url, token.as_deref(), &model).await {
                    tracing::warn!(error = %e, model = %model, "Ollama unload failed");
                }
            });
        }
    }
    // Invalidate balance cache so key changes trigger fresh checks
    {
        let mut cache = state.balance_cache.lock().map_err(|e| e.to_string())?;
        cache.deepgram = None;
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
pub async fn get_daily_provider_usage(state: State<'_, AppState>, days: i64) -> Result<Vec<DailyProviderUsage>, String> {
    tracing::debug!(days, "IPC: get_daily_provider_usage");
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.get_daily_provider_usage(days).map_err(|e| e.to_string())
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

        // Throttle progress emissions: bytes_stream yields ~8-16KB chunks, so a
        // 1GB model would emit ~100k events in a few seconds. The Tauri IPC +
        // Redux render pipeline can't drain that fast, leaving stale events
        // queued behind the terminal `done: true` and the modal frozen with
        // out-of-date progress long after the backend has finished. Cap to
        // ~10 emissions/sec, always letting the very first chunk through and
        // always emitting the final-byte chunk so the bar reaches 100% before
        // the verify state takes over.
        let last_emit = std::sync::Arc::new(std::sync::Mutex::new(
            std::time::Instant::now() - std::time::Duration::from_secs(1),
        ));

        let result = model_manager::download_model(
            &model_dir,
            &model_name,
            move |downloaded, total| {
                let is_final = total > 0 && downloaded >= total;
                {
                    let mut last = last_emit.lock().unwrap();
                    if !is_final
                        && last.elapsed() < std::time::Duration::from_millis(100)
                    {
                        return;
                    }
                    *last = std::time::Instant::now();
                }
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

// ── Ollama Lifecycle ─────────────────────────────────────────────────

#[derive(serde::Serialize)]
pub struct OllamaDetectInfo {
    pub reachable: bool,
    pub version: Option<String>,
    pub models: Vec<String>,
}

fn ollama_base_url_from_config(config: &Config) -> String {
    match config.post_processing.ollama_mode.as_str() {
        "managed" => format!("http://127.0.0.1:{}", config.post_processing.ollama_bundled_port),
        _ => config.post_processing.ollama_url.clone(),
    }
}

fn ollama_auth_from_config(config: &Config) -> Option<String> {
    let t = &config.post_processing.ollama_auth_token;
    if t.is_empty() { None } else { Some(t.clone()) }
}

#[tauri::command]
pub async fn ollama_detect(state: State<'_, AppState>) -> Result<OllamaDetectInfo, String> {
    let config = state.config.lock().await;
    let url = ollama_base_url_from_config(&config);
    let token = ollama_auth_from_config(&config);
    drop(config);
    let info = ollama_manager::probe(&url, token.as_deref())
        .await
        .map_err(|e| e.to_string())?;
    Ok(OllamaDetectInfo {
        reachable: info.reachable,
        version: info.version,
        models: info.models,
    })
}

#[derive(Clone, serde::Serialize)]
pub struct OllamaInstallProgress {
    pub step: &'static str,         // "download" | "extract" | "spawn" | "health" | "log" | "done"
    pub status: &'static str,       // "start" | "progress" | "ok" | "error"
    pub downloaded: u64,
    pub total: Option<u64>,
    pub message: Option<String>,
    pub done: bool,
    pub error: Option<String>,
    pub logs: Option<Vec<String>>,
}

impl OllamaInstallProgress {
    fn empty(step: &'static str, status: &'static str) -> Self {
        Self {
            step, status,
            downloaded: 0, total: None,
            message: None, done: false, error: None, logs: None,
        }
    }
}

/// Bounded ring of recent log lines used to populate the failure-log panel.
const LOG_TAIL_CAPACITY: usize = 200;

/// Build an InstallCallback that:
///  - emits Tauri events to the frontend
///  - appends Log lines to a shared tail buffer (used for error reports)
fn make_install_cb(
    app: &tauri::AppHandle,
    log_tail: std::sync::Arc<std::sync::Mutex<std::collections::VecDeque<String>>>,
) -> ollama_manager::InstallCallback {
    let app = app.clone();
    std::sync::Arc::new(move |evt: ollama_manager::InstallEvent| {
        use ollama_manager::InstallEvent::*;
        let payload = match &evt {
            DownloadStart { url } => {
                let mut p = OllamaInstallProgress::empty("download", "start");
                p.message = Some(format!("Downloading {}", url));
                p
            }
            DownloadProgress { downloaded, total } => {
                let mut p = OllamaInstallProgress::empty("download", "progress");
                p.downloaded = *downloaded;
                p.total = *total;
                p
            }
            DownloadOk => OllamaInstallProgress::empty("download", "ok"),
            ExtractStart => OllamaInstallProgress::empty("extract", "start"),
            ExtractOk => OllamaInstallProgress::empty("extract", "ok"),
            SpawnStart => OllamaInstallProgress::empty("spawn", "start"),
            SpawnOk => OllamaInstallProgress::empty("spawn", "ok"),
            HealthStart => OllamaInstallProgress::empty("health", "start"),
            HealthAttempt { attempt } => {
                let mut p = OllamaInstallProgress::empty("health", "progress");
                p.message = Some(format!("Health check attempt {}", attempt));
                p
            }
            HealthOk => OllamaInstallProgress::empty("health", "ok"),
            Log(msg) => {
                if let Ok(mut q) = log_tail.lock() {
                    if q.len() >= LOG_TAIL_CAPACITY { q.pop_front(); }
                    q.push_back(msg.clone());
                }
                let mut p = OllamaInstallProgress::empty("log", "progress");
                p.message = Some(msg.clone());
                p
            }
        };
        let _ = app.emit("ollama-install-progress", payload);
    })
}

fn drain_log_tail(
    log_tail: &std::sync::Arc<std::sync::Mutex<std::collections::VecDeque<String>>>,
) -> Vec<String> {
    log_tail.lock().map(|q| q.iter().cloned().collect()).unwrap_or_default()
}

/// Spawn the managed Ollama daemon, store the Child in AppState, and emit
/// step events through the same `ollama-install-progress` channel.
pub async fn spawn_managed_ollama(
    app: &tauri::AppHandle,
    state: &AppState,
) -> Result<(), String> {
    let config = state.config.lock().await;
    let port = config.post_processing.ollama_bundled_port;
    drop(config);

    let data_dir = dirs::data_dir().ok_or_else(|| "Data dir unavailable".to_string())?;
    let bin = ollama_manager::managed_binary(&data_dir);
    let models_dir = ollama_manager::managed_models_dir(&data_dir);
    if !bin.exists() {
        return Err(format!("managed Ollama binary not found at {}", bin.display()));
    }

    // If a child is already alive, skip.
    {
        let mut guard = state.ollama_child.lock().await;
        if let Some(existing) = guard.as_mut() {
            if let Ok(None) = existing.try_wait() {
                tracing::debug!("managed Ollama already running, skipping spawn");
                return Ok(());
            }
        }
    }

    let log_tail = std::sync::Arc::new(std::sync::Mutex::new(
        std::collections::VecDeque::<String>::with_capacity(LOG_TAIL_CAPACITY),
    ));
    let cb = make_install_cb(app, log_tail.clone());

    match ollama_manager::spawn_with_cb(&bin, port, &models_dir, Some(cb)).await {
        Ok(child) => {
            let mut guard = state.ollama_child.lock().await;
            *guard = Some(child);
            Ok(())
        }
        Err(e) => {
            tracing::error!("failed to spawn managed Ollama: {:#}", e);
            let logs = drain_log_tail(&log_tail);
            let mut p = OllamaInstallProgress::empty("done", "error");
            p.done = true;
            p.error = Some(format!("{:#}", e));
            p.logs = Some(logs);
            let _ = app.emit("ollama-install-progress", p);
            Err(format!("{:#}", e))
        }
    }
}

#[tauri::command]
pub async fn ollama_install(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    tracing::debug!("IPC: ollama_install");
    let data_dir = dirs::data_dir()
        .ok_or_else(|| "Data dir unavailable".to_string())?;

    let port = state.config.lock().await.post_processing.ollama_bundled_port;
    let ollama_child = state.ollama_child.clone();
    let app_clone = app.clone();
    tauri::async_runtime::spawn(async move {
        let log_tail = std::sync::Arc::new(std::sync::Mutex::new(
            std::collections::VecDeque::<String>::with_capacity(LOG_TAIL_CAPACITY),
        ));
        let cb = make_install_cb(&app_clone, log_tail.clone());

        let install_result = ollama_manager::ensure_installed(&data_dir, cb.clone()).await;
        let bin = match install_result {
            Ok(bin) => bin,
            Err(e) => {
                tracing::error!("Ollama install failed: {:#}", e);
                let logs = drain_log_tail(&log_tail);
                let mut p = OllamaInstallProgress::empty("done", "error");
                p.done = true;
                p.error = Some(format!("{:#}", e));
                p.logs = Some(logs);
                let _ = app_clone.emit("ollama-install-progress", p);
                return;
            }
        };

        // Auto-spawn after install: if a child is already running, skip.
        let already_running = {
            let mut guard = ollama_child.lock().await;
            match guard.as_mut() {
                Some(c) => matches!(c.try_wait(), Ok(None)),
                None => false,
            }
        };

        if !already_running {
            let models_dir = ollama_manager::managed_models_dir(&data_dir);
            match ollama_manager::spawn_with_cb(&bin, port, &models_dir, Some(cb.clone())).await {
                Ok(child) => {
                    let mut guard = ollama_child.lock().await;
                    *guard = Some(child);
                }
                Err(e) => {
                    tracing::error!("Ollama spawn after install failed: {:#}", e);
                    let logs = drain_log_tail(&log_tail);
                    let mut p = OllamaInstallProgress::empty("done", "error");
                    p.done = true;
                    p.error = Some(format!("{:#}", e));
                    p.logs = Some(logs);
                    let _ = app_clone.emit("ollama-install-progress", p);
                    return;
                }
            }
        }

        let mut p = OllamaInstallProgress::empty("done", "ok");
        p.done = true;
        let _ = app_clone.emit("ollama-install-progress", p);
    });
    Ok(())
}

#[derive(Clone, serde::Serialize)]
struct OllamaPullProgress {
    model: String,
    line: String,
    done: bool,
    error: Option<String>,
}

#[tauri::command]
pub async fn ollama_pull_model(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    model: String,
) -> Result<(), String> {
    tracing::debug!(model = %model, "IPC: ollama_pull_model");
    let config = state.config.lock().await;
    let url = ollama_base_url_from_config(&config);
    let token = ollama_auth_from_config(&config);
    drop(config);

    let model_clone = model.clone();
    let emit_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let h = emit_handle.clone();
        let m = model_clone.clone();
        let result = ollama_manager::pull_model(&url, token.as_deref(), &model_clone, move |line| {
            let _ = h.emit("ollama-pull-progress", OllamaPullProgress {
                model: m.clone(), line, done: false, error: None,
            });
        }).await;
        match result {
            Ok(_) => {
                let _ = emit_handle.emit("ollama-pull-progress", OllamaPullProgress {
                    model: model_clone, line: String::new(), done: true, error: None,
                });
            }
            Err(e) => {
                let _ = emit_handle.emit("ollama-pull-progress", OllamaPullProgress {
                    model: model_clone, line: String::new(), done: true, error: Some(e.to_string()),
                });
            }
        }
    });
    Ok(())
}

#[tauri::command]
pub async fn ollama_list_local(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    tracing::debug!("IPC: ollama_list_local");
    let config = state.config.lock().await;
    let url = ollama_base_url_from_config(&config);
    let token = ollama_auth_from_config(&config);
    drop(config);
    let info = ollama_manager::probe(&url, token.as_deref())
        .await
        .map_err(|e| e.to_string())?;
    Ok(info.models)
}

/// Whether the managed Ollama binary is installed under the user data dir.
#[tauri::command]
pub async fn ollama_managed_installed() -> Result<bool, String> {
    let data_dir = dirs::data_dir().ok_or_else(|| "Data dir unavailable".to_string())?;
    Ok(ollama_manager::managed_binary(&data_dir).exists())
}

/// Start the managed Ollama daemon if it isn't already running.
///
/// Idempotent: returns Ok(false) when no spawn was needed (already running, not
/// in managed mode, or binary not installed), Ok(true) when a fresh child was
/// spawned. Used by the UI to auto-start the daemon when the user selects
/// Ollama as the post-processing backend without going through the install
/// flow (e.g. after a settings reset).
#[tauri::command]
pub async fn ollama_start(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<bool, String> {
    tracing::debug!("IPC: ollama_start");
    let mode = state.config.lock().await.post_processing.ollama_mode.clone();
    if mode != "managed" {
        return Ok(false);
    }
    let data_dir = dirs::data_dir().ok_or_else(|| "Data dir unavailable".to_string())?;
    if !ollama_manager::managed_binary(&data_dir).exists() {
        return Ok(false);
    }
    {
        let mut guard = state.ollama_child.lock().await;
        if let Some(existing) = guard.as_mut() {
            if let Ok(None) = existing.try_wait() {
                return Ok(false);
            }
        }
    }
    spawn_managed_ollama(&app, &state).await?;
    Ok(true)
}

/// Stop the managed Ollama daemon (if we own one) and start it again.
/// Useful for debugging — surfaced as a "Restart" button in the UI.
#[tauri::command]
pub async fn ollama_restart(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    tracing::info!("IPC: ollama_restart");
    let data_dir = dirs::data_dir().ok_or_else(|| "Data dir unavailable".to_string())?;
    if !ollama_manager::managed_binary(&data_dir).exists() {
        return Err("Managed Ollama is not installed".to_string());
    }
    {
        let mut guard = state.ollama_child.lock().await;
        if let Some(mut child) = guard.take() {
            tracing::info!("restart: shutting down current managed Ollama");
            ollama_manager::shutdown(&mut child).await;
        }
    }
    spawn_managed_ollama(&app, &state).await
}

/// Uninstall managed Ollama: stop the running daemon (if we own it) and
/// remove `{data_dir}/verbatim/ollama/` (binary, models, everything).
/// Emits progress through `ollama-uninstall-progress` so the UI can show
/// step-by-step status.
#[derive(Clone, serde::Serialize)]
struct OllamaUninstallProgress {
    step: &'static str,   // "stop" | "remove" | "done"
    status: &'static str, // "start" | "ok" | "error"
    message: Option<String>,
    done: bool,
    error: Option<String>,
}

impl OllamaUninstallProgress {
    fn new(step: &'static str, status: &'static str) -> Self {
        Self { step, status, message: None, done: false, error: None }
    }
}

#[tauri::command]
pub async fn ollama_uninstall(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    tracing::info!("IPC: ollama_uninstall");
    let data_dir = dirs::data_dir().ok_or_else(|| "Data dir unavailable".to_string())?;
    let root = ollama_manager::managed_root(&data_dir);

    let app_clone = app.clone();
    let ollama_child = state.ollama_child.clone();

    tauri::async_runtime::spawn(async move {
        // Step 1: stop running managed daemon (if any).
        let mut p = OllamaUninstallProgress::new("stop", "start");
        p.message = Some("Stopping managed Ollama".to_string());
        let _ = app_clone.emit("ollama-uninstall-progress", p);

        {
            let mut guard = ollama_child.lock().await;
            if let Some(mut child) = guard.take() {
                tracing::info!("uninstall: shutting down managed Ollama");
                ollama_manager::shutdown(&mut child).await;
            }
        }
        let _ = app_clone.emit("ollama-uninstall-progress", OllamaUninstallProgress::new("stop", "ok"));

        // Step 2: delete the install directory (binary + models + temp tarball).
        if !root.exists() {
            tracing::info!(path = %root.display(), "uninstall: nothing to delete");
            let mut p = OllamaUninstallProgress::new("done", "ok");
            p.done = true;
            p.message = Some("Already uninstalled".to_string());
            let _ = app_clone.emit("ollama-uninstall-progress", p);
            return;
        }
        let mut p = OllamaUninstallProgress::new("remove", "start");
        p.message = Some(format!("Removing {}", root.display()));
        let _ = app_clone.emit("ollama-uninstall-progress", p);
        tracing::info!(path = %root.display(), "uninstall: removing managed install dir");

        let root_for_task = root.clone();
        let remove_result = tokio::task::spawn_blocking(move || {
            std::fs::remove_dir_all(&root_for_task)
        }).await;

        match remove_result {
            Ok(Ok(())) => {
                tracing::info!("uninstall: directory removed");
                let _ = app_clone.emit("ollama-uninstall-progress", OllamaUninstallProgress::new("remove", "ok"));
                let mut p = OllamaUninstallProgress::new("done", "ok");
                p.done = true;
                let _ = app_clone.emit("ollama-uninstall-progress", p);
            }
            Ok(Err(e)) => {
                tracing::error!(error = %e, "uninstall: remove_dir_all failed");
                let mut p = OllamaUninstallProgress::new("done", "error");
                p.done = true;
                p.error = Some(e.to_string());
                let _ = app_clone.emit("ollama-uninstall-progress", p);
            }
            Err(e) => {
                tracing::error!(error = %e, "uninstall: join error");
                let mut p = OllamaUninstallProgress::new("done", "error");
                p.done = true;
                p.error = Some(e.to_string());
                let _ = app_clone.emit("ollama-uninstall-progress", p);
            }
        }
    });

    Ok(())
}

/// One entry in a registry-search result.
#[derive(Clone, serde::Serialize)]
pub struct OllamaRegistryEntry {
    pub model_name: String,
    pub description: String,
    pub labels: Vec<String>,
    pub pulls: u64,
    pub last_updated: Option<String>,
    pub url: Option<String>,
}

/// Search the public Ollama model library. Ollama doesn't expose a JSON
/// search API, so we fetch `https://ollama.com/search?q=...`, extract the
/// Next.js `__NEXT_DATA__` script (which embeds the page's data as JSON),
/// and walk it for model entries. This is more stable than scraping the
/// rendered HTML — the JSON keys persist across UI refactors.
///
/// On any HTTP/parse failure the caller gets an `Err`. The frontend
/// renders curated results regardless, so the feature degrades gracefully.
#[tauri::command]
pub async fn ollama_search_registry(query: String) -> Result<Vec<OllamaRegistryEntry>, String> {
    tracing::debug!(query = %query, "IPC: ollama_search_registry");
    let q = query.trim();
    if q.is_empty() {
        return Ok(Vec::new());
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(6))
        // ollama.com gates non-browser UAs in some regions.
        .user_agent("Mozilla/5.0 (X11; Linux x86_64) verbatim-desktop")
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .get("https://ollama.com/search")
        .query(&[("q", q)])
        .send()
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "ollama.com search request failed");
            e.to_string()
        })?;
    if !resp.status().is_success() {
        let status = resp.status();
        tracing::warn!(%status, "ollama.com returned non-success");
        return Err(format!("ollama.com returned {}", status));
    }
    let html = resp.text().await.map_err(|e| e.to_string())?;

    let entries = parse_ollama_search_html(&html, 25);
    if entries.is_empty() {
        tracing::debug!(query = %q, "no results parsed from ollama.com search");
    }
    Ok(entries)
}

/// Extract model entries from an `ollama.com/search` HTML page.
///
/// ollama.com is rendered with Alpine.js + HTMX, not Next.js. Each result
/// card is an `<a href="/library/NAME">` block containing spans tagged
/// with stable `x-test-*` attributes:
///   - `x-test-search-response-title` — the model name
///   - `x-test-size` — one per available tag (e.g. "0.5b", "7b")
///   - `x-test-capability` — e.g. "tools", "vision", "embedding"
///   - `x-test-pull-count` — humanised string ("28.9M")
///   - `x-test-updated` — humanised ("1 year ago")
/// We carve the page into per-anchor slices and pull each field out by
/// looking for those attributes — no full HTML parser needed.
fn parse_ollama_search_html(html: &str, limit: usize) -> Vec<OllamaRegistryEntry> {
    // Find every `<a href="/library/NAME"` along with its byte offset and name.
    let needle = "href=\"/library/";
    let mut anchors: Vec<(usize, String)> = Vec::new();
    let mut idx = 0;
    while let Some(pos) = html[idx..].find(needle) {
        let abs = idx + pos + needle.len();
        let rest = &html[abs..];
        if let Some(end_quote) = rest.find('"') {
            let name = rest[..end_quote].trim().to_string();
            // Skip nested paths and namespaced models (e.g. user/model).
            if !name.is_empty() && !name.contains('/') {
                // Find the start of the actual <a tag (the '<' before "href=") so we
                // can search backwards for the anchor opening as the slice start.
                let anchor_start = html[..abs]
                    .rfind("<a")
                    .unwrap_or(abs.saturating_sub(20));
                anchors.push((anchor_start, name));
            }
            idx = abs + end_quote;
        } else {
            break;
        }
    }

    // Convert each (start, name) into a slice ending at the next anchor's start.
    let mut seen = std::collections::HashSet::<String>::new();
    let mut out: Vec<OllamaRegistryEntry> = Vec::new();
    for i in 0..anchors.len() {
        let (start, name) = (&anchors[i].0, &anchors[i].1);
        if !seen.insert(name.clone()) {
            continue;
        }
        let end = anchors.get(i + 1).map(|(s, _)| *s).unwrap_or(html.len());
        let slice = &html[*start..end];

        let description = extract_first_after(slice, "<p", ">", "</p>")
            .map(|s| html_decode(s.trim()))
            .unwrap_or_default();
        let labels = extract_all_attr_values(slice, "x-test-size");
        let pulls_raw = extract_all_attr_values(slice, "x-test-pull-count")
            .into_iter()
            .next();
        let pulls = pulls_raw.as_deref().map(parse_humanised_count).unwrap_or(0);
        let last_updated = extract_all_attr_values(slice, "x-test-updated")
            .into_iter()
            .next();

        out.push(OllamaRegistryEntry {
            model_name: name.clone(),
            description,
            labels,
            pulls,
            last_updated,
            url: Some(format!("https://ollama.com/library/{}", name)),
        });
        if out.len() >= limit {
            break;
        }
    }
    out
}

/// Find every span/element with the given attribute and return the inner text
/// content (assumes simple `<span attr…>VALUE</span>` shape that ollama.com uses).
fn extract_all_attr_values(haystack: &str, attr: &str) -> Vec<String> {
    let needle = attr;
    let mut out = Vec::new();
    let mut idx = 0;
    while let Some(pos) = haystack[idx..].find(needle) {
        let abs = idx + pos;
        // Find the '>' that closes the opening tag.
        if let Some(gt) = haystack[abs..].find('>') {
            let after = &haystack[abs + gt + 1..];
            // Find the next '<' which is the closing tag.
            if let Some(lt) = after.find('<') {
                let value = after[..lt].trim();
                if !value.is_empty() {
                    let decoded = html_decode(value);
                    if !out.iter().any(|x: &String| x == &decoded) {
                        out.push(decoded);
                    }
                }
                idx = abs + gt + 1 + lt;
                continue;
            }
        }
        idx = abs + needle.len();
    }
    out
}

/// Find the first occurrence of an opening tag prefix (e.g. `<p`), then capture
/// the text between the next `>` and the given closing tag.
fn extract_first_after(haystack: &str, open_prefix: &str, gt_marker: &str, close: &str) -> Option<String> {
    let pos = haystack.find(open_prefix)?;
    let after_open_prefix = &haystack[pos..];
    let gt = after_open_prefix.find(gt_marker)?;
    let inner_start = pos + gt + gt_marker.len();
    let close_at = haystack[inner_start..].find(close)?;
    Some(haystack[inner_start..inner_start + close_at].to_string())
}

/// Decode the small handful of HTML entities ollama.com emits in descriptions.
fn html_decode(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

/// Parse strings like "28.9M", "133k", "12,345", "999" into an approximate u64.
fn parse_humanised_count(s: &str) -> u64 {
    let s = s.trim().replace(',', "");
    if s.is_empty() {
        return 0;
    }
    let last = s.chars().last().unwrap();
    let multiplier: f64 = match last.to_ascii_uppercase() {
        'K' => 1_000.0,
        'M' => 1_000_000.0,
        'B' => 1_000_000_000.0,
        _ => 1.0,
    };
    let body = if multiplier > 1.0 { &s[..s.len() - 1] } else { &s };
    body.parse::<f64>().map(|n| (n * multiplier) as u64).unwrap_or(0)
}

#[cfg(test)]
mod ollama_search_tests {
    use super::*;

    /// Realistic shape of an ollama.com/search result card (trimmed).
    fn fixture_card(name: &str, desc: &str, sizes: &[&str], pulls: &str, updated: &str) -> String {
        let size_spans: String = sizes.iter()
            .map(|s| format!(r#"<span x-test-size class="…">{}</span>"#, s))
            .collect();
        format!(
            r#"
<a href="/library/{name}" class="group w-full">
  <div><h2><span x-test-search-response-title>{name}</span></h2>
  <p class="…">{desc}</p></div>
  <div><div class="flex flex-wrap space-x-2">{sizes}</div>
    <p class="…">
      <span><span x-test-pull-count>{pulls}</span><span> Pulls</span></span>
      <span><span x-test-updated>{updated}</span></span>
    </p>
  </div>
</a>
"#,
            name = name, desc = desc, sizes = size_spans, pulls = pulls, updated = updated,
        )
    }

    #[test]
    fn test_parses_real_card_structure() {
        let html = format!(
            "<html><body>{}{}</body></html>",
            fixture_card("qwen2.5", "Qwen 2.5 family", &["0.5b", "1.5b", "7b"], "28.9M", "1 year ago"),
            fixture_card("llama3.2", "Llama 3.2", &["1b", "3b"], "133k", "5 months ago"),
        );
        let entries = parse_ollama_search_html(&html, 25);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].model_name, "qwen2.5");
        assert_eq!(entries[0].description, "Qwen 2.5 family");
        assert_eq!(entries[0].labels, vec!["0.5b", "1.5b", "7b"]);
        assert_eq!(entries[0].pulls, 28_900_000);
        assert_eq!(entries[0].last_updated.as_deref(), Some("1 year ago"));
        assert_eq!(entries[1].model_name, "llama3.2");
        assert_eq!(entries[1].pulls, 133_000);
    }

    #[test]
    fn test_dedupes_repeated_anchors() {
        let html = format!(
            "<html>{}{}{}</html>",
            fixture_card("qwen2.5", "first", &["1b"], "1M", "now"),
            // Repeated anchor (e.g. nested or duplicate link to same model)
            r#"<a href="/library/qwen2.5">dup</a>"#,
            fixture_card("llama3.2", "second", &["3b"], "5k", "yesterday"),
        );
        let entries = parse_ollama_search_html(&html, 25);
        let names: Vec<&str> = entries.iter().map(|e| e.model_name.as_str()).collect();
        assert_eq!(names, vec!["qwen2.5", "llama3.2"]);
    }

    #[test]
    fn test_skips_namespaced_user_models() {
        let html = r#"<html><body>
<a href="/library/qwen2.5"><span x-test-search-response-title>qwen2.5</span><p>desc</p></a>
<a href="/charaf/Huihui-something">should not match</a>
</body></html>"#;
        let entries = parse_ollama_search_html(html, 25);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].model_name, "qwen2.5");
    }

    #[test]
    fn test_html_entity_decode_in_description() {
        let html = format!(
            "<html>{}</html>",
            fixture_card("foo", "Alibaba&#39;s &amp; friends", &["1b"], "0", "now"),
        );
        let entries = parse_ollama_search_html(&html, 25);
        assert_eq!(entries[0].description, "Alibaba's & friends");
    }

    #[test]
    fn test_humanised_pull_counts() {
        assert_eq!(parse_humanised_count("28.9M"), 28_900_000);
        assert_eq!(parse_humanised_count("133k"), 133_000);
        assert_eq!(parse_humanised_count("1.2B"), 1_200_000_000);
        assert_eq!(parse_humanised_count("12,345"), 12_345);
        assert_eq!(parse_humanised_count("999"), 999);
        assert_eq!(parse_humanised_count(""), 0);
        assert_eq!(parse_humanised_count("garbage"), 0);
    }

    #[test]
    fn test_empty_html_returns_empty() {
        assert!(parse_ollama_search_html("", 25).is_empty());
        assert!(parse_ollama_search_html("<html></html>", 25).is_empty());
    }
}

#[tauri::command]
pub async fn ollama_delete_model(state: State<'_, AppState>, model: String) -> Result<(), String> {
    tracing::debug!(model = %model, "IPC: ollama_delete_model");
    let config = state.config.lock().await;
    let url = ollama_base_url_from_config(&config);
    let token = ollama_auth_from_config(&config);
    drop(config);
    ollama_manager::delete_model(&url, token.as_deref(), &model)
        .await
        .map_err(|e| e.to_string())
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
        tracing::info!(accessibility, "Accessibility probe");

        // Input Monitoring: prefer the public CoreGraphics preflight API
        // (returns a plain bool); fall back to IOHIDCheckAccess(kIOHIDRequestTypeListenEvent)
        // if the preflight is unavailable. The combined OR avoids a false
        // negative when the TCC entry is queryable through one path but not
        // the other (common for unsigned dev binaries).
        #[link(name = "CoreGraphics", kind = "framework")]
        extern "C" {
            fn CGPreflightListenEventAccess() -> bool;
        }
        #[link(name = "IOKit", kind = "framework")]
        extern "C" {
            fn IOHIDCheckAccess(request_type: u32) -> u32;
        }
        const K_IOHID_REQUEST_TYPE_LISTEN_EVENT: u32 = 1;
        let cg_preflight = unsafe { CGPreflightListenEventAccess() };
        let iohid_raw = unsafe { IOHIDCheckAccess(K_IOHID_REQUEST_TYPE_LISTEN_EVENT) };
        let input_monitoring = cg_preflight || iohid_raw == 0;
        tracing::info!(
            cg_preflight,
            iohid_raw,
            input_monitoring,
            "Input Monitoring probe"
        );

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

        // Check Automation by running a tiny non-prompting AppleScript against
        // System Events. Exit code 1 with stderr containing -1743 (or "not
        // allowed") = denied; success = granted. We don't trigger the system
        // prompt here — that happens naturally on first window-detect call.
        let automation = tokio::task::spawn_blocking(|| {
            let out = std::process::Command::new("osascript")
                .args([
                    "-e",
                    "tell application \"System Events\" to return name of current user",
                ])
                .output();
            match out {
                Ok(o) => o.status.success(),
                Err(_) => false,
            }
        })
        .await
        .unwrap_or(false);

        tracing::info!(
            accessibility,
            microphone,
            input_monitoring,
            automation,
            "macOS permission check result"
        );
        Some(MacPermissions {
            accessibility,
            microphone,
            input_monitoring,
            automation,
        })
    }
}

/// Returns true if the user can read at least one keyboard-capable
/// `/dev/input/event*` device (i.e. is in the `input` group). On non-Linux
/// platforms this always returns true so the frontend's checklist can treat
/// the row as auto-passing.
#[tauri::command]
pub async fn check_linux_input_permission() -> bool {
    #[cfg(not(target_os = "linux"))]
    {
        true
    }

    #[cfg(target_os = "linux")]
    {
        tokio::task::spawn_blocking(verbatim_core::hotkey::evdev_listener::keyboard_input_accessible)
            .await
            .unwrap_or(false)
    }
}

// ── API Cost & Balance Commands ──────────────────────────────────────

const BALANCE_CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(4 * 60 * 60);

#[derive(serde::Serialize, Clone)]
pub struct CreditBalance {
    pub provider: String,
    /// Discriminator for the frontend. "balance" = real remaining credit;
    /// "estimated_cost" = fallback spend figure shown with a warning triangle.
    pub kind: String,
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
                kind: "balance".into(),
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
            kind: "balance".into(),
            checked_at: now,
            checked_at_instant: std::time::Instant::now(),
        });
    }

    Ok(CreditBalance {
        provider: "deepgram".into(),
        kind: "balance".into(),
        amount: balance.amount,
        currency: balance.currency,
        checked_at: now.to_rfc3339(),
        estimated_usage_since: 0.0,
        from_cache: false,
    })
}

/// Open a filesystem path in the OS file manager.
/// macOS → `open <path>`, Linux → `xdg-open <path>`.
#[tauri::command]
pub async fn open_path(path: String) -> Result<(), String> {
    tracing::debug!(path = %path, "IPC: open_path");
    if !std::path::Path::new(&path).exists() {
        return Err(format!("Path does not exist: {}", path));
    }
    #[cfg(target_os = "macos")]
    let cmd = "open";
    #[cfg(target_os = "linux")]
    let cmd = "xdg-open";
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    return Err("open_path is only supported on macOS and Linux".into());

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        std::process::Command::new(cmd)
            .arg(&path)
            .spawn()
            .map_err(|e| format!("failed to spawn {}: {}", cmd, e))?;
        Ok(())
    }
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
            "input-monitoring" => "x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent",
            "automation" => "x-apple.systempreferences:com.apple.preference.security?Privacy_Automation",
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
