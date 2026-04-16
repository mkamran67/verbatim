use anyhow::{Context, Result};
use evdev::{Device, InputEventKind, Key};
use std::collections::HashSet;
use std::fs;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

use super::HotkeyEvent;
use crate::errors::HotkeyError;

/// Shared hotkey configuration that can be updated while the listener is running.
#[derive(Clone)]
pub struct SharedHotkeyConfig(Arc<Mutex<HotkeyConfigInner>>);

struct HotkeyConfigInner {
    combos: Vec<HotkeyCombo>,
    generation: u64,
}

impl SharedHotkeyConfig {
    pub fn new(combos: Vec<HotkeyCombo>) -> Self {
        Self(Arc::new(Mutex::new(HotkeyConfigInner {
            combos,
            generation: 0,
        })))
    }

    /// Replace the active hotkey combos. The listener will pick up
    /// the change on the next poll cycle.
    pub fn update(&self, combos: Vec<HotkeyCombo>) {
        let mut inner = self.0.lock().unwrap();
        inner.combos = combos;
        inner.generation += 1;
        tracing::info!("Hotkey config updated (generation {})", inner.generation);
    }
}

/// A hotkey: a key with 0, 1, or 2 modifiers.
#[derive(Debug, Clone)]
pub struct HotkeyCombo {
    pub modifiers: Vec<Key>,
    pub key: Key,
}


/// Parse a key name like "KEY_RIGHTCTRL" to an evdev Key.
pub fn parse_key(name: &str) -> Result<Key, HotkeyError> {
    tracing::trace!(name, "parsing key name");
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
        // Letters
        "KEY_A" => Key::KEY_A,
        "KEY_B" => Key::KEY_B,
        "KEY_C" => Key::KEY_C,
        "KEY_D" => Key::KEY_D,
        "KEY_E" => Key::KEY_E,
        "KEY_F" => Key::KEY_F,
        "KEY_G" => Key::KEY_G,
        "KEY_H" => Key::KEY_H,
        "KEY_I" => Key::KEY_I,
        "KEY_J" => Key::KEY_J,
        "KEY_K" => Key::KEY_K,
        "KEY_L" => Key::KEY_L,
        "KEY_M" => Key::KEY_M,
        "KEY_N" => Key::KEY_N,
        "KEY_O" => Key::KEY_O,
        "KEY_P" => Key::KEY_P,
        "KEY_Q" => Key::KEY_Q,
        "KEY_R" => Key::KEY_R,
        "KEY_S" => Key::KEY_S,
        "KEY_T" => Key::KEY_T,
        "KEY_U" => Key::KEY_U,
        "KEY_V" => Key::KEY_V,
        "KEY_W" => Key::KEY_W,
        "KEY_X" => Key::KEY_X,
        "KEY_Y" => Key::KEY_Y,
        "KEY_Z" => Key::KEY_Z,
        // Numbers
        "KEY_0" => Key::KEY_0,
        "KEY_1" => Key::KEY_1,
        "KEY_2" => Key::KEY_2,
        "KEY_3" => Key::KEY_3,
        "KEY_4" => Key::KEY_4,
        "KEY_5" => Key::KEY_5,
        "KEY_6" => Key::KEY_6,
        "KEY_7" => Key::KEY_7,
        "KEY_8" => Key::KEY_8,
        "KEY_9" => Key::KEY_9,
        // Common keys
        "KEY_SPACE" => Key::KEY_SPACE,
        "KEY_TAB" => Key::KEY_TAB,
        "KEY_ENTER" => Key::KEY_ENTER,
        "KEY_BACKSPACE" => Key::KEY_BACKSPACE,
        "KEY_DELETE" => Key::KEY_DELETE,
        "KEY_HOME" => Key::KEY_HOME,
        "KEY_END" => Key::KEY_END,
        "KEY_PAGEUP" => Key::KEY_PAGEUP,
        "KEY_PAGEDOWN" => Key::KEY_PAGEDOWN,
        "KEY_UP" => Key::KEY_UP,
        "KEY_DOWN" => Key::KEY_DOWN,
        "KEY_LEFT" => Key::KEY_LEFT,
        "KEY_RIGHT" => Key::KEY_RIGHT,
        "KEY_MINUS" => Key::KEY_MINUS,
        "KEY_EQUAL" => Key::KEY_EQUAL,
        "KEY_COMMA" => Key::KEY_COMMA,
        "KEY_DOT" => Key::KEY_DOT,
        "KEY_SLASH" => Key::KEY_SLASH,
        "KEY_SEMICOLON" => Key::KEY_SEMICOLON,
        "KEY_APOSTROPHE" => Key::KEY_APOSTROPHE,
        "KEY_GRAVE" => Key::KEY_GRAVE,
        "KEY_BACKSLASH" => Key::KEY_BACKSLASH,
        "KEY_LEFTBRACE" => Key::KEY_LEFTBRACE,
        "KEY_RIGHTBRACE" => Key::KEY_RIGHTBRACE,
        _ => {
            return Err(HotkeyError::DeviceError(format!(
                "Unknown key: {}",
                name
            )))
        }
    };
    Ok(key)
}

