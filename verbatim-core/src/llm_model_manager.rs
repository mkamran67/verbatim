use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use tokio::sync::watch;

/// Definition of a downloadable LLM model.
pub struct LlmModelDef {
    pub id: &'static str,
    pub display_name: &'static str,
    pub filename: &'static str,
    pub url: &'static str,
    pub size: u64,
    pub context_length: u32,
    pub sha256: &'static str,
}

/// Known LLM models available for download.
const LLM_MODELS: &[LlmModelDef] = &[
    LlmModelDef {
        id: "llama-3.2-1b",
        display_name: "Llama 3.2 1B",
        filename: "Llama-3.2-1B-Instruct-Q4_K_M.gguf",
        url: "https://huggingface.co/bartowski/Llama-3.2-1B-Instruct-GGUF/resolve/main/Llama-3.2-1B-Instruct-Q4_K_M.gguf",
        size: 807_694_464,
        context_length: 131072,
        sha256: "6f85a640a97cf2bf5b8e764087b1e83da0fdb51d7c9fab7d0fece9385611df83",
    },
    LlmModelDef {
        id: "llama-3.2-3b",
        display_name: "Llama 3.2 3B",
        filename: "Llama-3.2-3B-Instruct-Q4_K_M.gguf",
        url: "https://huggingface.co/bartowski/Llama-3.2-3B-Instruct-GGUF/resolve/main/Llama-3.2-3B-Instruct-Q4_K_M.gguf",
        size: 2_019_377_696,
        context_length: 131072,
        sha256: "6c1a2b41161032677be168d354123594c0e6e67d2b9227c84f296ad037c728ff",
    },
    LlmModelDef {
        id: "gemma-3-1b",
        display_name: "Gemma 3 1B",
        filename: "gemma-3-1b-it-Q4_K_M.gguf",
        url: "https://huggingface.co/bartowski/google_gemma-3-1b-it-GGUF/resolve/main/google_gemma-3-1b-it-Q4_K_M.gguf",
        size: 806_058_496,
        context_length: 32768,
        sha256: "12bf0fff8815d5f73a3c9b586bd8fee8e7b248c935de70dec367679873d0f29d",
    },
    LlmModelDef {
        id: "qwen2.5-1.5b",
        display_name: "Qwen 2.5 1.5B",
        filename: "qwen2.5-1.5b-instruct-q4_k_m.gguf",
        url: "https://huggingface.co/Qwen/Qwen2.5-1.5B-Instruct-GGUF/resolve/main/qwen2.5-1.5b-instruct-q4_k_m.gguf",
        size: 1_117_320_736,
        context_length: 32768,
        sha256: "6a1a2eb6d15622bf3c96857206351ba97e1af16c30d7a74ee38970e434e9407e",
    },
    LlmModelDef {
        id: "smollm2-1.7b",
        display_name: "SmolLM2 1.7B",
        filename: "SmolLM2-1.7B-Instruct-Q4_K_M.gguf",
        url: "https://huggingface.co/bartowski/SmolLM2-1.7B-Instruct-GGUF/resolve/main/SmolLM2-1.7B-Instruct-Q4_K_M.gguf",
        size: 1_055_609_824,
        context_length: 8192,
        sha256: "77665ea4815999596525c636fbeb56ba8b080b46ae85efef4f0d986a139834d7",
    },
];

/// Get the full path to an LLM model file.
pub fn llm_model_path(model_dir: &Path, model_id: &str) -> PathBuf {
    let def = find_model(model_id);
    let filename = def.map(|d| d.filename).unwrap_or(model_id);
    let path = model_dir.join(filename);
    tracing::trace!(dir = %model_dir.display(), id = model_id, path = %path.display(), "resolved LLM model path");
    path
}

/// Check if an LLM model is already downloaded.
pub fn llm_model_exists(model_dir: &Path, model_id: &str) -> bool {
    let path = llm_model_path(model_dir, model_id);
    let exists = path.exists();
    tracing::trace!(path = %path.display(), exists, "checking LLM model existence");
    exists
}

/// List available LLM model definitions.
pub fn available_llm_models() -> &'static [LlmModelDef] {
    LLM_MODELS
}

/// Get the approximate size of an LLM model in bytes.
pub fn llm_model_size(model_id: &str) -> u64 {
    let size = find_model(model_id).map(|d| d.size).unwrap_or(0);
    tracing::trace!(model_id, size, "LLM model size lookup");
    size
}

