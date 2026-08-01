#![allow(missing_docs, unsafe_code)]
#![cfg(feature = "update")]

use librebar::cache::Cache;
use librebar::error::BoxError;
use librebar::http::HttpClient;
use librebar::update::{
    GitHubReleaseSource, ReleaseInfo, ReleaseSource, UpdateChecker, UpdateInfo, async_trait,
};
use std::error::Error as _;
use std::io::{self, Read as _, Write as _};
use std::net::TcpListener;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::Duration;
use tempfile::TempDir;

// Process-global env vars are shared across threads. nextest sidesteps this
// by running each test in its own process, but `cargo test` runs them on
// threads within one process — a mutation in one test will race with a read
// in another. This file-level lock serializes the tests that touch env so
// the suite works under either runner.
static ENV_LOCK: Mutex<()> = Mutex::new(());

fn spawn_release_server(
    body: &'static str,
) -> (String, mpsc::Receiver<String>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (requests, received) = mpsc::channel();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = Vec::new();
        let mut buffer = [0_u8; 1024];
        while !request.windows(4).any(|window| window == b"\r\n\r\n") {
            let read = stream.read(&mut buffer).unwrap();
            if read == 0 {
                break;
            }
            request.extend_from_slice(&buffer[..read]);
        }
        requests.send(String::from_utf8(request).unwrap()).unwrap();
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body,
        )
        .unwrap();
    });
    (format!("http://{address}"), received, server)
}

struct StaticSource {
    release: ReleaseInfo,
}

#[async_trait]
impl ReleaseSource for StaticSource {
    async fn latest_release(&self) -> Result<ReleaseInfo, BoxError> {
        Ok(self.release.clone())
    }
}

struct FailingSource;

#[async_trait]
impl ReleaseSource for FailingSource {
    async fn latest_release(&self) -> Result<ReleaseInfo, BoxError> {
        Err(Box::new(io::Error::other("source unavailable")))
    }
}

struct CountingSource {
    calls: Arc<AtomicUsize>,
    release: ReleaseInfo,
}

#[async_trait]
impl ReleaseSource for CountingSource {
    async fn latest_release(&self) -> Result<ReleaseInfo, BoxError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(self.release.clone())
    }
}

fn checker() -> UpdateChecker {
    UpdateChecker::new(
        "test-app",
        "0.1.0",
        StaticSource {
            release: ReleaseInfo::new("0.2.0", "https://example.com/releases/0.2.0"),
        },
    )
}

#[test]
fn checker_from_app_name() {
    let checker = checker();
    assert_eq!(checker.app_name(), "test-app");
    assert_eq!(checker.current_version(), "0.1.0");
}

#[test]
fn github_checker_wires_the_default_backend() {
    let checker = UpdateChecker::github("test-app", "0.1.0", "owner/repo").unwrap();

    assert_eq!(checker.app_name(), "test-app");
    assert_eq!(checker.current_version(), "0.1.0");
}

#[test]
fn checker_debug_is_source_opaque() {
    let debug = format!("{:?}", checker());

    assert!(debug.contains("UpdateChecker"));
    assert!(debug.contains("test-app"));
    assert!(!debug.contains("StaticSource"));
}

#[tokio::test]
async fn checker_uses_an_injected_release_source() {
    let checker = UpdateChecker::new(
        "test-app",
        "0.1.0",
        StaticSource {
            release: ReleaseInfo::new("0.2.0", "https://example.com/releases/0.2.0"),
        },
    )
    .without_cache();

    let update = checker.check().await.unwrap().unwrap();

    assert_eq!(update.current, "0.1.0");
    assert_eq!(update.latest, "0.2.0");
    assert_eq!(update.url, "https://example.com/releases/0.2.0");
}

#[tokio::test]
async fn source_errors_remain_in_the_error_chain() {
    let checker = UpdateChecker::new("test-app", "0.1.0", FailingSource).without_cache();

    let error = checker.check().await.unwrap_err();

    assert_eq!(error.to_string(), "release source failed");
    assert_eq!(error.source().unwrap().to_string(), "source unavailable");
}

