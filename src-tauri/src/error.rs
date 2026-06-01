use serde::Serialize;
use std::fmt;

/// Error returned from Tauri commands; serializes to `{ "message": ... }` so the
/// frontend can display it.
#[derive(Debug, Serialize)]
pub struct AppError {
    pub message: String,
}

impl AppError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for AppError {}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::new(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, AppError>;
