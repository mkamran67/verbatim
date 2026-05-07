//! Lifecycle management for a bundled Ollama binary.
//!
//! `managed` mode: Verbatim downloads the pinned Ollama release, spawns
//! `ollama serve` bound to localhost, and terminates it on app exit.
//! `existing` / `custom` modes only use `probe` and `pull_model` against
//! an externally-managed daemon.

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

/// Step-level progress events emitted during install + spawn.
#[derive(Debug, Clone)]
pub enum InstallEvent {
    DownloadStart { url: String },
    DownloadProgress { downloaded: u64, total: Option<u64> },
    DownloadOk,
    ExtractStart,
    ExtractOk,
    SpawnStart,
    SpawnOk,
    HealthStart,
    HealthAttempt { attempt: u32 },
    HealthOk,
    Log(String),
}

/// Type-erased callback used through the install pipeline.
pub type InstallCallback = Arc<dyn Fn(InstallEvent) + Send + Sync + 'static>;

/// Pinned Ollama release. Bump when a new stable is validated against
/// Verbatim's post-processing flow.
pub const OLLAMA_VERSION: &str = "v0.12.0";

#[derive(Debug, Clone, Deserialize)]
pub struct VersionInfo {
    pub version: String,
}

#[derive(Debug, Clone)]
pub struct DetectInfo {
    pub reachable: bool,
    pub version: Option<String>,
    pub models: Vec<String>,
}

fn platform_asset() -> Result<&'static str> {
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    { Ok("ollama-linux-amd64.tgz") }
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    { Ok("ollama-linux-arm64.tgz") }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    { Ok("ollama-darwin.tgz") }
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    { Ok("ollama-darwin.tgz") }
    #[cfg(not(any(
        all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64")),
        all(target_os = "macos", any(target_arch = "x86_64", target_arch = "aarch64")),
    )))]
    { bail!("Unsupported platform for managed Ollama") }
}

pub fn managed_root(data_dir: &Path) -> PathBuf {
    data_dir.join("verbatim/ollama")
}

pub fn managed_binary(data_dir: &Path) -> PathBuf {
    managed_root(data_dir).join("bin/ollama")
}

pub fn managed_models_dir(data_dir: &Path) -> PathBuf {
    managed_root(data_dir).join("models")
}

/// Ensure the Ollama binary is present under `{data_dir}/verbatim/ollama/bin/ollama`.
/// Downloads and extracts if missing. Emits step-level progress via `cb`.
pub async fn ensure_installed(
    data_dir: &Path,
    cb: InstallCallback,
) -> Result<PathBuf> {
    let bin = managed_binary(data_dir);
    if bin.exists() {
        cb(InstallEvent::Log(format!("Ollama already installed at {}", bin.display())));
        return Ok(bin);
    }

    let root = managed_root(data_dir);
    std::fs::create_dir_all(root.join("bin"))
        .with_context(|| format!("Failed to create {}", root.display()))?;

    let asset = platform_asset()?;
    let url = format!(
        "https://github.com/ollama/ollama/releases/download/{}/{}",
        OLLAMA_VERSION, asset
    );
    tracing::info!(%url, "downloading Ollama release");
    cb(InstallEvent::DownloadStart { url: url.clone() });
    cb(InstallEvent::Log(format!("Downloading {}", url)));

    let client = reqwest::Client::new();
    let mut resp = client.get(&url).send().await
        .with_context(|| format!("HTTP request failed for {}", url))?
        .error_for_status()
        .with_context(|| format!("HTTP error for {}", url))?;
    let total = resp.content_length();
    if let Some(t) = total {
        cb(InstallEvent::Log(format!("Total size: {} bytes", t)));
    }

    let tarball = root.join(asset);
    let mut file = tokio::fs::File::create(&tarball).await
        .with_context(|| format!("Failed to create {}", tarball.display()))?;
    use tokio::io::AsyncWriteExt;
    let mut downloaded: u64 = 0;
    while let Some(chunk) = resp.chunk().await.context("network read failed")? {
        file.write_all(&chunk).await.context("write to tarball failed")?;
        downloaded += chunk.len() as u64;
        cb(InstallEvent::DownloadProgress { downloaded, total });
    }
    file.flush().await?;
    drop(file);
    tracing::info!(downloaded, "Ollama download complete");
    cb(InstallEvent::Log(format!("Downloaded {} bytes", downloaded)));
    cb(InstallEvent::DownloadOk);

    tracing::info!(tarball = %tarball.display(), "extracting Ollama tarball");
    cb(InstallEvent::ExtractStart);
    cb(InstallEvent::Log(format!("Extracting {}", tarball.display())));
    let extract_output = Command::new("tar")
        .arg("-xzf").arg(&tarball)
        .arg("-C").arg(&root)
        .output()
        .await
        .context("failed to run tar")?;
    if !extract_output.status.success() {
        let stderr = String::from_utf8_lossy(&extract_output.stderr);
        for line in stderr.lines() {
            cb(InstallEvent::Log(format!("tar: {}", line)));
        }
        bail!("tar extraction failed with {}", extract_output.status);
    }
    // Some release tarballs place ollama at the root; normalize to bin/ollama.
    let extracted_root_candidate = root.join("ollama");
    if extracted_root_candidate.is_file() && !bin.exists() {
        std::fs::rename(&extracted_root_candidate, &bin)?;
    }
    // Tarballs that include a bin/ directory place the binary at bin/ollama directly.
    if !bin.exists() {
        bail!("Ollama binary not found after extraction at {}", bin.display());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perm = std::fs::metadata(&bin)?.permissions();
        perm.set_mode(0o755);
        std::fs::set_permissions(&bin, perm)?;
    }
    let _ = std::fs::remove_file(&tarball);
    tracing::info!("Ollama extraction complete");
    cb(InstallEvent::Log(format!("Installed binary at {}", bin.display())));
    cb(InstallEvent::ExtractOk);
    Ok(bin)
}