/// Find a model definition by id.
fn find_model(model_id: &str) -> Option<&'static LlmModelDef> {
    LLM_MODELS.iter().find(|m| m.id == model_id)
}

/// Default directory for LLM model storage.
pub fn default_llm_model_dir() -> String {
    dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("~/.local/share"))
        .join("verbatim/llm-models")
        .to_string_lossy()
        .into_owned()
}

/// Download an LLM model with progress reporting and cancellation support.
///
/// `on_progress` is called with (bytes_downloaded, total_bytes) after each chunk.
/// `on_verify_start` is called once when SHA256 verification begins.
/// `cancel_rx` is watched each chunk — if it becomes `true`, download is aborted and .part file cleaned up.
pub async fn download_llm_model<F, V>(
    model_dir: &Path,
    model_id: &str,
    on_progress: F,
    on_verify_start: V,
    cancel_rx: watch::Receiver<bool>,
) -> Result<PathBuf>
where
    F: Fn(u64, u64),
    V: FnOnce(),
{
    let def = find_model(model_id).ok_or_else(|| {
        let available: Vec<&str> = LLM_MODELS.iter().map(|m| m.id).collect();
        anyhow::anyhow!(
            "Unknown LLM model '{}'. Available: {}",
            model_id,
            available.join(", ")
        )
    })?;

    std::fs::create_dir_all(model_dir)
        .with_context(|| format!("Failed to create LLM model directory {}", model_dir.display()))?;

    let dest = model_dir.join(def.filename);
    if dest.exists() {
        tracing::info!("LLM model '{}' already exists at {}", model_id, dest.display());
        return Ok(dest);
    }

    tracing::info!("Downloading LLM model '{}' from {}", model_id, def.url);

    let client = reqwest::Client::new();
    let response = client
        .get(def.url)
        .send()
        .await
        .context("Failed to start LLM model download")?;

    if !response.status().is_success() {
        bail!("Download failed with status: {}", response.status());
    }

    let total_size = response.content_length().unwrap_or(def.size);
    tracing::debug!(total_size, "LLM download total size in bytes");

    let tmp_dest = dest.with_extension("gguf.part");
    tracing::debug!(path = %tmp_dest.display(), "writing to temporary file");
    let mut file = tokio::fs::File::create(&tmp_dest)
        .await
        .with_context(|| format!("Failed to create {}", tmp_dest.display()))?;

    use tokio::io::AsyncWriteExt;

    let mut downloaded: u64 = 0;
    let mut stream = response.bytes_stream();
    use futures::StreamExt;

    while let Some(chunk_result) = stream.next().await {
        if *cancel_rx.borrow() {
            drop(file);
            let _ = tokio::fs::remove_file(&tmp_dest).await;
            bail!("Download cancelled");
        }

        let chunk = chunk_result.context("Error reading download stream")?;
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;
        tracing::trace!(downloaded, total_size, pct = format_args!("{:.0}", downloaded as f64 / total_size as f64 * 100.0), "LLM download progress");
        on_progress(downloaded, total_size);
    }

    file.flush().await?;
    drop(file);

    // Verify SHA256 integrity
    on_verify_start();
    tracing::info!("Verifying SHA256 checksum for '{}'", model_id);
    let expected = def.sha256;
    let tmp_path = tmp_dest.clone();
    let actual = tokio::task::spawn_blocking(move || compute_sha256(&tmp_path))
        .await
        .context("SHA256 computation task failed")?
        .context("Failed to compute SHA256")?;

    if actual != expected {
        let _ = tokio::fs::remove_file(&tmp_dest).await;
        bail!(
            "SHA256 mismatch for '{}': expected {}, got {}. The download may be corrupted.",
            model_id, expected, actual
        );
    }
    tracing::info!("SHA256 verified for '{}'", model_id);

    tracing::debug!(from = %tmp_dest.display(), to = %dest.display(), "renaming temp file to final destination");
    tokio::fs::rename(&tmp_dest, &dest)
        .await
        .with_context(|| format!("Failed to rename {} to {}", tmp_dest.display(), dest.display()))?;

    tracing::info!("LLM model saved to {}", dest.display());
    Ok(dest)
}

