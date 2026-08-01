#![allow(missing_docs)]
#![cfg(feature = "http")]

// Tests marked `#[ignore]` hit the public internet. The default `just test`
// run skips them (nextest reports them as "skipped" rather than claiming a
// silent pass). Run them explicitly with:
//
//     cargo nextest run --all-features --run-ignored only
//
// or, under the stock runner:
//
//     cargo test --all-features -- --ignored

use hyper::header::{ETAG, LAST_MODIFIED};
use librebar::http::{
    ConditionalResponse, HeaderMap, HeaderValue, HttpClient, HttpClientConfig, ModificationCheck,
    RetryPolicy, StatusCode, Validator, Version,
};
#[cfg(feature = "logging")]
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener};
use std::sync::mpsc;
#[cfg(feature = "logging")]
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use flate2::Compression;
use flate2::write::GzEncoder;
#[cfg(feature = "logging")]
use tracing::field::{Field, Visit};
#[cfg(feature = "logging")]
use tracing_subscriber::layer::SubscriberExt as _;

#[cfg(feature = "logging")]
#[derive(Clone)]
struct RequestSpanCapture(Arc<Mutex<Option<BTreeMap<String, String>>>>);

#[cfg(feature = "logging")]
impl<S> tracing_subscriber::Layer<S> for RequestSpanCapture
where
    S: tracing::Subscriber,
{
    fn on_new_span(
        &self,
        attributes: &tracing::span::Attributes<'_>,
        _id: &tracing::span::Id,
        _context: tracing_subscriber::layer::Context<'_, S>,
    ) {
        if attributes.metadata().name() != "send" {
            return;
        }

        let mut visitor = RequestSpanVisitor::default();
        attributes.record(&mut visitor);
        *self.0.lock().unwrap() = Some(visitor.fields);
    }
}

#[cfg(feature = "logging")]
#[derive(Default)]
struct RequestSpanVisitor {
    fields: BTreeMap<String, String>,
}

#[cfg(feature = "logging")]
impl Visit for RequestSpanVisitor {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.fields
            .insert(field.name().to_string(), value.to_string());
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.fields
            .insert(field.name().to_string(), format!("{value:?}"));
    }
}

fn read_request(stream: &mut impl Read) -> String {
    let mut request = Vec::new();
    let mut buffer = [0_u8; 1024];
    loop {
        let count = stream.read(&mut buffer).unwrap();
        request.extend_from_slice(&buffer[..count]);

        let Some(header_end) = request.windows(4).position(|window| window == b"\r\n\r\n") else {
            continue;
        };
        let headers = String::from_utf8_lossy(&request[..header_end]);
        let content_length = headers
            .lines()
            .find_map(|line| {
                line.to_ascii_lowercase()
                    .strip_prefix("content-length: ")
                    .and_then(|value| value.parse::<usize>().ok())
            })
            .unwrap_or_default();
        if request.len() >= header_end + 4 + content_length {
            return String::from_utf8(request).unwrap();
        }
    }
}

fn spawn_server(
    request_count: usize,
    responder: impl Fn(usize, &str) -> Vec<u8> + Send + 'static,
) -> (SocketAddr, mpsc::Receiver<String>, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (request_tx, request_rx) = mpsc::channel();
    let server = std::thread::spawn(move || {
        for index in 0..request_count {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let request = read_request(&mut stream);
            let response = responder(index, &request);
            request_tx.send(request).unwrap();
            stream.write_all(&response).unwrap();
        }
    });
    (address, request_rx, server)
}

fn response(status: &str, headers: &[(&str, &str)], body: &[u8]) -> Vec<u8> {
    let mut response = format!(
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    )
    .into_bytes();
    for (name, value) in headers {
        response.extend_from_slice(format!("{name}: {value}\r\n").as_bytes());
    }
    response.extend_from_slice(b"\r\n");
    response.extend_from_slice(body);
    response
}

