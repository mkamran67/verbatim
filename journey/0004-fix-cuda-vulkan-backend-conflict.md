# Fix CUDA build and runtime crashes

## Symptom

Three related crashes when building/running with `--features cuda`:

1. **Build failure:** `could not find native static library cudart_static` —
   the linker couldn't find CUDA libraries on Debian.

2. **Backend conflict crash:** Both CUDA and Vulkan backends initialized
   simultaneously, causing `ggml-backend.cpp:1535: pre-allocated tensor in a
   backend that cannot run the operation`.

3. **Silent segfault after transcription:** The app transcribed successfully
   but crashed immediately after with no error output. Happened even with
   LLM post-processing disabled.

## Root cause

Three issues:

1. **Missing CUDA library path (Debian):** `llama-cpp-sys-2` uses
   `find_cuda_helper` to locate CUDA libraries, but it only checks
   `/usr/local/cuda/lib64` and `/opt/cuda/lib64`. Debian/Ubuntu's
   `nvidia-cuda-toolkit` installs to `/usr/lib/x86_64-linux-gnu/` instead.

2. **Dual GPU backends:** The `[target.'cfg(target_os = "linux")']` section in
   `src-tauri/Cargo.toml` unconditionally enabled Vulkan. Adding `--features
   cuda` stacked CUDA on top. Both backends registered with ggml and whisper
   crashed trying to arbitrate tensor allocation between them.

3. **WhisperState drop segfault:** `WhisperLocal::transcribe()` created a new
   `WhisperState` per call and dropped it on return. The Drop impl calls
   `whisper_free_state()` (unsafe FFI), which triggers CUDA memory cleanup.
   With llama-cpp also holding CUDA resources on the same GPU, the teardown
   caused a segmentation fault — silent because it's below Rust's panic handler.

## Fix

- **Added `.cargo/config.toml`** with
  `rustflags = ["-L", "/usr/lib/x86_64-linux-gnu"]` for x86_64 Linux targets,
  so the linker can find CUDA static libraries on Debian.

- **Made GPU backends mutually exclusive features** in `src-tauri/Cargo.toml`.
  Removed the unconditional `[target.linux]` Vulkan dependency. CPU is now the
  default; users opt into exactly one GPU backend via `--features cuda`,
  `--features vulkan`, or `--features rocm`.

- **Reused WhisperState across transcriptions** in `whisper_local.rs`. The state
  is created once during `WhisperLocal::new()` and held in a `Mutex<WhisperState>`
  for the app's lifetime, avoiding the per-call `whisper_free_state()` that
  crashed during CUDA teardown.