/// Compute the SHA256 hex digest of a file.
fn compute_sha256(path: &Path) -> Result<String> {
    use sha2::{Sha256, Digest};
    use std::io::Read;

    let mut file = std::fs::File::open(path)
        .with_context(|| format!("Failed to open {} for checksum", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 { break; }
        hasher.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_llm_model_path_format() {
        let path = llm_model_path(Path::new("/tmp"), "llama-3.2-1b");
        assert_eq!(path, PathBuf::from("/tmp/Llama-3.2-1B-Instruct-Q4_K_M.gguf"));
    }

    #[test]
    fn test_llm_model_exists_false_for_missing() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!llm_model_exists(dir.path(), "llama-3.2-1b"));
    }

    #[test]
    fn test_llm_model_exists_true_for_present() {
        let dir = tempfile::tempdir().unwrap();
        let path = llm_model_path(dir.path(), "llama-3.2-1b");
        fs::write(&path, b"fake model").unwrap();
        assert!(llm_model_exists(dir.path(), "llama-3.2-1b"));
    }

    #[test]
    fn test_available_llm_models_count() {
        let models = available_llm_models();
        assert_eq!(models.len(), 5);
    }

    #[test]
    fn test_available_llm_models_contains_expected() {
        let models = available_llm_models();
        let ids: Vec<&str> = models.iter().map(|m| m.id).collect();
        assert!(ids.contains(&"llama-3.2-1b"));
        assert!(ids.contains(&"llama-3.2-3b"));
        assert!(ids.contains(&"gemma-3-1b"));
    }

    #[test]
    fn test_llm_model_size_known() {
        assert_eq!(llm_model_size("llama-3.2-1b"), 807_694_464);
        assert_eq!(llm_model_size("llama-3.2-3b"), 2_019_377_696);
        assert_eq!(llm_model_size("gemma-3-1b"), 806_058_496);
    }

    #[test]
    fn test_llm_model_size_unknown_returns_zero() {
        assert_eq!(llm_model_size("nonexistent"), 0);
    }

    #[tokio::test]
    async fn test_download_unknown_llm_model_errors() {
        let dir = tempfile::tempdir().unwrap();
        let (_tx, rx) = watch::channel(false);
        let result = download_llm_model(dir.path(), "bogus-model", |_, _| {}, || {}, rx).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unknown LLM model"));
    }

    #[test]
    fn test_default_llm_model_dir_not_empty() {
        let dir = default_llm_model_dir();
        assert!(!dir.is_empty());
        assert!(dir.contains("verbatim"));
    }

    #[test]
    fn test_compute_sha256_known_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.bin");
        fs::write(&path, b"hello world").unwrap();
        let hash = compute_sha256(&path).unwrap();
        // SHA256("hello world") = b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9
        assert_eq!(hash, "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9");
    }

    #[test]
    fn test_all_llm_models_have_valid_sha256() {
        for model in available_llm_models() {
            assert_eq!(model.sha256.len(), 64, "model '{}' SHA256 should be 64 hex chars", model.id);
            assert!(model.sha256.chars().all(|c| c.is_ascii_hexdigit()), "model '{}' SHA256 should be hex", model.id);
        }
    }

    // ── Edge case tests ──────────────────────────────────

    #[test]
    fn test_all_llm_models_have_nonzero_size() {
        for model in available_llm_models() {
            assert!(model.size > 0, "model '{}' should have nonzero size", model.id);
        }
    }

    #[test]
    fn test_all_llm_models_have_positive_context_length() {
        for model in available_llm_models() {
            assert!(model.context_length > 0, "model '{}' should have positive context_length", model.id);
        }
    }

    #[test]
    fn test_all_llm_models_have_nonempty_url() {
        for model in available_llm_models() {
            assert!(!model.url.is_empty(), "model '{}' should have a URL", model.id);
            assert!(model.url.starts_with("https://"), "model '{}' URL should be HTTPS", model.id);
        }
    }

    #[test]
    fn test_all_llm_models_have_nonempty_display_name() {
        for model in available_llm_models() {
            assert!(!model.display_name.is_empty(), "model '{}' should have a display name", model.id);
        }
    }

    #[test]
    fn test_llm_model_path_unknown_uses_id() {
        let path = llm_model_path(Path::new("/tmp"), "unknown-model");
        assert_eq!(path, PathBuf::from("/tmp/unknown-model"));
    }

    #[test]
    fn test_default_llm_model_dir_contains_llm() {
        let dir = default_llm_model_dir();
        assert!(dir.contains("llm"), "LLM model dir should contain 'llm': {}", dir);
    }
}
