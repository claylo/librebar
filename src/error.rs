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

    /// OpenTelemetry initialization failed.
    #[error("failed to initialize OpenTelemetry: {0}")]
    OtelInit(Box<dyn std::error::Error + Send + Sync>),

    /// Tracing global subscriber already set.
    #[error("failed to initialize tracing subscriber: {0}")]
    TracingInit(Box<dyn std::error::Error + Send + Sync>),

    /// Shutdown signal handler registration failed.
    #[error("failed to register shutdown handler: {0}")]
    ShutdownInit(Box<dyn std::error::Error + Send + Sync>),

    /// No Tokio runtime available for async initialization.
    #[error("no active Tokio runtime: {0}")]
    NoRuntime(Box<dyn std::error::Error + Send + Sync>),

    /// Lockfile acquisition failed.
    #[error("failed to acquire lock: {0}")]
    Lock(Box<dyn std::error::Error + Send + Sync>),

    /// HTTP client error.
    #[error("HTTP error: {0}")]
    Http(Box<dyn std::error::Error + Send + Sync>),

    /// Cache I/O error.
    #[error("cache error: {0}")]
    Cache(Box<dyn std::error::Error + Send + Sync>),

    /// Update check error.
    #[error("update check error: {0}")]
    Update(Box<dyn std::error::Error + Send + Sync>),

    /// External command dispatch error.
    #[error("dispatch error: {0}")]
    Dispatch(Box<dyn std::error::Error + Send + Sync>),

    /// Diagnostic error.
    #[error("diagnostic error: {0}")]
    Diagnostic(Box<dyn std::error::Error + Send + Sync>),

    /// I/O error during initialization.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Result type alias using rebar's [`enum@Error`].
pub type Result<T> = std::result::Result<T, Error>;
