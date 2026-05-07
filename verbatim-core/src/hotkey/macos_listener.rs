use anyhow::{Context, Result};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

use super::{CaptureSlot, CapturedHotkey, HotkeyEvent};
use crate::config::Hotkey as ConfigHotkey;

// macOS modifier virtual key codes (CGKeyCode values).
const KC_SHIFT_LEFT: u16 = 56;
const KC_SHIFT_RIGHT: u16 = 60;
const KC_CONTROL_LEFT: u16 = 59;
const KC_CONTROL_RIGHT: u16 = 62;
const KC_ALT_LEFT: u16 = 58;
const KC_ALT_RIGHT: u16 = 61;
const KC_META_LEFT: u16 = 55;
const KC_META_RIGHT: u16 = 54;
const KC_CAPS_LOCK: u16 = 57;
const KC_FUNCTION: u16 = 63;

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

    pub fn update(&self, combos: Vec<HotkeyCombo>) {
        let mut inner = self.0.lock().unwrap();
        inner.combos = combos;
        inner.generation += 1;
        tracing::info!("Hotkey config updated (generation {})", inner.generation);
    }
}

/// A hotkey combo using raw macOS key codes.
#[derive(Debug, Clone)]
pub struct HotkeyCombo {
    pub modifiers: Vec<u16>,
    pub key: u16,
}

impl From<&ConfigHotkey> for HotkeyCombo {
    fn from(h: &ConfigHotkey) -> Self {
        HotkeyCombo {
            modifiers: h.modifiers.iter().map(|m| *m as u16).collect(),
            key: h.key as u16,
        }
    }
}

pub fn combos_from_hotkeys(hotkeys: &[ConfigHotkey]) -> Vec<HotkeyCombo> {
    hotkeys.iter().map(HotkeyCombo::from).collect()
}

// CGEvent types we care about
const K_CG_EVENT_KEY_DOWN: u32 = 10;
const K_CG_EVENT_KEY_UP: u32 = 11;
const K_CG_EVENT_FLAGS_CHANGED: u32 = 12;

const K_CG_HEAD_INSERT_EVENT_TAP: u32 = 0;
const K_CG_EVENT_TAP_OPTION_LISTEN_ONLY: u32 = 1;

const EVENT_MASK: u64 = (1 << K_CG_EVENT_KEY_DOWN as u64)
    | (1 << K_CG_EVENT_KEY_UP as u64)
    | (1 << K_CG_EVENT_FLAGS_CHANGED as u64);

// Note: KC_FUNCTION (fn / Globe) is intentionally absent. On macOS fn is a
// hardware translation key, not a real modifier — the OS consumes it to
// produce a different virtual keycode (e.g. fn + F5_key → F5 = 96, F5_key
// alone → dictation). Treating fn as a modifier produced unreachable
// bindings like "fn + F5" because by the time F5 arrives, fn has already
// done its job and the user can't reproduce a held-fn + F5 sequence.
const MODIFIER_KEYCODES: &[u16] = &[
    KC_SHIFT_LEFT, KC_SHIFT_RIGHT,
    KC_CONTROL_LEFT, KC_CONTROL_RIGHT,
    KC_ALT_LEFT, KC_ALT_RIGHT,
    KC_META_LEFT, KC_META_RIGHT,
    KC_CAPS_LOCK,
];

fn is_modifier_keycode(code: u16) -> bool {
    MODIFIER_KEYCODES.contains(&code)
}

