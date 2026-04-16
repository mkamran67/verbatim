use std::collections::BTreeSet;
use std::process::Command;

pub fn is_wayland() -> bool {
    std::env::var("WAYLAND_DISPLAY").is_ok()
}

pub fn has_command(cmd: &str) -> bool {
    Command::new("which")
        .arg(cmd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Detect the WM_CLASS / app_id of the currently focused window.
pub fn get_active_window_class() -> Option<String> {
    tracing::trace!("detecting active window class");

    #[cfg(target_os = "macos")]
    {
        let result = get_active_window_macos();
        tracing::debug!(class = ?result, "active window class (macOS)");
        return result;
    }

    #[cfg(not(target_os = "macos"))]
    {
        let result = if is_wayland() {
            tracing::trace!("using Wayland window detection");
            get_active_window_wayland()
        } else {
            tracing::trace!("using X11 window detection");
            get_active_window_x11()
        };
        tracing::debug!(class = ?result, "active window class");
        result
    }
}

#[cfg(target_os = "macos")]
fn get_active_window_macos() -> Option<String> {
    let output = Command::new("osascript")
        .args([
            "-e",
            "tell application \"System Events\" to get name of first application process whose frontmost is true",
        ])
        .output()
        .ok()?;
    if output.status.success() {
        let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if name.is_empty() { None } else { Some(name) }
    } else {
        None
    }
}

#[cfg(target_os = "linux")]
fn get_active_window_x11() -> Option<String> {
    let output = Command::new("xdotool")
        .args(["getactivewindow", "getwindowclassname"])
        .output()
        .ok()?;
    if output.status.success() {
        let class = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if class.is_empty() { None } else { Some(class) }
    } else {
        None
    }
}

#[cfg(target_os = "linux")]
fn get_active_window_wayland() -> Option<String> {
    tracing::trace!("trying Wayland window detection methods");
    if has_command("hyprctl") {
        tracing::trace!("trying hyprctl for active window");
        if let Some(class) = parse_hyprctl_active() {
            return Some(class);
        }
    }
    if has_command("swaymsg") {
        tracing::trace!("trying swaymsg for active window");
        if let Some(class) = parse_swaymsg_focused() {
            return Some(class);
        }
    }
    tracing::trace!("no Wayland window detection method succeeded");
    None
}

/// List unique window class names of all open windows.
/// Returns a sorted, deduplicated list.
pub fn list_open_windows() -> Vec<String> {
    tracing::debug!("listing open windows");
    #[cfg(target_os = "macos")]
    let classes = list_windows_macos();

    #[cfg(not(target_os = "macos"))]
    let classes = if is_wayland() {
        list_windows_wayland()
    } else {
        list_windows_x11()
    };

    let unique: BTreeSet<String> = classes.into_iter().filter(|s| !s.is_empty()).collect();
    tracing::debug!(count = unique.len(), "found unique window classes");
    unique.into_iter().collect()
}

#[cfg(target_os = "macos")]
fn list_windows_macos() -> Vec<String> {
    let output = Command::new("osascript")
        .args([
            "-e",
            "tell application \"System Events\" to get name of every application process whose visible is true",
        ])
        .output()
        .ok();
    match output {
        Some(out) if out.status.success() => {
            let text = String::from_utf8_lossy(&out.stdout);
            text.split(", ")
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        }
        _ => Vec::new(),
    }
}

#[cfg(target_os = "linux")]
fn list_windows_x11() -> Vec<String> {
    // Try wmctrl first (gives class names directly)
    if has_command("wmctrl") {
        if let Ok(output) = Command::new("wmctrl").args(["-l", "-x"]).output() {
            if output.status.success() {
                let text = String::from_utf8_lossy(&output.stdout);
                // wmctrl -l -x format: <id> <desktop> <class> <host> <title>
                // class field is like "navigator.Firefox" — we want the part after the dot
                let mut classes = Vec::new();
                for line in text.lines() {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 3 {
                        let class_field = parts[2];
                        // class_field is "instance.ClassName", take the part after the dot
                        let class = class_field
                            .rsplit('.')
                            .next()
                            .unwrap_or(class_field)
                            .to_string();
                        classes.push(class);
                    }
                }
                return classes;
            }
        }
    }

    // Fallback: xdotool search
    if has_command("xdotool") {
        if let Ok(output) = Command::new("xdotool")
            .args(["search", "--onlyvisible", "--name", ""])
            .output()
        {
            if output.status.success() {
                let text = String::from_utf8_lossy(&output.stdout);
                let mut classes = Vec::new();
                for window_id in text.lines() {
                    let window_id = window_id.trim();
                    if window_id.is_empty() {
                        continue;
                    }
                    if let Ok(class_out) = Command::new("xdotool")
                        .args(["getwindowclassname", window_id])
                        .output()
                    {
                        if class_out.status.success() {
                            let class = String::from_utf8_lossy(&class_out.stdout)
                                .trim()
                                .to_string();
                            if !class.is_empty() {
                                classes.push(class);
                            }
                        }
                    }
                }
                return classes;
            }
        }
    }

    Vec::new()
}

#[cfg(target_os = "linux")]
fn list_windows_wayland() -> Vec<String> {
    // Try hyprctl clients
    if has_command("hyprctl") {
        if let Ok(output) = Command::new("hyprctl")
            .args(["clients", "-j"])
            .output()
        {
            if output.status.success() {
                let json = String::from_utf8_lossy(&output.stdout);
                return parse_hyprctl_classes(&json);
            }
        }
    }

    // Try swaymsg
    if has_command("swaymsg") {
        if let Ok(output) = Command::new("swaymsg")
            .args(["-t", "get_tree"])
            .output()
        {
            if output.status.success() {
                let json = String::from_utf8_lossy(&output.stdout);
                return parse_swaymsg_app_ids(&json);
            }
        }
    }

    Vec::new()
}

/// Parse all "class" fields from hyprctl clients -j output.
#[cfg(target_os = "linux")]
fn parse_hyprctl_classes(json: &str) -> Vec<String> {
    let mut classes = Vec::new();
    let mut search_from = 0;
    while let Some(pos) = json[search_from..].find("\"class\"") {
        let abs_pos = search_from + pos;
        if let Some(value) = extract_json_string_value(&json[abs_pos..]) {
            if !value.is_empty() {
                classes.push(value);
            }
        }
        search_from = abs_pos + 7; // skip past "class"
    }
    classes
}

/// Parse all "app_id" fields from swaymsg -t get_tree output.
#[cfg(target_os = "linux")]
fn parse_swaymsg_app_ids(json: &str) -> Vec<String> {
    let mut ids = Vec::new();
    let mut search_from = 0;
    while let Some(pos) = json[search_from..].find("\"app_id\"") {
        let abs_pos = search_from + pos;
        if let Some(value) = extract_json_string_value(&json[abs_pos..]) {
            if !value.is_empty() && value != "null" {
                ids.push(value);
            }
        }
        search_from = abs_pos + 8;
    }
    ids
}

/// Extract a JSON string value from a `"key": "value"` fragment.
#[cfg(target_os = "linux")]
fn extract_json_string_value(fragment: &str) -> Option<String> {
    let colon = fragment.find(':')?;
    let after = fragment[colon + 1..].trim_start();
    if after.starts_with('"') {
        let inner = &after[1..];
        let end = inner.find('"')?;
        Some(inner[..end].to_string())
    } else {
        None
    }
}

/// Parse hyprctl activewindow -j for the focused window's class.
#[cfg(target_os = "linux")]
fn parse_hyprctl_active() -> Option<String> {
    let output = Command::new("hyprctl")
        .args(["activewindow", "-j"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let json = String::from_utf8_lossy(&output.stdout);
    if let Some(pos) = json.find("\"class\"") {
        extract_json_string_value(&json[pos..])
    } else {
        None
    }
}

/// Parse swaymsg -t get_tree for the focused window's app_id.
#[cfg(target_os = "linux")]
fn parse_swaymsg_focused() -> Option<String> {
    let output = Command::new("swaymsg")
        .args(["-t", "get_tree"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let json = String::from_utf8_lossy(&output.stdout);
    let pos = json.find("\"focused\":true")?;
    let before = &json[..pos];
    let app_id_pos = before.rfind("\"app_id\"")?;
    let value = extract_json_string_value(&before[app_id_pos..])?;
    if value.is_empty() || value == "null" {
        None
    } else {
        Some(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_wayland() {
        // Save original value
        let original = std::env::var("WAYLAND_DISPLAY").ok();

        unsafe { std::env::set_var("WAYLAND_DISPLAY", "wayland-0") };
        assert!(is_wayland());

        unsafe { std::env::remove_var("WAYLAND_DISPLAY") };
        assert!(!is_wayland());

        // Restore
        if let Some(val) = original {
            unsafe { std::env::set_var("WAYLAND_DISPLAY", val) };
        }
    }

    #[test]
    fn test_has_command_exists() {
        assert!(has_command("ls"));
    }

    #[test]
    fn test_has_command_missing() {
        assert!(!has_command("nonexistent_xyz_binary_12345"));
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_parse_hyprctl_classes_basic() {
        let json = r#"[{"class":"firefox","title":"Mozilla Firefox"},{"class":"kitty","title":"Terminal"}]"#;
        let classes = parse_hyprctl_classes(json);
        assert_eq!(classes, vec!["firefox", "kitty"]);
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_parse_hyprctl_classes_empty_filtered() {
        let json = r#"[{"class":"","title":"Empty"},{"class":"firefox","title":"FF"}]"#;
        let classes = parse_hyprctl_classes(json);
        assert_eq!(classes, vec!["firefox"]);
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_parse_hyprctl_classes_no_class_field() {
        let json = r#"[{"title":"No class here"}]"#;
        let classes = parse_hyprctl_classes(json);
        assert!(classes.is_empty());
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_parse_swaymsg_app_ids_basic() {
        let json = r#"{"nodes":[{"app_id":"foot"},{"app_id":"firefox"}]}"#;
        let ids = parse_swaymsg_app_ids(json);
        assert_eq!(ids, vec!["foot", "firefox"]);
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_parse_swaymsg_app_ids_filters_null() {
        let json = r#"{"nodes":[{"app_id":"null"},{"app_id":"foot"}]}"#;
        let ids = parse_swaymsg_app_ids(json);
        assert_eq!(ids, vec!["foot"]);
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_parse_swaymsg_app_ids_empty() {
        let json = r#"{}"#;
        let ids = parse_swaymsg_app_ids(json);
        assert!(ids.is_empty());
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_extract_json_string_value_basic() {
        let fragment = r#""class": "firefox""#;
        assert_eq!(extract_json_string_value(fragment), Some("firefox".to_string()));
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_extract_json_string_value_non_string() {
        let fragment = r#""focused": true"#;
        assert_eq!(extract_json_string_value(fragment), None);
    }

    // ── Edge case tests ──────────────────────────────────

    #[test]
    #[cfg(target_os = "linux")]
    fn test_extract_json_string_value_no_colon() {
        assert_eq!(extract_json_string_value("no colon here"), None);
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_extract_json_string_value_empty_string_value() {
        let fragment = r#""class": """#;
        assert_eq!(extract_json_string_value(fragment), Some("".to_string()));
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_extract_json_string_value_with_escaped_quote() {
        // The parser is naive and finds the first unescaped quote boundary
        let fragment = r#""class": "fire\"fox""#;
        // Will extract "fire\" — stops at the escaped quote's backslash-quote pair
        let result = extract_json_string_value(fragment);
        assert!(result.is_some(), "should parse something");
        // Documents that escaped quotes are NOT handled properly
        assert_ne!(result.unwrap(), "fire\"fox");
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_extract_json_string_value_spaces_around_colon() {
        let fragment = r#""class"  :   "firefox""#;
        assert_eq!(extract_json_string_value(fragment), Some("firefox".to_string()));
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_parse_hyprctl_classes_many_entries() {
        let entries: Vec<String> = (0..100)
            .map(|i| format!(r#"{{"class":"app{}","title":"Title {}"}}"#, i, i))
            .collect();
        let json = format!("[{}]", entries.join(","));
        let classes = parse_hyprctl_classes(&json);
        assert_eq!(classes.len(), 100);
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_parse_swaymsg_app_ids_deeply_nested() {
        let json = r#"{"nodes":[{"nodes":[{"app_id":"deep"}]},{"app_id":"shallow"}]}"#;
        let ids = parse_swaymsg_app_ids(json);
        assert!(ids.contains(&"deep".to_string()));
        assert!(ids.contains(&"shallow".to_string()));
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_extract_json_string_value_number_value() {
        let fragment = r#""pid": 1234"#;
        assert_eq!(extract_json_string_value(fragment), None);
    }
}
