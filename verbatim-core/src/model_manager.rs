use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use tokio::sync::watch;

/// Definition of a whisper.cpp model: (name, size_bytes, sha256).
struct WhisperModelDef {
    name: &'static str,
    size: u64,
    sha256: &'static str,
}

/// Known whisper.cpp models with exact sizes and SHA256 checksums from Hugging Face.
const MODELS: &[WhisperModelDef] = &[
    WhisperModelDef { name: "tiny",      size: 77_691_713,    sha256: "be07e048e1e599ad46341c8d2a135645097a538221678b7acdd1b1919c6e1b21" },
    WhisperModelDef { name: "tiny.en",   size: 77_704_715,    sha256: "921e4cf8686fdd993dcd081a5da5b6c365bfde1162e72b08d75ac75289920b1f" },
    WhisperModelDef { name: "base",      size: 147_951_465,   sha256: "60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe" },
    WhisperModelDef { name: "base.en",   size: 147_964_211,   sha256: "a03779c86df3323075f5e796cb2ce5029f00ec8869eee3fdfb897afe36c6d002" },
    WhisperModelDef { name: "small",     size: 487_601_967,   sha256: "1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b" },
    WhisperModelDef { name: "small.en",  size: 487_614_201,   sha256: "c6138d6d58ecc8322097e0f987c32f1be8bb0a18532a3f88f734d1bbf9c41e5d" },
    WhisperModelDef { name: "medium",    size: 1_533_763_059, sha256: "6c14d5adee5f86394037b4e4e8b59f1673b6cee10e3cf0b11bbdbee79c156208" },
    WhisperModelDef { name: "medium.en", size: 1_533_774_781, sha256: "cc37e93478338ec7700281a7ac30a10128929eb8f427dda2e865faa8f6da4356" },
    WhisperModelDef { name: "large-v3",  size: 3_095_033_483, sha256: "64d182b440b98d5203c4f9bd541544d84c605196c4f7b845dfa11fb23594d1e2" },
];

fn model_url(name: &str) -> String {
    format!(
        "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-{}.bin",
        name
    )
}

/// Get the full path to a model file.
pub fn model_path(model_dir: &Path, model_name: &str) -> PathBuf {
    let path = model_dir.join(format!("ggml-{}.bin", model_name));
    tracing::trace!(dir = %model_dir.display(), name = model_name, path = %path.display(), "resolved model path");
    path
}

/// Check if a model is already downloaded.
pub fn model_exists(model_dir: &Path, model_name: &str) -> bool {
    let path = model_path(model_dir, model_name);
    let exists = path.exists();
    tracing::trace!(path = %path.display(), exists, "checking model existence");
    exists
}

/// List available model names.
pub fn available_models() -> Vec<&'static str> {
    MODELS.iter().map(|m| m.name).collect()
}

/// Get the size of a model in bytes.
pub fn model_size(model_name: &str) -> u64 {
    let size = find_model(model_name).map(|m| m.size).unwrap_or(0);
    tracing::trace!(model_name, size, "model size lookup");
    size
}

/// Find a model definition by name.
fn find_model(model_name: &str) -> Option<&'static WhisperModelDef> {
    MODELS.iter().find(|m| m.name == model_name)
}