/// Best-effort label for a CGKeyCode. Falls back to "Code N".
pub fn label_for(code: u16) -> String {
    match code {
        // Modifiers
        KC_CONTROL_LEFT => "Left Ctrl".into(),
        KC_CONTROL_RIGHT => "Right Ctrl".into(),
        KC_ALT_LEFT => "Left Option".into(),
        KC_ALT_RIGHT => "Right Option".into(),
        KC_SHIFT_LEFT => "Left Shift".into(),
        KC_SHIFT_RIGHT => "Right Shift".into(),
        KC_META_LEFT => "Left Cmd".into(),
        KC_META_RIGHT => "Right Cmd".into(),
        KC_CAPS_LOCK => "Caps Lock".into(),
        KC_FUNCTION => "Fn".into(),
        // Function keys
        122 => "F1".into(), 120 => "F2".into(), 99 => "F3".into(), 118 => "F4".into(),
        96 => "F5".into(), 97 => "F6".into(), 98 => "F7".into(), 100 => "F8".into(),
        101 => "F9".into(), 109 => "F10".into(), 103 => "F11".into(), 111 => "F12".into(),
        105 => "F13".into(), 107 => "F14".into(), 113 => "F15".into(), 106 => "F16".into(),
        64 => "F17".into(), 79 => "F18".into(), 80 => "F19".into(), 90 => "F20".into(),
        // Whitespace / nav
        49 => "Space".into(), 48 => "Tab".into(), 36 => "Return".into(),
        51 => "Backspace".into(), 117 => "Delete".into(),
        115 => "Home".into(), 119 => "End".into(),
        116 => "Page Up".into(), 121 => "Page Down".into(),
        123 => "Left".into(), 124 => "Right".into(), 125 => "Down".into(), 126 => "Up".into(),
        53 => "Esc".into(),
        c => format!("Code {}", c),
    }
}

fn label_for_combo(modifiers: &[u16], key: u16) -> String {
    let mut parts: Vec<String> = modifiers.iter().map(|c| label_for(*c)).collect();
    parts.push(label_for(key));
    parts.join(" + ")
}

// FFI declarations for CGEventTap
type CGEventTapProxy = *mut std::ffi::c_void;
type CGEventRef = core_graphics::event::CGEvent;
type CFMachPortRef = *mut std::ffi::c_void;
type CFRunLoopSourceRef = *mut std::ffi::c_void;
type CFRunLoopRef = *mut std::ffi::c_void;
type CFRunLoopMode = *mut std::ffi::c_void;

type CGEventTapCallBack = unsafe extern "C" fn(
    proxy: CGEventTapProxy,
    event_type: u32,
    event: CGEventRef,
    user_info: *mut std::ffi::c_void,
) -> CGEventRef;

extern "C" {
    fn CGEventTapCreate(
        tap: u32,
        place: u32,
        options: u32,
        events_of_interest: u64,
        callback: CGEventTapCallBack,
        user_info: *mut std::ffi::c_void,
    ) -> CFMachPortRef;
    fn CFMachPortCreateRunLoopSource(
        allocator: *const std::ffi::c_void,
        port: CFMachPortRef,
        order: i64,
    ) -> CFRunLoopSourceRef;
    fn CFRunLoopGetCurrent() -> CFRunLoopRef;
    fn CFRunLoopAddSource(rl: CFRunLoopRef, source: CFRunLoopSourceRef, mode: CFRunLoopMode);
    fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);
    fn CFRunLoopRun();

    static kCFRunLoopCommonModes: CFRunLoopMode;
}

use core_graphics::event::EventField;

/// State passed to the CGEventTap callback via user_info pointer.
struct CallbackState {
    config: SharedHotkeyConfig,
    capture: CaptureSlot,
    event_tx: mpsc::UnboundedSender<HotkeyEvent>,
    last_generation: u64,
    states: Vec<HkState>,
    modifiers_held: HashSet<u16>,
    /// Track which modifier keys are currently pressed (for FlagsChanged press/release detection)
    prev_modifier_flags: HashSet<u16>,
    /// Capture-mode bookkeeping.
    capture_mods_held: HashSet<u16>,
    capture_seen_non_modifier: bool,
}

struct HkState {
    required_modifiers: HashSet<u16>,
    key: u16,
    active: bool,
}

