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
