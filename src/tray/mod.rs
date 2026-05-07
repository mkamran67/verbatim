/// Commands sent from the tray menu to the application.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum TrayCommand {
    ShowWindow,
    SetBackend(String),
    ToggleClipboardOnly,
    Quit,
}

/// State of the tray icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum TrayState {
    Idle,
    Recording,
    Processing,
}
