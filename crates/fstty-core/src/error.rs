//! Error types for fstty-core

use thiserror::Error;

/// Result type alias for fstty-core operations
pub type Result<T> = std::result::Result<T, Error>;

/// Errors that can occur in fstty-core
#[derive(Error, Debug)]
pub enum Error {
    #[error("Failed to open waveform file: {0}")]
    FileOpen(String),

    #[error("Failed to load signals: {0}")]
    SignalLoad(String),

    #[error("Failed to write FST file: {0}")]
    FstWrite(String),

    #[error("Invalid filter pattern: {0}")]
    InvalidPattern(String),

    #[error("Signal not found: {0}")]
    SignalNotFound(String),

    #[error("Scope not found: {0}")]
    ScopeNotFound(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
