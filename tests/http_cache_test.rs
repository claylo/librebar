#![allow(missing_docs)]
#![cfg(feature = "http-cache")]

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener};
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Duration;

use base64::Engine as _;
use flate2::Compression;
use flate2::write::GzEncoder;
use librebar::cache::Cache;
use librebar::http::{CacheStatus, HttpClient, RetryPolicy, StatusCode};

fn read_request(stream: &mut impl Read) -> String {
    let mut request = Vec::new();
    let mut buffer = [0_u8; 1024];
    loop {
        let count = stream.read(&mut buffer).unwrap();
        request.extend_from_slice(&buffer[..count]);
        if request.windows(4).any(|window| window == b"\r\n\r\n") {
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
            stream.write_all(&response).unwrap();
            request_tx.send(request).unwrap();
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

#[tokio::test]
async fn direct_get_has_no_cache_status() {
    let (address, requests, server) = spawn_server(1, |_, _| response("200 OK", &[], b"direct"));
    let client = HttpClient::from_app("librebar-test", "0.1.0").unwrap();

    let response = client.get(&format!("http://{address}/item")).await.unwrap();

    assert_eq!(response.cache_status(), None);
    requests.recv().unwrap();
    server.join().unwrap();
}

#[tokio::test]
async fn cached_get_misses_then_serves_a_fresh_hit_without_network() {
    let cache_dir = tempfile::tempdir().unwrap();
    let cache = Cache::new(cache_dir.path());
    let (address, requests, server) = spawn_server(1, |_, _| {
        response(
            "200 OK",
            &[("Cache-Control", "max-age=3600"), ("ETag", "\"v1\"")],
            b"version one",
        )
    });
    let client = HttpClient::from_app("librebar-test", "0.1.0").unwrap();
    let url = format!("http://{address}/item");

    let miss = client.get_cached(&cache, "item", &url).await.unwrap();
    let hit = client.get_cached(&cache, "item", &url).await.unwrap();

    assert_eq!(miss.cache_status(), Some(CacheStatus::Miss));
    assert_eq!(hit.cache_status(), Some(CacheStatus::Hit));
    assert_eq!(hit.status(), StatusCode::OK);
    assert_eq!(hit.bytes(), b"version one");
    requests.recv().unwrap();
    server.join().unwrap();
}

#[tokio::test]
async fn stale_entry_revalidates_and_losslessly_merges_304_headers() {
    let cache_dir = tempfile::tempdir().unwrap();
    let cache = Cache::new(cache_dir.path());
    let (address, requests, server) = spawn_server(2, |index, request| {
        if index == 0 {
            response(
                "200 OK",
                &[
                    ("Cache-Control", "max-age=0"),
                    ("ETag", "\"v1\""),
                    ("Link", "</old-a>"),
                    ("Link", "</old-b>"),
                ],
                b"version one",
            )
        } else {
            assert!(
                request
                    .to_ascii_lowercase()
                    .contains("if-none-match: \"v1\"\r\n")
            );
            response(
                "304 Not Modified",
                &[
                    ("ETag", "\"v1\""),
                    ("Cache-Control", "max-age=3600"),
                    ("Link", "</new-a>"),
                    ("Link", "</new-b>"),
                ],
                b"",
            )
        }
    });
    let client = HttpClient::from_app("librebar-test", "0.1.0").unwrap();
    let url = format!("http://{address}/item");

    let miss = client.get_cached(&cache, "item", &url).await.unwrap();
    let revalidated = client.get_cached(&cache, "item", &url).await.unwrap();

    assert_eq!(miss.cache_status(), Some(CacheStatus::Miss));
    assert_eq!(revalidated.cache_status(), Some(CacheStatus::Revalidated));
    assert_eq!(revalidated.status(), StatusCode::OK);
    assert_eq!(revalidated.bytes(), b"version one");
    assert_eq!(
        revalidated
            .headers()
            .get_all("link")
            .iter()
            .map(|value| value.to_str().unwrap())
            .collect::<Vec<_>>(),
        ["</new-a>", "</new-b>"]
    );
    requests.recv().unwrap();
    requests.recv().unwrap();
    server.join().unwrap();
}

#[tokio::test]
async fn modified_revalidation_replaces_the_cached_representation() {
    let cache_dir = tempfile::tempdir().unwrap();
    let cache = Cache::new(cache_dir.path());
    let (address, requests, server) = spawn_server(2, |index, _| {
        if index == 0 {
            response(
                "200 OK",
                &[("Cache-Control", "max-age=0"), ("ETag", "\"v1\"")],
                b"version one",
            )
        } else {
            response(
                "200 OK",
                &[("Cache-Control", "max-age=3600"), ("ETag", "\"v2\"")],
                b"version two",
            )
        }
    });
    let client = HttpClient::from_app("librebar-test", "0.1.0").unwrap();
    let url = format!("http://{address}/item");

    client.get_cached(&cache, "item", &url).await.unwrap();
    let replaced = client.get_cached(&cache, "item", &url).await.unwrap();
    let hit = client.get_cached(&cache, "item", &url).await.unwrap();

    assert_eq!(replaced.cache_status(), Some(CacheStatus::Miss));
    assert_eq!(replaced.bytes(), b"version two");
    assert_eq!(replaced.validator().unwrap().etag().unwrap(), "\"v2\"");
    assert_eq!(hit.cache_status(), Some(CacheStatus::Hit));
    assert_eq!(hit.bytes(), b"version two");
    requests.recv().unwrap();
    requests.recv().unwrap();
    server.join().unwrap();
}

#[tokio::test]
async fn vary_mismatch_fetches_and_replaces_the_entry() {
    let cache_dir = tempfile::tempdir().unwrap();
    let cache = Cache::new(cache_dir.path());
    let (address, requests, server) = spawn_server(2, |index, _| {
        response(
            "200 OK",
            &[("Cache-Control", "max-age=3600"), ("Vary", "User-Agent")],
            if index == 0 { b"first" } else { b"second" },
        )
    });
    let first = HttpClient::builder("first", "1").build().unwrap();
    let second = HttpClient::builder("second", "1").build().unwrap();
    let url = format!("http://{address}/item");

    first.get_cached(&cache, "item", &url).await.unwrap();
    let changed = second.get_cached(&cache, "item", &url).await.unwrap();

    assert_eq!(changed.cache_status(), Some(CacheStatus::Miss));
    assert_eq!(changed.bytes(), b"second");
    requests.recv().unwrap();
    requests.recv().unwrap();
    server.join().unwrap();
}

#[tokio::test]
async fn no_store_response_is_returned_without_persistence() {
    let cache_dir = tempfile::tempdir().unwrap();
    let cache = Cache::new(cache_dir.path());
    let (address, requests, server) = spawn_server(1, |_, _| {
        response("200 OK", &[("Cache-Control", "no-store")], b"private")
    });
    let client = HttpClient::from_app("librebar-test", "0.1.0").unwrap();

    let result = client
        .get_cached(&cache, "item", &format!("http://{address}/item"))
        .await
        .unwrap();

    assert_eq!(result.cache_status(), Some(CacheStatus::Miss));
    assert_eq!(result.bytes(), b"private");
    assert!(
        std::fs::read_dir(cache_dir.path())
            .unwrap()
            .next()
            .is_none()
    );
    requests.recv().unwrap();
    server.join().unwrap();
}

#[tokio::test]
async fn non_storable_revalidation_removes_the_old_representation() {
    let cache_dir = tempfile::tempdir().unwrap();
    let cache = Cache::new(cache_dir.path());
    let (address, requests, server) = spawn_server(3, |index, _| {
        if index == 0 {
            response(
                "200 OK",
                &[("Cache-Control", "max-age=0"), ("ETag", "\"v1\"")],
                b"old",
            )
        } else {
            response("200 OK", &[("Cache-Control", "no-store")], b"private")
        }
    });
    let client = HttpClient::from_app("librebar-test", "0.1.0").unwrap();
    let url = format!("http://{address}/item");

    client.get_cached(&cache, "item", &url).await.unwrap();
    let replacement = client.get_cached(&cache, "item", &url).await.unwrap();
    let next = client.get_cached(&cache, "item", &url).await.unwrap();

    assert_eq!(replacement.cache_status(), Some(CacheStatus::Miss));
    assert_eq!(replacement.bytes(), b"private");
    assert_eq!(next.cache_status(), Some(CacheStatus::Miss));
    for _ in 0..3 {
        requests.recv().unwrap();
    }
    server.join().unwrap();
}

#[tokio::test]
async fn zero_retention_turns_immediately_stale_entries_into_misses() {
    let cache_dir = tempfile::tempdir().unwrap();
    let cache = Cache::new(cache_dir.path());
    let (address, requests, server) = spawn_server(2, |_, _| {
        response(
            "200 OK",
            &[("Cache-Control", "max-age=0"), ("ETag", "\"v1\"")],
            b"version",
        )
    });
    let client = HttpClient::builder("librebar-test", "0.1.0")
        .http_cache_stale_retention(Duration::ZERO)
        .build()
        .unwrap();
    let url = format!("http://{address}/item");

    client.get_cached(&cache, "item", &url).await.unwrap();
    let second = client.get_cached(&cache, "item", &url).await.unwrap();

    assert_eq!(second.cache_status(), Some(CacheStatus::Miss));
    requests.recv().unwrap();
    let second_request = requests.recv().unwrap().to_ascii_lowercase();
    assert!(!second_request.contains("if-none-match:"));
    server.join().unwrap();
}

#[tokio::test]
async fn corrupt_entry_is_removed_and_refetched() {
    let cache_dir = tempfile::tempdir().unwrap();
    let cache = Cache::new(cache_dir.path());
    let (address, requests, server) = spawn_server(2, |index, _| {
        response(
            "200 OK",
            &[("Cache-Control", "max-age=3600")],
            if index == 0 { b"first" } else { b"recovered" },
        )
    });
    let client = HttpClient::from_app("librebar-test", "0.1.0").unwrap();
    let url = format!("http://{address}/item");
    client.get_cached(&cache, "item", &url).await.unwrap();
    let path = std::fs::read_dir(cache_dir.path())
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    std::fs::write(path, b"not an envelope").unwrap();

    let recovered = client.get_cached(&cache, "item", &url).await.unwrap();

    assert_eq!(recovered.cache_status(), Some(CacheStatus::Miss));
    assert_eq!(recovered.bytes(), b"recovered");
    requests.recv().unwrap();
    requests.recv().unwrap();
    server.join().unwrap();
}

#[tokio::test]
async fn cache_write_failure_does_not_discard_network_response() {
    let cache_dir = tempfile::tempdir().unwrap();
    let internal_key = "http:v1:item";
    let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(internal_key);
    let entry_path = cache_dir.path().join(format!("v1-{encoded}.json"));
    let cache = Cache::new(cache_dir.path());
    let (address, requests, server) = spawn_server(1, move |_, _| {
        std::fs::create_dir(&entry_path).unwrap();
        response(
            "200 OK",
            &[("Cache-Control", "max-age=3600")],
            b"network still wins",
        )
    });
    let client = HttpClient::from_app("librebar-test", "0.1.0").unwrap();

    let result = client
        .get_cached(&cache, "item", &format!("http://{address}/item"))
        .await
        .unwrap();

    assert_eq!(result.cache_status(), Some(CacheStatus::Miss));
    assert_eq!(result.bytes(), b"network still wins");
    requests.recv().unwrap();
    server.join().unwrap();
}

#[tokio::test]
async fn ordinary_cache_read_errors_are_returned_before_network() {
    let directory = tempfile::tempdir().unwrap();
    let path_that_is_a_file = directory.path().join("not-a-directory");
    std::fs::write(&path_that_is_a_file, b"file").unwrap();
    let cache = Cache::new(&path_that_is_a_file);
    let client = HttpClient::from_app("librebar-test", "0.1.0").unwrap();

    let error = client
        .get_cached(&cache, "item", "http://127.0.0.1:9/item")
        .await
        .expect_err("ordinary cache read I/O errors must be returned");

    assert!(error.to_string().contains("cache error"), "{error}");
}

#[tokio::test]
async fn caches_the_final_response_after_a_redirect() {
    let cache_dir = tempfile::tempdir().unwrap();
    let cache = Cache::new(cache_dir.path());
    let (address, requests, server) = spawn_server(2, |index, _| {
        if index == 0 {
            response("302 Found", &[("Location", "/final")], b"")
        } else {
            response(
                "200 OK",
                &[("Cache-Control", "max-age=3600")],
                b"final representation",
            )
        }
    });
    let client = HttpClient::from_app("librebar-test", "0.1.0").unwrap();
    let url = format!("http://{address}/start");

    let miss = client.get_cached(&cache, "redirect", &url).await.unwrap();
    let hit = client.get_cached(&cache, "redirect", &url).await.unwrap();

    assert_eq!(miss.cache_status(), Some(CacheStatus::Miss));
    assert_eq!(hit.cache_status(), Some(CacheStatus::Hit));
    assert_eq!(hit.status(), StatusCode::OK);
    assert_eq!(hit.bytes(), b"final representation");
    assert!(requests.recv().unwrap().starts_with("GET /start "));
    assert!(requests.recv().unwrap().starts_with("GET /final "));
    server.join().unwrap();
}

#[tokio::test]
async fn caches_the_decompressed_representation() {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(b"decompressed representation").unwrap();
    let compressed = encoder.finish().unwrap();
    let cache_dir = tempfile::tempdir().unwrap();
    let cache = Cache::new(cache_dir.path());
    let (address, requests, server) = spawn_server(1, move |_, _| {
        response(
            "200 OK",
            &[
                ("Cache-Control", "max-age=3600"),
                ("Content-Encoding", "gzip"),
            ],
            &compressed,
        )
    });
    let client = HttpClient::from_app("librebar-test", "0.1.0").unwrap();
    let url = format!("http://{address}/compressed");

    let miss = client.get_cached(&cache, "compressed", &url).await.unwrap();
    let hit = client.get_cached(&cache, "compressed", &url).await.unwrap();

    assert_eq!(miss.bytes(), b"decompressed representation");
    assert_eq!(hit.cache_status(), Some(CacheStatus::Hit));
    assert_eq!(hit.bytes(), b"decompressed representation");
    assert!(hit.header("content-encoding").is_none());
    requests.recv().unwrap();
    server.join().unwrap();
}

#[tokio::test]
async fn caches_only_the_successful_response_after_retries() {
    let cache_dir = tempfile::tempdir().unwrap();
    let cache = Cache::new(cache_dir.path());
    let (address, requests, server) = spawn_server(2, |index, _| {
        if index == 0 {
            response("503 Service Unavailable", &[], b"retry")
        } else {
            response("200 OK", &[("Cache-Control", "max-age=3600")], b"recovered")
        }
    });
    let client = HttpClient::builder("librebar-test", "0.1.0")
        .retry_policy(RetryPolicy::new().max_retries(1))
        .build()
        .unwrap();
    let url = format!("http://{address}/retry");

    let miss = client.get_cached(&cache, "retry", &url).await.unwrap();
    let hit = client.get_cached(&cache, "retry", &url).await.unwrap();

    assert_eq!(miss.status(), StatusCode::OK);
    assert_eq!(miss.bytes(), b"recovered");
    assert_eq!(hit.cache_status(), Some(CacheStatus::Hit));
    assert_eq!(hit.bytes(), b"recovered");
    requests.recv().unwrap();
    requests.recv().unwrap();
    server.join().unwrap();
}

#[cfg(feature = "http-cookies")]
#[tokio::test]
async fn vary_cookie_uses_fingerprints_without_persisting_cookie_values() {
    let cache_dir = tempfile::tempdir().unwrap();
    let cache = Cache::new(cache_dir.path());
    let (address, requests, server) = spawn_server(4, |index, request| match index {
        0 => response(
            "200 OK",
            &[("Set-Cookie", "session=one; Path=/; HttpOnly")],
            b"logged in",
        ),
        1 => {
            assert!(!request.contains("sha256:"), "{request}");
            assert!(
                request
                    .to_ascii_lowercase()
                    .contains("cookie: session=one\r\n"),
                "{request}"
            );
            response(
                "200 OK",
                &[("Cache-Control", "max-age=3600"), ("Vary", "Cookie")],
                b"first identity",
            )
        }
        2 => response(
            "200 OK",
            &[("Set-Cookie", "session=two; Path=/; HttpOnly")],
            b"rotated",
        ),
        _ => {
            assert!(!request.contains("sha256:"), "{request}");
            assert!(
                request
                    .to_ascii_lowercase()
                    .contains("cookie: session=two\r\n"),
                "{request}"
            );
            response(
                "200 OK",
                &[("Cache-Control", "max-age=3600"), ("Vary", "Cookie")],
                b"second identity",
            )
        }
    });
    let client = HttpClient::builder("librebar-test", "0.1.0")
        .with_cookie_jar()
        .build()
        .unwrap();
    let base = format!("http://{address}");

    client.get(&format!("{base}/login")).await.unwrap();
    let first = client
        .get_cached(&cache, "profile", &format!("{base}/profile"))
        .await
        .unwrap();
    let stored = cache.get("http:v1:profile").unwrap().unwrap();
    assert!(
        !stored
            .windows(b"session=one".len())
            .any(|value| value == b"session=one")
    );

    client.get(&format!("{base}/rotate")).await.unwrap();
    let changed = client
        .get_cached(&cache, "profile", &format!("{base}/profile"))
        .await
        .unwrap();
    let hit = client
        .get_cached(&cache, "profile", &format!("{base}/profile"))
        .await
        .unwrap();

    assert_eq!(first.bytes(), b"first identity");
    assert_eq!(changed.cache_status(), Some(CacheStatus::Miss));
    assert_eq!(changed.bytes(), b"second identity");
    assert_eq!(hit.cache_status(), Some(CacheStatus::Hit));
    assert_eq!(hit.bytes(), b"second identity");
    for _ in 0..4 {
        requests.recv().unwrap();
    }
    server.join().unwrap();
}