unsafe extern "C" fn raw_callback(
    _proxy: CGEventTapProxy,
    event_type: u32,
    cg_event: CGEventRef,
    user_info: *mut std::ffi::c_void,
) -> CGEventRef {
    let state = &mut *(user_info as *mut CallbackState);

    if let Ok(inner) = state.config.0.lock() {
        if inner.generation != state.last_generation {
            state.states = inner.combos.iter().map(|combo| HkState {
                required_modifiers: combo.modifiers.iter().copied().collect(),
                key: combo.key,
                active: false,
            }).collect();
            state.modifiers_held.clear();
            state.prev_modifier_flags.clear();
            state.last_generation = inner.generation;
        }
    }

    let keycode = cg_event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE) as u16;

    // Determine press vs release
    let press_release: Option<bool> = match event_type {
        K_CG_EVENT_KEY_DOWN => Some(true),
        K_CG_EVENT_KEY_UP => Some(false),
        K_CG_EVENT_FLAGS_CHANGED if is_modifier_keycode(keycode) => {
            if state.prev_modifier_flags.contains(&keycode) {
                state.prev_modifier_flags.remove(&keycode);
                Some(false)
            } else {
                state.prev_modifier_flags.insert(keycode);
                Some(true)
            }
        }
        _ => None,
    };

    let Some(is_press) = press_release else { return cg_event; };

    // Capture mode short-circuits hotkey matching.
    if state.capture.is_armed() {
        handle_capture(state, keycode, is_press);
        return cg_event;
    } else if !state.capture_mods_held.is_empty() || state.capture_seen_non_modifier {
        state.capture_mods_held.clear();
        state.capture_seen_non_modifier = false;
    }

    if state.states.is_empty() {
        return cg_event;
    }

    if is_press {
        handle_key_press(state, keycode);
    } else {
        handle_key_release(state, keycode);
    }

    cg_event
}

fn handle_capture(state: &mut CallbackState, code: u16, is_press: bool) {
    let modifier = is_modifier_keycode(code);
    match (modifier, is_press) {
        (true, true) => { state.capture_mods_held.insert(code); }
        (false, true) => {
            state.capture_seen_non_modifier = true;
            if let Some(tx) = state.capture.take() {
                let modifiers: Vec<u32> = state.capture_mods_held.iter().map(|c| *c as u32).collect();
                let mods_u16: Vec<u16> = state.capture_mods_held.iter().copied().collect();
                let label = label_for_combo(&mods_u16, code);
                let _ = tx.send(CapturedHotkey { key: code as u32, modifiers, label });
                state.capture_mods_held.clear();
                state.capture_seen_non_modifier = false;
            }
        }
        (true, false) => {
            state.capture_mods_held.remove(&code);
            if state.capture_mods_held.is_empty() && !state.capture_seen_non_modifier {
                if let Some(tx) = state.capture.take() {
                    let label = label_for(code);
                    let _ = tx.send(CapturedHotkey { key: code as u32, modifiers: vec![], label });
                }
            }
            state.capture_seen_non_modifier = false;
        }
        _ => {}
    }
}

fn handle_key_press(state: &mut CallbackState, keycode: u16) {
    let is_modifier = state.states.iter().any(|s| s.required_modifiers.contains(&keycode));
    if is_modifier {
        state.modifiers_held.insert(keycode);
    }

    for hk in state.states.iter_mut() {
        if hk.required_modifiers.is_empty() {
            if keycode == hk.key && !hk.active {
                hk.active = true;
                let _ = state.event_tx.send(HotkeyEvent::Pressed);
            }
        } else if keycode == hk.key {
            if hk.required_modifiers.is_subset(&state.modifiers_held) && !hk.active {
                hk.active = true;
                let _ = state.event_tx.send(HotkeyEvent::Pressed);
            }
        }
    }
}

fn handle_key_release(state: &mut CallbackState, keycode: u16) {
    let is_modifier = state.states.iter().any(|s| s.required_modifiers.contains(&keycode));
    if is_modifier {
        state.modifiers_held.remove(&keycode);
    }

    for hk in state.states.iter_mut() {
        if !hk.active {
            continue;
        }
        if hk.required_modifiers.is_empty() {
            if keycode == hk.key {
                hk.active = false;
                let _ = state.event_tx.send(HotkeyEvent::Released);
            }
        } else if hk.required_modifiers.contains(&keycode) {
            hk.active = false;
            let _ = state.event_tx.send(HotkeyEvent::Released);
        } else if keycode == hk.key {
            hk.active = false;
            let _ = state.event_tx.send(HotkeyEvent::Released);
        }
    }
}

