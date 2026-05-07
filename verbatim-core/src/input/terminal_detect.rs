//! Detection of common Linux terminal emulators by WM_CLASS / app_id.
//!
//! Used at paste time to auto-create a per-app paste rule when the user
//! pastes into a known terminal that they haven't configured yet.

/// Default paste shortcut applied when an unconfigured terminal is detected.
pub const DEFAULT_TERMINAL_PASTE_COMMAND: &str = "ctrl+shift+v";

/// Known Linux terminal emulator class identifiers. Match is performed
/// case-insensitively against the full active-window class string. We list
/// every variant we've seen in the wild because reverse-DNS forms
/// (`com.mitchellh.ghostty`) and short forms (`ghostty`) coexist across
/// X11/Wayland and across desktop environments.
const TERMINAL_CLASSES: &[&str] = &[
    // Ghostty
    "com.mitchellh.ghostty",
    "ghostty",
    // GNOME Terminal
    "org.gnome.terminal",
    "gnome-terminal-server",
    "gnome-terminal",
    // GNOME Console (king's-cross)
    "org.gnome.console",
    "kgx",
    // Konsole
    "org.kde.konsole",
    "konsole",
    // Alacritty
    "org.alacritty",
    "alacritty",
    // Kitty
    "kitty",
    "xterm-kitty",
    // WezTerm
    "org.wezfurlong.wezterm",
    "wezterm",
    // foot
    "org.codeberg.dnkl.foot",
    "foot",
    "footclient",
    // xterm and friends
    "xterm",
    "uxterm",
    "urxvt",
    "rxvt",
    "rxvt-unicode",
    // Terminator
    "terminator",
    // Tilix
    "com.gexperts.tilix",
    "tilix",
    // XFCE Terminal
    "xfce4-terminal",
    "org.xfce.terminal",
    // Elementary Terminal
    "io.elementary.terminal",
    "pantheon-terminal",
    // Deepin Terminal
    "deepin-terminal",
    // LXTerminal
    "lxterminal",
    // Tilda / Guake (drop-down)
    "tilda",
    "guake",
    // Termite (legacy)
    "termite",
    // suckless st
    "st",
    "st-256color",
    // Hyper
    "hyper",
    // Cool Retro Term
    "cool-retro-term",
    // QTerminal (LXQt)
    "qterminal",
    "lxqt-qterminal",
    // Mlterm
    "mlterm",
    // Terminology (Enlightenment)
    "terminology",
    // Black Box
    "com.raggesilver.blackbox",
    "blackbox",
    // Contour
    "org.contourterminal.contour",
    "contour",
    // Rio
    "rio",
];

/// Returns `true` if `active_class` exactly matches a known Linux terminal
/// (case-insensitive). Substring match is intentionally avoided here: this
/// drives auto-rule creation, and a false positive would silently install a
/// non-functional shortcut into a non-terminal app.
pub fn is_known_linux_terminal(active_class: &str) -> bool {
    let lower = active_class.trim().to_ascii_lowercase();
    if lower.is_empty() {
        return false;
    }
    TERMINAL_CLASSES.iter().any(|t| t.eq_ignore_ascii_case(&lower))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ghostty_reverse_dns_matches() {
        assert!(is_known_linux_terminal("com.mitchellh.ghostty"));
    }

    #[test]
    fn test_ghostty_short_name_matches() {
        assert!(is_known_linux_terminal("ghostty"));
    }

    #[test]
    fn test_match_is_case_insensitive() {
        assert!(is_known_linux_terminal("Alacritty"));
        assert!(is_known_linux_terminal("ALACRITTY"));
        assert!(is_known_linux_terminal("Org.Kde.Konsole"));
    }

    #[test]
    fn test_match_trims_whitespace() {
        assert!(is_known_linux_terminal("  kitty  "));
    }

    #[test]
    fn test_unknown_app_does_not_match() {
        assert!(!is_known_linux_terminal("firefox"));
        assert!(!is_known_linux_terminal("code"));
        assert!(!is_known_linux_terminal(""));
    }

    #[test]
    fn test_substring_does_not_falsely_match() {
        // We explicitly use exact match — "kittyhawk" should not match "kitty".
        assert!(!is_known_linux_terminal("kittyhawk"));
        assert!(!is_known_linux_terminal("not-ghostty"));
    }

    #[test]
    fn test_default_command_is_ctrl_shift_v() {
        assert_eq!(DEFAULT_TERMINAL_PASTE_COMMAND, "ctrl+shift+v");
    }
}
