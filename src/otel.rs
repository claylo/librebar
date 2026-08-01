//! OpenTelemetry tracing with OTLP export.
//!
//! Provides configuration and layer construction for exporting spans via
//! OTLP (HTTP protobuf by default, gRPC with the `otel-grpc` feature).
//! The layer composes with the logging layer on a single `tracing_subscriber::Registry`.
//!
//! # Standalone usage
//!
//! ```no_run
//! use librebar::otel::{OtelConfig, build_otel_layer};
//!
//! # fn main() -> librebar::Result<()> {
//! let cfg = OtelConfig::from_app_name("my-tool", "0.1.0");
//! let (layer, guard) = build_otel_layer(&cfg)?;
//! // layer is Option — None when no endpoint is configured
//! # let _ = (layer, guard);
//! # Ok(())
//! # }
//! ```
//!
//! # Environment variables
//!
//! - `OTEL_EXPORTER_OTLP_ENDPOINT` — collector URL (required for export)
//! - `OTEL_EXPORTER_OTLP_PROTOCOL` — `http/protobuf` (default), `http/json`
//!   (requires `otel-http-json`), or `grpc` (requires `otel-grpc`)
//! - `{APP}_ENV` — deployment environment label (defaults to `"dev"`)

use crate::error::{Result, boxed_error};

/// Re-export of [`tracing_subscriber`], used by the OTEL layer API.
pub use tracing_subscriber;

mod blocking_hyper;
#[cfg(feature = "otel-grpc")]
mod blocking_tonic;

/// A boxed tracing layer that can be composed on a `Registry`.
pub type BoxedLayer =
    Box<dyn tracing_subscriber::Layer<tracing_subscriber::Registry> + Send + Sync>;

/// Configuration for OpenTelemetry tracing export.
#[derive(Clone, Debug)]
#[non_exhaustive]
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
}

impl OtelConfig {
    /// Standard environment variable for the OTLP collector endpoint.
    pub const ENV_VAR_ENDPOINT: &str = "OTEL_EXPORTER_OTLP_ENDPOINT";

    /// Standard environment variable for the OTLP transport protocol.
    pub const ENV_VAR_PROTOCOL: &str = "OTEL_EXPORTER_OTLP_PROTOCOL";

