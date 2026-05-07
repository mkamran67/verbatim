use thiserror::Error;

#[derive(Error, Debug)]
pub enum SttError {
    #[error("Model not found: {0}")]
    ModelNotFound(String),

    #[error("Inference failed: {0}")]
    InferenceFailed(String),

    #[error("API error: {0}")]
    ApiError(String),

    #[error("Network error: {0}")]
    NetworkError(#[from] reqwest::Error),

    #[error("Invalid audio data: {0}")]
    InvalidAudio(String),
}

#[derive(Error, Debug)]
pub enum AudioError {
    #[error("No input device available")]
    NoInputDevice,

    #[error("Device error: {0}")]
    DeviceError(String),

    #[error("Stream error: {0}")]
    #[allow(dead_code)]
    StreamError(String),
}

#[derive(Error, Debug)]
pub enum HotkeyError {
    #[error("Cannot open input devices: {0}. Is the user in the 'input' group?")]
    PermissionDenied(String),

    #[error("Device error: {0}")]
    DeviceError(String),
}

#[derive(Error, Debug)]
pub enum InputError {
    #[error("Keyboard simulation failed: {0}")]
    SimulationFailed(String),

    #[error("Clipboard error: {0}")]
    ClipboardError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stt_error_display() {
        let e = SttError::ModelNotFound("base.en".into());
        assert_eq!(e.to_string(), "Model not found: base.en");

        let e = SttError::InferenceFailed("oom".into());
        assert_eq!(e.to_string(), "Inference failed: oom");

        let e = SttError::ApiError("401 Unauthorized".into());
        assert_eq!(e.to_string(), "API error: 401 Unauthorized");

        let e = SttError::InvalidAudio("too short".into());
        assert_eq!(e.to_string(), "Invalid audio data: too short");
    }

    #[test]
    fn test_audio_error_display() {
        let e = AudioError::NoInputDevice;
        assert_eq!(e.to_string(), "No input device available");

        let e = AudioError::DeviceError("not found".into());
        assert_eq!(e.to_string(), "Device error: not found");

        let e = AudioError::StreamError("broken".into());
        assert_eq!(e.to_string(), "Stream error: broken");
    }

    #[test]
    fn test_hotkey_error_display() {
        let e = HotkeyError::PermissionDenied("access denied".into());
        assert!(e.to_string().contains("access denied"));

        let e = HotkeyError::DeviceError("no device".into());
        assert_eq!(e.to_string(), "Device error: no device");
    }

    #[test]
    fn test_input_error_display() {
        let e = InputError::SimulationFailed("enigo error".into());
        assert_eq!(e.to_string(), "Keyboard simulation failed: enigo error");

        let e = InputError::ClipboardError("locked".into());
        assert_eq!(e.to_string(), "Clipboard error: locked");
    }

    // ── Edge case tests ──────────────────────────────────

    #[test]
    fn test_stt_error_empty_message() {
        let e = SttError::ApiError("".into());
        assert_eq!(e.to_string(), "API error: ");
    }

    #[test]
    fn test_hotkey_error_permission_denied_includes_hint() {
        let e = HotkeyError::PermissionDenied("test reason".into());
        let msg = e.to_string();
        assert!(msg.contains("'input' group"), "should mention 'input' group hint: {}", msg);
    }

    #[test]
    fn test_error_debug_vs_display_differ() {
        let e = SttError::ApiError("test".into());
        let display = format!("{}", e);
        let debug = format!("{:?}", e);
        assert_ne!(display, debug, "Debug and Display should produce different output");
    }

    #[test]
    fn test_audio_error_stream_variant_exists() {
        // StreamError is marked #[allow(dead_code)] — verify it still compiles and formats
        let e = AudioError::StreamError("test stream error".into());
        assert_eq!(e.to_string(), "Stream error: test stream error");
    }
}