/// Start the hotkey listener on a dedicated OS thread using a native CGEventTap.
pub fn start_listener(
    config: SharedHotkeyConfig,
    capture: CaptureSlot,
    event_tx: mpsc::UnboundedSender<HotkeyEvent>,
) -> Result<std::thread::JoinHandle<()>> {
    tracing::info!("Starting macOS hotkey listener (requires Accessibility permission)");

    let handle = std::thread::Builder::new()
        .name("hotkey-listener".into())
        .spawn(move || {
            let mut cb_state = Box::new(CallbackState {
                config,
                capture,
                event_tx,
                last_generation: u64::MAX,
                states: Vec::new(),
                modifiers_held: HashSet::new(),
                prev_modifier_flags: HashSet::new(),
                capture_mods_held: HashSet::new(),
                capture_seen_non_modifier: false,
            });

            unsafe {
                let user_info = &mut *cb_state as *mut CallbackState as *mut std::ffi::c_void;

                let tap = CGEventTapCreate(
                    0, // kCGHIDEventTap
                    K_CG_HEAD_INSERT_EVENT_TAP,
                    K_CG_EVENT_TAP_OPTION_LISTEN_ONLY,
                    EVENT_MASK,
                    raw_callback,
                    user_info,
                );
                if tap.is_null() {
                    tracing::error!(
                        "Failed to create CGEventTap. \
                         Ensure Accessibility permission is granted in \
                         System Settings > Privacy & Security > Accessibility"
                    );
                    return;
                }

                let source = CFMachPortCreateRunLoopSource(std::ptr::null(), tap, 0);
                if source.is_null() {
                    tracing::error!("Failed to create CFRunLoopSource for CGEventTap");
                    return;
                }

                let run_loop = CFRunLoopGetCurrent();
                CFRunLoopAddSource(run_loop, source, kCFRunLoopCommonModes);
                CGEventTapEnable(tap, true);

                tracing::debug!("CGEventTap created and enabled, entering run loop");
                CFRunLoopRun();
            }
        })
        .context("Failed to spawn hotkey listener thread")?;

    Ok(handle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_label_for_function_keys() {
        assert_eq!(label_for(96), "F5");
        assert_eq!(label_for(107), "F14");
        assert_eq!(label_for(64), "F17");
    }

    #[test]
    fn test_label_for_unknown_falls_back_to_code() {
        assert_eq!(label_for(250), "Code 250");
    }

    #[test]
    fn test_combos_from_hotkeys_passthrough() {
        let h = ConfigHotkey { key: 96, modifiers: vec![59], label: "Ctrl+F5".into() };
        let combos = combos_from_hotkeys(&[h]);
        assert_eq!(combos[0].key, 96);
        assert_eq!(combos[0].modifiers, vec![59]);
    }

    #[test]
    fn test_shared_hotkey_config_update() {
        let config = SharedHotkeyConfig::new(vec![]);
        let gen_before = config.0.lock().unwrap().generation;
        config.update(vec![HotkeyCombo { modifiers: vec![], key: 96 }]);
        let gen_after = config.0.lock().unwrap().generation;
        assert!(gen_after > gen_before);
    }

    #[test]
    fn test_is_modifier_keycode() {
        assert!(is_modifier_keycode(KC_SHIFT_LEFT));
        assert!(is_modifier_keycode(KC_CONTROL_RIGHT));
        assert!(!is_modifier_keycode(96)); // F5
    }

    #[test]
    fn test_fn_is_not_a_modifier() {
        // fn (Globe) is a hardware translation key, not a hotkey modifier.
        // Capturing fn+F5_key on macOS yields just F5 because fn is consumed
        // by the OS to produce the F5 keycode in the first place.
        assert!(!is_modifier_keycode(KC_FUNCTION));
    }
}
