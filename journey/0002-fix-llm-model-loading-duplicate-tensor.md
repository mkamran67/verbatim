# Fix LLM model loading "tensor is duplicated" error

## Symptom

All LLM models (SmolLM2, Gemma 3) failed to load with errors like:

```
llama_model_load: error loading model: invalid model: tensor 'blk.0.attn_norm.weight' is duplicated
```

## Root cause

A bug in `src-tauri/build.rs` prevented ggml symbol isolation between whisper-rs-sys and llama-cpp-sys-2.

The `patch_whisper_ggml` function renames whisper's ggml C symbols (`ggml_*` → `whisper_ggml_*`) using objcopy so they don't collide with llama's. However, the "already patched" check (line 180) aggregated `nm` output from ALL whisper-rs-sys build directories — including stale ones from previous compilations. If any stale directory had already-patched symbols, the check triggered and newer unpatched directories were skipped.

With unpatched symbols and `--allow-multiple-definition`, both whisper and llama shared the same ggml functions. When llama tried to load a GGUF model, it encountered tensors that whisper had already loaded into the shared ggml state, causing the "duplicated tensor" error.

## Fix

Changed the symbol extraction loop to track per-library patched status instead of using a global skip check. Each library is independently checked: already-patched libs are skipped, unpatched libs always get patched. Added a `lib_is_patched` helper for the same check on whisper `.a` and rlib files.

Also removed C++ mangled symbol renaming (symbols containing "ggml" with `_Z` prefix). While well-intentioned, renaming these creates undefined weak references that the linker cannot resolve from archive members, causing `cxa_atexit(func=NULL)` crashes during Vulkan backend initialization. The C function renaming (`ggml_*`, `gguf_*`) is sufficient to isolate the tensor/model loading code paths.
