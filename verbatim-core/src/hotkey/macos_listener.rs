use anyhow::{Context, Result};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

use super::HotkeyEvent;
use crate::errors::HotkeyError;

// macOS virtual key codes (CGKeyCode values)
const KC_A: u16 = 0;
const KC_S: u16 = 1;
const KC_D: u16 = 2;
const KC_F: u16 = 3;
const KC_H: u16 = 4;
const KC_G: u16 = 5;
const KC_Z: u16 = 6;
const KC_X: u16 = 7;
const KC_C: u16 = 8;
const KC_V: u16 = 9;
const KC_B: u16 = 11;
const KC_Q: u16 = 12;
const KC_W: u16 = 13;
const KC_E: u16 = 14;
const KC_R: u16 = 15;
const KC_Y: u16 = 16;
const KC_T: u16 = 17;
const KC_1: u16 = 18;
const KC_2: u16 = 19;
const KC_3: u16 = 20;
const KC_4: u16 = 21;
const KC_6: u16 = 22;
const KC_5: u16 = 23;
const KC_EQUAL: u16 = 24;
const KC_9: u16 = 25;
const KC_7: u16 = 26;
const KC_MINUS: u16 = 27;
const KC_8: u16 = 28;
const KC_0: u16 = 29;
const KC_RIGHT_BRACKET: u16 = 30;
const KC_O: u16 = 31;
const KC_U: u16 = 32;
const KC_LEFT_BRACKET: u16 = 33;
const KC_I: u16 = 34;
const KC_P: u16 = 35;
const KC_RETURN: u16 = 36;
const KC_L: u16 = 37;
const KC_J: u16 = 38;
const KC_QUOTE: u16 = 39;
const KC_K: u16 = 40;
const KC_SEMICOLON: u16 = 41;
const KC_BACKSLASH: u16 = 42;
const KC_COMMA: u16 = 43;
const KC_SLASH: u16 = 44;
const KC_N: u16 = 45;
const KC_M: u16 = 46;
const KC_DOT: u16 = 47;
const KC_TAB: u16 = 48;
const KC_SPACE: u16 = 49;
const KC_GRAVE: u16 = 50;
const KC_BACKSPACE: u16 = 51;
const _KC_ESCAPE: u16 = 53;
const KC_META_RIGHT: u16 = 54;
const KC_META_LEFT: u16 = 55;
const KC_SHIFT_LEFT: u16 = 56;
const KC_CAPS_LOCK: u16 = 57;
const KC_ALT_LEFT: u16 = 58;
const KC_CONTROL_LEFT: u16 = 59;
const KC_SHIFT_RIGHT: u16 = 60;
const KC_ALT_RIGHT: u16 = 61;
const KC_CONTROL_RIGHT: u16 = 62;
const KC_FUNCTION: u16 = 63;
const KC_F5: u16 = 96;
const KC_F6: u16 = 97;
const KC_F7: u16 = 98;
const KC_F3: u16 = 99;
const KC_F8: u16 = 100;
const KC_F9: u16 = 101;
const KC_F11: u16 = 103;
const KC_F10: u16 = 109;
const KC_F12: u16 = 111;
const KC_INSERT: u16 = 114;
const KC_HOME: u16 = 115;
const KC_PAGE_UP: u16 = 116;
const KC_DELETE: u16 = 117;
const KC_F4: u16 = 118;
const KC_END: u16 = 119;
const KC_F2: u16 = 120;
const KC_PAGE_DOWN: u16 = 121;
const KC_F1: u16 = 122;
const KC_LEFT: u16 = 123;
const KC_RIGHT: u16 = 124;
const KC_DOWN: u16 = 125;
const KC_UP: u16 = 126;

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

    /// Replace the active hotkey combos. The listener callback will pick up
    /// the change on the next key event.
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