#[tokio::test]
async fn injected_cache_preserves_the_release_and_skips_the_source() {
    let tmp = TempDir::new().unwrap();
    let cache = Cache::new(tmp.path());
    let calls = Arc::new(AtomicUsize::new(0));

    for _ in 0..2 {
        let checker = UpdateChecker::new(
            "test-app",
            "0.1.0",
            CountingSource {
                calls: Arc::clone(&calls),
                release: ReleaseInfo::new("0.2.0", "https://example.com/custom-release"),
            },
        )
        .with_cache(cache.clone());

        let update = checker.check().await.unwrap().unwrap();
        assert_eq!(update.url, "https://example.com/custom-release");
    }

    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn corrupt_cache_falls_back_to_the_source() {
    let tmp = TempDir::new().unwrap();
    let cache = Cache::new(tmp.path());
    cache
        .set("latest-release", b"not JSON", Duration::from_secs(60))
        .unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let checker = UpdateChecker::new(
        "test-app",
        "0.1.0",
        CountingSource {
            calls: Arc::clone(&calls),
            release: ReleaseInfo::new("0.2.0", "https://example.com/fallback-release"),
        },
    )
    .with_cache(cache);

    let update = checker.check().await.unwrap().unwrap();

    assert_eq!(update.url, "https://example.com/fallback-release");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn github_source_fetches_a_release_from_a_custom_api() {
    let (api_base, requests, server) = spawn_release_server(
        r#"{"tag_name":"v0.2.0","html_url":"https://example.com/releases/0.2.0"}"#,
    );
    let source = GitHubReleaseSource::new(
        "owner/repo",
        HttpClient::from_app("test-app", "0.1.0").unwrap(),
    )
    .with_api_base(api_base);

    let release = source.latest_release().await.unwrap();

    assert_eq!(release.version, "0.2.0");
    assert_eq!(release.url, "https://example.com/releases/0.2.0");
    assert!(
        requests
            .recv()
            .unwrap()
            .starts_with("GET /repos/owner/repo/releases/latest ")
    );
    server.join().unwrap();
}

#[tokio::test]
async fn github_source_sends_bearer_auth_without_debug_exposure() {
    let (api_base, requests, server) = spawn_release_server(
        r#"{"tag_name":"v0.2.0","html_url":"https://example.com/releases/0.2.0"}"#,
    );
    let source = GitHubReleaseSource::new(
        "owner/repo",
        HttpClient::from_app("test-app", "0.1.0").unwrap(),
    )
    .with_api_base(api_base)
    .with_bearer_token("secret-token");

    assert!(!format!("{source:?}").contains("secret-token"));
    source.latest_release().await.unwrap();

    assert!(
        requests
            .recv()
            .unwrap()
            .to_ascii_lowercase()
            .contains("authorization: bearer secret-token")
    );
    server.join().unwrap();
}

#[tokio::test]
async fn malformed_github_release_preserves_the_decode_error() {
    let (api_base, _requests, server) = spawn_release_server("{}");
    let source = GitHubReleaseSource::new(
        "owner/repo",
        HttpClient::from_app("test-app", "0.1.0").unwrap(),
    )
    .with_api_base(api_base);

    let error = source.latest_release().await.unwrap_err();

    assert_eq!(
        error.to_string(),
        "GitHub release API returned invalid JSON"
    );
    assert!(error.source().is_some());
    server.join().unwrap();
}

#[test]
fn suppressed_by_env_var() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    // SAFETY: ENV_LOCK serializes env-touching tests in this file.
    unsafe { std::env::set_var("TEST_APP_NO_UPDATE_CHECK", "1") };
    let checker = checker();
    assert!(checker.is_suppressed());
    // SAFETY: still holding ENV_LOCK.
    unsafe { std::env::remove_var("TEST_APP_NO_UPDATE_CHECK") };
}

#[test]
fn not_suppressed_by_default() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    // SAFETY: ENV_LOCK serializes env-touching tests in this file.
    unsafe { std::env::remove_var("TEST_APP_NO_UPDATE_CHECK") };
    let checker = checker();
    assert!(!checker.is_suppressed());
}

#[test]
fn version_is_newer() {
    assert!(librebar::update::is_newer("0.1.0", "0.2.0"));
    assert!(librebar::update::is_newer("0.1.0", "1.0.0"));
    assert!(librebar::update::is_newer("1.2.3", "1.2.4"));
    assert!(!librebar::update::is_newer("0.2.0", "0.1.0"));
    assert!(!librebar::update::is_newer("1.0.0", "1.0.0"));
}

#[test]
fn update_info_display() {
    let info = UpdateInfo::new(
        "0.1.0",
        "0.2.0",
        "https://github.com/owner/repo/releases/tag/v0.2.0",
    );
    let msg = info.message();
    assert!(msg.contains("0.2.0"));
    assert!(msg.contains("0.1.0"));
}