#[cfg(feature = "logging")]
#[tokio::test(flavor = "current_thread")]
async fn request_span_omits_uri_credentials_and_query() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);

    let captured = Arc::new(Mutex::new(None));
    let subscriber = tracing_subscriber::registry().with(RequestSpanCapture(captured.clone()));
    let _subscriber_guard = tracing::subscriber::set_default(subscriber);

    let mut config =
        HttpClientConfig::new("librebar-test", "0.1.0").with_timeout(Duration::from_millis(100));
    config.retry_policy = RetryPolicy::none();
    let client = HttpClient::new(config).unwrap();
    let url = format!("http://alice:hunter2@{address}/private/path?access_token=query-secret");

    assert!(client.get(&url).await.is_err());

    let fields = captured.lock().unwrap().take().unwrap();
    assert_eq!(
        fields.get("url").unwrap(),
        &format!("http://{address}/private/path")
    );
}

#[tokio::test]
async fn response_preserves_version_repeated_headers_and_trailers() {
    let (address, requests, server) = spawn_server(1, |_, _| {
        b"HTTP/1.1 200 OK\r\n\
          Transfer-Encoding: chunked\r\n\
          X-Trace: first\r\n\
          X-Trace: second\r\n\
          Trailer: X-Checksum\r\n\
          Connection: close\r\n\r\n\
          5\r\nhello\r\n\
          0\r\nX-Checksum: complete\r\n\r\n"
            .to_vec()
    });
    let client = HttpClient::from_app("librebar-test", "0.1.0").unwrap();

    let response = client
        .get(&format!("http://{address}/metadata"))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.version(), Version::HTTP_11);
    assert_eq!(
        response
            .headers()
            .get_all("x-trace")
            .iter()
            .map(|value| value.to_str().unwrap())
            .collect::<Vec<_>>(),
        ["first", "second"]
    );
    assert_eq!(response.header("x-trace").unwrap(), "first");
    assert_eq!(response.trailers().unwrap()["x-checksum"], "complete");
    assert_eq!(response.bytes(), b"hello");
    requests.recv().unwrap();
    server.join().unwrap();
}

#[test]
fn validator_keeps_both_server_values() {
    let mut headers = HeaderMap::new();
    headers.insert(ETAG, HeaderValue::from_static("W/\"v7\""));
    headers.insert(
        LAST_MODIFIED,
        HeaderValue::from_static("Wed, 21 Oct 2015 07:28:00 GMT"),
    );

    let validator = Validator::from_headers(&headers).unwrap();

    assert_eq!(validator.etag().unwrap(), "W/\"v7\"");
    assert_eq!(
        validator.last_modified().unwrap(),
        "Wed, 21 Oct 2015 07:28:00 GMT"
    );
}

#[tokio::test]
async fn conditional_get_prefers_etag_and_maps_304() {
    let (address, requests, server) = spawn_server(1, |_, _| {
        response("304 Not Modified", &[("ETag", "\"v1\"")], b"")
    });
    let client = HttpClient::from_app("librebar-test", "0.1.0").unwrap();
    let validator = Validator::from_etag(HeaderValue::from_static("\"v1\""));

    let outcome = client
        .get_if_modified(&format!("http://{address}/item"), &validator)
        .await
        .unwrap();

    assert!(matches!(outcome, ConditionalResponse::NotModified(_)));
    let request = requests.recv().unwrap().to_ascii_lowercase();
    assert!(request.contains("if-none-match: \"v1\"\r\n"));
    assert!(!request.contains("if-modified-since:"));
    server.join().unwrap();
}

#[tokio::test]
async fn conditional_get_maps_success_to_modified() {
    let (address, requests, server) = spawn_server(1, |_, _| response("200 OK", &[], b"new"));
    let client = HttpClient::from_app("librebar-test", "0.1.0").unwrap();
    let validator = Validator::from_etag(HeaderValue::from_static("\"old\""));

    let result = client
        .get_if_modified(&format!("http://{address}/item"), &validator)
        .await
        .unwrap();

    assert!(matches!(
        result,
        ConditionalResponse::Modified(response) if response.bytes() == b"new"
    ));
    requests.recv().unwrap();
    server.join().unwrap();
}