/// Parse a key name like "KEY_RIGHTCTRL" to a macOS virtual key code.
pub fn parse_key(name: &str) -> Result<u16, HotkeyError> {
    tracing::trace!(name, "parsing key name");
    let key = match name {
        "KEY_RIGHTCTRL" => KC_CONTROL_RIGHT,
        "KEY_LEFTCTRL" => KC_CONTROL_LEFT,
        "KEY_RIGHTALT" => KC_ALT_RIGHT,
        "KEY_LEFTALT" => KC_ALT_LEFT,
        "KEY_RIGHTSHIFT" => KC_SHIFT_RIGHT,
        "KEY_LEFTSHIFT" => KC_SHIFT_LEFT,
        "KEY_F1" => KC_F1,
        "KEY_F2" => KC_F2,
        "KEY_F3" => KC_F3,
        "KEY_F4" => KC_F4,
        "KEY_F5" => KC_F5,
        "KEY_F6" => KC_F6,
        "KEY_F7" => KC_F7,
        "KEY_F8" => KC_F8,
        "KEY_F9" => KC_F9,
        "KEY_F10" => KC_F10,
        "KEY_F11" => KC_F11,
        "KEY_F12" => KC_F12,
        "KEY_CAPSLOCK" => KC_CAPS_LOCK,
        "KEY_FN" | "KEY_GLOBE" => KC_FUNCTION, // Globe/Fn key on macOS keyboards
        "KEY_SCROLLLOCK" | "KEY_PAUSE" => KC_FUNCTION, // no direct equivalent on macOS
        "KEY_INSERT" => KC_INSERT,
        // Letters
        "KEY_A" => KC_A,
        "KEY_B" => KC_B,
        "KEY_C" => KC_C,
        "KEY_D" => KC_D,
        "KEY_E" => KC_E,
        "KEY_F" => KC_F,
        "KEY_G" => KC_G,
        "KEY_H" => KC_H,
        "KEY_I" => KC_I,
        "KEY_J" => KC_J,
        "KEY_K" => KC_K,
        "KEY_L" => KC_L,
        "KEY_M" => KC_M,
        "KEY_N" => KC_N,
        "KEY_O" => KC_O,
        "KEY_P" => KC_P,
        "KEY_Q" => KC_Q,
        "KEY_R" => KC_R,
        "KEY_S" => KC_S,
        "KEY_T" => KC_T,
        "KEY_U" => KC_U,
        "KEY_V" => KC_V,
        "KEY_W" => KC_W,
        "KEY_X" => KC_X,
        "KEY_Y" => KC_Y,
        "KEY_Z" => KC_Z,
        // Numbers
        "KEY_0" => KC_0,
        "KEY_1" => KC_1,
        "KEY_2" => KC_2,
        "KEY_3" => KC_3,
        "KEY_4" => KC_4,
        "KEY_5" => KC_5,
        "KEY_6" => KC_6,
        "KEY_7" => KC_7,
        "KEY_8" => KC_8,
        "KEY_9" => KC_9,
        // Common keys
        "KEY_SPACE" => KC_SPACE,
        "KEY_TAB" => KC_TAB,
        "KEY_ENTER" => KC_RETURN,
        "KEY_BACKSPACE" => KC_BACKSPACE,
        "KEY_DELETE" => KC_DELETE,
        "KEY_HOME" => KC_HOME,
        "KEY_END" => KC_END,
        "KEY_PAGEUP" => KC_PAGE_UP,
        "KEY_PAGEDOWN" => KC_PAGE_DOWN,
        "KEY_UP" => KC_UP,
        "KEY_DOWN" => KC_DOWN,
        "KEY_LEFT" => KC_LEFT,
        "KEY_RIGHT" => KC_RIGHT,
        "KEY_MINUS" => KC_MINUS,
        "KEY_EQUAL" => KC_EQUAL,
        "KEY_COMMA" => KC_COMMA,
        "KEY_DOT" => KC_DOT,
        "KEY_SLASH" => KC_SLASH,
        "KEY_SEMICOLON" => KC_SEMICOLON,
        "KEY_APOSTROPHE" => KC_QUOTE,
        "KEY_GRAVE" => KC_GRAVE,
        "KEY_BACKSLASH" => KC_BACKSLASH,
        "KEY_LEFTBRACE" => KC_LEFT_BRACKET,
        "KEY_RIGHTBRACE" => KC_RIGHT_BRACKET,
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

// CGEvent types we care about
const K_CG_EVENT_KEY_DOWN: u32 = 10;
const K_CG_EVENT_KEY_UP: u32 = 11;
const K_CG_EVENT_FLAGS_CHANGED: u32 = 12;

// CGEventTap constants
const K_CG_HEAD_INSERT_EVENT_TAP: u32 = 0;
const K_CG_EVENT_TAP_OPTION_LISTEN_ONLY: u32 = 1;

// Event mask: KeyDown | KeyUp | FlagsChanged
const EVENT_MASK: u64 = (1 << K_CG_EVENT_KEY_DOWN as u64)
    | (1 << K_CG_EVENT_KEY_UP as u64)
    | (1 << K_CG_EVENT_FLAGS_CHANGED as u64);

/// Modifier key codes that use FlagsChanged events
const MODIFIER_KEYCODES: &[u16] = &[
    KC_SHIFT_LEFT, KC_SHIFT_RIGHT,
    KC_CONTROL_LEFT, KC_CONTROL_RIGHT,
    KC_ALT_LEFT, KC_ALT_RIGHT,
    KC_META_LEFT, KC_META_RIGHT,
    KC_CAPS_LOCK, KC_FUNCTION,
];

fn is_modifier_keycode(code: u16) -> bool {
    MODIFIER_KEYCODES.contains(&code)
}

// FFI declarations for CGEventTap (avoid going through rdev which calls TSM APIs)
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
        tap: u32,  // CGEventTapLocation::HID = 0
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
    event_tx: mpsc::UnboundedSender<HotkeyEvent>,
    last_generation: u64,
    states: Vec<HkState>,
    modifiers_held: HashSet<u16>,
    /// Track which modifier keys are currently pressed (for FlagsChanged press/release detection)
    prev_modifier_flags: HashSet<u16>,
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

    // Check if config has changed (brief lock)
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
            tracing::debug!(
                generation = state.last_generation,
                combo_count = state.states.len(),
                "hotkey config reloaded in callback"
            );
        }
    }

    if state.states.is_empty() {
        return cg_event;
    }

    let keycode = cg_event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE) as u16;

    match event_type {
        K_CG_EVENT_KEY_DOWN => {
            handle_key_press(state, keycode);
        }
        K_CG_EVENT_KEY_UP => {
            handle_key_release(state, keycode);
        }
        K_CG_EVENT_FLAGS_CHANGED => {
            // FlagsChanged fires for both press and release of modifier keys.
            // We detect press/release by tracking which modifiers we've seen.
            if is_modifier_keycode(keycode) {
                if state.prev_modifier_flags.contains(&keycode) {
                    // Was pressed, now released
                    state.prev_modifier_flags.remove(&keycode);
                    handle_key_release(state, keycode);
                } else {
                    // Newly pressed
                    state.prev_modifier_flags.insert(keycode);
                    handle_key_press(state, keycode);
                }
            }
        }
        _ => {}
    }

    cg_event
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
                tracing::debug!("Hotkey event: Pressed");
                let _ = state.event_tx.send(HotkeyEvent::Pressed);
            }
        } else if keycode == hk.key {
            if hk.required_modifiers.is_subset(&state.modifiers_held) && !hk.active {
                hk.active = true;
                tracing::debug!("Hotkey event: Pressed (combo)");
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
                tracing::debug!("Hotkey event: Released");
                let _ = state.event_tx.send(HotkeyEvent::Released);
            }
        } else if hk.required_modifiers.contains(&keycode) {
            hk.active = false;
            tracing::debug!("Hotkey event: Released (modifier up)");
            let _ = state.event_tx.send(HotkeyEvent::Released);
        } else if keycode == hk.key {
            hk.active = false;
            tracing::debug!("Hotkey event: Released (key up)");
            let _ = state.event_tx.send(HotkeyEvent::Released);
        }
    }
}

