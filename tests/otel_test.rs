#![allow(missing_docs, unsafe_code)]
#![cfg(feature = "otel")]

use librebar::otel::OtelConfig;
use librebar::otel::tracing_subscriber::layer::SubscriberExt as _;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use std::sync::{Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

// Process-global env vars are shared across threads. nextest sidesteps this
// by running each test in its own process, but `cargo test` runs them on
// threads within one process — a mutation in one test will race with a read
// in another. This file-level lock serializes the tests that touch env so
// the suite works under either runner.
static ENV_LOCK: Mutex<()> = Mutex::new(());

struct OtlpReceiver {
    endpoint: String,
    request: mpsc::Receiver<Result<Vec<u8>, String>>,
    server: thread::JoinHandle<()>,
}

/// Clear OTEL env vars so tests run deterministically regardless of the
/// host environment. Returns a guard the caller must hold for the duration
/// of the test body — dropping it before the test ends reopens the race.
#[must_use = "hold the returned guard for the whole test"]
fn clear_otel_env() -> MutexGuard<'static, ()> {
    let guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    // SAFETY: ENV_LOCK serializes env-touching tests in this file.
    unsafe {
        std::env::remove_var("OTEL_EXPORTER_OTLP_ENDPOINT");
        std::env::remove_var("OTEL_EXPORTER_OTLP_PROTOCOL");
        std::env::remove_var("MY_TOOL_ENV");
    }
    guard
}

fn spawn_otlp_receiver() -> OtlpReceiver {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind OTLP receiver");
    listener
        .set_nonblocking(true)
        .expect("make OTLP receiver nonblocking");
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let (request_tx, request_rx) = mpsc::sync_channel(1);

    let server = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(6);
        let result = loop {
            match listener.accept() {
                Ok((mut stream, _)) => break read_otlp_request(&mut stream),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    if Instant::now() >= deadline {
                        break Err("timed out waiting for OTLP request".to_string());
                    }
                    thread::sleep(Duration::from_millis(10));
                }
                Err(error) => break Err(format!("accept OTLP request: {error}")),
            }
        };
        let _ = request_tx.send(result);
    });

    OtlpReceiver {
        endpoint,
        request: request_rx,
        server,
    }
}

fn read_otlp_request(stream: &mut TcpStream) -> Result<Vec<u8>, String> {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|error| format!("set OTLP read timeout: {error}"))?;

    let mut request = Vec::new();
    let mut buffer = [0_u8; 4096];
    loop {
        let read = stream
            .read(&mut buffer)
            .map_err(|error| format!("read OTLP request: {error}"))?;
        if read == 0 {
            break;
        }
        request.extend_from_slice(&buffer[..read]);

        if request_is_complete(&request) {
            break;
        }
    }

    stream
        .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
        .map_err(|error| format!("respond to OTLP request: {error}"))?;
    Ok(request)
}

fn request_is_complete(request: &[u8]) -> bool {
    let Some(header_end) = request.windows(4).position(|window| window == b"\r\n\r\n") else {
        return false;
    };
    let headers = String::from_utf8_lossy(&request[..header_end]);
    let content_length = headers
        .lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.trim().parse::<usize>().ok())
        .unwrap_or_default();
    request.len() >= header_end + 4 + content_length
}

fn export_span(receiver: OtlpReceiver) -> Vec<u8> {
    let cfg = OtelConfig::from_app_name("test-app", "0.1.0").with_endpoint(Some(receiver.endpoint));
    let (layer, guard) = librebar::otel::build_otel_layer(&cfg).expect("build OTLP layer");
    let subscriber = librebar::otel::tracing_subscriber::registry().with(layer);

    tracing::subscriber::with_default(subscriber, || {
        tracing::info_span!("exported-span").in_scope(|| {});
    });
    drop(guard);

    let request = receiver
        .request
        .recv_timeout(Duration::from_secs(7))
        .expect("OTLP receiver thread stopped")
        .expect("receive OTLP request");
    receiver.server.join().expect("join OTLP receiver");
    request
}

fn request_parts(request: &[u8]) -> (&str, &[u8]) {
    let header_end = request
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .expect("OTLP request has headers");
    let headers = std::str::from_utf8(&request[..header_end]).expect("OTLP headers are UTF-8");
    (headers, &request[header_end + 4..])
}

