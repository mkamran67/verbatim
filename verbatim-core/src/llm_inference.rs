use anyhow::Result;
use std::path::Path;
use std::sync::OnceLock;

use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{LlamaChatMessage, LlamaModel};
use llama_cpp_2::sampling::LlamaSampler;

/// Maximum number of tokens to generate in a single completion.
const MAX_GENERATION_TOKENS: usize = 512;

/// GGUF magic bytes: "GGUF" in little-endian.
const GGUF_MAGIC: [u8; 4] = [0x47, 0x47, 0x55, 0x46];

/// Minimum plausible GGUF file size (header + at least some data).
const GGUF_MIN_SIZE: u64 = 1024;

/// Global singleton for the llama.cpp backend.
///
/// Must be initialized once (via `init_backend()`) before whisper loads,
/// because both whisper-rs-sys and llama-cpp-sys-2 share the same ggml
/// symbols (due to `--allow-multiple-definition`). Calling `llama_backend_init()`
/// after whisper has already initialized ggml causes a segfault.
static LLAMA_BACKEND: OnceLock<LlamaBackend> = OnceLock::new();

/// Initialize the llama.cpp backend singleton.
///
/// Call this once at app startup, **before** creating the whisper STT backend.
/// Subsequent calls are no-ops. The backend lives for the process lifetime.
pub fn init_backend() -> Result<()> {
    if LLAMA_BACKEND.get().is_some() {
        return Ok(());
    }
    tracing::info!("initializing llama.cpp backend (singleton)");
    let backend = LlamaBackend::init()
        .map_err(|e| anyhow::anyhow!("Failed to initialize llama.cpp backend: {:?}", e))?;
    if let Err(extra) = LLAMA_BACKEND.set(backend) {
        // Another thread won the race. Dropping `extra` would call
        // llama_backend_free(), corrupting the winner — leak it instead.
        // Practically unreachable since LlamaBackend::init() has its own guard.
        std::mem::forget(extra);
        tracing::warn!("llama backend race: leaking duplicate");
    }
    tracing::info!("llama.cpp backend initialized successfully");
    Ok(())
}

fn get_backend() -> Result<&'static LlamaBackend> {
    LLAMA_BACKEND
        .get()
        .ok_or_else(|| anyhow::anyhow!("llama.cpp backend not initialized — call init_backend() first"))
}

/// Validate that a file looks like a valid GGUF model before passing it to FFI.
fn validate_gguf_file(path: &Path) -> Result<()> {
    use std::io::Read;

    if !path.exists() {
        anyhow::bail!("Model file does not exist: {}", path.display());
    }

    let metadata = std::fs::metadata(path)
        .map_err(|e| anyhow::anyhow!("Cannot read model file metadata: {}", e))?;

    let file_size = metadata.len();
    tracing::debug!(file_size, path = %path.display(), "validating GGUF file");

    if file_size < GGUF_MIN_SIZE {
        anyhow::bail!(
            "Model file too small ({} bytes) — likely corrupted or incomplete download: {}",
            file_size,
            path.display()
        );
    }

    // Check GGUF magic bytes
    let mut file = std::fs::File::open(path)
        .map_err(|e| anyhow::anyhow!("Cannot open model file: {}", e))?;
    let mut magic = [0u8; 4];
    file.read_exact(&mut magic)
        .map_err(|e| anyhow::anyhow!("Cannot read model file header: {}", e))?;

    if magic != GGUF_MAGIC {
        anyhow::bail!(
            "Invalid GGUF file (magic {:02x}{:02x}{:02x}{:02x}, expected GGUF): {}",
            magic[0], magic[1], magic[2], magic[3],
            path.display()
        );
    }

    tracing::debug!(file_size, "GGUF file validation passed");
    Ok(())
}

/// Attempt to load a GGUF model with the given parameters.
/// Catches both errors and panics from the FFI call.
fn try_load_model(
    backend: &'static LlamaBackend,
    model_path: &Path,
    model_params: &LlamaModelParams,
    _model_id: &str,
) -> Result<LlamaModel> {
    tracing::info!(n_gpu_layers = model_params.n_gpu_layers(), "attempting model load");
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        LlamaModel::load_from_file(backend, model_path, model_params)
    })) {
        Ok(Ok(model)) => Ok(model),
        Ok(Err(e)) => {
            anyhow::bail!("{:?}", e);
        }
        Err(panic_info) => {
            let msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = panic_info.downcast_ref::<String>() {
                s.clone()
            } else {
                "unknown panic".to_string()
            };
            anyhow::bail!("panic during load: {}", msg);
        }
    }
}