/// Start the hotkey listener on a dedicated OS thread using a native CGEventTap.
/// This avoids rdev's TSM API calls which crash when called from a non-main thread.
/// The listener is persistent — update hotkeys via `SharedHotkeyConfig::update()`.
pub fn start_listener(
    config: SharedHotkeyConfig,
    event_tx: mpsc::UnboundedSender<HotkeyEvent>,
) -> Result<std::thread::JoinHandle<()>> {
    tracing::info!("Starting macOS hotkey listener (requires Accessibility permission)");

    let handle = std::thread::Builder::new()
        .name("hotkey-listener".into())
        .spawn(move || {
            let mut cb_state = Box::new(CallbackState {
                config,
                event_tx,
                last_generation: u64::MAX,
                states: Vec::new(),
                modifiers_held: HashSet::new(),
                prev_modifier_flags: HashSet::new(),
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
    fn test_parse_key_f5() {
        assert_eq!(parse_key("KEY_F5").unwrap(), KC_F5);
        assert_eq!(parse_key("KEY_F5").unwrap(), 96);
    }

    #[test]
    fn test_parse_key_rightctrl() {
        assert_eq!(parse_key("KEY_RIGHTCTRL").unwrap(), KC_CONTROL_RIGHT);
        assert_eq!(parse_key("KEY_RIGHTCTRL").unwrap(), 62);
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
    fn test_parse_hotkey_single_key() {
        let combo = parse_hotkey("KEY_F5").unwrap();
        assert!(combo.modifiers.is_empty());
        assert_eq!(combo.key, KC_F5);
    }

    #[test]
    fn test_parse_hotkey_two_part_combo() {
        let combo = parse_hotkey("KEY_LEFTCTRL+KEY_F5").unwrap();
        assert_eq!(combo.modifiers.len(), 1);
        assert_eq!(combo.modifiers[0], KC_CONTROL_LEFT);
        assert_eq!(combo.key, KC_F5);
    }

    #[test]
    fn test_parse_hotkey_three_part_combo() {
        let combo = parse_hotkey("KEY_LEFTCTRL+KEY_LEFTSHIFT+KEY_A").unwrap();
        assert_eq!(combo.modifiers.len(), 2);
        assert_eq!(combo.modifiers[0], KC_CONTROL_LEFT);
        assert_eq!(combo.modifiers[1], KC_SHIFT_LEFT);
        assert_eq!(combo.key, KC_A);
    }

    #[test]
    fn test_parse_hotkey_too_many_parts_errors() {
        let result = parse_hotkey("KEY_A+KEY_B+KEY_C+KEY_D");
        assert!(result.is_err());
    }

    #[test]
    fn test_shared_hotkey_config_update() {
        let config = SharedHotkeyConfig::new(vec![]);
        let gen_before = config.0.lock().unwrap().generation;

        config.update(vec![HotkeyCombo { modifiers: vec![], key: KC_F5 }]);
        let gen_after = config.0.lock().unwrap().generation;
        assert!(gen_after > gen_before);
        assert_eq!(config.0.lock().unwrap().combos.len(), 1);
    }

    #[test]
    fn test_handle_key_press_simple() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let config = SharedHotkeyConfig::new(vec![HotkeyCombo { modifiers: vec![], key: KC_F5 }]);
        let mut state = CallbackState {
            config,
            event_tx: tx,
            last_generation: u64::MAX,
            states: vec![HkState {
                required_modifiers: HashSet::new(),
                key: KC_F5,
                active: false,
            }],
            modifiers_held: HashSet::new(),
            prev_modifier_flags: HashSet::new(),
        };

        handle_key_press(&mut state, KC_F5);
        assert!(rx.try_recv().is_ok()); // Should receive Pressed

        handle_key_release(&mut state, KC_F5);
        assert!(rx.try_recv().is_ok()); // Should receive Released
    }

    #[test]
    fn test_handle_key_press_combo_requires_modifier() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let config = SharedHotkeyConfig::new(vec![]);
        let mut state = CallbackState {
            config,
            event_tx: tx,
            last_generation: u64::MAX,
            states: vec![HkState {
                required_modifiers: [KC_CONTROL_LEFT].into_iter().collect(),
                key: KC_F5,
                active: false,
            }],
            modifiers_held: HashSet::new(),
            prev_modifier_flags: HashSet::new(),
        };

        // Press key without modifier -- no event
        handle_key_press(&mut state, KC_F5);
        assert!(rx.try_recv().is_err());

        // Press modifier then key -- should fire
        handle_key_press(&mut state, KC_CONTROL_LEFT);
        handle_key_press(&mut state, KC_F5);
        assert!(rx.try_recv().is_ok());
    }

    #[test]
    fn test_parse_key_letters_and_numbers() {
        assert_eq!(parse_key("KEY_A").unwrap(), KC_A);
        assert_eq!(parse_key("KEY_Z").unwrap(), KC_Z);
        assert_eq!(parse_key("KEY_0").unwrap(), KC_0);
        assert_eq!(parse_key("KEY_9").unwrap(), KC_9);
    }

    #[test]
    fn test_is_modifier_keycode() {
        assert!(is_modifier_keycode(KC_SHIFT_LEFT));
        assert!(is_modifier_keycode(KC_CONTROL_RIGHT));
        assert!(is_modifier_keycode(KC_ALT_LEFT));
        assert!(!is_modifier_keycode(KC_F5));
        assert!(!is_modifier_keycode(KC_A));
    }
}
