use std::path::{Path, PathBuf};

fn main() {
    if cfg!(target_os = "linux") {
        println!("cargo:rustc-link-arg=-Wl,--allow-multiple-definition");
    }

    // When building with --features cuda, the llama-cpp-sys-2 crate emits
    // `rustc-link-lib=static=cudart_static` but doesn't always emit the
    // search path for the system CUDA libraries. Add it here so the linker
    // can find libcudart_static.a from nvidia-cuda-toolkit.
    #[cfg(feature = "cuda")]
    {
        let cuda_lib = std::env::var("CUDA_PATH")
            .map(|p| PathBuf::from(p).join("lib64"))
            .unwrap_or_else(|_| PathBuf::from("/usr/lib/x86_64-linux-gnu"));
        if cuda_lib.exists() {
            println!("cargo:rustc-link-search=native={}", cuda_lib.display());
        }
    }

    // Both whisper-rs-sys and llama-cpp-sys-2 bundle their own ggml with
    // incompatible versions. Use objcopy to prefix whisper's ggml symbols
    // (ggml_* → whisper_ggml_*) so both copies coexist without collision.
    // On Linux we use GNU objcopy/ar/nm; on macOS we use the LLVM equivalents
    // from the rustup toolchain sysroot.
    if cfg!(target_os = "linux") || cfg!(target_os = "macos") {
        let out_dir = std::env::var("OUT_DIR").unwrap_or_default();
        if let Some(build_dir) = Path::new(&out_dir).ancestors().nth(2) {
            patch_whisper_ggml(&build_dir.to_path_buf(), &out_dir);

            // Force-load ggml backends that the linker would otherwise
            // dead-strip because they are discovered at runtime via a
            // global registry, not called directly.
            if cfg!(target_os = "macos") {
                force_load_ggml_backends(build_dir);
            }
            if cfg!(target_os = "linux") {
                force_load_ggml_backends_linux(build_dir);
            }
        }
    }

    tauri_build::build()
}

/// Platform tool paths for symbol patching.
struct PatchTools {
    objcopy: PathBuf,
    ar: PathBuf,
    nm: PathBuf,
}

/// Find the binary toolchain needed for symbol patching.
/// Linux: GNU `objcopy`, `ar`, `nm` from PATH.
/// macOS: `llvm-objcopy`, `llvm-ar`, `llvm-nm` from the rustup toolchain sysroot.
fn find_tools() -> Option<PatchTools> {
    if cfg!(target_os = "linux") {
        return Some(PatchTools {
            objcopy: PathBuf::from("objcopy"),
            ar: PathBuf::from("ar"),
            nm: PathBuf::from("nm"),
        });
    }

    // macOS: find LLVM tools via rustc sysroot
    let bin_dir = find_llvm_bin_dir()?;

    let objcopy = bin_dir.join("llvm-objcopy");
    let ar = bin_dir.join("llvm-ar");
    let nm = bin_dir.join("llvm-nm");

    if objcopy.exists() && ar.exists() && nm.exists() {
        return Some(PatchTools { objcopy, ar, nm });
    }

    println!(
        "cargo:warning=LLVM tools not found in rustup sysroot; ggml symbol isolation \
         disabled on macOS. Install with: rustup component add llvm-tools"
    );
    None
}

/// Locate the LLVM bin directory inside the rustup toolchain sysroot.
fn find_llvm_bin_dir() -> Option<PathBuf> {
    let sysroot = std::process::Command::new("rustc")
        .arg("--print")
        .arg("sysroot")
        .output()
        .ok()?;
    let sysroot = String::from_utf8_lossy(&sysroot.stdout).trim().to_string();

    let version_info = std::process::Command::new("rustc")
        .arg("-vV")
        .output()
        .ok()?;
    let version_str = String::from_utf8_lossy(&version_info.stdout);
    let host = version_str
        .lines()
        .find(|l| l.starts_with("host:"))?
        .strip_prefix("host:")?
        .trim()
        .to_string();

    let dir = PathBuf::from(&sysroot)
        .join("lib/rustlib")
        .join(&host)
        .join("bin");

    if dir.is_dir() { Some(dir) } else { None }
}

/// Collect all whisper-rs-sys static libraries that contain ggml symbols.
/// When GPU backends (CUDA, ROCm, Vulkan) are enabled, additional libggml-*.a
/// files are produced alongside the base libggml.a. All of them need patching.
fn find_whisper_static_libs(build_dir: &Path) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let mut ggml_libs = Vec::new();
    let mut whisper_libs = Vec::new();

    if let Ok(entries) = std::fs::read_dir(build_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            if name.to_string_lossy().starts_with("whisper-rs-sys-") {
                // Scan all directories where whisper-rs-sys places static libs
                for search_dir in [
                    entry.path().join("out/build/ggml/src"),
                    entry.path().join("out/lib"),
                ] {
                    if let Ok(lib_entries) = std::fs::read_dir(&search_dir) {
                        for lib_entry in lib_entries.flatten() {
                            let lib_name = lib_entry.file_name().to_string_lossy().to_string();
                            if lib_name.starts_with("libggml") && lib_name.ends_with(".a") {
                                ggml_libs.push(lib_entry.path());
                            }
                        }
                    }
                }
                // Find libwhisper.a
                for path in [
                    entry.path().join("out/build/src/libwhisper.a"),
                    entry.path().join("out/lib/libwhisper.a"),
                ] {
                    if path.exists() {
                        whisper_libs.push(path);
                    }
                }
            }
        }
    }

    (ggml_libs, whisper_libs)
}