#[test]
fn otel_config_from_app_name() {
    let _guard = clear_otel_env();
    let cfg = OtelConfig::from_app_name("test-app", "0.1.0");
    assert_eq!(cfg.service, "test-app");
    assert_eq!(cfg.version, "0.1.0");
    assert_eq!(cfg.env, "dev");
    assert!(cfg.endpoint.is_none());
}

#[test]
fn otel_config_exposes_standard_env_var_names_as_constants() {
    assert_eq!(OtelConfig::ENV_VAR_ENDPOINT, "OTEL_EXPORTER_OTLP_ENDPOINT");
    assert_eq!(OtelConfig::ENV_VAR_PROTOCOL, "OTEL_EXPORTER_OTLP_PROTOCOL");
}

#[test]
fn otel_config_reads_the_app_specific_environment() {
    let _guard = clear_otel_env();
    // SAFETY: ENV_LOCK serializes env-touching tests in this file.
    unsafe { std::env::set_var("MY_TOOL_ENV", "production") };

    let cfg = OtelConfig::from_app_name("my-tool", "1.0.0");

    assert_eq!(cfg.env, "production");
}

#[test]
fn otel_config_with_endpoint() {
    let _guard = clear_otel_env();
    let cfg = OtelConfig::from_app_name("test-app", "0.1.0")
        .with_endpoint(Some("http://localhost:4318".to_string()));
    assert_eq!(cfg.endpoint.as_deref(), Some("http://localhost:4318"));
}

#[test]
fn build_layer_returns_none_without_endpoint() {
    let _guard = clear_otel_env();
    let cfg = OtelConfig::from_app_name("test-app", "0.1.0");
    let result = librebar::otel::build_otel_layer(&cfg);
    assert!(result.is_ok());
    let (layer, guard) = result.unwrap();
    assert!(layer.is_none(), "no endpoint means no layer");
    assert!(guard.is_none(), "no endpoint means no guard");
}

#[test]
fn http_export_reaches_collector_without_async_runtime() {
    let _guard = clear_otel_env();
    let request = export_span(spawn_otlp_receiver());
    let (headers, body) = request_parts(&request);
    assert!(headers.starts_with("POST "));
    assert!(
        headers
            .lines()
            .any(|line| line.eq_ignore_ascii_case("content-type: application/x-protobuf"))
    );
    assert!(!body.is_empty(), "OTLP request has a body");
}

#[cfg(feature = "otel-grpc")]
#[test]
fn grpc_protocol_builds_without_async_runtime() {
    let _guard = clear_otel_env();
    // SAFETY: ENV_LOCK serializes env-touching tests in this file.
    unsafe { std::env::set_var(OtelConfig::ENV_VAR_PROTOCOL, "grpc") };
    let cfg = OtelConfig::from_app_name("test-app", "0.1.0")
        .with_endpoint(Some("http://127.0.0.1:4317".to_string()));

    let (layer, guard) = librebar::otel::build_otel_layer(&cfg).expect("build gRPC OTLP layer");

    assert!(layer.is_some(), "gRPC endpoint enables the OTEL layer");
    assert!(guard.is_some(), "gRPC exporter retains its provider");
}

#[cfg(feature = "otel-http-json")]
#[test]
fn http_json_protocol_sends_json() {
    let _guard = clear_otel_env();
    // SAFETY: ENV_LOCK serializes env-touching tests in this file.
    unsafe { std::env::set_var("OTEL_EXPORTER_OTLP_PROTOCOL", "http/json") };

    let request = export_span(spawn_otlp_receiver());
    let (headers, body) = request_parts(&request);
    assert!(
        headers
            .lines()
            .any(|line| line.eq_ignore_ascii_case("content-type: application/json"))
    );
    let body: serde_json::Value = serde_json::from_slice(body).expect("OTLP body is JSON");
    assert!(body.to_string().contains("exported-span"));
}

#[cfg(not(feature = "otel-http-json"))]
#[test]
fn http_json_protocol_requires_feature() {
    let _guard = clear_otel_env();
    // SAFETY: ENV_LOCK serializes env-touching tests in this file.
    unsafe { std::env::set_var("OTEL_EXPORTER_OTLP_PROTOCOL", "http/json") };
    let cfg = OtelConfig::from_app_name("test-app", "0.1.0")
        .with_endpoint(Some("http://127.0.0.1:4318".to_string()));

    let result = librebar::otel::build_otel_layer(&cfg);
    assert!(result.is_err(), "http/json requires otel-http-json");
    assert!(
        result
            .err()
            .is_some_and(|error| error.to_string().contains("otel-http-json"))
    );
}