#[tokio::test]
async fn conditional_get_maps_non_success_to_indeterminate() {
    for (status, expected) in [
        ("404 Not Found", StatusCode::NOT_FOUND),
        (
            "500 Internal Server Error",
            StatusCode::INTERNAL_SERVER_ERROR,
        ),
    ] {
        let (address, requests, server) =
            spawn_server(1, move |_, _| response(status, &[], b"failed"));
        let client = HttpClient::builder("librebar-test", "0.1.0")
            .retry_policy(RetryPolicy::none())
            .build()
            .unwrap();
        let validator = Validator::from_etag(HeaderValue::from_static("\"old\""));

        let result = client
            .get_if_modified(&format!("http://{address}/item"), &validator)
            .await
            .unwrap();

        assert!(matches!(
            result,
            ConditionalResponse::Indeterminate(response) if response.status() == expected
        ));
        requests.recv().unwrap();
        server.join().unwrap();
    }
}

#[tokio::test]
async fn conditional_get_falls_back_to_last_modified() {
    let (address, requests, server) =
        spawn_server(1, |_, _| response("304 Not Modified", &[], b""));
    let client = HttpClient::from_app("librebar-test", "0.1.0").unwrap();
    let validator =
        Validator::from_last_modified(HeaderValue::from_static("Wed, 21 Oct 2015 07:28:00 GMT"));

    client
        .get_if_modified(&format!("http://{address}/item"), &validator)
        .await
        .unwrap();

    let request = requests.recv().unwrap().to_ascii_lowercase();
    assert!(request.contains("if-modified-since: wed, 21 oct 2015 07:28:00 gmt\r\n"));
    server.join().unwrap();
}

#[tokio::test]
async fn check_modified_uses_head_and_does_not_fallback_on_405() {
    let (address, requests, server) =
        spawn_server(1, |_, _| response("405 Method Not Allowed", &[], b""));
    let client = HttpClient::from_app("librebar-test", "0.1.0").unwrap();
    let validator = Validator::from_etag(HeaderValue::from_static("\"v1\""));

    let result = client
        .check_modified(&format!("http://{address}/item"), &validator)
        .await
        .unwrap();

    assert!(matches!(result, ModificationCheck::Indeterminate(_)));
    assert!(requests.recv().unwrap().starts_with("HEAD /item "));
    server.join().unwrap();
}

#[tokio::test]
async fn check_modified_maps_304_and_200_without_bodies() {
    let (address, requests, server) = spawn_server(2, |index, _| {
        if index == 0 {
            response("304 Not Modified", &[("ETag", "\"v1\"")], b"")
        } else {
            response("200 OK", &[("ETag", "\"v2\"")], b"")
        }
    });
    let client = HttpClient::from_app("librebar-test", "0.1.0").unwrap();
    let validator = Validator::from_etag(HeaderValue::from_static("\"v1\""));
    let url = format!("http://{address}/item");

    let unchanged = client.check_modified(&url, &validator).await.unwrap();
    let changed = client.check_modified(&url, &validator).await.unwrap();

    assert!(matches!(unchanged, ModificationCheck::NotModified(_)));
    assert!(matches!(changed, ModificationCheck::Modified(_)));
    assert!(requests.recv().unwrap().starts_with("HEAD /item "));
    assert!(requests.recv().unwrap().starts_with("HEAD /item "));
    server.join().unwrap();
}

#[test]
fn client_config_defaults() {
    let cfg = HttpClientConfig::new("test-app", "0.1.0");
    assert_eq!(cfg.user_agent, "test-app/0.1.0");
    assert_eq!(cfg.timeout, Duration::from_secs(30));
    assert_eq!(cfg.max_redirects, 10);
    assert!(cfg.decompression);
    assert_eq!(cfg.retry_policy.retries(), 3);
    assert!(!cfg.retry_policy.retries_all_methods());
    assert_eq!(cfg.max_response_size, 16 * 1024 * 1024);
    #[cfg(feature = "http-cache")]
    assert_eq!(
        cfg.http_cache_stale_retention,
        Duration::from_secs(7 * 24 * 60 * 60)
    );
}

#[cfg(feature = "http-cache")]
#[test]
fn client_builder_configures_http_cache_retention() {
    let client = HttpClient::builder("test-app", "0.1.0")
        .http_cache_stale_retention(Duration::from_secs(90))
        .build()
        .unwrap();

    assert_eq!(
        client.config().http_cache_stale_retention,
        Duration::from_secs(90)
    );
}

