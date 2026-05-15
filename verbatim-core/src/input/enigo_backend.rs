#[cfg(not(target_os = "macos"))]
use std::process::Command;

#[cfg(not(target_os = "macos"))]
use enigo::{Enigo, Key, Keyboard, Settings};

#[cfg(not(target_os = "macos"))]
use super::window_detect::{has_command, is_wayland};
use super::window_detect::get_active_window_class;
use super::InputMethod;
use crate::config::{OutputMode, PasteRule};
use crate::errors::InputError;

// =============================================================================
// macOS: CGEvent-based paste (thread-safe, bypasses TSM/HIToolbox)
// =============================================================================
// enigo calls TSMGetInputSourceProperty which asserts it's on the main dispatch
// queue. The STT service runs on a background thread, so enigo crashes with
// EXC_BREAKPOINT. We use CGEvent APIs directly — they are thread-safe.
// =============================================================================
#[cfg(target_os = "macos")]
mod macos_cgevent {
    use crate::errors::InputError;
    use std::ffi::c_void;

    type CGEventSourceRef = *mut c_void;
    type CGEventRef = *mut c_void;

    // CGEventSourceStateID
    const K_CG_EVENT_SOURCE_STATE_HID: i32 = 1;
    // CGEventTapLocation
    const K_CG_HID_EVENT_TAP: i32 = 0;

    // CGEventFlags bitmasks
    const K_CG_EVENT_FLAG_MASK_SHIFT: u64 = 1 << 17;
    const K_CG_EVENT_FLAG_MASK_CONTROL: u64 = 1 << 18;
    const K_CG_EVENT_FLAG_MASK_ALTERNATE: u64 = 1 << 19;
    const K_CG_EVENT_FLAG_MASK_COMMAND: u64 = 1 << 20;

    extern "C" {
        fn CGEventSourceCreate(stateID: i32) -> CGEventSourceRef;
        fn CGEventCreateKeyboardEvent(
            source: CGEventSourceRef,
            virtual_key: u16,
            key_down: bool,
        ) -> CGEventRef;
        fn CGEventPost(tap: i32, event: CGEventRef);
        fn CGEventSetFlags(event: CGEventRef, flags: u64);
        fn CGEventKeyboardSetUnicodeString(
            event: CGEventRef,
            string_length: usize,
            unicode_string: *const u16,
        );
        fn CFRelease(cf: *mut c_void);
    }

    // macOS virtual key codes (subset needed for paste commands)
    pub fn name_to_macos_keycode(name: &str) -> Option<u16> {
        match name.to_lowercase().as_str() {
            "a" => Some(0),
            "s" => Some(1),
            "d" => Some(2),
            "f" => Some(3),
            "h" => Some(4),
            "g" => Some(5),
            "z" => Some(6),
            "x" => Some(7),
            "c" => Some(8),
            "v" => Some(9),
            "b" => Some(11),
            "q" => Some(12),
            "w" => Some(13),
            "e" => Some(14),
            "r" => Some(15),
            "y" => Some(16),
            "t" => Some(17),
            "1" => Some(18),
            "2" => Some(19),
            "3" => Some(20),
            "4" => Some(21),
            "5" => Some(23),
            "6" => Some(22),
            "7" => Some(26),
            "8" => Some(28),
            "9" => Some(25),
            "0" => Some(29),
            "n" => Some(45),
            "m" => Some(46),
            "i" => Some(34),
            "j" => Some(38),
            "k" => Some(40),
            "l" => Some(37),
            "o" => Some(31),
            "p" => Some(35),
            "u" => Some(32),
            "return" | "enter" => Some(36),
            "tab" => Some(48),
            "space" => Some(49),
            "backspace" => Some(51),
            "delete" => Some(117),
            "insert" => Some(114),
            "home" => Some(115),
            "end" => Some(119),
            "pageup" => Some(116),
            "pagedown" => Some(121),
            "up" => Some(126),
            "down" => Some(125),
            "left" => Some(123),
            "right" => Some(124),
            "f1" => Some(122),
            "f2" => Some(120),
            "f3" => Some(99),
            "f4" => Some(118),
            "f5" => Some(96),
            "f6" => Some(97),
            "f7" => Some(98),
            "f8" => Some(100),
            "f9" => Some(101),
            "f10" => Some(109),
            "f11" => Some(103),
            "f12" => Some(111),
            _ => None,
        }
    }

