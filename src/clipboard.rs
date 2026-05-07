use arboard::Clipboard;
use crate::errors::InputError;

/// Copy text to the system clipboard.
pub fn copy_to_clipboard(text: &str) -> Result<(), InputError> {
    let mut clipboard = Clipboard::new()
        .map_err(|e| InputError::ClipboardError(e.to_string()))?;

    clipboard
        .set_text(text)
        .map_err(|e| InputError::ClipboardError(e.to_string()))?;

    tracing::debug!("Copied {} chars to clipboard", text.len());
    Ok(())
}
