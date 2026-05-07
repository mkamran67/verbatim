use anyhow::{Context, Result};
use evdev::{Device, InputEventKind, Key};
use std::collections::HashSet;
use std::fs;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

use super::{CaptureSlot, CapturedHotkey, HotkeyEvent};
use crate::config::Hotkey as ConfigHotkey;
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

/// A hotkey: a key with 0+ modifiers, all expressed as raw evdev codes.
#[derive(Debug, Clone)]
pub struct HotkeyCombo {
    pub modifiers: Vec<u32>,
    pub key: u32,
}

impl From<&ConfigHotkey> for HotkeyCombo {
    fn from(h: &ConfigHotkey) -> Self {
        HotkeyCombo { modifiers: h.modifiers.clone(), key: h.key }
    }
}

pub fn combos_from_hotkeys(hotkeys: &[ConfigHotkey]) -> Vec<HotkeyCombo> {
    hotkeys.iter().map(HotkeyCombo::from).collect()
}

/// evdev codes for the keys we treat as modifiers when capturing or matching.
const MODIFIER_CODES: &[Key] = &[
    Key::KEY_LEFTCTRL, Key::KEY_RIGHTCTRL,
    Key::KEY_LEFTALT, Key::KEY_RIGHTALT,
    Key::KEY_LEFTSHIFT, Key::KEY_RIGHTSHIFT,
    Key::KEY_LEFTMETA, Key::KEY_RIGHTMETA,
];

fn is_modifier_code(code: u32) -> bool {
    MODIFIER_CODES.iter().any(|k| k.code() as u32 == code)
}

/// Best-effort label for a captured key. Falls back to "Code N" so unknown
/// keys still surface usefully in the UI.
pub fn label_for(code: u32) -> String {
    let key = Key::new(code as u16);
    // evdev's Debug for Key is "KEY_F14" etc.; strip the prefix for display.
    let dbg = format!("{:?}", key);
    if let Some(rest) = dbg.strip_prefix("KEY_") {
        // Title-case-ish: leave function keys / acronyms alone.
        return rest.to_string();
    }
    format!("Code {}", code)
}

fn label_for_combo(modifiers: &[u32], key: u32) -> String {
    let mut parts: Vec<String> = modifiers.iter().map(|c| label_for(*c)).collect();
    parts.push(label_for(key));
    parts.join(" + ")
}

