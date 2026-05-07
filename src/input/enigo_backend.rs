use std::process::Command;

use enigo::{Enigo, Keyboard, Settings};

use super::InputMethod;
use crate::errors::InputError;

fn is_wayland() -> bool {
    std::env::var("WAYLAND_DISPLAY").is_ok()
}

fn has_command(cmd: &str) -> bool {
    Command::new("which")
        .arg(cmd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
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

pub struct EnigoBackend {
    enigo: Enigo,
    method: Method,
}

impl EnigoBackend {
    pub fn new(input_method: &str) -> Result<Self, InputError> {
        let enigo = Enigo::new(&Settings::default())
            .map_err(|e| InputError::SimulationFailed(e.to_string()))?;

        Ok(Self {
            enigo,
            method: Method::from_config(input_method),
        })
    }
}

fn type_via_wtype(text: &str) -> Result<(), InputError> {
    if !has_command("wtype") {
        return Err(InputError::SimulationFailed(
            "wtype is not installed (sudo apt install wtype)".into(),
        ));
    }

    let status = Command::new("wtype")
        .arg("--")
        .arg(text)
        .status()
        .map_err(|e| InputError::SimulationFailed(format!("wtype exec: {}", e)))?;

    if status.success() {
        tracing::debug!("Typed {} chars via wtype", text.len());
        return Ok(());
    }
    tracing::warn!("wtype exited with {}", status);

    // Fallback: simulate Ctrl+V (text is already on clipboard from app.rs)
    let status = Command::new("wtype")
        .arg("-M")
        .arg("ctrl")
        .arg("-k")
        .arg("v")
        .arg("-m")
        .arg("ctrl")
        .status()
        .map_err(|e| InputError::SimulationFailed(format!("wtype paste: {}", e)))?;

    if status.success() {
        tracing::debug!("Pasted via wtype Ctrl+V");
        return Ok(());
    }

    Err(InputError::SimulationFailed(
        "wtype failed to type or paste text".into(),
    ))
}

impl InputMethod for EnigoBackend {
    fn type_text(&mut self, text: &str) -> Result<(), InputError> {
        match self.method {
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
        }
    }
}
