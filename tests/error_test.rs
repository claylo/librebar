#![allow(missing_docs)]

use std::error::Error as StdError;
use std::fmt;

fn boxed(message: &'static str) -> librebar::error::BoxError {
    Box::new(std::io::Error::other(message))
}

fn assert_immediate_source(error: &(dyn StdError + 'static), expected: &str) {
    let source = error.source().expect("wrapped error must remain a source");
    assert_eq!(source.to_string(), expected);
}

#[derive(Debug)]
struct NestedError {
    source: std::io::Error,
}

impl fmt::Display for NestedError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("outer dependency error")
    }
}

impl StdError for NestedError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        Some(&self.source)
    }
}

#[test]
fn boxed_error_is_send_sync_and_static() {
    fn assert_bounds<T: Send + Sync + 'static>() {}

    assert_bounds::<librebar::error::BoxError>();
}

#[cfg(feature = "config")]
#[test]
fn boxed_source_preserves_nested_error_chain() {
    let error = librebar::Error::ConfigDeserialize(Box::new(NestedError {
        source: std::io::Error::other("root cause"),
    }));

    let dependency = error.source().expect("dependency error must be a source");
    assert!(dependency.downcast_ref::<NestedError>().is_some());
    assert_eq!(dependency.to_string(), "outer dependency error");
    assert_eq!(
        dependency
            .source()
            .expect("dependency source chain must remain intact")
            .to_string(),
        "root cause"
    );
}

#[cfg(feature = "config")]
#[test]
fn config_dependency_errors_use_boxed_sources() {
    assert_immediate_source(
        &librebar::Error::ConfigDeserialize(boxed("deserialize")),
        "deserialize",
    );
    assert_immediate_source(
        &librebar::error::ConfigParseError::Toml(boxed("toml")),
        "toml",
    );
    assert_immediate_source(
        &librebar::error::ConfigParseError::Yaml(boxed("yaml")),
        "yaml",
    );
    assert_immediate_source(
        &librebar::error::ConfigParseError::Json(boxed("json")),
        "json",
    );
}

#[cfg(feature = "logging")]
#[test]
fn logging_dependency_errors_use_boxed_sources() {
    assert_immediate_source(&librebar::Error::TracingInit(boxed("tracing")), "tracing");
}

#[cfg(feature = "otel")]
#[test]
fn otel_dependency_errors_use_boxed_sources() {
    assert_immediate_source(&librebar::Error::OtelInit(boxed("otel")), "otel");
}

#[cfg(feature = "shutdown")]
#[test]
fn shutdown_errors_preserve_sources() {
    assert_immediate_source(
        &librebar::Error::ShutdownInit(std::io::Error::other("signal")),
        "signal",
    );
    assert_immediate_source(&librebar::Error::NoRuntime(boxed("runtime")), "runtime");
}

#[cfg(feature = "lockfile")]
#[test]
fn lock_errors_preserve_sources() {
    assert_immediate_source(
        &librebar::Error::Lock(std::io::Error::other("lock")),
        "lock",
    );
}

#[cfg(feature = "dispatch")]
#[test]
fn dispatch_errors_preserve_sources() {
    assert_immediate_source(
        &librebar::Error::Dispatch(std::io::Error::other("dispatch")),
        "dispatch",
    );
}

#[cfg(feature = "diagnostics")]
#[test]
fn diagnostic_errors_preserve_sources() {
    assert_immediate_source(
        &librebar::Error::Diagnostic(std::io::Error::other("diagnostic")),
        "diagnostic",
    );
}

#[cfg(feature = "http")]
#[test]
fn http_dependency_errors_use_boxed_sources() {
    use librebar::error::HttpError;

    assert_immediate_source(&HttpError::Tls(boxed("tls")), "tls");
    assert_immediate_source(&HttpError::InvalidUrl(boxed("uri")), "uri");
    assert_immediate_source(&HttpError::RequestBuild(boxed("request")), "request");
    assert_immediate_source(&HttpError::InvalidHeaderValue(boxed("header")), "header");
    assert_immediate_source(&HttpError::Request(boxed("transport")), "transport");
    assert_immediate_source(&HttpError::Body(boxed("body")), "body");
    assert_immediate_source(&HttpError::Json(boxed("json")), "json");
}

#[cfg(feature = "http-cookies")]
#[test]
fn cookie_jar_errors_use_boxed_sources() {
    let error = librebar::error::HttpError::CookieJar {
        operation: "load",
        path: "cookies.json".to_string(),
        source: boxed("cookie"),
    };

    assert_immediate_source(&error, "cookie");
}

#[cfg(feature = "cache")]
#[test]
fn cache_dependency_errors_use_boxed_sources() {
    use librebar::error::CacheError;

    assert_immediate_source(&CacheError::Json(boxed("json")), "json");
    assert_immediate_source(&CacheError::Decode(boxed("base64")), "base64");
}
