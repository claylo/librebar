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

use librebar::http::{HttpClient, HttpClientConfig};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::time::Duration;

#[test]
fn client_config_defaults() {
    let cfg = HttpClientConfig::new("test-app", "0.1.0");
    assert_eq!(cfg.user_agent, "test-app/0.1.0");
    assert_eq!(cfg.timeout, Duration::from_secs(30));
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

    assert_eq!(response.status, 200);
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

#[tokio::test]
#[ignore = "hits api.github.com; run with --run-ignored or `-- --ignored`"]
async fn https_get_succeeds() {
    let client = HttpClient::from_app("librebar-test", "0.1.0").unwrap();
    let resp = client
        .get("https://api.github.com/zen")
        .await
        .expect("HTTPS GET should succeed");
    assert!(resp.is_success(), "status: {}", resp.status);
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
    assert!(resp.is_success(), "status: {}", resp.status);
}