    /// Create an OTEL config from an application name and version.
    ///
    /// Reads `OTEL_EXPORTER_OTLP_ENDPOINT` for the collector URL and
    /// `{APP}_ENV` for the deployment environment (defaults to `"dev"`).
    pub fn from_app_name(app_name: &str, version: &str) -> Self {
        let prefix = app_name.to_uppercase().replace('-', "_");
        let env_var_env = format!("{prefix}_ENV");

        let endpoint = std::env::var(Self::ENV_VAR_ENDPOINT)
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
    #[cfg(feature = "otel-grpc")]
    tonic_runtime: Option<blocking_tonic::BlockingTonicRuntime>,
}

impl Drop for OtelGuard {
    fn drop(&mut self) {
        if let Err(e) = self.provider.shutdown() {
            write_shutdown_error(std::io::stderr().lock(), e);
        }
        #[cfg(feature = "otel-grpc")]
        drop(self.tonic_runtime.take());
    }
}

fn write_shutdown_error(mut writer: impl std::io::Write, error: impl std::fmt::Display) {
    let _ = writeln!(writer, "Error shutting down tracer provider: {error}");
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
/// or tracer provider fails to build, or [`Error::Io`](crate::Error::Io) if
/// a private exporter runtime thread cannot be created.
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

    let protocol = std::env::var(OtelConfig::ENV_VAR_PROTOCOL)
        .ok()
        .unwrap_or_default();

    let BuiltExporter {
        exporter,
        #[cfg(feature = "otel-grpc")]
        tonic_runtime,
    } = build_exporter(endpoint, &protocol)?;

    let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .with_resource(resource)
        .build();

    // TracerProvider trait must be in scope for .tracer()
    use opentelemetry::trace::TracerProvider as _;
    let tracer = provider.tracer(cfg.service.clone());

    let layer = tracing_opentelemetry::layer().with_tracer(tracer);

    let boxed: BoxedLayer = Box::new(layer);
    Ok((
        Some(boxed),
        Some(OtelGuard {
            provider,
            #[cfg(feature = "otel-grpc")]
            tonic_runtime,
        }),
    ))
}

struct BuiltExporter {
    exporter: opentelemetry_otlp::SpanExporter,
    #[cfg(feature = "otel-grpc")]
    tonic_runtime: Option<blocking_tonic::BlockingTonicRuntime>,
}

/// Build the span exporter based on the protocol string.
fn build_exporter(endpoint: &str, protocol: &str) -> Result<BuiltExporter> {
    match protocol {
        #[cfg(feature = "otel-grpc")]
        "grpc" => {
            let (exporter, tonic_runtime) =
                blocking_tonic::BlockingTonicRuntime::build_exporter(endpoint)?;
            Ok(BuiltExporter {
                exporter,
                tonic_runtime: Some(tonic_runtime),
            })
        }

        #[cfg(feature = "otel-http-json")]
        "http/json" => {
            build_http_exporter(endpoint, opentelemetry_otlp::Protocol::HttpJson).map(|exporter| {
                BuiltExporter {
                    exporter,
                    #[cfg(feature = "otel-grpc")]
                    tonic_runtime: None,
                }
            })
        }

        #[cfg(not(feature = "otel-http-json"))]
        "http/json" => Err(crate::Error::OtelInit(boxed_error(
            opentelemetry_otlp::ExporterBuildError::InvalidConfig {
                name: OtelConfig::ENV_VAR_PROTOCOL.to_string(),
                reason: "http/json requires librebar feature 'otel-http-json'".to_string(),
            },
        ))),

        // http/protobuf or anything else — preserve the protobuf default.
        _ => build_http_exporter(endpoint, opentelemetry_otlp::Protocol::HttpBinary).map(
            |exporter| BuiltExporter {
                exporter,
                #[cfg(feature = "otel-grpc")]
                tonic_runtime: None,
            },
        ),
    }
}

fn build_http_exporter(
    endpoint: &str,
    protocol: opentelemetry_otlp::Protocol,
) -> Result<opentelemetry_otlp::SpanExporter> {
    use opentelemetry_otlp::{WithExportConfig as _, WithHttpConfig as _};

    let timeout = otlp_trace_timeout();
    let client = blocking_hyper::BlockingHyperClient::new(timeout)?;
    opentelemetry_otlp::SpanExporter::builder()
        .with_http()
        .with_http_client(client)
        .with_endpoint(endpoint)
        .with_timeout(timeout)
        .with_protocol(protocol)
        .build()
        .map_err(|error| crate::Error::OtelInit(boxed_error(error)))
}

fn otlp_trace_timeout() -> std::time::Duration {
    [
        opentelemetry_otlp::OTEL_EXPORTER_OTLP_TRACES_TIMEOUT,
        "OTEL_EXPORTER_OTLP_TIMEOUT",
    ]
    .into_iter()
    .find_map(|name| std::env::var(name).ok()?.parse::<u64>().ok())
    .map(std::time::Duration::from_millis)
    .unwrap_or_else(|| std::time::Duration::from_secs(10))
}

#[cfg(test)]
mod tests {
    use std::io;

    use super::write_shutdown_error;

    struct BrokenWriter;

    impl io::Write for BrokenWriter {
        fn write(&mut self, _buffer: &[u8]) -> io::Result<usize> {
            Err(io::Error::new(io::ErrorKind::BrokenPipe, "closed stderr"))
        }

        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::new(io::ErrorKind::BrokenPipe, "closed stderr"))
        }
    }

    #[test]
    fn shutdown_error_notice_ignores_broken_stderr() {
        write_shutdown_error(BrokenWriter, "shutdown failed");
    }
}