    pub fn modifier_to_cgflag(name: &str) -> u64 {
        match name {
            "ctrl" | "control" => K_CG_EVENT_FLAG_MASK_CONTROL,
            "shift" => K_CG_EVENT_FLAG_MASK_SHIFT,
            "alt" | "option" => K_CG_EVENT_FLAG_MASK_ALTERNATE,
            "meta" | "super" | "win" | "cmd" | "command" => K_CG_EVENT_FLAG_MASK_COMMAND,
            _ => 0,
        }
    }

    /// Type `text` by posting synthetic key events carrying its Unicode payload.
    /// CGEventKeyboardSetUnicodeString is thread-safe (unlike enigo/TSM), so this is
    /// the safe path from the STT background thread.
    pub fn type_unicode_string(text: &str) -> Result<(), InputError> {
        if text.is_empty() {
            return Ok(());
        }
        // Apple recommends batches of <= 20 UTF-16 code units per event.
        let utf16: Vec<u16> = text.encode_utf16().collect();
        unsafe {
            let source = CGEventSourceCreate(K_CG_EVENT_SOURCE_STATE_HID);
            if source.is_null() {
                return Err(InputError::SimulationFailed(
                    "Failed to create CGEventSource".into(),
                ));
            }
            for chunk in utf16.chunks(20) {
                // virtual_key=0 is fine; CGEventKeyboardSetUnicodeString overrides the payload.
                let key_down = CGEventCreateKeyboardEvent(source, 0, true);
                if key_down.is_null() {
                    CFRelease(source);
                    return Err(InputError::SimulationFailed(
                        "Failed to create key down event".into(),
                    ));
                }
                CGEventKeyboardSetUnicodeString(key_down, chunk.len(), chunk.as_ptr());
                CGEventPost(K_CG_HID_EVENT_TAP, key_down);

                let key_up = CGEventCreateKeyboardEvent(source, 0, false);
                if key_up.is_null() {
                    CFRelease(key_down);
                    CFRelease(source);
                    return Err(InputError::SimulationFailed(
                        "Failed to create key up event".into(),
                    ));
                }
                CGEventKeyboardSetUnicodeString(key_up, chunk.len(), chunk.as_ptr());
                CGEventPost(K_CG_HID_EVENT_TAP, key_up);

                CFRelease(key_up);
                CFRelease(key_down);
            }
            CFRelease(source);
        }
        Ok(())
    }

    pub fn paste_via_cgevent(modifiers: &[String], key_name: &str) -> Result<(), InputError> {
        let keycode = name_to_macos_keycode(key_name).ok_or_else(|| {
            InputError::SimulationFailed(format!("Unknown macOS key: {}", key_name))
        })?;

        let combined_flags: u64 = modifiers.iter().map(|m| modifier_to_cgflag(m)).sum();

        unsafe {
            let source = CGEventSourceCreate(K_CG_EVENT_SOURCE_STATE_HID);
            if source.is_null() {
                return Err(InputError::SimulationFailed(
                    "Failed to create CGEventSource".into(),
                ));
            }

            // Key down with modifier flags
            let key_down = CGEventCreateKeyboardEvent(source, keycode, true);
            if key_down.is_null() {
                CFRelease(source);
                return Err(InputError::SimulationFailed(
                    "Failed to create key down event".into(),
                ));
            }
            if combined_flags != 0 {
                CGEventSetFlags(key_down, combined_flags);
            }
            CGEventPost(K_CG_HID_EVENT_TAP, key_down);

            // Key up
            let key_up = CGEventCreateKeyboardEvent(source, keycode, false);
            if key_up.is_null() {
                CFRelease(key_down);
                CFRelease(source);
                return Err(InputError::SimulationFailed(
                    "Failed to create key up event".into(),
                ));
            }
            if combined_flags != 0 {
                CGEventSetFlags(key_up, combined_flags);
            }
            CGEventPost(K_CG_HID_EVENT_TAP, key_up);

            CFRelease(key_up);
            CFRelease(key_down);
            CFRelease(source);
        }

        Ok(())
    }
}

/// The configured input method preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Method {
    Auto,
    Enigo,
    Wtype,
}

impl Method {
    fn from_config(s: &str) -> Self {
        match s {
            "enigo" => Method::Enigo,
            "wtype" => Method::Wtype,
            _ => Method::Auto,
        }
    }
}

