#[cfg(target_os = "linux")]
pub mod evdev_listener;

#[cfg(target_os = "macos")]
pub mod macos_listener;

/// Events emitted by the hotkey listener.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyEvent {
    Pressed,
    Released,
}
