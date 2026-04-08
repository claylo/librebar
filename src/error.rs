//! Error types for rebar.

use thiserror::Error;

/// Errors that can occur during rebar initialization.
#[derive(Error, Debug)]
pub enum Error {
    /// Configuration file could not be parsed.
    #[error("failed to parse config file {path}: {source}")]
    ConfigParse {
        /// Path to the config file that failed.
        path: String,
        /// Underlying parse error.
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// Configuration deserialization failed.
    #[error("failed to deserialize config: {0}")]
    ConfigDeserialize(Box<dyn std::error::Error + Send + Sync>),

    /// No configuration file found (when one was required).
    #[error("no configuration file found")]
    ConfigNotFound,

    /// Log directory is not writable.
    #[error("no writable log directory found")]
    LogDirNotWritable,

    /// I/O error during initialization.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Result type alias using rebar's [`Error`].
pub type Result<T> = std::result::Result<T, Error>;
