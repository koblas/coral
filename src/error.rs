//! Application-level error types for the Coral Redis server.

use thiserror::Error;

/// Top-level application error type.
#[derive(Debug, Error)]
pub enum AppError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("configuration error: {0}")]
    Config(#[from] ConfigError),

    #[error("storage error: {0}")]
    Storage(#[from] crate::storage::StorageError),

    #[error("telemetry error: {0}")]
    Telemetry(#[from] TelemetryError),
}

/// Configuration-related errors.
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("validation failed: {0}")]
    Validation(String),

    #[error("file error: {0}")]
    FileError(#[from] std::io::Error),

    #[error("parse error: {0}")]
    ParseError(#[from] serde_json::Error),

    #[error("missing required field: {0}")]
    MissingField(String),
}

/// Telemetry initialization errors.
#[derive(Debug, Error)]
pub enum TelemetryError {
    #[error("metrics initialization failed: {0}")]
    MetricsInit(String),

    #[error("provider setup failed: {0}")]
    ProviderSetup(String),
}

pub type Result<T> = std::result::Result<T, AppError>;
