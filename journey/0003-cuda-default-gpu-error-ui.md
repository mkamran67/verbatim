# CUDA default for NVIDIA + GPU error UI

## What

Made CUDA the recommended GPU backend for NVIDIA systems and added UI error
display for post-processor/GPU failures.

## Why

The Vulkan backend is ~6x slower than CUDA on NVIDIA hardware and has known
stability issues (crashes during model loading, `vk::DeviceLostError`). Vulkan
is designed for cross-platform compatibility (AMD/Intel), not as the optimal
NVIDIA path.

## How

**Build system**: CUDA is now an additive feature (`--features cuda`) that can
be combined with the default Vulkan. When both are compiled in, ggml
auto-selects CUDA on NVIDIA hardware at runtime. Vulkan remains the
compile-time default so developer builds work without the CUDA toolkit.
Release builds should use `cargo tauri build --features cuda`.

**Vulkan/WebKit conflict**: The Vulkan LLM backend conflicts with WebKitGTK's
GPU compositor on Linux, causing a segfault when both try to use the GPU. Fix:
Vulkan-only builds now force CPU for LLM inference. Whisper STT keeps Vulkan
GPU since it initializes before the webview starts. CUDA builds are unaffected
since CUDA and WebKit don't share GPU state.

**LLM engine**: Added `gpu_fallback` tracking to `LlmEngine` so the app knows
when a model fell back from GPU to CPU-only mode.

**Error events**: Added `PostProcessorError(String)` and `GpuFallback(String)`
variants to `SttEvent`. These are emitted when the LLM post-processor fails to
load or falls back to CPU.

**UI**: The TopBar now displays a red error banner when a post-processor error
or GPU fallback occurs. The banner auto-dismisses after 10 seconds or can be
closed manually.

**Debug info**: `compiled_gpu_backend()` now reports `"cuda+vulkan"` when both
backends are compiled in, visible in Settings → Debug.