fn patch_whisper_ggml(build_dir: &Path, out_dir: &str) {
    let tools = match find_tools() {
        Some(t) => t,
        None => return,
    };

    let (ggml_libs, whisper_libs) = find_whisper_static_libs(build_dir);

    if ggml_libs.is_empty() {
        return;
    }

    // On macOS (Mach-O), C symbols have a leading underscore: _ggml_foo
    // On Linux (ELF), symbols have no prefix: ggml_foo
    let is_macos = cfg!(target_os = "macos");

    // Use "whisper_ggml_" as the sentinel to detect already-patched libs.
    // We can't use "whisper_" because whisper's own API has functions like
    // whisper_full, whisper_init etc. that would create false positives.
    let patched_prefix = if is_macos { "_whisper_ggml_" } else { "whisper_ggml_" };

    // Extract symbols from ggml libs, tracking which are already patched.
    // Stale build directories from previous compilations may coexist with
    // fresh ones — only aggregate symbols from unpatched libs so the rename
    // map is correct, and skip already-patched libs during objcopy later.
    let mut all_stdout = String::new();
    let mut patched_libs = std::collections::HashSet::new();
    for lib in &ggml_libs {
        if let Ok(output) = std::process::Command::new(&tools.nm)
            .arg("--defined-only")
            .arg("--no-sort")
            .arg("-g")
            .arg(lib)
            .output()
        {
            let lib_stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let already_patched = lib_stdout.lines().any(|line| {
                let parts: Vec<&str> = line.split_whitespace().collect();
                parts.len() >= 3 && parts[2].starts_with(patched_prefix)
            });
            if already_patched {
                patched_libs.insert(lib.clone());
            } else {
                all_stdout.push_str(&lib_stdout);
            }
        }
    }

    let stdout = all_stdout;

    let (ggml_prefix, gguf_prefix) = if is_macos {
        ("_ggml_", "_gguf_")
    } else {
        ("ggml_", "gguf_")
    };

    let mut seen = std::collections::HashSet::new();
    let pairs: Vec<String> = stdout
        .lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 {
                let sym = parts[2];
                // Rename C symbols: ggml_*, gguf_*
                if sym.starts_with(ggml_prefix) || sym.starts_with(gguf_prefix) {
                    let pair = if is_macos {
                        format!("{} _whisper_{}", sym, &sym[1..])
                    } else {
                        format!("{} whisper_{}", sym, sym)
                    };
                    if seen.insert(sym.to_string()) { return Some(pair); }
                    return None;
                }
                // Note: C++ mangled symbols (e.g. ggml_backend_registry destructor)
                // are intentionally NOT renamed. They are shared between whisper
                // and llama via --allow-multiple-definition. Renaming them creates
                // undefined weak references that the linker cannot resolve from
                // archive members. The C function renaming above is sufficient to
                // isolate the tensor/model loading code paths.
            }
            None
        })
        .collect();

    if pairs.is_empty() {
        return;
    }

    let syms_file = PathBuf::from(out_dir).join("whisper_ggml_syms.txt");
    std::fs::write(&syms_file, format!("{}\n", pairs.join("\n"))).unwrap();

    // Patch unpatched ggml and whisper .a files (skip stale already-patched libs)
    for lib in &ggml_libs {
        if !patched_libs.contains(lib) {
            objcopy_redefine(&tools.objcopy, &syms_file, lib);
        }
    }
    for lib in &whisper_libs {
        if !lib_is_patched(&tools.nm, lib, patched_prefix) {
            objcopy_redefine(&tools.objcopy, &syms_file, lib);
        }
    }

    // Patch whisper rlibs (skip already-patched ones)
    let deps_dir = build_dir.parent().unwrap().join("deps");
    if let Ok(entries) = std::fs::read_dir(&deps_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.starts_with("libwhisper_rs_sys-") && name_str.ends_with(".rlib") {
                if !lib_is_patched(&tools.nm, &entry.path(), patched_prefix) {
                    patch_rlib(&tools, &syms_file, &entry.path());
                }
            }
        }
    }
}