/// Spawn `ollama serve` bound to `127.0.0.1:{port}` with models in `models_dir`.
/// Without callback: discards stdout/stderr.
pub async fn spawn(bin: &Path, port: u16, models_dir: &Path) -> Result<Child> {
    spawn_with_cb(bin, port, models_dir, None).await
}

/// Spawn `ollama serve` and stream progress + log lines through `cb`.
/// Captures stderr line-by-line and forwards to the callback as `Log`.
pub async fn spawn_with_cb(
    bin: &Path,
    port: u16,
    models_dir: &Path,
    cb: Option<InstallCallback>,
) -> Result<Child> {
    std::fs::create_dir_all(models_dir).ok();
    tracing::info!(bin = %bin.display(), port, models_dir = %models_dir.display(), "spawning ollama serve");
    if let Some(cb) = &cb {
        cb(InstallEvent::SpawnStart);
        cb(InstallEvent::Log(format!("Starting ollama serve on 127.0.0.1:{}", port)));
    }

    let (stdout_mode, stderr_mode) = if cb.is_some() {
        (Stdio::piped(), Stdio::piped())
    } else {
        (Stdio::null(), Stdio::null())
    };

    let mut child = Command::new(bin)
        .arg("serve")
        .env("OLLAMA_HOST", format!("127.0.0.1:{}", port))
        .env("OLLAMA_MODELS", models_dir)
        .stdout(stdout_mode)
        .stderr(stderr_mode)
        .kill_on_drop(true)
        .spawn()
        .context("failed to spawn ollama")?;

    if let Some(cb_ref) = cb.clone() {
        if let Some(stderr) = child.stderr.take() {
            let cb_clone = cb_ref.clone();
            tokio::spawn(async move {
                let reader = BufReader::new(stderr);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    tracing::warn!(target = "ollama-serve", "{}", line);
                    cb_clone(InstallEvent::Log(format!("ollama: {}", line)));
                }
            });
        }
        if let Some(stdout) = child.stdout.take() {
            let cb_clone = cb_ref.clone();
            tokio::spawn(async move {
                let reader = BufReader::new(stdout);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    tracing::info!(target = "ollama-serve", "{}", line);
                    cb_clone(InstallEvent::Log(format!("ollama: {}", line)));
                }
            });
        }
    }

    if let Some(cb) = &cb {
        cb(InstallEvent::SpawnOk);
        cb(InstallEvent::HealthStart);
    }

    // Health-check loop (10s timeout).
    let url = format!("http://127.0.0.1:{}/api/version", port);
    let client = reqwest::Client::new();
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    let mut attempt: u32 = 0;
    loop {
        if std::time::Instant::now() >= deadline {
            bail!("ollama did not become healthy within 10s");
        }
        attempt += 1;
        if let Some(cb) = &cb {
            cb(InstallEvent::HealthAttempt { attempt });
        }
        if let Ok(r) = client.get(&url).timeout(Duration::from_millis(500)).send().await {
            if r.status().is_success() {
                tracing::info!("ollama healthy");
                if let Some(cb) = &cb {
                    cb(InstallEvent::Log("Health check OK".to_string()));
                    cb(InstallEvent::HealthOk);
                }
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    Ok(child)
}

/// SIGTERM, 3s grace, then SIGKILL.
pub async fn shutdown(child: &mut Child) {
    #[cfg(unix)]
    if let Some(pid) = child.id() {
        unsafe {
            libc::kill(pid as i32, libc::SIGTERM);
        }
    }
    let _ = tokio::time::timeout(Duration::from_secs(3), child.wait()).await;
    let _ = child.kill().await;
}

pub async fn probe(url: &str, token: Option<&str>) -> Result<DetectInfo> {
    let client = reqwest::Client::new();
    let base = url.trim_end_matches('/');

    let mut req = client.get(format!("{}/api/version", base)).timeout(Duration::from_secs(2));
    if let Some(t) = token { req = req.bearer_auth(t); }
    let ver: Option<VersionInfo> = match req.send().await {
        Ok(r) if r.status().is_success() => r.json().await.ok(),
        _ => None,
    };
    let reachable = ver.is_some();

    let mut models = Vec::new();
    if reachable {
        let mut req = client.get(format!("{}/api/tags", base)).timeout(Duration::from_secs(2));
        if let Some(t) = token { req = req.bearer_auth(t); }
        if let Ok(r) = req.send().await {
            if let Ok(body) = r.json::<serde_json::Value>().await {
                if let Some(arr) = body.get("models").and_then(|v| v.as_array()) {
                    for m in arr {
                        if let Some(name) = m.get("name").and_then(|s| s.as_str()) {
                            models.push(name.to_string());
                        }
                    }
                }
            }
        }
    }

    Ok(DetectInfo {
        reachable,
        version: ver.map(|v| v.version),
        models,
    })
}

/// Pull a model via HTTP (works for all modes). Streams progress lines.
pub async fn pull_model<F: Fn(String) + Send + 'static>(
    url: &str,
    token: Option<&str>,
    model: &str,
    on_progress: F,
) -> Result<()> {
    let client = reqwest::Client::new();
    let base = url.trim_end_matches('/');
    let body = serde_json::json!({ "name": model, "stream": true });
    let mut req = client.post(format!("{}/api/pull", base)).json(&body);
    if let Some(t) = token { req = req.bearer_auth(t); }
    let mut resp = req.send().await?.error_for_status()?;
    while let Some(chunk) = resp.chunk().await? {
        if let Ok(s) = std::str::from_utf8(&chunk) {
            for line in s.lines() {
                if !line.trim().is_empty() {
                    on_progress(line.to_string());
                }
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct RunningModel {
    pub name: String,
    pub size_vram: u64,
}

/// Query `GET /api/ps` for currently loaded models. Returns an empty Vec when
/// the daemon is reachable but idle, and an error when unreachable.
pub async fn query_running_models(url: &str, token: Option<&str>) -> Result<Vec<RunningModel>> {
    let client = reqwest::Client::new();
    let base = url.trim_end_matches('/');
    let mut req = client
        .get(format!("{}/api/ps", base))
        .timeout(Duration::from_millis(500));
    if let Some(t) = token { req = req.bearer_auth(t); }
    let resp = req.send().await?.error_for_status()?;
    let body: serde_json::Value = resp.json().await?;
    let mut out = Vec::new();
    if let Some(arr) = body.get("models").and_then(|v| v.as_array()) {
        for m in arr {
            let name = m.get("name").and_then(|s| s.as_str()).unwrap_or("").to_string();
            let size_vram = m.get("size_vram").and_then(|n| n.as_u64()).unwrap_or(0);
            out.push(RunningModel { name, size_vram });
        }
    }
    Ok(out)
}

/// Ask Ollama to immediately unload `model` from memory by issuing an empty
/// `/api/generate` request with `keep_alive: 0`. Used when the user switches
/// post-processing models so the previous one doesn't sit in RAM/VRAM until
/// its 5-minute idle timeout expires.
pub async fn unload_model(url: &str, token: Option<&str>, model: &str) -> Result<()> {
    let client = reqwest::Client::new();
    let base = url.trim_end_matches('/');
    let body = serde_json::json!({ "model": model, "keep_alive": 0 });
    let mut req = client.post(format!("{}/api/generate", base)).json(&body);
    if let Some(t) = token { req = req.bearer_auth(t); }
    req.send().await?.error_for_status()?;
    Ok(())
}

pub async fn delete_model(url: &str, token: Option<&str>, model: &str) -> Result<()> {
    let client = reqwest::Client::new();
    let base = url.trim_end_matches('/');
    let body = serde_json::json!({ "name": model });
    let mut req = client.delete(format!("{}/api/delete", base)).json(&body);
    if let Some(t) = token { req = req.bearer_auth(t); }
    req.send().await?.error_for_status()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_managed_paths_under_data_dir() {
        let data = Path::new("/tmp/verbatim-data");
        assert!(managed_root(data).ends_with("verbatim/ollama"));
        assert!(managed_binary(data).ends_with("verbatim/ollama/bin/ollama"));
        assert!(managed_models_dir(data).ends_with("verbatim/ollama/models"));
    }

    #[tokio::test]
    async fn test_probe_unreachable_url_returns_not_reachable() {
        // Port 1 is reserved/unusable — probe should return reachable=false.
        let info = probe("http://127.0.0.1:1", None).await.unwrap();
        assert!(!info.reachable);
        assert!(info.version.is_none());
        assert!(info.models.is_empty());
    }
}
