// Prevent incompatible GPU backends from being enabled simultaneously.
#[cfg(all(feature = "cuda", feature = "rocm"))]
compile_error!("Cannot enable both 'cuda' and 'rocm' features simultaneously");

pub mod app;
pub mod audio;
pub mod clipboard;
pub mod config;
pub mod db;
pub mod errors;
pub mod hotkey;
pub mod input;
pub mod keyring_store;
pub mod gpu_detect;
pub mod model_manager;
pub mod ollama_manager;
pub mod platform;
pub mod post_processing;
pub mod provider_error;
pub mod rotation;
pub mod stt;

#[cfg(test)]
mod test_helpers;