/// A parsed paste command: modifier keys held down while the final key is clicked.
struct PasteKeys {
    modifiers: Vec<String>,
    key: String,
}

/// Parse a paste command string like "ctrl+shift+v" or "shift+Insert".
/// Everything before the last `+` is a modifier, the last part is the key to click.
fn parse_paste_command(cmd: &str) -> PasteKeys {
    let parts: Vec<&str> = cmd.split('+').map(|s| s.trim()).collect();
    if parts.is_empty() {
        return PasteKeys {
            modifiers: vec!["shift".into()],
            key: "Insert".into(),
        };
    }
    let (mods, key) = parts.split_at(parts.len() - 1);
    PasteKeys {
        modifiers: mods.iter().map(|s| s.to_lowercase()).collect(),
        key: key[0].to_string(),
    }
}

/// Map a modifier name to an enigo Key.
#[cfg(not(target_os = "macos"))]
fn modifier_to_enigo_key(name: &str) -> Option<Key> {
    match name {
        "ctrl" | "control" => Some(Key::Control),
        "shift" => Some(Key::Shift),
        "alt" => Some(Key::Alt),
        "meta" | "super" | "win" => Some(Key::Meta),
        _ => None,
    }
}

/// Map a key name to an enigo Key.
#[cfg(not(target_os = "macos"))]
fn name_to_enigo_key(name: &str) -> Key {
    match name.to_lowercase().as_str() {
        #[cfg(target_os = "linux")]
        "insert" => Key::Other(0xff63),     // XK_Insert
        #[cfg(not(target_os = "linux"))]
        "insert" => Key::Other(0),          // Insert not available on macOS keyboards
        "return" | "enter" => Key::Return,
        "tab" => Key::Tab,
        "space" => Key::Space,
        "backspace" => Key::Backspace,
        "delete" => Key::Delete,
        "escape" | "esc" => Key::Escape,
        "up" => Key::UpArrow,
        "down" => Key::DownArrow,
        "left" => Key::LeftArrow,
        "right" => Key::RightArrow,
        "home" => Key::Home,
        "end" => Key::End,
        "pageup" => Key::PageUp,
        "pagedown" => Key::PageDown,
        "f1" => Key::F1,
        "f2" => Key::F2,
        "f3" => Key::F3,
        "f4" => Key::F4,
        "f5" => Key::F5,
        "f6" => Key::F6,
        "f7" => Key::F7,
        "f8" => Key::F8,
        "f9" => Key::F9,
        "f10" => Key::F10,
        "f11" => Key::F11,
        "f12" => Key::F12,
        s if s.len() == 1 => Key::Unicode(s.chars().next().unwrap()),
        _ => Key::Unicode(name.chars().next().unwrap_or('v')),
    }
}

pub struct EnigoBackend {
    #[cfg(not(target_os = "macos"))]
    enigo: Enigo,
    method: Method,
    paste_command: String,
    paste_rules: Vec<PasteRule>,
    default_output_mode: OutputMode,
}

impl EnigoBackend {
    pub fn new(
        input_method: &str,
        paste_command: &str,
        paste_rules: &[PasteRule],
        default_output_mode: OutputMode,
    ) -> Result<Self, InputError> {
        tracing::debug!(
            input_method,
            paste_command,
            paste_rules_count = paste_rules.len(),
            ?default_output_mode,
            "creating EnigoBackend"
        );

        Ok(Self {
            // On macOS, we use CGEvent directly (enigo calls TSM which crashes on non-main thread)
            #[cfg(not(target_os = "macos"))]
            enigo: Enigo::new(&Settings::default())
                .map_err(|e| InputError::SimulationFailed(e.to_string()))?,
            method: Method::from_config(input_method),
            paste_command: paste_command.to_string(),
            paste_rules: paste_rules.to_vec(),
            default_output_mode,
        })
    }

    /// Resolve the output strategy (mode + paste shortcut) for the currently
    /// focused app. Per-app rules take precedence over the global defaults.
    fn resolve_output(&self) -> (OutputMode, String) {
        tracing::trace!(rules_count = self.paste_rules.len(), "resolving output strategy");
        if !self.paste_rules.is_empty() {
            if let Some(class) = get_active_window_class() {
                let class_lower = class.to_lowercase();
                for rule in &self.paste_rules {
                    if class_lower.contains(&rule.app_class.to_lowercase()) {
                        tracing::debug!(
                            app = %class,
                            cmd = %rule.paste_command,
                            mode = ?rule.output_mode,
                            "matched per-app rule"
                        );
                        return (rule.output_mode, rule.paste_command.clone());
                    }
                }
            }
        }
        tracing::trace!(default = %self.paste_command, mode = ?self.default_output_mode, "using global default");
        (self.default_output_mode, self.paste_command.clone())
    }
}

