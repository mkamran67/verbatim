mod app;
mod audio;
mod clipboard;
mod config;
mod db;
mod errors;
mod hotkey;
mod input;
mod model_manager;
mod platform;
mod stt;
mod tray;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "verbatim", about = "Real-time speech-to-text with push-to-talk")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Download a whisper model for local STT
    DownloadModel {
        /// Model name (e.g., tiny.en, base.en, small, medium, large-v3)
        name: String,
        /// Custom model directory (default: ~/.local/share/verbatim/models)
        #[arg(long)]
        model_dir: Option<PathBuf>,
    },
    /// Print the config file path
    ConfigPath,
    /// List available whisper models
    ListModels,
    /// Generate a default config file
    InitConfig,
}

fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Some(cmd) => {
            // CLI subcommands run in a tokio runtime
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(handle_command(cmd))?;
        }
        None => {
            // Default: run headless STT service (GUI is provided by Tauri)
            eprintln!("No command specified. Use --help for usage, or run via the Tauri desktop app.");
        }
    }

    Ok(())
}

async fn handle_command(cmd: Command) -> Result<()> {
    match cmd {
        Command::DownloadModel { name, model_dir } => {
            let config = config::Config::load()?;
            let dir = model_dir.unwrap_or_else(|| config.resolved_model_dir());
            model_manager::download_model(&dir, &name).await?;
        }
        Command::ConfigPath => {
            println!("{}", config::Config::config_path().display());
        }
        Command::ListModels => {
            println!("Available whisper models:");
            for model in model_manager::available_models() {
                println!("  {}", model);
            }
        }
        Command::InitConfig => {
            config::Config::save_default_config()?;
            println!(
                "Config written to {}",
                config::Config::config_path().display()
            );
        }
    }
    Ok(())
}
