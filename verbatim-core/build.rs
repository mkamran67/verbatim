fn main() {
    // ggml symbol isolation is handled by src-tauri/build.rs which runs after
    // all dependencies (including whisper-rs-sys) are fully built.
    // --allow-multiple-definition handles duplication between llama-cpp-sys-2's
    // rlib and --whole-archive'd .a files. Whisper ggml symbols are fully
    // renamed via objcopy, so this flag only affects llama's self-duplication.
    if cfg!(target_os = "linux") {
        println!("cargo:rustc-link-arg=-Wl,--allow-multiple-definition");
    }
}