#[test]
fn client_builder_configures_production_defaults() {
    let client = HttpClient::builder("test-app", "0.1.0")
        .max_redirects(5)
        .no_decompression()
        .retry_policy(RetryPolicy::none())
        .max_response_size(1024)
        .build()
        .unwrap();

    assert_eq!(client.config().max_redirects, 5);
    assert!(!client.config().decompression);
    assert_eq!(client.config().retry_policy.retries(), 0);
    assert_eq!(client.config().max_response_size, 1024);
}

#[test]
fn retry_policy_can_include_non_idempotent_methods() {
    let policy = RetryPolicy::new().max_retries(5).all_methods();

    assert_eq!(policy.retries(), 5);
    assert!(policy.retries_all_methods());
}

#[test]
fn client_config_custom_timeout() {
    let cfg = HttpClientConfig::new("test-app", "0.1.0").with_timeout(Duration::from_secs(5));
    assert_eq!(cfg.timeout, Duration::from_secs(5));
}

#[test]
fn client_config_custom_user_agent() {
    let cfg = HttpClientConfig::new("test-app", "0.1.0").with_user_agent("custom/1.0");
    assert_eq!(cfg.user_agent, "custom/1.0");
}

#[test]
fn client_construction() {
    let cfg = HttpClientConfig::new("test-app", "0.1.0");
    let client = HttpClient::new(cfg);
    assert!(client.is_ok());
}

#[test]
fn client_exposes_standard_http_methods() {
    let client = HttpClient::from_app("librebar-test", "0.1.0").unwrap();
    let url = "http://127.0.0.1:9/resource";

    std::mem::drop(client.post(url, b"create"));
    std::mem::drop(client.put(url, b"replace"));
    std::mem::drop(client.patch(url, b"change"));
    std::mem::drop(client.delete(url));
}

#[tokio::test]
async fn post_sends_method_body_and_default_user_agent() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (request_tx, request_rx) = mpsc::channel();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();

        let mut request = Vec::new();
        let mut buffer = [0_u8; 1024];
        loop {
            let count = stream.read(&mut buffer).unwrap();
            request.extend_from_slice(&buffer[..count]);

            let Some(header_end) = request.windows(4).position(|window| window == b"\r\n\r\n")
            else {
                continue;
            };
            let headers = String::from_utf8_lossy(&request[..header_end]);
            let content_length = headers
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length: ")
                        .and_then(|value| value.parse::<usize>().ok())
                })
                .unwrap_or_default();
            if request.len() >= header_end + 4 + content_length {
                break;
            }
        }

        request_tx
            .send(String::from_utf8(request).unwrap())
            .unwrap();
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
            .unwrap();
    });

    let client = HttpClient::from_app("librebar-test", "0.1.0").unwrap();
    let response = client
        .post(&format!("http://{address}/resource"), b"create")
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.bytes(), b"ok");
    let request = request_rx.recv().unwrap();
    assert!(
        request.starts_with("POST /resource HTTP/1.1\r\n"),
        "{request}"
    );
    assert!(
        request
            .to_ascii_lowercase()
            .contains("user-agent: librebar-test/0.1.0\r\n"),
        "{request}"
    );
    assert!(request.ends_with("\r\n\r\ncreate"), "{request}");
    server.join().unwrap();
}

async fn assert_follows_redirect(status: &'static str) {
    let (address, requests, server) = spawn_server(2, move |index, _| {
        if index == 0 {
            response(status, &[("Location", "/final")], b"")
        } else {
            response("200 OK", &[], b"arrived")
        }
    });
    let client = HttpClient::from_app("librebar-test", "0.1.0").unwrap();

    let result = client
        .get(&format!("http://{address}/start"))
        .await
        .unwrap();

    assert_eq!(result.status(), StatusCode::OK);
    assert_eq!(result.bytes(), b"arrived");
    assert!(requests.recv().unwrap().starts_with("GET /start "));
    assert!(requests.recv().unwrap().starts_with("GET /final "));
    server.join().unwrap();
}

#[tokio::test]
async fn follows_standard_redirect_statuses() {
    for status in [
        "301 Moved Permanently",
        "302 Found",
        "307 Temporary Redirect",
        "308 Permanent Redirect",
    ] {
        assert_follows_redirect(status).await;
    }
}

