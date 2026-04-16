pub mod enigo_backend;
pub mod window_detect;

use crate::errors::InputError;

pub trait InputMethod {
    /// Type the given text into the currently focused window.
    fn type_text(&mut self, text: &str) -> Result<(), InputError>;
}