/// Parse a hotkey string: single key, modifier+key, or modifier+modifier+key.
pub fn parse_hotkey(name: &str) -> Result<HotkeyCombo, HotkeyError> {
    tracing::debug!(name, "parsing hotkey string");
    let parts: Vec<&str> = name.split('+').collect();
    match parts.len() {
        1 => Ok(HotkeyCombo {
            modifiers: vec![],
            key: parse_key(parts[0])?,
        }),
        2 => Ok(HotkeyCombo {
            modifiers: vec![parse_key(parts[0])?],
            key: parse_key(parts[1])?,
        }),
        3 => Ok(HotkeyCombo {
            modifiers: vec![parse_key(parts[0])?, parse_key(parts[1])?],
            key: parse_key(parts[2])?,
        }),
        _ => Err(HotkeyError::DeviceError(format!(
            "Invalid hotkey format: {}",
            name
        ))),
    }
}

/// Find all input devices that support keyboard events.
fn find_keyboard_devices() -> Result<Vec<Device>, HotkeyError> {
    tracing::debug!("scanning /dev/input for keyboard devices");
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

    tracing::debug!(count = devices.len(), "keyboard device scan complete");

    if devices.is_empty() {
        return Err(HotkeyError::PermissionDenied(
            "No keyboard devices found. Is the user in the 'input' group?".into(),
        ));
    }

    Ok(devices)
}

/// Parse multiple hotkey strings into combos.
pub fn parse_hotkeys(names: &[String]) -> Result<Vec<HotkeyCombo>, HotkeyError> {
    names.iter().map(|n| parse_hotkey(n)).collect()
}