async fn assert_post_redirect_semantics(
    status: &'static str,
    expected_method: &'static str,
    expected_body: &'static str,
) {
    let (address, requests, server) = spawn_server(2, move |index, _| {
        if index == 0 {
            response(status, &[("Location", "/final")], b"")
        } else {
            response("200 OK", &[], b"arrived")
        }
    });
    let client = HttpClient::from_app("librebar-test", "0.1.0").unwrap();

    let result = client
        .post(&format!("http://{address}/start"), b"payload")
        .await
        .unwrap();

    assert_eq!(result.status(), StatusCode::OK);
    let initial = requests.recv().unwrap();
    assert!(initial.starts_with("POST /start "), "{initial}");
    assert!(initial.ends_with("\r\n\r\npayload"), "{initial}");
    let redirected = requests.recv().unwrap();
    assert!(
        redirected.starts_with(&format!("{expected_method} /final ")),
        "{redirected}"
    );
    assert!(
        redirected.ends_with(&format!("\r\n\r\n{expected_body}")),
        "{redirected}"
    );
    server.join().unwrap();
}

#[tokio::test]
async fn post_301_and_302_redirect_as_bodyless_get() {
    for status in ["301 Moved Permanently", "302 Found"] {
        assert_post_redirect_semantics(status, "GET", "").await;
    }
}

#[tokio::test]
async fn post_307_and_308_redirect_replay_method_and_body() {
    for status in ["307 Temporary Redirect", "308 Permanent Redirect"] {
        assert_post_redirect_semantics(status, "POST", "payload").await;
    }
}

#[tokio::test]
async fn zero_max_redirects_returns_redirect_response() {
    let (address, requests, server) = spawn_server(1, |_, _| {
        response("302 Found", &[("Location", "/final")], b"redirect")
    });
    let client = HttpClient::builder("librebar-test", "0.1.0")
        .max_redirects(0)
        .build()
        .unwrap();

    let result = client
        .get(&format!("http://{address}/start"))
        .await
        .unwrap();

    assert_eq!(result.status(), StatusCode::FOUND);
    assert_eq!(result.bytes(), b"redirect");
    requests.recv().unwrap();
    server.join().unwrap();
}

#[tokio::test]
async fn redirect_loop_is_an_error() {
    let (address, requests, server) = spawn_server(2, |index, _| {
        let location = if index == 0 { "/two" } else { "/one" };
        response("302 Found", &[("Location", location)], b"")
    });
    let client = HttpClient::from_app("librebar-test", "0.1.0").unwrap();

    let error = client
        .get(&format!("http://{address}/one"))
        .await
        .unwrap_err();

    assert!(error.to_string().contains("redirect loop"), "{error}");
    requests.recv().unwrap();
    requests.recv().unwrap();
    server.join().unwrap();
}

#[tokio::test]
async fn exceeding_redirect_limit_is_an_error() {
    let (address, requests, server) = spawn_server(2, |index, _| {
        let location = format!("/hop-{}", index + 1);
        response("302 Found", &[("Location", &location)], b"")
    });
    let client = HttpClient::builder("librebar-test", "0.1.0")
        .max_redirects(1)
        .build()
        .unwrap();

    let error = client
        .get(&format!("http://{address}/start"))
        .await
        .unwrap_err();

    assert!(error.to_string().contains("too many redirects"), "{error}");
    requests.recv().unwrap();
    requests.recv().unwrap();
    server.join().unwrap();
}

#[tokio::test]
async fn decompresses_gzip_responses_by_default() {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(b"compressed response").unwrap();
    let compressed = encoder.finish().unwrap();
    let (address, requests, server) = spawn_server(1, move |_, _| {
        response("200 OK", &[("Content-Encoding", "gzip")], &compressed)
    });
    let client = HttpClient::from_app("librebar-test", "0.1.0").unwrap();

    let result = client.get(&format!("http://{address}/gzip")).await.unwrap();

    assert_eq!(result.bytes(), b"compressed response");
    let request = requests.recv().unwrap().to_ascii_lowercase();
    assert!(request.contains("accept-encoding:"), "{request}");
    assert!(request.contains("gzip"), "{request}");
    server.join().unwrap();
}

