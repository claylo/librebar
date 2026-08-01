//! Error types for librebar.

use thiserror::Error;

/// Errors that can occur during librebar initialization and operation.
///
/// This enum is `#[non_exhaustive]`: downstream `match` expressions must include
/// a wildcard arm, which allows future librebar releases to add variants
/// without a breaking change. See the README's "Versioning" section.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum Error {
    /// Configuration file could not be parsed.
    #[cfg(feature = "config")]
    #[error("failed to parse config file {path}: {source}")]
    ConfigParse {
        /// Path to the config file that failed.
        path: String,
        /// Underlying parse error.
        source: Box<ConfigParseError>,
    },

    /// Configuration deserialization failed.
    #[cfg(feature = "config")]
    #[error("failed to deserialize config: {0}")]
    ConfigDeserialize(serde_json::Error),

    /// Config nesting exceeded the maximum depth during merge.
    #[cfg(feature = "config")]
    #[error("config nesting exceeds maximum merge depth")]
    ConfigMergeDepth,

    /// Environment configuration could not be applied.
    #[cfg(feature = "config")]
    #[error("invalid environment configuration from {variable}: {reason}")]
    ConfigEnvironment {
        /// Variable that failed validation or parsing.
        variable: String,
        /// Safe diagnostic that never includes the variable's value.
        reason: String,
    },

    /// A programmatic configuration override is invalid.
    #[cfg(feature = "config")]
    #[error("invalid configuration override {path}: {reason}")]
    ConfigOverride {
        /// Dotted configuration path.
        path: String,
        /// Serialization or path error.
        reason: String,
    },

    /// No configuration file found (when one was required).
    #[cfg(feature = "config")]
    #[error("no configuration file found")]
    ConfigNotFound,

    /// Log directory is not writable.
    #[cfg(feature = "logging")]
    #[error("no writable log directory found")]
    LogDirNotWritable,

    /// OpenTelemetry exporter initialization failed.
    #[cfg(feature = "otel")]
    #[error("failed to initialize OpenTelemetry: {0}")]
    OtelInit(opentelemetry_otlp::ExporterBuildError),

    /// Tracing global subscriber already set or initialization failed.
    #[cfg(feature = "logging")]
    #[error("failed to initialize tracing subscriber: {0}")]
    TracingInit(tracing_subscriber::util::TryInitError),

    /// Shutdown signal handler registration failed.
    #[cfg(feature = "shutdown")]
    #[error("failed to register shutdown handler: {0}")]
    ShutdownInit(std::io::Error),

    /// No Tokio runtime available for async initialization.
    #[cfg(feature = "shutdown")]
    #[error("no active Tokio runtime: {0}")]
    NoRuntime(tokio::runtime::TryCurrentError),

    /// Lockfile acquisition failed.
    #[cfg(feature = "lockfile")]
    #[error("failed to acquire lock: {0}")]
    Lock(std::io::Error),

    /// HTTP client error.
    #[cfg(feature = "http")]
    #[error("HTTP error: {0}")]
    Http(#[from] HttpError),

    /// Cache I/O error.
    #[cfg(feature = "cache")]
    #[error("cache error: {0}")]
    Cache(#[from] CacheError),

    /// External command dispatch error.
    #[cfg(feature = "dispatch")]
    #[error("dispatch error: {0}")]
    Dispatch(std::io::Error),

    /// Diagnostic error.
    #[cfg(feature = "diagnostics")]
    #[error("diagnostic error: {0}")]
    Diagnostic(std::io::Error),

    /// I/O error during initialization.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Result type alias using librebar's [`enum@Error`].
pub type Result<T> = std::result::Result<T, Error>;

// ─── Per-module error enums ─────────────────────────────────────────

/// Errors from the HTTP client.
#[cfg(feature = "http")]
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum HttpError {
    /// TLS provider initialization failed.
    #[error("TLS: {0}")]
    Tls(#[from] rustls::Error),
    /// URL could not be parsed.
    #[error("invalid URL: {0}")]
    InvalidUrl(#[from] http::uri::InvalidUri),
    /// HTTP request could not be constructed.
    #[error("request build: {0}")]
    RequestBuild(#[from] http::Error),
    /// HTTP header value is invalid.
    #[error("invalid header value: {0}")]
    InvalidHeaderValue(#[from] http::header::InvalidHeaderValue),
    /// Connection, protocol, or middleware error during request.
    #[error("request: {0}")]
    Request(#[source] tower::BoxError),
    /// A redirect attempted to move an HTTPS request to plaintext HTTP.
    #[error("refused HTTPS-to-HTTP redirect")]
    RedirectDowngrade,
    /// A redirect returned to a URI already visited by this request.
    #[error("redirect loop detected")]
    RedirectLoop,
    /// The configured redirect limit was exceeded.
    #[error("too many redirects (maximum {maximum})")]
    TooManyRedirects {
        /// Maximum number of redirects configured for the client.
        maximum: usize,
    },
    /// A cookie jar could not be loaded or saved.
    #[cfg(feature = "http-cookies")]
    #[error("cookie jar {operation} failed for {path}: {source}")]
    CookieJar {
        /// Operation that failed (`load` or `save`).
        operation: &'static str,
        /// Cookie jar path.
        path: String,
        /// Underlying filesystem or serialization error.
        source: tower::BoxError,
    },
    /// The decoded response exceeded the configured in-memory limit.
    #[error("response body exceeds maximum of {maximum} bytes")]
    ResponseTooLarge {
        /// Maximum decoded response size configured for the client.
        maximum: usize,
    },
    /// Error reading response body.
    #[error("response body: {0}")]
    Body(#[source] tower::BoxError),
    /// I/O or timeout error.
    #[error("{0}")]
    Io(#[from] std::io::Error),
    /// JSON deserialization of response body failed.
    #[error("JSON: {0}")]
    Json(#[from] serde_json::Error),
}

/// Errors from the file cache.
#[cfg(feature = "cache")]
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum CacheError {
    /// Filesystem I/O error.
    #[error("{0}")]
    Io(#[from] std::io::Error),
    /// JSON serialization or deserialization error.
    #[error("JSON: {0}")]
    Json(#[from] serde_json::Error),
    /// Base64 decoding error.
    #[error("decode: {0}")]
    Decode(#[from] base64::DecodeError),
    /// Invalid or unsupported on-disk cache framing.
    #[error("invalid cache entry format: {0}")]
    Format(String),
}

/// Errors from config file parsing.
#[cfg(feature = "config")]
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum ConfigParseError {
    /// TOML parse error.
    #[error("{0}")]
    Toml(#[from] toml::de::Error),
    /// YAML parse error.
    #[error("{0}")]
    Yaml(#[from] serde_saphyr::Error),
    /// JSON parse error.
    #[error("{0}")]
    Json(#[from] serde_json::Error),
    /// I/O error reading the file.
    #[error("{0}")]
    Io(#[from] std::io::Error),
}
