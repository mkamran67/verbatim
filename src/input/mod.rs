pub mod enigo_backend;

use crate::errors::InputError;

pub trait InputMethod: Send {
    /// Type the given text into the currently focused window.
    fn type_text(&mut self, text: &str) -> Result<(), InputError>;
}