#[tokio::test]
async fn decompresses_brotli_responses_by_default() {
    let mut compressed = Vec::new();
    {
        let mut encoder = brotli::CompressorWriter::new(&mut compressed, 4096, 5, 22);
        encoder.write_all(b"compressed response").unwrap();
    }
    let (address, requests, server) = spawn_server(1, move |_, _| {
        response("200 OK", &[("Content-Encoding", "br")], &compressed)
    });
    let client = HttpClient::from_app("librebar-test", "0.1.0").unwrap();

    let result = client
        .get(&format!("http://{address}/brotli"))
        .await
        .unwrap();

    assert_eq!(result.bytes(), b"compressed response");
    let request = requests.recv().unwrap().to_ascii_lowercase();
    assert!(request.contains("accept-encoding:"), "{request}");
    assert!(request.contains("br"), "{request}");
    server.join().unwrap();
}

#[tokio::test]
async fn decompression_can_be_disabled() {
    let compressed = b"still compressed".to_vec();
    let expected = compressed.clone();
    let (address, requests, server) = spawn_server(1, move |_, _| {
        response("200 OK", &[("Content-Encoding", "gzip")], &compressed)
    });
    let client = HttpClient::builder("librebar-test", "0.1.0")
        .no_decompression()
        .build()
        .unwrap();

    let result = client.get(&format!("http://{address}/raw")).await.unwrap();

    assert_eq!(result.bytes(), expected);
    let request = requests.recv().unwrap().to_ascii_lowercase();
    assert!(!request.contains("accept-encoding:"), "{request}");
    server.join().unwrap();
}

#[tokio::test]
async fn retries_server_errors_for_idempotent_methods() {
    let (address, requests, server) = spawn_server(4, |index, _| {
        if index < 3 {
            response("503 Service Unavailable", &[], b"retry")
        } else {
            response("200 OK", &[], b"recovered")
        }
    });
    let client = HttpClient::from_app("librebar-test", "0.1.0").unwrap();

    let result = client
        .get(&format!("http://{address}/retry"))
        .await
        .unwrap();

    assert_eq!(result.status(), StatusCode::OK);
    assert_eq!(result.bytes(), b"recovered");
    for _ in 0..4 {
        assert!(requests.recv().unwrap().starts_with("GET /retry "));
    }
    server.join().unwrap();
}

#[tokio::test]
async fn does_not_retry_client_errors() {
    let (address, requests, server) = spawn_server(1, |_, _| {
        response("429 Too Many Requests", &[], b"slow down")
    });
    let client = HttpClient::from_app("librebar-test", "0.1.0").unwrap();

    let result = client
        .get(&format!("http://{address}/limited"))
        .await
        .unwrap();

    assert_eq!(result.status(), StatusCode::TOO_MANY_REQUESTS);
    requests.recv().unwrap();
    server.join().unwrap();
}

#[tokio::test]
async fn does_not_retry_non_idempotent_methods_by_default() {
    let (address, requests, server) = spawn_server(1, |_, _| {
        response("503 Service Unavailable", &[], b"do not replay")
    });
    let client = HttpClient::from_app("librebar-test", "0.1.0").unwrap();

    let result = client
        .post(&format!("http://{address}/create"), b"payload")
        .await
        .unwrap();

    assert_eq!(result.status(), StatusCode::SERVICE_UNAVAILABLE);
    requests.recv().unwrap();
    server.join().unwrap();
}

#[tokio::test]
async fn all_methods_policy_retries_post() {
    let (address, requests, server) = spawn_server(2, |index, _| {
        if index == 0 {
            response("503 Service Unavailable", &[], b"retry")
        } else {
            response("200 OK", &[], b"created")
        }
    });
    let client = HttpClient::builder("librebar-test", "0.1.0")
        .retry_policy(RetryPolicy::new().max_retries(1).all_methods())
        .build()
        .unwrap();

    let result = client
        .post(&format!("http://{address}/create"), b"payload")
        .await
        .unwrap();

    assert_eq!(result.status(), StatusCode::OK);
    for _ in 0..2 {
        let request = requests.recv().unwrap();
        assert!(request.starts_with("POST /create "), "{request}");
        assert!(request.ends_with("\r\n\r\npayload"), "{request}");
    }
    server.join().unwrap();
}

