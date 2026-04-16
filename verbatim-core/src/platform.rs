use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayServer {
    Wayland,
    X11,
    MacOS,
    Unknown,
}

pub fn detect_display_server() -> DisplayServer {
    tracing::debug!("detecting display server");

    #[cfg(target_os = "macos")]
    {
        tracing::debug!(server = ?DisplayServer::MacOS, "detected display server");
        return DisplayServer::MacOS;
    }

    #[cfg(not(target_os = "macos"))]
    {
        let wayland = std::env::var("WAYLAND_DISPLAY").is_ok();
        let x11 = std::env::var("DISPLAY").is_ok();
        tracing::trace!(WAYLAND_DISPLAY = wayland, DISPLAY = x11, "env var check");

        let server = if wayland {
            DisplayServer::Wayland
        } else if x11 {
            DisplayServer::X11
        } else {
            DisplayServer::Unknown
        };
        tracing::debug!(?server, "detected display server");
        server
    }
}

fn has_command(cmd: &str) -> bool {
    let result = Command::new("which")
        .arg(cmd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    tracing::trace!(cmd, available = result, "command check");
    result
}

pub fn check_input_requirements(display: &DisplayServer) -> Vec<String> {
    let ds = display;
    tracing::debug!("checking input requirements for {:?}", ds);
    let mut warnings = Vec::new();

    match display {
        DisplayServer::MacOS => {
            // No external tool dependencies on macOS.
            // Accessibility permission is checked at runtime by rdev.
        }
        DisplayServer::Wayland => {
            if !has_command("wtype") {
                warnings.push(
                    "Wayland detected but 'wtype' is not installed. Text input will not work.\n      Install with: sudo apt install wtype".to_string(),
                );
            }
        }
        DisplayServer::X11 => {
            if !has_command("xdotool") {
                warnings.push(
                    "X11 detected but 'xdotool' is not installed. Text input may not work.\n      Install with: sudo apt install xdotool".to_string(),
                );
            }
        }
        DisplayServer::Unknown => {
            warnings.push(
                "Could not detect display server (neither WAYLAND_DISPLAY nor DISPLAY is set). Text input may not work.".to_string(),
            );
        }
    }

    tracing::debug!(warning_count = warnings.len(), "input requirements check complete");
    warnings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(target_os = "macos")]
    fn test_detect_display_server_returns_macos() {
        assert_eq!(detect_display_server(), DisplayServer::MacOS);
    }

    #[test]
    fn test_check_input_requirements_macos_empty() {
        let warnings = check_input_requirements(&DisplayServer::MacOS);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_check_input_requirements_unknown_has_warning() {
        let warnings = check_input_requirements(&DisplayServer::Unknown);
        assert!(!warnings.is_empty());
        assert!(warnings[0].contains("Could not detect display server"));
    }

    // ── Edge case tests ──────────────────────────────────

    #[test]
    #[cfg(not(target_os = "macos"))]
    fn test_detect_display_server_returns_variant() {
        let server = detect_display_server();
        // On Linux CI/dev, should return Wayland, X11, or Unknown — never MacOS
        assert_ne!(server, DisplayServer::MacOS);
    }

    #[test]
    #[cfg(not(target_os = "macos"))]
    fn test_check_input_requirements_linux() {
        // Test both Wayland and X11 requirements produce meaningful results
        let wayland_warnings = check_input_requirements(&DisplayServer::Wayland);
        let x11_warnings = check_input_requirements(&DisplayServer::X11);
        // These may or may not have warnings depending on installed tools,
        // but they should not panic
        let _ = wayland_warnings;
        let _ = x11_warnings;
    }

    #[test]
    fn test_display_server_variants_distinct() {
        assert_ne!(DisplayServer::Wayland, DisplayServer::X11);
        assert_ne!(DisplayServer::Wayland, DisplayServer::MacOS);
        assert_ne!(DisplayServer::Wayland, DisplayServer::Unknown);
        assert_ne!(DisplayServer::X11, DisplayServer::MacOS);
        assert_ne!(DisplayServer::X11, DisplayServer::Unknown);
        assert_ne!(DisplayServer::MacOS, DisplayServer::Unknown);
    }
}
