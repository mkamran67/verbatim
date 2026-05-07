//! Runtime GPU presence detection.
//!
//! `detect_cuda_device()` returns true iff `libcuda.so.1` loads and at least one
//! CUDA device is visible. Safe to call regardless of whether the crate was
//! built with the `cuda` feature — dlopen at runtime means no link-time
//! dependency on libcuda. Call sites gate on `cfg!(feature = "cuda")` before
//! using the result.

#[cfg(target_os = "linux")]
use std::sync::OnceLock;

#[cfg(target_os = "linux")]
static CUDA_AVAILABLE: OnceLock<bool> = OnceLock::new();

#[cfg(target_os = "linux")]
pub fn detect_cuda_device() -> bool {
    *CUDA_AVAILABLE.get_or_init(detect_cuda_device_inner)
}

#[cfg(not(target_os = "linux"))]
pub fn detect_cuda_device() -> bool {
    false
}

#[cfg(target_os = "linux")]
fn detect_cuda_device_inner() -> bool {
    use std::ffi::CString;
    use std::os::raw::{c_int, c_void};

    unsafe {
        let lib = CString::new("libcuda.so.1").unwrap();
        let handle = libc::dlopen(lib.as_ptr(), libc::RTLD_NOW);
        if handle.is_null() {
            tracing::debug!("libcuda.so.1 not loadable — no NVIDIA driver");
            return false;
        }

        // cuInit(unsigned int Flags)
        let init_sym = CString::new("cuInit").unwrap();
        let cu_init: *mut c_void = libc::dlsym(handle, init_sym.as_ptr());
        if cu_init.is_null() {
            libc::dlclose(handle);
            return false;
        }
        let cu_init: extern "C" fn(c_int) -> c_int = std::mem::transmute(cu_init);
        if cu_init(0) != 0 {
            libc::dlclose(handle);
            tracing::debug!("cuInit failed — no usable CUDA device");
            return false;
        }

        let count_sym = CString::new("cuDeviceGetCount").unwrap();
        let cu_count: *mut c_void = libc::dlsym(handle, count_sym.as_ptr());
        if cu_count.is_null() {
            libc::dlclose(handle);
            return false;
        }
        let cu_count: extern "C" fn(*mut c_int) -> c_int = std::mem::transmute(cu_count);
        let mut n: c_int = 0;
        let rc = cu_count(&mut n);
        libc::dlclose(handle);
        let present = rc == 0 && n >= 1;
        tracing::info!(devices = n, present, "CUDA device probe");
        present
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_cuda_device_does_not_panic() {
        // Smoke test — just ensure the probe runs without panicking.
        // Returns true or false depending on the host; both are acceptable.
        let _ = detect_cuda_device();
        // Second call must be cached.
        let _ = detect_cuda_device();
    }
}