#[tokio::test]
async fn retry_policy_none_disables_retries() {
    let (address, requests, server) = spawn_server(1, |_, _| {
        response("503 Service Unavailable", &[], b"unavailable")
    });
    let client = HttpClient::builder("librebar-test", "0.1.0")
        .retry_policy(RetryPolicy::none())
        .build()
        .unwrap();

    let result = client
        .get(&format!("http://{address}/retry"))
        .await
        .unwrap();

    assert_eq!(result.status(), StatusCode::SERVICE_UNAVAILABLE);
    requests.recv().unwrap();
    server.join().unwrap();
}

#[tokio::test]
async fn retries_transport_errors() {
    let (address, requests, server) = spawn_server(4, |index, _| {
        if index < 3 {
            Vec::new()
        } else {
            response("200 OK", &[], b"recovered")
        }
    });
    let client = HttpClient::from_app("librebar-test", "0.1.0").unwrap();

    let result = client
        .get(&format!("http://{address}/unstable"))
        .await
        .unwrap();

    assert_eq!(result.status(), StatusCode::OK);
    for _ in 0..4 {
        requests.recv().unwrap();
    }
    server.join().unwrap();
}

#[tokio::test]
async fn retries_connection_errors_while_reading_the_response_body() {
    let (address, requests, server) = spawn_server(2, |index, _| {
        if index == 0 {
            b"HTTP/1.1 200 OK\r\nContent-Length: 20\r\nConnection: close\r\n\r\npartial".to_vec()
        } else {
            response("200 OK", &[], b"recovered")
        }
    });
    let client = HttpClient::from_app("librebar-test", "0.1.0").unwrap();

    let result = client
        .get(&format!("http://{address}/unstable-body"))
        .await
        .unwrap();

    assert_eq!(result.status(), StatusCode::OK);
    assert_eq!(result.bytes(), b"recovered");
    for _ in 0..2 {
        requests.recv().unwrap();
    }
    server.join().unwrap();
}

#[tokio::test]
async fn decompressed_responses_are_bounded() {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&vec![b'x'; 1024]).unwrap();
    let compressed = encoder.finish().unwrap();
    let (address, requests, server) = spawn_server(1, move |_, _| {
        response("200 OK", &[("Content-Encoding", "gzip")], &compressed)
    });
    let client = HttpClient::builder("librebar-test", "0.1.0")
        .max_response_size(64)
        .build()
        .unwrap();

    let error = client
        .get(&format!("http://{address}/large"))
        .await
        .unwrap_err();

    assert!(error.to_string().contains("64 bytes"), "{error}");
    requests.recv().unwrap();
    server.join().unwrap();
}

#[tokio::test]
async fn retries_use_exponential_backoff() {
    let (address, requests, server) = spawn_server(3, |index, _| {
        if index < 2 {
            response("503 Service Unavailable", &[], b"retry")
        } else {
            response("200 OK", &[], b"recovered")
        }
    });
    let client = HttpClient::builder("librebar-test", "0.1.0")
        .retry_policy(RetryPolicy::new().max_retries(2))
        .build()
        .unwrap();

    let started = Instant::now();
    let result = client
        .get(&format!("http://{address}/retry"))
        .await
        .unwrap();
    let elapsed = started.elapsed();

    assert_eq!(result.status(), StatusCode::OK);
    assert!(
        elapsed >= Duration::from_millis(140),
        "elapsed: {elapsed:?}"
    );
    for _ in 0..3 {
        requests.recv().unwrap();
    }
    server.join().unwrap();
}

#[tokio::test]
#[ignore = "hits api.github.com; run with --run-ignored or `-- --ignored`"]
async fn https_get_succeeds() {
    let client = HttpClient::from_app("librebar-test", "0.1.0").unwrap();
    let resp = client
        .get("https://api.github.com/zen")
        .await
        .expect("HTTPS GET should succeed");
    assert!(resp.is_success(), "status: {}", resp.status());
    let body = resp.text().unwrap();
    assert!(
        !body.is_empty(),
        "GitHub zen should return a non-empty string"
    );
}

#[tokio::test]
#[ignore = "hits httpbin.org; run with --run-ignored or `-- --ignored`"]
async fn http_get_succeeds() {
    let client = HttpClient::from_app("librebar-test", "0.1.0").unwrap();
    let resp = client
        .get("http://httpbin.org/get")
        .await
        .expect("HTTP GET should succeed");
    assert!(resp.is_success(), "status: {}", resp.status());
}
