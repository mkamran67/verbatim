use anyhow::{bail, Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use std::path::{Path, PathBuf};

/// Known whisper.cpp model names and their approximate sizes (for progress display).
const MODELS: &[(&str, u64)] = &[
    ("tiny", 75_000_000),
    ("tiny.en", 75_000_000),
    ("base", 142_000_000),
    ("base.en", 142_000_000),
    ("small", 466_000_000),
    ("small.en", 466_000_000),
    ("medium", 1_500_000_000),
    ("medium.en", 1_500_000_000),
    ("large-v3", 2_900_000_000),
];

fn model_url(name: &str) -> String {
    format!(
        "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-{}.bin",
        name
    )
}

/// Get the full path to a model file.
pub fn model_path(model_dir: &Path, model_name: &str) -> PathBuf {
    model_dir.join(format!("ggml-{}.bin", model_name))
}

/// Check if a model is already downloaded.
pub fn model_exists(model_dir: &Path, model_name: &str) -> bool {
    model_path(model_dir, model_name).exists()
}

/// List available model names.
pub fn available_models() -> Vec<&'static str> {
    MODELS.iter().map(|(name, _)| *name).collect()
}

/// Download a whisper model to the given directory.
pub async fn download_model(model_dir: &Path, model_name: &str) -> Result<PathBuf> {
    // Validate model name
    if !MODELS.iter().any(|(name, _)| *name == model_name) {
        bail!(
            "Unknown model '{}'. Available: {}",
            model_name,
            available_models().join(", ")
        );
    }

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

    let total_size = response.content_length().unwrap_or(
        MODELS
            .iter()
            .find(|(n, _)| *n == model_name)
            .map(|(_, s)| *s)
            .unwrap_or(0),
    );

    let pb = ProgressBar::new(total_size);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{msg} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
            .unwrap()
            .progress_chars("=>-"),
    );
    pb.set_message(format!("ggml-{}.bin", model_name));

    // Download to a temp file first, then rename (atomic-ish)
    let tmp_dest = dest.with_extension("bin.part");
    let mut file = tokio::fs::File::create(&tmp_dest)
        .await
        .with_context(|| format!("Failed to create {}", tmp_dest.display()))?;

    use tokio::io::AsyncWriteExt;

    let bytes = response
        .bytes()
        .await
        .context("Failed to download model bytes")?;

    file.write_all(&bytes).await?;
    file.flush().await?;
    drop(file);

    pb.set_position(bytes.len() as u64);
    pb.finish_with_message("Download complete");

    tokio::fs::rename(&tmp_dest, &dest)
        .await
        .with_context(|| format!("Failed to rename {} to {}", tmp_dest.display(), dest.display()))?;

    tracing::info!("Model saved to {}", dest.display());
    Ok(dest)
}
