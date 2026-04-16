use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayServer {
    Wayland,
    X11,
    Unknown,
}

pub fn detect_display_server() -> DisplayServer {
    if std::env::var("WAYLAND_DISPLAY").is_ok() {
        DisplayServer::Wayland
    } else if std::env::var("DISPLAY").is_ok() {
        DisplayServer::X11
    } else {
        DisplayServer::Unknown
    }
}

fn has_command(cmd: &str) -> bool {
    Command::new("which")
        .arg(cmd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn check_input_requirements(display: &DisplayServer) -> Vec<String> {
    let mut warnings = Vec::new();

    match display {
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

    warnings
}
