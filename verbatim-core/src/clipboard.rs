use arboard::Clipboard;
use crate::errors::InputError;

/// Read the current clipboard text, if any.
pub fn get_clipboard_text() -> Option<String> {
    tracing::trace!("reading current clipboard text");
    let result = Clipboard::new()
        .ok()
        .and_then(|mut cb| cb.get_text().ok());
    tracing::trace!(chars = result.as_ref().map(|t| t.len()), "clipboard read result");
    result
}

/// Copy text to the system clipboard.
pub fn copy_to_clipboard(text: &str) -> Result<(), InputError> {
    tracing::trace!(chars = text.len(), "copying text to clipboard");
    let mut clipboard = Clipboard::new()
        .map_err(|e| InputError::ClipboardError(e.to_string()))?;

    clipboard
        .set_text(text)
        .map_err(|e| InputError::ClipboardError(e.to_string()))?;

    tracing::debug!("Copied {} chars to clipboard", text.len());
    Ok(())
}

/// Restore previous clipboard contents. If `previous` is None, clears the clipboard.
pub fn restore_clipboard(previous: Option<&str>) -> Result<(), InputError> {
    let mut clipboard = Clipboard::new()
        .map_err(|e| InputError::ClipboardError(e.to_string()))?;

    match previous {
        Some(text) => {
            clipboard
                .set_text(text)
                .map_err(|e| InputError::ClipboardError(e.to_string()))?;
            tracing::debug!("Restored clipboard ({} chars)", text.len());
        }
        None => {
            // Clear by setting empty string
            let _ = clipboard.set_text("");
            tracing::debug!("Cleared clipboard (was empty before)");
        }
    }
    Ok(())
}