/// Start the hotkey listener on a dedicated OS thread.
/// The listener is persistent — update hotkeys via `SharedHotkeyConfig::update()`
/// instead of restarting the listener.
pub fn start_listener(
    config: SharedHotkeyConfig,
    event_tx: mpsc::UnboundedSender<HotkeyEvent>,
) -> Result<std::thread::JoinHandle<()>> {
    tracing::debug!("starting evdev hotkey listener");
    let devices = find_keyboard_devices()
        .context("Failed to find keyboard devices")?;

    let handle = std::thread::Builder::new()
        .name("hotkey-listener".into())
        .spawn(move || {
            if let Err(e) = listener_loop(devices, config, &event_tx) {
                tracing::error!("Hotkey listener error: {}", e);
            }
        })
        .context("Failed to spawn hotkey listener thread")?;

    Ok(handle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use evdev::Key;

    #[test]
    fn test_parse_key_f5() {
        assert_eq!(parse_key("KEY_F5").unwrap(), Key::KEY_F5);
    }

    #[test]
    fn test_parse_key_rightctrl() {
        assert_eq!(parse_key("KEY_RIGHTCTRL").unwrap(), Key::KEY_RIGHTCTRL);
    }

    #[test]
    fn test_parse_key_unknown_errors() {
        let result = parse_key("KEY_BANANA");
        assert!(result.is_err());
        match result {
            Err(HotkeyError::DeviceError(msg)) => assert!(msg.contains("Unknown key")),
            _ => panic!("expected DeviceError"),
        }
    }

    #[test]
    fn test_parse_key_letters_and_numbers() {
        assert_eq!(parse_key("KEY_A").unwrap(), Key::KEY_A);
        assert_eq!(parse_key("KEY_Z").unwrap(), Key::KEY_Z);
        assert_eq!(parse_key("KEY_0").unwrap(), Key::KEY_0);
        assert_eq!(parse_key("KEY_9").unwrap(), Key::KEY_9);
    }

    #[test]
    fn test_parse_key_modifiers() {
        assert_eq!(parse_key("KEY_LEFTCTRL").unwrap(), Key::KEY_LEFTCTRL);
        assert_eq!(parse_key("KEY_RIGHTCTRL").unwrap(), Key::KEY_RIGHTCTRL);
        assert_eq!(parse_key("KEY_LEFTALT").unwrap(), Key::KEY_LEFTALT);
        assert_eq!(parse_key("KEY_RIGHTALT").unwrap(), Key::KEY_RIGHTALT);
        assert_eq!(parse_key("KEY_LEFTSHIFT").unwrap(), Key::KEY_LEFTSHIFT);
        assert_eq!(parse_key("KEY_RIGHTSHIFT").unwrap(), Key::KEY_RIGHTSHIFT);
    }

    #[test]
    fn test_parse_key_common_keys() {
        assert_eq!(parse_key("KEY_SPACE").unwrap(), Key::KEY_SPACE);
        assert_eq!(parse_key("KEY_TAB").unwrap(), Key::KEY_TAB);
        assert_eq!(parse_key("KEY_ENTER").unwrap(), Key::KEY_ENTER);
        assert_eq!(parse_key("KEY_DELETE").unwrap(), Key::KEY_DELETE);
    }

    #[test]
    fn test_parse_hotkey_single_key() {
        let combo = parse_hotkey("KEY_F5").unwrap();
        assert!(combo.modifiers.is_empty());
        assert_eq!(combo.key, Key::KEY_F5);
    }

    #[test]
    fn test_parse_hotkey_two_part_combo() {
        let combo = parse_hotkey("KEY_LEFTCTRL+KEY_F5").unwrap();
        assert_eq!(combo.modifiers.len(), 1);
        assert_eq!(combo.modifiers[0], Key::KEY_LEFTCTRL);
        assert_eq!(combo.key, Key::KEY_F5);
    }

    #[test]
    fn test_parse_hotkey_three_part_combo() {
        let combo = parse_hotkey("KEY_LEFTCTRL+KEY_LEFTSHIFT+KEY_A").unwrap();
        assert_eq!(combo.modifiers.len(), 2);
        assert_eq!(combo.modifiers[0], Key::KEY_LEFTCTRL);
        assert_eq!(combo.modifiers[1], Key::KEY_LEFTSHIFT);
        assert_eq!(combo.key, Key::KEY_A);
    }

    #[test]
    fn test_parse_hotkey_too_many_parts_errors() {
        let result = parse_hotkey("KEY_A+KEY_B+KEY_C+KEY_D");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_hotkey_invalid_key_in_combo() {
        let result = parse_hotkey("KEY_LEFTCTRL+KEY_BANANA");
        assert!(result.is_err());
        match result {
            Err(HotkeyError::DeviceError(msg)) => assert!(msg.contains("Unknown key")),
            _ => panic!("expected DeviceError"),
        }
    }

    #[test]
    fn test_parse_hotkeys_multiple() {
        let names = vec!["KEY_F5".to_string(), "KEY_LEFTCTRL+KEY_A".to_string()];
        let combos = parse_hotkeys(&names).unwrap();
        assert_eq!(combos.len(), 2);
        assert!(combos[0].modifiers.is_empty());
        assert_eq!(combos[0].key, Key::KEY_F5);
        assert_eq!(combos[1].modifiers.len(), 1);
        assert_eq!(combos[1].key, Key::KEY_A);
    }

    #[test]
    fn test_parse_hotkeys_empty() {
        let combos = parse_hotkeys(&[]).unwrap();
        assert!(combos.is_empty());
    }

    #[test]
    fn test_shared_hotkey_config_update() {
        let config = SharedHotkeyConfig::new(vec![]);
        let gen_before = config.0.lock().unwrap().generation;

        config.update(vec![HotkeyCombo {
            modifiers: vec![],
            key: Key::KEY_F5,
        }]);
        let gen_after = config.0.lock().unwrap().generation;
        assert!(gen_after > gen_before);
        assert_eq!(config.0.lock().unwrap().combos.len(), 1);
    }

    // ── Edge case tests ──────────────────────────────────

    #[test]
    fn test_parse_key_empty_string_errors() {
        assert!(parse_key("").is_err());
    }

    #[test]
    fn test_parse_key_lowercase_rejected() {
        // parse_key is case-sensitive, lowercase "key_f5" is not recognized
        assert!(parse_key("key_f5").is_err());
    }

    #[test]
    fn test_parse_key_with_whitespace_errors() {
        assert!(parse_key(" KEY_F5 ").is_err());
    }

    #[test]
    fn test_parse_hotkey_empty_string_errors() {
        assert!(parse_hotkey("").is_err());
    }

    #[test]
    fn test_parse_hotkey_trailing_plus_errors() {
        // "KEY_F5+" splits to ["KEY_F5", ""], second part fails parse_key
        assert!(parse_hotkey("KEY_F5+").is_err());
    }

    #[test]
    fn test_parse_hotkey_leading_plus_errors() {
        // "+KEY_F5" splits to ["", "KEY_F5"], first part fails parse_key
        assert!(parse_hotkey("+KEY_F5").is_err());
    }

    #[test]
    fn test_parse_hotkeys_one_bad_fails_all() {
        let names = vec!["KEY_F5".to_string(), "KEY_BANANA".to_string()];
        assert!(parse_hotkeys(&names).is_err());
    }

    #[test]
    fn test_shared_hotkey_config_multiple_updates() {
        let config = SharedHotkeyConfig::new(vec![]);
        for i in 1..=3 {
            config.update(vec![HotkeyCombo {
                modifiers: vec![],
                key: Key::KEY_F5,
            }]);
            assert_eq!(config.0.lock().unwrap().generation, i);
        }
        assert_eq!(config.0.lock().unwrap().combos.len(), 1);
    }

    #[test]
    fn test_parse_key_all_function_keys() {
        for i in 1..=12 {
            let name = format!("KEY_F{}", i);
            assert!(parse_key(&name).is_ok(), "Failed to parse {}", name);
        }
    }
}

/// Per-hotkey tracking state.
struct HotkeyState {
    combo: HotkeyCombo,
    required_modifiers: HashSet<Key>,
    active: bool,
}

fn listener_loop(
    mut devices: Vec<Device>,
    config: SharedHotkeyConfig,
    event_tx: &mpsc::UnboundedSender<HotkeyEvent>,
) -> Result<()> {
    use nix::poll::{poll, PollFd, PollFlags, PollTimeout};
    use std::os::fd::{AsRawFd, BorrowedFd};

    let raw_fds: Vec<i32> = devices.iter().map(|d| d.as_raw_fd()).collect();
    // SAFETY: these fds are valid for the lifetime of `devices` which lives for the loop
    let mut pollfds: Vec<PollFd> = raw_fds
        .iter()
        .map(|&fd| PollFd::new(unsafe { BorrowedFd::borrow_raw(fd) }, PollFlags::POLLIN))
        .collect();

    tracing::info!(
        "Hotkey listener started, watching {} devices",
        devices.len(),
    );

    let mut last_generation: u64 = u64::MAX; // force initial rebuild
    let mut states: Vec<HotkeyState> = Vec::new();
    let mut modifiers_held: HashSet<Key> = HashSet::new();

    loop {
        // Check for config updates at the top of each poll cycle
        if let Ok(inner) = config.0.lock() {
            if inner.generation != last_generation {
                states = inner.combos.iter().map(|combo| {
                    let required_modifiers: HashSet<Key> = combo.modifiers.iter().copied().collect();
                    HotkeyState { combo: combo.clone(), required_modifiers, active: false }
                }).collect();
                modifiers_held.clear();
                last_generation = inner.generation;
                tracing::debug!(
                    generation = last_generation,
                    combo_count = states.len(),
                    "hotkey config reloaded in listener loop"
                );
            }
        }

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
                        let value = event.value(); // 1=down, 0=up, 2=repeat

                        // Track global modifier state
                        let is_modifier = states.iter().any(|s| s.required_modifiers.contains(&key));
                        if is_modifier {
                            match value {
                                1 => {
                                    modifiers_held.insert(key);
                                    tracing::trace!(?key, "modifier down");
                                }
                                0 => {
                                    modifiers_held.remove(&key);
                                    tracing::trace!(?key, "modifier up");
                                }
                                _ => {}
                            }
                        }

                        for state in states.iter_mut() {
                            if state.required_modifiers.is_empty() {
                                // Single key mode (no modifiers)
                                if key == state.combo.key {
                                    let hotkey_event = match value {
                                        1 => Some(HotkeyEvent::Pressed),
                                        0 => Some(HotkeyEvent::Released),
                                        _ => None,
                                    };
                                    if let Some(evt) = hotkey_event {
                                        tracing::debug!("Hotkey event: {:?}", evt);
                                        if event_tx.send(evt).is_err() {
                                            tracing::info!("Hotkey channel closed, exiting listener");
                                            return Ok(());
                                        }
                                    }
                                }
                            } else {
                                // Combo mode: modifier(s) + key
                                if state.required_modifiers.contains(&key) && value == 0 && state.active {
                                    state.active = false;
                                    tracing::debug!("Hotkey event: Released (modifier up)");
                                    if event_tx.send(HotkeyEvent::Released).is_err() {
                                        return Ok(());
                                    }
                                } else if key == state.combo.key {
                                    let all_mods_held = state.required_modifiers.is_subset(&modifiers_held);
                                    match value {
                                        1 if all_mods_held && !state.active => {
                                            state.active = true;
                                            tracing::debug!("Hotkey event: Pressed (combo)");
                                            if event_tx.send(HotkeyEvent::Pressed).is_err() {
                                                return Ok(());
                                            }
                                        }
                                        0 if state.active => {
                                            state.active = false;
                                            tracing::debug!("Hotkey event: Released (key up)");
                                            if event_tx.send(HotkeyEvent::Released).is_err() {
                                                return Ok(());
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
