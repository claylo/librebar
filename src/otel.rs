//! OpenTelemetry tracing with OTLP export.
//!
//! Provides configuration and layer construction for exporting spans via
//! OTLP (HTTP protobuf by default, gRPC with the `otel-grpc` feature).
//! The layer composes with the logging layer on a single `tracing_subscriber::Registry`.
//!
//! # Standalone usage
//!
//! ```ignore
//! use librebar::otel::{OtelConfig, build_otel_layer};
//!
//! let cfg = OtelConfig::from_app_name("my-tool", "0.1.0");
//! let (layer, guard) = build_otel_layer(&cfg)?;
//! // layer is Option — None when no endpoint is configured
//! ```
//!
//! # Environment variables
//!
//! - `OTEL_EXPORTER_OTLP_ENDPOINT` — collector URL (required for export)
//! - `OTEL_EXPORTER_OTLP_PROTOCOL` — `http/protobuf` (default), `http/json`, or `grpc`
//! - `{APP}_ENV` — deployment environment label (defaults to `"dev"`)

use crate::error::Result;

/// A boxed tracing layer that can be composed on a `Registry`.
pub type BoxedLayer =
    Box<dyn tracing_subscriber::Layer<tracing_subscriber::Registry> + Send + Sync>;

/// Configuration for OpenTelemetry tracing export.
#[derive(Clone, Debug)]
pub struct OtelConfig {
    /// Service name (used as `service.name` resource attribute).
    pub service: String,
    /// Service version (used as `service.version` resource attribute).
    pub version: String,
    /// Deployment environment (used as `deployment.environment` resource attribute).
    /// Defaults to `"dev"`.
    pub env: String,
    /// OTLP collector endpoint. `None` means export is disabled.
    pub endpoint: Option<String>,
    /// Env var name for the OTLP endpoint.
    pub env_var_endpoint: String,
    /// Env var name for the OTLP protocol.
    pub env_var_protocol: String,
    /// Env var name for the deployment environment (e.g., `MY_TOOL_ENV`).
    pub env_var_env: String,
}

impl OtelConfig {
    /// Create an OTEL config from an application name and version.
    ///
    /// Reads `OTEL_EXPORTER_OTLP_ENDPOINT` for the collector URL and
    /// `{APP}_ENV` for the deployment environment (defaults to `"dev"`).
    pub fn from_app_name(app_name: &str, version: &str) -> Self {
        let prefix = app_name.to_uppercase().replace('-', "_");
        let env_var_env = format!("{prefix}_ENV");

        let endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
            .ok()
            .filter(|v| !v.is_empty());

        let env = std::env::var(&env_var_env)
            .ok()
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "dev".to_string());

        Self {
            service: app_name.to_string(),
            version: version.to_string(),
            env,
            endpoint,
            env_var_endpoint: "OTEL_EXPORTER_OTLP_ENDPOINT".to_string(),
            env_var_protocol: "OTEL_EXPORTER_OTLP_PROTOCOL".to_string(),
            env_var_env,
        }
    }

    /// Override the endpoint. Only applies if the env var was not already set.
    #[must_use]
    pub fn with_endpoint(mut self, endpoint: Option<String>) -> Self {
        if self.endpoint.is_none() {
            self.endpoint = endpoint;
        }
        self
    }
}

/// Guard that holds the `TracerProvider` and flushes spans on drop.
///
/// Must be held for the application lifetime. Dropping it triggers
/// `provider.shutdown()` which flushes any pending span batches.
pub struct OtelGuard {
    provider: opentelemetry_sdk::trace::SdkTracerProvider,
}

impl Drop for OtelGuard {
    fn drop(&mut self) {
        if let Err(e) = self.provider.shutdown() {
            eprintln!("Error shutting down tracer provider: {e}");
        }
    }
}

/// Build the OpenTelemetry tracing layer and its guard.
///
/// Returns `(None, None)` when no endpoint is configured — this makes it
/// safe to always call and compose with `Option<Layer>` (which is a no-op
/// when `None`).
///
/// The layer is boxed so it can compose freely with other layers on any
/// subscriber type that supports `LookupSpan`.
///
/// # Errors
///
/// Returns [`Error::OtelInit`](crate::Error::OtelInit) if the exporter
/// or tracer provider fails to build.
pub fn build_otel_layer(cfg: &OtelConfig) -> Result<(Option<BoxedLayer>, Option<OtelGuard>)> {
    let endpoint = match cfg.endpoint.as_deref() {
        Some(ep) if !ep.is_empty() => ep,
        _ => return Ok((None, None)),
    };

    let resource = opentelemetry_sdk::Resource::builder()
        .with_attributes([
            opentelemetry::KeyValue::new("service.name", cfg.service.clone()),
            opentelemetry::KeyValue::new("deployment.environment", cfg.env.clone()),
            opentelemetry::KeyValue::new("service.version", cfg.version.clone()),
        ])
        .build();

    let protocol = std::env::var("OTEL_EXPORTER_OTLP_PROTOCOL")
        .ok()
        .unwrap_or_default();

    let exporter = build_exporter(endpoint, &protocol)?;

    let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .with_resource(resource)
        .build();

    // TracerProvider trait must be in scope for .tracer()
    use opentelemetry::trace::TracerProvider as _;
    let tracer = provider.tracer(cfg.service.clone());

    let layer = tracing_opentelemetry::layer().with_tracer(tracer);

    let boxed: BoxedLayer = Box::new(layer);
    Ok((Some(boxed), Some(OtelGuard { provider })))
}

/// Build the span exporter based on the protocol string.
fn build_exporter(endpoint: &str, protocol: &str) -> Result<opentelemetry_otlp::SpanExporter> {
    use opentelemetry_otlp::WithExportConfig as _;

    match protocol {
        #[cfg(feature = "otel-grpc")]
        "grpc" => opentelemetry_otlp::SpanExporter::builder()
            .with_tonic()
            .with_endpoint(endpoint)
            .build()
            .map_err(crate::Error::OtelInit),

        // http/protobuf, http/json, or anything else — use HTTP transport
        _ => opentelemetry_otlp::SpanExporter::builder()
            .with_http()
            .with_endpoint(endpoint)
            .build()
            .map_err(crate::Error::OtelInit),
    }
}
