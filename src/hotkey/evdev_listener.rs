use anyhow::{Context, Result};
use evdev::{Device, InputEventKind, Key};
use std::fs;
use tokio::sync::mpsc;

use super::HotkeyEvent;
use crate::errors::HotkeyError;

/// Parse a key name like "KEY_RIGHTCTRL" to an evdev Key.
pub fn parse_key(name: &str) -> Result<Key, HotkeyError> {
    // Common key mappings
    let key = match name {
        "KEY_RIGHTCTRL" => Key::KEY_RIGHTCTRL,
        "KEY_LEFTCTRL" => Key::KEY_LEFTCTRL,
        "KEY_RIGHTALT" => Key::KEY_RIGHTALT,
        "KEY_LEFTALT" => Key::KEY_LEFTALT,
        "KEY_RIGHTSHIFT" => Key::KEY_RIGHTSHIFT,
        "KEY_LEFTSHIFT" => Key::KEY_LEFTSHIFT,
        "KEY_F1" => Key::KEY_F1,
        "KEY_F2" => Key::KEY_F2,
        "KEY_F3" => Key::KEY_F3,
        "KEY_F4" => Key::KEY_F4,
        "KEY_F5" => Key::KEY_F5,
        "KEY_F6" => Key::KEY_F6,
        "KEY_F7" => Key::KEY_F7,
        "KEY_F8" => Key::KEY_F8,
        "KEY_F9" => Key::KEY_F9,
        "KEY_F10" => Key::KEY_F10,
        "KEY_F11" => Key::KEY_F11,
        "KEY_F12" => Key::KEY_F12,
        "KEY_CAPSLOCK" => Key::KEY_CAPSLOCK,
        "KEY_SCROLLLOCK" => Key::KEY_SCROLLLOCK,
        "KEY_PAUSE" => Key::KEY_PAUSE,
        "KEY_INSERT" => Key::KEY_INSERT,
        _ => {
            return Err(HotkeyError::DeviceError(format!(
                "Unknown key: {}",
                name
            )))
        }
    };
    Ok(key)
}

/// Find all input devices that support keyboard events.
fn find_keyboard_devices() -> Result<Vec<Device>, HotkeyError> {
    let mut devices = Vec::new();

    let entries = fs::read_dir("/dev/input")
        .map_err(|e| HotkeyError::PermissionDenied(e.to_string()))?;

    for entry in entries.flatten() {
        let path = entry.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        if !name.starts_with("event") {
            continue;
        }

        match Device::open(&path) {
            Ok(device) => {
                // Check if this device has keyboard capabilities
                if let Some(keys) = device.supported_keys() {
                    if keys.contains(Key::KEY_A) || keys.contains(Key::KEY_RIGHTCTRL) {
                        tracing::debug!(
                            "Found keyboard device: {} ({})",
                            device.name().unwrap_or("unknown"),
                            path.display()
                        );
                        devices.push(device);
                    }
                }
            }
            Err(e) => {
                tracing::trace!("Cannot open {}: {}", path.display(), e);
            }
        }
    }

    if devices.is_empty() {
        return Err(HotkeyError::PermissionDenied(
            "No keyboard devices found. Is the user in the 'input' group?".into(),
        ));
    }

    Ok(devices)
}

/// Start the hotkey listener on a dedicated OS thread.
/// Sends `HotkeyEvent::Pressed` and `HotkeyEvent::Released` through the channel.
pub fn start_listener(
    hotkeys: Vec<Key>,
    event_tx: mpsc::UnboundedSender<HotkeyEvent>,
) -> Result<std::thread::JoinHandle<()>> {
    let devices = find_keyboard_devices()
        .context("Failed to find keyboard devices")?;

    let handle = std::thread::Builder::new()
        .name("hotkey-listener".into())
        .spawn(move || {
            if let Err(e) = listener_loop(devices, hotkeys, &event_tx) {
                tracing::error!("Hotkey listener error: {}", e);
            }
        })
        .context("Failed to spawn hotkey listener thread")?;

    Ok(handle)
}

fn listener_loop(
    mut devices: Vec<Device>,
    hotkeys: Vec<Key>,
    event_tx: &mpsc::UnboundedSender<HotkeyEvent>,
) -> Result<()> {
    use nix::poll::{poll, PollFd, PollFlags, PollTimeout};
    use std::os::fd::{AsRawFd, BorrowedFd};
    use std::collections::HashSet;

    let raw_fds: Vec<i32> = devices.iter().map(|d| d.as_raw_fd()).collect();
    // SAFETY: these fds are valid for the lifetime of `devices` which lives for the loop
    let mut pollfds: Vec<PollFd> = raw_fds
        .iter()
        .map(|&fd| PollFd::new(unsafe { BorrowedFd::borrow_raw(fd) }, PollFlags::POLLIN))
        .collect();

    let hotkey_set: HashSet<Key> = hotkeys.into_iter().collect();

    tracing::info!(
        "Hotkey listener started, watching {} devices for {:?}",
        devices.len(),
        hotkey_set
    );

    loop {
        // Block until an event is ready
        if poll(&mut pollfds, PollTimeout::NONE).is_err() {
            continue;
        }

        for (i, pollfd) in pollfds.iter().enumerate() {
            if let Some(revents) = pollfd.revents() {
                if !revents.contains(PollFlags::POLLIN) {
                    continue;
                }
            }

            if let Ok(events) = devices[i].fetch_events() {
                for event in events {
                    if let InputEventKind::Key(key) = event.kind() {
                        if hotkey_set.contains(&key) {
                            let hotkey_event = match event.value() {
                                1 => Some(HotkeyEvent::Pressed),  // Key down
                                0 => Some(HotkeyEvent::Released), // Key up
                                _ => None, // Repeat (value=2), ignore
                            };

                            if let Some(evt) = hotkey_event {
                                tracing::debug!("Hotkey event: {:?}", evt);
                                if event_tx.send(evt).is_err() {
                                    tracing::info!("Hotkey channel closed, exiting listener");
                                    return Ok(());
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