/// Wraps a loaded GGUF model for chat completion.
pub struct LlmEngine {
    model: LlamaModel,
    n_threads: u32,
    model_id: String,
    display_name: String,
    gpu_fallback: bool,
}

// SAFETY: LlamaModel is Send + Sync per llama-cpp-2 crate contract.
// The backend is a global singleton and never mutated after init.
unsafe impl Send for LlmEngine {}
unsafe impl Sync for LlmEngine {}

impl LlmEngine {
    /// Load a GGUF model from disk.
    ///
    /// This is expensive (1-3 seconds) and should be done once per model.
    /// The llama.cpp backend must already be initialized via `init_backend()`.
    /// `model_id` is the logical name (e.g. "gemma-3-1b") used to detect model changes.
    pub fn load(model_path: &Path, n_threads: u32, model_id: &str) -> Result<Self> {
        tracing::info!(path = %model_path.display(), n_threads, model_id, "loading LLM model");
        let start = std::time::Instant::now();

        // Validate the GGUF file before passing to FFI (prevents segfault on corrupt files)
        validate_gguf_file(model_path)?;

        // NOTE: We previously used a fork-based probe to test model loading in a
        // child process. This is no longer needed because the ggml symbol conflict
        // between whisper-rs and llama-cpp has been resolved at link time via
        // objcopy symbol renaming in build.rs. The probe would actually give false
        // negatives because the forked child inherits inconsistent ggml state.

        let backend = get_backend()?;
        tracing::debug!("llama backend acquired, loading model from file...");

        // Decide whether to attempt GPU offload.
        // Vulkan-only builds (no CUDA) force CPU for the LLM because the Vulkan
        // backend conflicts with WebKitGTK's GPU compositor, causing a segfault.
        // CUDA builds can safely use GPU since CUDA and WebKit don't share state.
        // Whisper STT keeps using Vulkan for GPU — it initializes before the
        // webview and doesn't cause the same conflict.
        let use_gpu = cfg!(feature = "cuda");

        let mut gpu_fallback = false;
        let model = if use_gpu {
            match try_load_model(backend, model_path, &LlamaModelParams::default(), model_id) {
                Ok(model) => model,
                Err(gpu_err) => {
                    tracing::warn!(
                        error = %gpu_err,
                        "model load failed with default params, retrying CPU-only (n_gpu_layers=0)"
                    );
                    match try_load_model(
                        backend,
                        model_path,
                        &LlamaModelParams::default().with_n_gpu_layers(0),
                        model_id,
                    ) {
                        Ok(model) => {
                            tracing::info!("LLM model loaded in CPU-only mode");
                            gpu_fallback = true;
                            model
                        }
                        Err(cpu_err) => {
                            anyhow::bail!(
                                "Failed to load LLM model '{}': GPU: {}; CPU: {}",
                                model_id, gpu_err, cpu_err
                            );
                        }
                    }
                }
            }
        } else {
            tracing::info!("loading LLM model in CPU-only mode (no CUDA backend)");
            try_load_model(
                backend,
                model_path,
                &LlamaModelParams::default().with_n_gpu_layers(0),
                model_id,
            ).map_err(|e| anyhow::anyhow!("Failed to load LLM model '{}': {}", model_id, e))?
        };

        let display_name = model
            .meta_val_str("general.name")
            .ok()
            .unwrap_or_else(|| model_id.to_string());

        tracing::info!(
            elapsed_ms = start.elapsed().as_millis(),
            model_id,
            display_name,
            "LLM model loaded successfully"
        );

        Ok(Self {
            model,
            n_threads,
            model_id: model_id.to_string(),
            display_name,
            gpu_fallback,
        })
    }

    /// The logical model identifier passed at load time (e.g. "gemma-3-1b").
    pub fn model_id(&self) -> &str {
        &self.model_id
    }

    /// Whether the model fell back to CPU-only mode after GPU loading failed.
    pub fn gpu_fallback(&self) -> bool {
        self.gpu_fallback
    }