/// Execute the paste command via wtype (Wayland).
#[cfg(not(target_os = "macos"))]
fn paste_via_wtype(paste: &PasteKeys) -> Result<(), InputError> {
    tracing::debug!(modifiers = ?paste.modifiers, key = %paste.key, "attempting paste via wtype");
    if !has_command("wtype") {
        return Err(InputError::SimulationFailed(
            "wtype is not installed (sudo apt install wtype)".into(),
        ));
    }

    let mut args: Vec<String> = Vec::new();
    for m in &paste.modifiers {
        args.push("-M".into());
        args.push(m.clone());
    }
    args.push("-k".into());
    args.push(paste.key.clone());
    for m in paste.modifiers.iter().rev() {
        args.push("-m".into());
        args.push(m.clone());
    }

    let status = Command::new("wtype")
        .args(&args)
        .status()
        .map_err(|e| InputError::SimulationFailed(format!("wtype: {}", e)))?;

    if status.success() {
        tracing::debug!("Pasted via wtype");
        Ok(())
    } else {
        Err(InputError::SimulationFailed("wtype paste failed".into()))
    }
}

/// Execute the paste command via enigo (X11).
#[cfg(not(target_os = "macos"))]
fn paste_via_enigo(enigo: &mut Enigo, paste: &PasteKeys) -> Result<(), InputError> {
    tracing::debug!(modifiers = ?paste.modifiers, key = %paste.key, "attempting paste via enigo");
    // Press modifiers
    for m in &paste.modifiers {
        if let Some(key) = modifier_to_enigo_key(m) {
            enigo
                .key(key, enigo::Direction::Press)
                .map_err(|e| InputError::SimulationFailed(e.to_string()))?;
        }
    }

    // Click the key
    let key = name_to_enigo_key(&paste.key);
    enigo
        .key(key, enigo::Direction::Click)
        .map_err(|e| InputError::SimulationFailed(e.to_string()))?;

    // Release modifiers in reverse
    for m in paste.modifiers.iter().rev() {
        if let Some(key) = modifier_to_enigo_key(m) {
            enigo
                .key(key, enigo::Direction::Release)
                .map_err(|e| InputError::SimulationFailed(e.to_string()))?;
        }
    }

    Ok(())
}

/// Type the text directly via `wtype --` (no key combos), used on Wayland for OutputMode::Type.
#[cfg(not(target_os = "macos"))]
fn type_via_wtype(text: &str) -> Result<(), InputError> {
    tracing::debug!(chars = text.len(), "typing via wtype --");
    if !has_command("wtype") {
        return Err(InputError::SimulationFailed(
            "wtype is not installed (sudo apt install wtype)".into(),
        ));
    }
    let status = Command::new("wtype")
        .arg("--")
        .arg(text)
        .status()
        .map_err(|e| InputError::SimulationFailed(format!("wtype: {}", e)))?;
    if status.success() {
        Ok(())
    } else {
        Err(InputError::SimulationFailed("wtype typing failed".into()))
    }
}

