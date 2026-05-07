use std::path::Path;
#[allow(unused_imports)]
use std::path::PathBuf;

fn main() {
    // With llama-cpp-2 removed, only whisper-rs links ggml — no symbol
    // isolation is needed. We still have to force-load ggml backends so
    // the runtime registry discovers them; the linker otherwise drops them
    // because they're referenced indirectly.
    #[cfg(feature = "cuda")]
    {
        let cuda_lib = std::env::var("CUDA_PATH")
            .map(|p| PathBuf::from(p).join("lib64"))
            .unwrap_or_else(|_| PathBuf::from("/usr/lib/x86_64-linux-gnu"));
        if cuda_lib.exists() {
            println!("cargo:rustc-link-search=native={}", cuda_lib.display());
        }
    }

    if cfg!(target_os = "linux") || cfg!(target_os = "macos") {
        let out_dir = std::env::var("OUT_DIR").unwrap_or_default();
        if let Some(build_dir) = Path::new(&out_dir).ancestors().nth(2) {
            force_load_whisper_backends(build_dir);
        }
    }

    tauri_build::build()
}

fn force_load_whisper_backends(build_dir: &Path) {
    let Ok(entries) = std::fs::read_dir(build_dir) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name();
        if !name.to_string_lossy().starts_with("whisper-rs-sys-") { continue; }

        // Scan for libggml-*.a — we need the backend registration objects.
        for search_dir in [
            entry.path().join("out/build/ggml/src"),
            entry.path().join("out/lib"),
        ] {
            let Ok(lib_entries) = std::fs::read_dir(&search_dir) else { continue };
            for lib_entry in lib_entries.flatten() {
                let lib_name = lib_entry.file_name().to_string_lossy().to_string();
                let is_backend = lib_name.starts_with("libggml-")
                    && lib_name.ends_with(".a")
                    && lib_name != "libggml-base.a";
                if !is_backend { continue; }

                if cfg!(target_os = "macos") {
                    println!("cargo:rustc-link-arg=-Wl,-force_load,{}", lib_entry.path().display());
                } else {
                    println!("cargo:rustc-link-arg=-Wl,--whole-archive");
                    println!("cargo:rustc-link-arg={}", lib_entry.path().display());
                    println!("cargo:rustc-link-arg=-Wl,--no-whole-archive");
                }
            }
        }
    }
}