/// Force the linker to include ggml backends that would otherwise be
/// dead-stripped on macOS.
///
/// The macOS linker only pulls objects from static archives when they resolve
/// an undefined reference. Backend registration functions like
/// `ggml_backend_cpu_reg` and `ggml_backend_metal_reg` are discovered at
/// runtime via a global registry — nothing calls them directly — so the
/// linker drops them. We use `-u` to mark them as required and `-force_load`
/// to ensure all supporting objects come along.
fn force_load_ggml_backends(build_dir: &Path) {
    if let Ok(entries) = std::fs::read_dir(build_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            if name.to_string_lossy().starts_with("llama-cpp-sys-2-") {
                let lib_dir = entry.path().join("out/lib");
                if !lib_dir.exists() {
                    continue;
                }

                // CPU backend — required for tensor allocation on host memory.
                let cpu_lib = lib_dir.join("libggml-cpu.a");
                if cpu_lib.exists() {
                    println!("cargo:rustc-link-arg=-Wl,-u,_ggml_backend_cpu_reg");
                    println!(
                        "cargo:rustc-link-arg=-Wl,-force_load,{}",
                        cpu_lib.display()
                    );
                }

                // Metal backend — GPU acceleration on Apple Silicon.
                let metal_lib = lib_dir.join("libggml-metal.a");
                if metal_lib.exists() {
                    println!("cargo:rustc-link-arg=-Wl,-u,_ggml_backend_metal_reg");
                    println!(
                        "cargo:rustc-link-arg=-Wl,-force_load,{}",
                        metal_lib.display()
                    );
                }

                return;
            }
        }
    }
}

/// Force the linker to include ggml backends on Linux that would otherwise be
/// dropped because they are discovered at runtime via a global registry.
///
/// Uses `--whole-archive` / `--no-whole-archive` wrapping to ensure GPU backend
/// registration functions (e.g., `ggml_backend_cuda_reg`) are linked in.
fn force_load_ggml_backends_linux(build_dir: &Path) {
    if let Ok(entries) = std::fs::read_dir(build_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            if name.to_string_lossy().starts_with("llama-cpp-sys-2-") {
                let lib_dir = entry.path().join("out/lib");
                if !lib_dir.exists() {
                    continue;
                }

                // Force-load CPU and any GPU backends that were compiled.
                // Uses --whole-archive to ensure all objects (including weak
                // symbol definitions like vk_instance_t destructors) are linked.
                // --allow-multiple-definition handles duplication with the rlib.
                // Only force-load GPU backends when the corresponding
                // feature is enabled; otherwise the CUDA/Vulkan/HIP runtime
                // libraries won't be on the link line and we get undefined
                // symbols.
                let mut backends = vec!["cpu"];
                if cfg!(feature = "cuda") {
                    backends.push("cuda");
                }
                if cfg!(feature = "vulkan") {
                    backends.push("vulkan");
                }
                if cfg!(feature = "rocm") {
                    backends.push("hip");
                }

                for backend in backends {
                    let backend_lib = lib_dir.join(format!("libggml-{}.a", backend));
                    if backend_lib.exists() {
                        println!(
                            "cargo:warning=force-loading ggml backend: {}",
                            backend_lib.display()
                        );
                        println!("cargo:rustc-link-arg=-Wl,--whole-archive");
                        println!(
                            "cargo:rustc-link-arg={}",
                            backend_lib.display()
                        );
                        println!("cargo:rustc-link-arg=-Wl,--no-whole-archive");
                    }
                }

                return;
            }
        }
    }
}

/// Check if a static library has already been patched (contains whisper_ggml_* symbols).
fn lib_is_patched(nm: &Path, lib: &Path, patched_prefix: &str) -> bool {
    if let Ok(output) = std::process::Command::new(nm)
        .arg("--defined-only")
        .arg("--no-sort")
        .arg("-g")
        .arg(lib)
        .output()
    {
        let stdout = String::from_utf8_lossy(&output.stdout);
        return stdout.lines().any(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            parts.len() >= 3 && parts[2].starts_with(patched_prefix)
        });
    }
    false
}

fn objcopy_redefine(objcopy: &Path, syms_file: &Path, target: &Path) {
    let _ = std::process::Command::new(objcopy)
        .arg(format!("--redefine-syms={}", syms_file.display()))
        .arg(target)
        .status();
}

fn patch_rlib(tools: &PatchTools, syms_file: &Path, rlib_path: &Path) {
    let out_dir = std::env::var("OUT_DIR").unwrap_or_default();
    let tmp = PathBuf::from(&out_dir).join("rlib_patch");
    let _ = std::fs::remove_dir_all(&tmp);
    if std::fs::create_dir_all(&tmp).is_err() {
        return;
    }

    let ok = std::process::Command::new(&tools.ar)
        .arg("x")
        .arg(rlib_path)
        .current_dir(&tmp)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !ok {
        return;
    }

    if let Ok(entries) = std::fs::read_dir(&tmp) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let n = name.to_string_lossy();
            if (n.starts_with("ggml") || n == "whisper.cpp.o") && n.ends_with(".o") {
                let _ = std::process::Command::new(&tools.objcopy)
                    .arg(format!("--redefine-syms={}", syms_file.display()))
                    .arg(entry.path())
                    .status();
            }
        }
    }

    let files: Vec<PathBuf> = std::fs::read_dir(&tmp)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    let _ = std::process::Command::new(&tools.ar)
        .arg("rcs")
        .arg(rlib_path)
        .args(&files)
        .status();
}