impl InputMethod for EnigoBackend {
    fn type_text(&mut self, text: &str) -> Result<(), InputError> {
        tracing::debug!(chars = text.len(), method = ?self.method, "type_text called");
        let (mode, cmd) = self.resolve_output();

        // ── Type mode: synthesize keystrokes for each character ──────────
        if mode == OutputMode::Type {
            #[cfg(target_os = "macos")]
            {
                macos_cgevent::type_unicode_string(text)?;
                tracing::debug!("Typed {} chars via CGEvent Unicode", text.len());
                return Ok(());
            }
            #[cfg(not(target_os = "macos"))]
            return match self.method {
                Method::Wtype => type_via_wtype(text),
                Method::Enigo => {
                    self.enigo
                        .text(text)
                        .map_err(|e| InputError::SimulationFailed(e.to_string()))?;
                    tracing::debug!("Typed {} chars via enigo", text.len());
                    Ok(())
                }
                Method::Auto => {
                    if is_wayland() {
                        type_via_wtype(text)
                    } else {
                        self.enigo
                            .text(text)
                            .map_err(|e| InputError::SimulationFailed(e.to_string()))?;
                        tracing::debug!("Typed {} chars via enigo", text.len());
                        Ok(())
                    }
                }
            };
        }

        // ── Paste mode (default): send the paste shortcut ────────────────
        let _ = text; // text is already on clipboard
        let paste = parse_paste_command(&cmd);

        // On macOS, always use CGEvent to avoid TSM thread-safety crash (EXC_BREAKPOINT).
        // enigo calls TSMGetInputSourceProperty which asserts it's on the main dispatch queue.
        #[cfg(target_os = "macos")]
        {
            macos_cgevent::paste_via_cgevent(&paste.modifiers, &paste.key)?;
            tracing::debug!("Pasted {} chars via CGEvent ({})", text.len(), cmd);
            return Ok(());
        }

        #[cfg(not(target_os = "macos"))]
        match self.method {
            Method::Wtype => paste_via_wtype(&paste),
            Method::Enigo => {
                paste_via_enigo(&mut self.enigo, &paste)?;
                tracing::debug!("Pasted {} chars via enigo ({})", text.len(), cmd);
                Ok(())
            }
            Method::Auto => {
                let wayland = is_wayland();
                tracing::debug!(wayland, "auto-detecting input method");
                if wayland {
                    paste_via_wtype(&paste)
                } else {
                    paste_via_enigo(&mut self.enigo, &paste)?;
                    tracing::debug!("Pasted {} chars via enigo ({})", text.len(), cmd);
                    Ok(())
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_paste_command_meta_v() {
        let p = parse_paste_command("meta+v");
        assert_eq!(p.modifiers, vec!["meta"]);
        assert_eq!(p.key, "v");
    }

    #[test]
    fn test_parse_paste_command_shift_insert() {
        let p = parse_paste_command("shift+Insert");
        assert_eq!(p.modifiers, vec!["shift"]);
        assert_eq!(p.key, "Insert");
    }

    #[test]
    fn test_parse_paste_command_triple_combo() {
        let p = parse_paste_command("ctrl+shift+v");
        assert_eq!(p.modifiers, vec!["ctrl", "shift"]);
        assert_eq!(p.key, "v");
    }

    #[test]
    fn test_parse_paste_command_single_key() {
        let p = parse_paste_command("v");
        assert!(p.modifiers.is_empty());
        assert_eq!(p.key, "v");
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn test_modifier_to_enigo_key_known() {
        assert!(modifier_to_enigo_key("ctrl").is_some());
        assert!(modifier_to_enigo_key("shift").is_some());
        assert!(modifier_to_enigo_key("alt").is_some());
        assert!(modifier_to_enigo_key("meta").is_some());
        assert!(modifier_to_enigo_key("super").is_some());
        assert!(modifier_to_enigo_key("control").is_some());
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn test_modifier_to_enigo_key_unknown() {
        assert!(modifier_to_enigo_key("banana").is_none());
        assert!(modifier_to_enigo_key("").is_none());
    }

    #[test]
    fn test_method_from_config() {
        assert_eq!(Method::from_config("enigo"), Method::Enigo);
        assert_eq!(Method::from_config("wtype"), Method::Wtype);
        assert_eq!(Method::from_config("auto"), Method::Auto);
        assert_eq!(Method::from_config("anything"), Method::Auto);
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn test_name_to_enigo_key_common_keys() {
        assert!(matches!(name_to_enigo_key("return"), Key::Return));
        assert!(matches!(name_to_enigo_key("tab"), Key::Tab));
        assert!(matches!(name_to_enigo_key("space"), Key::Space));
        assert!(matches!(name_to_enigo_key("f1"), Key::F1));
        assert!(matches!(name_to_enigo_key("f12"), Key::F12));
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn test_name_to_enigo_key_single_char() {
        match name_to_enigo_key("v") {
            Key::Unicode(c) => assert_eq!(c, 'v'),
            _ => panic!("expected Unicode key"),
        }
    }

    // ── Edge case tests ──────────────────────────────────

    #[test]
    fn test_parse_paste_command_empty_string() {
        let p = parse_paste_command("");
        // Empty string: split('+') yields [""], which has len 1 so key=""
        assert!(p.modifiers.is_empty());
        assert_eq!(p.key, "");
    }

    #[test]
    fn test_parse_paste_command_whitespace_around_plus() {
        let p = parse_paste_command("ctrl + v");
        // Parts are trimmed by the implementation
        assert_eq!(p.modifiers, vec!["ctrl"]);
        assert_eq!(p.key, "v");
    }

    #[test]
    fn test_parse_paste_command_case_preserved_for_key() {
        let p = parse_paste_command("ctrl+V");
        assert_eq!(p.key, "V");
    }

    #[test]
    fn test_method_from_config_empty_string() {
        assert_eq!(Method::from_config(""), Method::Auto);
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn test_name_to_enigo_key_case_insensitive() {
        // The function lowercases input, so "Return" and "return" should both work
        assert!(matches!(name_to_enigo_key("Return"), Key::Return));
        assert!(matches!(name_to_enigo_key("RETURN"), Key::Return));
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn test_name_to_enigo_key_unknown_multichar() {
        // Unknown multi-char string falls through to Unicode of first char
        match name_to_enigo_key("banana") {
            Key::Unicode(c) => assert_eq!(c, 'b'),
            _ => panic!("expected Unicode key for unknown multi-char string"),
        }
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn test_modifier_to_enigo_key_win_variant() {
        // "win" should map to Meta key
        assert!(modifier_to_enigo_key("win").is_some());
    }

    #[cfg(target_os = "macos")]
    mod macos_tests {
        use super::super::macos_cgevent;

        #[test]
        fn test_name_to_macos_keycode_common() {
            assert_eq!(macos_cgevent::name_to_macos_keycode("v"), Some(9));
            assert_eq!(macos_cgevent::name_to_macos_keycode("c"), Some(8));
            assert_eq!(macos_cgevent::name_to_macos_keycode("a"), Some(0));
            assert_eq!(macos_cgevent::name_to_macos_keycode("insert"), Some(114));
            assert_eq!(macos_cgevent::name_to_macos_keycode("return"), Some(36));
            assert_eq!(macos_cgevent::name_to_macos_keycode("space"), Some(49));
        }

        #[test]
        fn test_name_to_macos_keycode_case_insensitive() {
            assert_eq!(macos_cgevent::name_to_macos_keycode("V"), Some(9));
            assert_eq!(macos_cgevent::name_to_macos_keycode("Insert"), Some(114));
            assert_eq!(macos_cgevent::name_to_macos_keycode("RETURN"), Some(36));
        }

        #[test]
        fn test_name_to_macos_keycode_unknown() {
            assert_eq!(macos_cgevent::name_to_macos_keycode("banana"), None);
        }

        #[test]
        fn test_modifier_to_cgflag() {
            assert_ne!(macos_cgevent::modifier_to_cgflag("meta"), 0);
            assert_ne!(macos_cgevent::modifier_to_cgflag("cmd"), 0);
            assert_ne!(macos_cgevent::modifier_to_cgflag("command"), 0);
            assert_ne!(macos_cgevent::modifier_to_cgflag("ctrl"), 0);
            assert_ne!(macos_cgevent::modifier_to_cgflag("shift"), 0);
            assert_ne!(macos_cgevent::modifier_to_cgflag("alt"), 0);
            assert_eq!(macos_cgevent::modifier_to_cgflag("banana"), 0);
        }

        #[test]
        fn test_modifier_flags_are_distinct() {
            let meta = macos_cgevent::modifier_to_cgflag("meta");
            let ctrl = macos_cgevent::modifier_to_cgflag("ctrl");
            let shift = macos_cgevent::modifier_to_cgflag("shift");
            let alt = macos_cgevent::modifier_to_cgflag("alt");
            // All flags should be distinct powers of 2
            assert_ne!(meta, ctrl);
            assert_ne!(meta, shift);
            assert_ne!(meta, alt);
            assert_ne!(ctrl, shift);
            // Combined flags should be the sum (no overlap)
            assert_eq!(meta | ctrl, meta + ctrl);
        }

        #[test]
        fn test_name_to_macos_keycode_function_keys() {
            assert_eq!(macos_cgevent::name_to_macos_keycode("f1"), Some(122));
            assert_eq!(macos_cgevent::name_to_macos_keycode("f5"), Some(96));
            assert_eq!(macos_cgevent::name_to_macos_keycode("f12"), Some(111));
        }
    }
}