    /// Run chat completion with a system prompt and user message.
    pub fn chat_complete(&self, system_prompt: &str, user_message: &str) -> Result<String> {
        tracing::debug!(
            system_prompt_len = system_prompt.len(),
            user_message_len = user_message.len(),
            model_id = %self.model_id,
            "starting LLM chat completion"
        );
        let start = std::time::Instant::now();

        // Build chat messages
        let messages = vec![
            LlamaChatMessage::new("system".to_string(), system_prompt.to_string())
                .map_err(|e| anyhow::anyhow!("Failed to create system message: {:?}", e))?,
            LlamaChatMessage::new("user".to_string(), user_message.to_string())
                .map_err(|e| anyhow::anyhow!("Failed to create user message: {:?}", e))?,
        ];

        // Apply chat template from model metadata
        let template = self.model.chat_template(None)
            .map_err(|e| anyhow::anyhow!("Failed to get chat template: {:?}", e))?;
        let prompt = self.model.apply_chat_template(&template, &messages, true)
            .map_err(|e| anyhow::anyhow!("Failed to apply chat template: {:?}", e))?;

        tracing::trace!(prompt_len = prompt.len(), "chat template applied");

        // Tokenize the prompt
        let tokens = self.model
            .str_to_token(&prompt, llama_cpp_2::model::AddBos::Always)
            .map_err(|e| anyhow::anyhow!("Failed to tokenize prompt: {:?}", e))?;

        let n_prompt_tokens = tokens.len();
        tracing::debug!(n_prompt_tokens, "prompt tokenized");

        // Create context with enough room for prompt + generation
        let n_ctx = (n_prompt_tokens + MAX_GENERATION_TOKENS) as u32;
        let mut ctx_params = LlamaContextParams::default();
        ctx_params = ctx_params.with_n_ctx(std::num::NonZeroU32::new(n_ctx));
        let n_threads = self.n_threads as i32;
        ctx_params = ctx_params.with_n_threads(n_threads);
        ctx_params = ctx_params.with_n_threads_batch(n_threads);

        let backend = get_backend()?;
        let mut ctx = self.model.new_context(backend, ctx_params)
            .map_err(|e| anyhow::anyhow!("Failed to create context: {:?}", e))?;

        // Create sampler chain: temp(0.1) -> greedy for deterministic output
        let sampler = LlamaSampler::chain_simple([
            LlamaSampler::temp(0.1),
            LlamaSampler::greedy(),
        ]);
        let mut sampler = sampler;

        // Process prompt in a single batch
        // add_sequence with logits_all=false enables logits for the last token automatically
        let mut batch = LlamaBatch::new(n_prompt_tokens, 1);
        batch.add_sequence(&tokens, 0, false)
            .map_err(|e| anyhow::anyhow!("Failed to add prompt to batch: {:?}", e))?;

        ctx.decode(&mut batch)
            .map_err(|e| anyhow::anyhow!("Failed to decode prompt: {:?}", e))?;

        // Generate tokens
        let mut output = String::new();
        let mut n_generated = 0;

        loop {
            if n_generated >= MAX_GENERATION_TOKENS {
                break;
            }

            let token = sampler.sample(&ctx, -1);
            sampler.accept(token);

            // Check for end of generation
            if self.model.is_eog_token(token) {
                break;
            }

            // Decode token to bytes and convert to string
            let bytes = self.model.token_to_piece_bytes(token, 32, true, None)
                .map_err(|e| anyhow::anyhow!("Failed to decode token: {:?}", e))?;
            let piece = String::from_utf8_lossy(&bytes);
            output.push_str(piece.as_ref());

            // Prepare next batch with just the new token
            let mut next_batch = LlamaBatch::new(1, 1);
            let pos = (n_prompt_tokens + n_generated) as i32;
            next_batch.add(token, pos, &[0], true)
                .map_err(|e| anyhow::anyhow!("Failed to add token to batch: {:?}", e))?;

            ctx.decode(&mut next_batch)
                .map_err(|e| anyhow::anyhow!("Failed to decode token: {:?}", e))?;

            n_generated += 1;
        }

        let output = output.trim().to_string();
        tracing::debug!(
            n_generated,
            output_len = output.len(),
            elapsed_ms = start.elapsed().as_millis(),
            "LLM chat completion finished"
        );

        Ok(output)
    }

    /// Get the model name for display purposes.
    pub fn model_name(&self) -> &str {
        &self.display_name
    }
}