/// Cheap probe: returns `true` if any `/dev/input/event*` node exposing
/// keyboard keys can be opened by the current process. Used by the
/// permissions-checklist UI so the user can confirm `input`-group
/// membership without starting the full listener.
pub fn keyboard_input_accessible() -> bool {
    let entries = match fs::read_dir("/dev/input") {
        Ok(e) => e,
        Err(_) => return false,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !name.starts_with("event") {
            continue;
        }
        if let Ok(device) = Device::open(&path) {
            if let Some(keys) = device.supported_keys() {
                if keys.contains(Key::KEY_A) || keys.contains(Key::KEY_RIGHTCTRL) {
                    return true;
                }
            }
        }
    }
    false
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

/// Start the hotkey listener on a dedicated OS thread.
/// The listener is persistent — update hotkeys via `SharedHotkeyConfig::update()`
/// instead of restarting the listener.
pub fn start_listener(
    config: SharedHotkeyConfig,
    capture: CaptureSlot,
    event_tx: mpsc::UnboundedSender<HotkeyEvent>,
) -> Result<std::thread::JoinHandle<()>> {
    tracing::debug!("starting evdev hotkey listener");
    let devices = find_keyboard_devices()
        .context("Failed to find keyboard devices")?;

    let handle = std::thread::Builder::new()
        .name("hotkey-listener".into())
        .spawn(move || {
            if let Err(e) = listener_loop(devices, config, capture, &event_tx) {
                tracing::error!("Hotkey listener error: {}", e);
            }
        })
        .context("Failed to spawn hotkey listener thread")?;

    Ok(handle)
}

/// Per-hotkey tracking state.
struct HotkeyState {
    combo: HotkeyCombo,
    required_modifiers: HashSet<u32>,
    active: bool,
}

fn listener_loop(
    mut devices: Vec<Device>,
    config: SharedHotkeyConfig,
    capture: CaptureSlot,
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
    let mut modifiers_held: HashSet<u32> = HashSet::new();
    // Capture-mode bookkeeping.
    let mut capture_mods_held: HashSet<u32> = HashSet::new();
    let mut capture_seen_non_modifier = false;

    loop {
        // Check for config updates at the top of each poll cycle
        if let Ok(inner) = config.0.lock() {
            if inner.generation != last_generation {
                states = inner.combos.iter().map(|combo| {
                    let required_modifiers: HashSet<u32> =
                        combo.modifiers.iter().copied().collect();
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
                        let code = key.code() as u32;

                        // Capture mode: route the next press to the capture slot.
                        if capture.is_armed() {
                            handle_capture(
                                &capture,
                                code,
                                value,
                                &mut capture_mods_held,
                                &mut capture_seen_non_modifier,
                            );
                            // Skip normal hotkey matching while armed so we don't
                            // accidentally fire a recording session.
                            continue;
                        } else if !capture_mods_held.is_empty() || capture_seen_non_modifier {
                            // Reset capture bookkeeping once disarmed.
                            capture_mods_held.clear();
                            capture_seen_non_modifier = false;
                        }

                        // Track global modifier state
                        let is_modifier = states.iter().any(|s| s.required_modifiers.contains(&code));
                        if is_modifier {
                            match value {
                                1 => { modifiers_held.insert(code); }
                                0 => { modifiers_held.remove(&code); }
                                _ => {}
                            }
                        }

                        for state in states.iter_mut() {
                            if state.required_modifiers.is_empty() {
                                if code == state.combo.key {
                                    let hotkey_event = match value {
                                        1 => Some(HotkeyEvent::Pressed),
                                        0 => Some(HotkeyEvent::Released),
                                        _ => None,
                                    };
                                    if let Some(evt) = hotkey_event {
                                        tracing::debug!("Hotkey event: {:?}", evt);
                                        if event_tx.send(evt).is_err() {
                                            return Ok(());
                                        }
                                    }
                                }
                            } else {
                                if state.required_modifiers.contains(&code) && value == 0 && state.active {
                                    state.active = false;
                                    if event_tx.send(HotkeyEvent::Released).is_err() {
                                        return Ok(());
                                    }
                                } else if code == state.combo.key {
                                    let all_mods_held = state.required_modifiers.is_subset(&modifiers_held);
                                    match value {
                                        1 if all_mods_held && !state.active => {
                                            state.active = true;
                                            if event_tx.send(HotkeyEvent::Pressed).is_err() {
                                                return Ok(());
                                            }
                                        }
                                        0 if state.active => {
                                            state.active = false;
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

/// Capture-mode rules (matching the previous UI behaviour):
///   - Modifier presses are accumulated as held modifiers.
///   - The first non-modifier press fires the capture, with the held mods.
///   - If the user releases the only held modifier without ever pressing a
///     non-modifier, that modifier itself is the captured key.
fn handle_capture(
    capture: &CaptureSlot,
    code: u32,
    value: i32,
    mods_held: &mut HashSet<u32>,
    seen_non_modifier: &mut bool,
) {
    let modifier = is_modifier_code(code);
    match (modifier, value) {
        (true, 1) => { mods_held.insert(code); }
        (false, 1) => {
            // Non-modifier down: capture immediately with the held modifiers.
            *seen_non_modifier = true;
            if let Some(tx) = capture.take() {
                let modifiers: Vec<u32> = mods_held.iter().copied().collect();
                let label = label_for_combo(&modifiers, code);
                let _ = tx.send(CapturedHotkey { key: code, modifiers, label });
                mods_held.clear();
                *seen_non_modifier = false;
            }
        }
        (true, 0) => {
            mods_held.remove(&code);
            if mods_held.is_empty() && !*seen_non_modifier {
                if let Some(tx) = capture.take() {
                    let label = label_for(code);
                    let _ = tx.send(CapturedHotkey { key: code, modifiers: vec![], label });
                }
            }
            *seen_non_modifier = false;
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_label_for_known_key() {
        assert_eq!(label_for(Key::KEY_F5.code() as u32), "F5");
        assert_eq!(label_for(Key::KEY_RIGHTCTRL.code() as u32), "RIGHTCTRL");
    }

    #[test]
    fn test_combos_from_hotkeys_passthrough() {
        let h = ConfigHotkey { key: 42, modifiers: vec![29], label: "Ctrl+X".into() };
        let combos = combos_from_hotkeys(&[h]);
        assert_eq!(combos.len(), 1);
        assert_eq!(combos[0].key, 42);
        assert_eq!(combos[0].modifiers, vec![29]);
    }

    #[test]
    fn test_shared_hotkey_config_update() {
        let config = SharedHotkeyConfig::new(vec![]);
        let gen_before = config.0.lock().unwrap().generation;
        config.update(vec![HotkeyCombo { modifiers: vec![], key: Key::KEY_F5.code() as u32 }]);
        let gen_after = config.0.lock().unwrap().generation;
        assert!(gen_after > gen_before);
        assert_eq!(config.0.lock().unwrap().combos.len(), 1);
    }

    #[test]
    fn test_capture_non_modifier_press() {
        let slot = CaptureSlot::new();
        let mut rx = slot.arm();
        let mut held = HashSet::new();
        let mut seen = false;
        // Press F5
        handle_capture(&slot, Key::KEY_F5.code() as u32, 1, &mut held, &mut seen);
        let captured = rx.try_recv().unwrap();
        assert_eq!(captured.key, Key::KEY_F5.code() as u32);
        assert!(captured.modifiers.is_empty());
        assert_eq!(captured.label, "F5");
    }

    #[test]
    fn test_capture_combo_with_modifier() {
        let slot = CaptureSlot::new();
        let mut rx = slot.arm();
        let mut held = HashSet::new();
        let mut seen = false;
        // Press Ctrl, then F5
        handle_capture(&slot, Key::KEY_LEFTCTRL.code() as u32, 1, &mut held, &mut seen);
        assert!(rx.try_recv().is_err()); // not yet
        handle_capture(&slot, Key::KEY_F5.code() as u32, 1, &mut held, &mut seen);
        let captured = rx.try_recv().unwrap();
        assert_eq!(captured.key, Key::KEY_F5.code() as u32);
        assert_eq!(captured.modifiers, vec![Key::KEY_LEFTCTRL.code() as u32]);
    }

    #[test]
    fn test_capture_modifier_only_on_release() {
        let slot = CaptureSlot::new();
        let mut rx = slot.arm();
        let mut held = HashSet::new();
        let mut seen = false;
        // Press and release Right Ctrl with no other key
        handle_capture(&slot, Key::KEY_RIGHTCTRL.code() as u32, 1, &mut held, &mut seen);
        assert!(rx.try_recv().is_err());
        handle_capture(&slot, Key::KEY_RIGHTCTRL.code() as u32, 0, &mut held, &mut seen);
        let captured = rx.try_recv().unwrap();
        assert_eq!(captured.key, Key::KEY_RIGHTCTRL.code() as u32);
        assert!(captured.modifiers.is_empty());
    }
}