/// Download a whisper model with progress reporting and cancellation support.
///
/// `on_progress` is called with (bytes_downloaded, total_bytes) after each chunk.
/// `on_verify_start` is called once when SHA256 verification begins.
/// `cancel_rx` is watched each chunk — if it becomes `true`, download is aborted and .part file cleaned up.
pub async fn download_model<F, V>(
    model_dir: &Path,
    model_name: &str,
    on_progress: F,
    on_verify_start: V,
    cancel_rx: watch::Receiver<bool>,
) -> Result<PathBuf>
where
    F: Fn(u64, u64),
    V: FnOnce(),
{
    let def = find_model(model_name).ok_or_else(|| {
        anyhow::anyhow!(
            "Unknown model '{}'. Available: {}",
            model_name,
            available_models().join(", ")
        )
    })?;

    std::fs::create_dir_all(model_dir)
        .with_context(|| format!("Failed to create model directory {}", model_dir.display()))?;

    let dest = model_path(model_dir, model_name);
    if dest.exists() {
        tracing::info!("Model '{}' already exists at {}", model_name, dest.display());
        return Ok(dest);
    }

    let url = model_url(model_name);
    tracing::info!("Downloading model '{}' from {}", model_name, url);

    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .send()
        .await
        .context("Failed to start model download")?;

    if !response.status().is_success() {
        bail!("Download failed with status: {}", response.status());
    }

    let total_size = response.content_length().unwrap_or(def.size);
    tracing::debug!(total_size, "download total size in bytes");

    let tmp_dest = dest.with_extension("bin.part");
    tracing::debug!(path = %tmp_dest.display(), "writing to temporary file");
    let mut file = tokio::fs::File::create(&tmp_dest)
        .await
        .with_context(|| format!("Failed to create {}", tmp_dest.display()))?;

    use tokio::io::AsyncWriteExt;

    let mut downloaded: u64 = 0;
    let mut stream = response.bytes_stream();
    use futures::StreamExt;

    while let Some(chunk_result) = stream.next().await {
        // Check cancellation
        if *cancel_rx.borrow() {
            drop(file);
            let _ = tokio::fs::remove_file(&tmp_dest).await;
            bail!("Download cancelled");
        }

        let chunk = chunk_result.context("Error reading download stream")?;
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;
        tracing::trace!(downloaded, total_size, pct = format_args!("{:.0}", downloaded as f64 / total_size as f64 * 100.0), "download progress");
        on_progress(downloaded, total_size);
    }

    file.flush().await?;
    drop(file);

    // Verify SHA256 integrity
    on_verify_start();
    tracing::info!("Verifying SHA256 checksum for '{}'", model_name);
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
            model_name, expected, actual
        );
    }
    tracing::info!("SHA256 verified for '{}'", model_name);

    tracing::debug!(from = %tmp_dest.display(), to = %dest.display(), "renaming temp file to final destination");
    tokio::fs::rename(&tmp_dest, &dest)
        .await
        .with_context(|| format!("Failed to rename {} to {}", tmp_dest.display(), dest.display()))?;

    tracing::info!("Model saved to {}", dest.display());
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
    fn test_model_path_format() {
        let path = model_path(Path::new("/tmp"), "base.en");
        assert_eq!(path, PathBuf::from("/tmp/ggml-base.en.bin"));
    }

    #[test]
    fn test_model_exists_false_for_missing() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!model_exists(dir.path(), "base.en"));
    }

    #[test]
    fn test_model_exists_true_for_present() {
        let dir = tempfile::tempdir().unwrap();
        let path = model_path(dir.path(), "base.en");
        fs::write(&path, b"fake model").unwrap();
        assert!(model_exists(dir.path(), "base.en"));
    }

    #[test]
    fn test_available_models_contains_expected() {
        let models = available_models();
        assert!(models.contains(&"tiny"));
        assert!(models.contains(&"base.en"));
        assert!(models.contains(&"large-v3"));
    }

    #[test]
    fn test_model_size_known() {
        assert_eq!(model_size("base.en"), 147_964_211);
        assert_eq!(model_size("tiny"), 77_691_713);
    }

    #[test]
    fn test_model_size_unknown_returns_zero() {
        assert_eq!(model_size("nonexistent"), 0);
    }

    #[tokio::test]
    async fn test_download_unknown_model_errors() {
        let dir = tempfile::tempdir().unwrap();
        let (_tx, rx) = watch::channel(false);
        let result = download_model(dir.path(), "bogus-model", |_, _| {}, || {}, rx).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unknown model"));
    }

    #[test]
    fn test_compute_sha256_known_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.bin");
        fs::write(&path, b"hello world").unwrap();
        let hash = compute_sha256(&path).unwrap();
        assert_eq!(hash, "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9");
    }

    #[test]
    fn test_all_models_have_valid_sha256() {
        for m in MODELS {
            assert_eq!(m.sha256.len(), 64, "model '{}' SHA256 should be 64 hex chars", m.name);
            assert!(m.sha256.chars().all(|c| c.is_ascii_hexdigit()), "model '{}' SHA256 should be hex", m.name);
        }
    }

    // ── Edge case tests ──────────────────────────────────

    #[test]
    fn test_all_models_have_nonzero_size() {
        for name in available_models() {
            assert!(model_size(name) > 0, "model '{}' should have nonzero size", name);
        }
    }

    #[test]
    fn test_model_path_special_characters() {
        let path = model_path(Path::new("/tmp/my dir"), "base.en");
        assert_eq!(path, PathBuf::from("/tmp/my dir/ggml-base.en.bin"));
    }

    #[test]
    fn test_model_url_format() {
        let url = model_url("base.en");
        assert!(url.contains("huggingface.co"), "URL should point to huggingface");
        assert!(url.contains("base.en"), "URL should contain model name");
        assert!(url.ends_with(".bin"), "URL should end with .bin");
    }
}
