use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;

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

/// A hotkey captured from a live OS keypress.
/// `key` and `modifiers` are raw OS keycodes (evdev or CGKeyCode).
#[derive(Debug, Clone)]
pub struct CapturedHotkey {
    pub key: u32,
    pub modifiers: Vec<u32>,
    pub label: String,
}

/// A one-shot capture target: when armed, the next hotkey-eligible keypress
/// from the listener fills this slot instead of being matched against
/// configured hotkeys.
#[derive(Clone, Default)]
pub struct CaptureSlot(Arc<Mutex<Option<oneshot::Sender<CapturedHotkey>>>>);

impl CaptureSlot {
    pub fn new() -> Self { Self::default() }

    /// Arm the slot. Returns the receiver to await the captured hotkey.
    /// If the slot was already armed, the previous sender is dropped (its
    /// receiver will see a cancelled error).
    pub fn arm(&self) -> oneshot::Receiver<CapturedHotkey> {
        let (tx, rx) = oneshot::channel();
        *self.0.lock().unwrap() = Some(tx);
        rx
    }

    /// Take the armed sender, leaving the slot disarmed. Called by the
    /// listener when it wants to deliver a captured hotkey.
    pub fn take(&self) -> Option<oneshot::Sender<CapturedHotkey>> {
        self.0.lock().unwrap().take()
    }

    /// Whether the slot is armed.
    pub fn is_armed(&self) -> bool {
        self.0.lock().unwrap().is_some()
    }
}
