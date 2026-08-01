//! HTTP/HTTPS client with tracing integration.
//!
//! Provides a production-oriented wrapper around Hyper with:
//! - TLS via rustls (Mozilla CA roots, no system OpenSSL dependency)
//! - HTTP/2 with HTTP/1.1 fallback
//! - Redirect following (10 hops by default, with loop detection)
//! - Transparent gzip and Brotli response decompression
//! - Retries for idempotent methods on 5xx and transport failures
//! - A 16 MiB decoded response limit, configurable through the builder
//! - Configurable user-agent and whole-operation timeout
//! - Explicit per-client cookies behind the `http-cookies` feature
//! - `#[tracing::instrument]` on every request
//! - GET, POST, PUT, PATCH, DELETE, and arbitrary [`Request`] support
//! - Simple [`Response`] type with status and body bytes
//!
//! # Example
//!
//! ```no_run
//! use librebar::http::HttpClient;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let client = HttpClient::from_app("my-app", "1.0.0")?;
//! let resp = client.get("https://api.github.com/repos/owner/repo/releases/latest").await?;
//! if resp.is_success() {
//!     println!("{}", resp.text()?);
//! }
//! # Ok(())
//! # }
//! ```
//!
//! Custom headers use the standard HTTP request builder:
//!
//! ```no_run
//! use librebar::http::{Bytes, HttpClient, Method, Request};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let client = HttpClient::from_app("my-app", "1.0.0")?;
//! let request = Request::builder()
//!     .method(Method::POST)
//!     .uri("https://example.com/widgets")
//!     .header("content-type", "application/json")
//!     .body(Bytes::from_static(br#"{"name":"gizmo"}"#))?;
//! let response = client.send(request).await?;
//! # let _ = response;
//! # Ok(())
//! # }
//! ```

use std::collections::HashSet;
use std::time::Duration;

use http_body_util::{BodyExt, Full, combinators::UnsyncBoxBody};
use hyper_util::rt::TokioExecutor;
use tower::util::BoxCloneSyncService;
use tower::{BoxError, ServiceBuilder, ServiceExt};
use tower_http::decompression::DecompressionLayer;
use tower_http::follow_redirect::FollowRedirect;
use tower_http::follow_redirect::policy::{
    Action, Attempt, FilterCredentials, Policy as RedirectPolicyTrait,
};

#[cfg(feature = "http-cookies")]
mod cookies;
#[cfg(feature = "http-cookies")]
pub use cookies::CookieJar;

use crate::Result;
use crate::error::HttpError;

pub use hyper::body::Bytes;
pub use hyper::{Method, Request};

// ─── Config ─────────────────────────────────────────────────────────

/// Configuration for [`HttpClient`].
#[derive(Debug)]
pub struct HttpClientConfig {
    /// Value sent as the `User-Agent` header on every request.
    pub user_agent: String,
    /// Whole-operation timeout, including redirects and retry backoff.
    pub timeout: Duration,
    /// Maximum number of redirects to follow. Zero disables redirect following.
    pub max_redirects: usize,
    /// Whether gzip and Brotli responses are requested and decompressed.
    pub decompression: bool,
    /// Retry behavior for transient failures.
    pub retry_policy: RetryPolicy,
    /// Maximum decoded response body retained in memory. Zero disables the limit.
    pub max_response_size: usize,
}

impl HttpClientConfig {
    /// Build a config with a `"name/version"` user-agent and 30 s timeout.
    pub fn new(app_name: &str, version: &str) -> Self {
        Self {
            user_agent: format!("{app_name}/{version}"),
            timeout: Duration::from_secs(30),
            max_redirects: 10,
            decompression: true,
            retry_policy: RetryPolicy::new(),
            max_response_size: 16 * 1024 * 1024,
        }
    }

    /// Override the timeout (builder style).
    #[must_use]
    pub const fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Override the user-agent string (builder style).
    #[must_use]
    pub fn with_user_agent(mut self, user_agent: &str) -> Self {
        self.user_agent = user_agent.to_string();
        self
    }
}

/// Retry behavior for transient HTTP failures.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetryPolicy {
    max_retries: usize,
    all_methods: bool,
}

impl RetryPolicy {
    /// Production default: three retries for idempotent methods.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            max_retries: 3,
            all_methods: false,
        }
    }

    /// Disable retries.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            max_retries: 0,
            all_methods: false,
        }
    }

    /// Set the number of retries after the initial request.
    #[must_use]
    pub const fn max_retries(mut self, max_retries: usize) -> Self {
        self.max_retries = max_retries;
        self
    }

    /// Retry all HTTP methods, including POST and PATCH.
    #[must_use]
    pub const fn all_methods(mut self) -> Self {
        self.all_methods = true;
        self
    }

    /// Return the configured number of retries after the initial request.
    pub const fn retries(&self) -> usize {
        self.max_retries
    }

    /// Return whether non-idempotent methods may be retried.
    pub const fn retries_all_methods(&self) -> bool {
        self.all_methods
    }
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for an [`HttpClient`].
#[derive(Debug)]
pub struct HttpClientBuilder {
    config: HttpClientConfig,
    #[cfg(feature = "http-cookies")]
    cookie_jar: Option<CookieJarSource>,
}

#[cfg(feature = "http-cookies")]
#[derive(Debug)]
enum CookieJarSource {
    Empty,
    File(std::path::PathBuf),
}

impl HttpClientBuilder {
    /// Set the maximum number of redirects to follow. Zero disables redirects.
    #[must_use]
    pub const fn max_redirects(mut self, max_redirects: usize) -> Self {
        self.config.max_redirects = max_redirects;
        self
    }

    /// Disable automatic gzip and Brotli response decompression.
    #[must_use]
    pub const fn no_decompression(mut self) -> Self {
        self.config.decompression = false;
        self
    }

    /// Override the retry behavior.
    #[must_use]
    pub const fn retry_policy(mut self, retry_policy: RetryPolicy) -> Self {
        self.config.retry_policy = retry_policy;
        self
    }

    /// Set the maximum decoded response body retained in memory.
    ///
    /// Set this to zero to allow unbounded response bodies.
    #[must_use]
    pub const fn max_response_size(mut self, max_response_size: usize) -> Self {
        self.config.max_response_size = max_response_size;
        self
    }

    /// Override the whole-request timeout.
    #[must_use]
    pub const fn timeout(mut self, timeout: Duration) -> Self {
        self.config.timeout = timeout;
        self
    }

    /// Override the user-agent string.
    #[must_use]
    pub fn user_agent(mut self, user_agent: &str) -> Self {
        self.config.user_agent = user_agent.to_string();
        self
    }

    /// Enable an in-memory cookie jar for this client.
    #[cfg(feature = "http-cookies")]
    #[must_use]
    pub fn with_cookie_jar(mut self) -> Self {
        self.cookie_jar = Some(CookieJarSource::Empty);
        self
    }

    /// Load a persistent cookie jar when the client is built.
    #[cfg(feature = "http-cookies")]
    #[must_use]
    pub fn with_cookie_jar_from(mut self, path: impl AsRef<std::path::Path>) -> Self {
        self.cookie_jar = Some(CookieJarSource::File(path.as_ref().to_path_buf()));
        self
    }

    /// Build the configured client.
    pub fn build(self) -> Result<HttpClient> {
        #[cfg(feature = "http-cookies")]
        {
            let cookie_jar = self
                .cookie_jar
                .map(|source| match source {
                    CookieJarSource::Empty => Ok(CookieJar::default()),
                    CookieJarSource::File(path) => CookieJar::load_from(&path),
                })
                .transpose()?;
            HttpClient::build_inner(self.config, cookie_jar)
        }
        #[cfg(not(feature = "http-cookies"))]
        HttpClient::new(self.config)
    }
}

// ─── Client ─────────────────────────────────────────────────────────

type RequestBody = Full<Bytes>;
type ResponseBody = UnsyncBoxBody<Bytes, BoxError>;
type HttpService =
    BoxCloneSyncService<Request<RequestBody>, hyper::Response<ResponseBody>, BoxError>;

#[derive(Debug)]
enum RedirectError {
    Loop,
    TooMany { maximum: usize },
}

impl std::fmt::Display for RedirectError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Loop => formatter.write_str("redirect loop detected"),
            Self::TooMany { maximum } => {
                write!(formatter, "too many redirects (maximum {maximum})")
            }
        }
    }
}

impl std::error::Error for RedirectError {}

#[derive(Clone, Debug)]
struct RedirectPolicy {
    maximum: usize,
    remaining: usize,
    visited: HashSet<(Method, hyper::Uri)>,
    credentials: FilterCredentials,
}

impl RedirectPolicy {
    fn new(maximum: usize) -> Self {
        Self {
            maximum,
            remaining: maximum,
            visited: HashSet::new(),
            credentials: FilterCredentials::default(),
        }
    }
}

impl RedirectPolicyTrait<RequestBody, BoxError> for RedirectPolicy {
    fn redirect(&mut self, attempt: &Attempt<'_>) -> std::result::Result<Action, BoxError> {
        self.visited.insert((
            attempt.previous_method().clone(),
            attempt.previous().clone(),
        ));
        if self
            .visited
            .contains(&(attempt.method().clone(), attempt.location().clone()))
        {
            return Err(Box::new(RedirectError::Loop));
        }
        if self.remaining == 0 {
            return Err(Box::new(RedirectError::TooMany {
                maximum: self.maximum,
            }));
        }

        self.remaining -= 1;
        RedirectPolicyTrait::<RequestBody, BoxError>::redirect(&mut self.credentials, attempt)
    }

    fn on_request(&mut self, request: &mut Request<RequestBody>) {
        RedirectPolicyTrait::<RequestBody, BoxError>::on_request(&mut self.credentials, request);
    }

    fn clone_body(&self, body: &RequestBody) -> Option<RequestBody> {
        Some(body.clone())
    }
}

const fn is_idempotent(method: &Method) -> bool {
    matches!(
        *method,
        Method::GET | Method::PUT | Method::DELETE | Method::HEAD | Method::OPTIONS
    )
}

/// HTTP/HTTPS client with tracing and timeout support.
///
/// Uses rustls for TLS with Mozilla's CA root certificates.
/// HTTP/2 with HTTP/1.1 fallback. Connection pooling handled
/// automatically.
pub struct HttpClient {
    inner: HttpService,
    config: HttpClientConfig,
    #[cfg(feature = "http-cookies")]
    cookie_jar: Option<CookieJar>,
}

impl HttpClient {
    /// Start building a client with production defaults.
    pub fn builder(app_name: &str, version: &str) -> HttpClientBuilder {
        HttpClientBuilder {
            config: HttpClientConfig::new(app_name, version),
            #[cfg(feature = "http-cookies")]
            cookie_jar: None,
        }
    }

    /// Create a new client from an explicit [`HttpClientConfig`].
    pub fn new(config: HttpClientConfig) -> Result<Self> {
        Self::build_inner(
            config,
            #[cfg(feature = "http-cookies")]
            None,
        )
    }

    fn build_inner(
        config: HttpClientConfig,
        #[cfg(feature = "http-cookies")] cookie_jar: Option<CookieJar>,
    ) -> Result<Self> {
        let https = hyper_rustls::HttpsConnectorBuilder::new()
            .with_provider_and_webpki_roots(rustls::crypto::ring::default_provider())
            .map_err(HttpError::Tls)?
            .https_or_http()
            .enable_all_versions()
            .build();

        let transport =
            hyper_util::client::legacy::Client::builder(TokioExecutor::new()).build(https);
        let transport = ServiceBuilder::new()
            .map_err(|error: hyper_util::client::legacy::Error| -> BoxError { Box::new(error) })
            .map_response(|response: hyper::Response<hyper::body::Incoming>| {
                response.map(|body| {
                    body.map_err(|error| -> BoxError { Box::new(error) })
                        .boxed_unsync()
                })
            })
            .service(transport);
        let mut inner = HttpService::new(transport);

        if config.decompression {
            let decompressed = ServiceBuilder::new()
                .map_response(
                    |response: hyper::Response<
                        tower_http::decompression::DecompressionBody<ResponseBody>,
                    >| { response.map(|body| body.boxed_unsync()) },
                )
                .layer(DecompressionLayer::new())
                .service(inner);
            inner = HttpService::new(decompressed);
        }

        #[cfg(feature = "http-cookies")]
        if let Some(jar) = cookie_jar.clone() {
            inner = HttpService::new(cookies::CookieService::new(inner, jar));
        }

        if config.max_redirects > 0 {
            inner = HttpService::new(FollowRedirect::with_policy(
                inner,
                RedirectPolicy::new(config.max_redirects),
            ));
        }
        Ok(Self {
            inner,
            config,
            #[cfg(feature = "http-cookies")]
            cookie_jar,
        })
    }

    /// Create a new client using `"app_name/version"` as the user-agent.
    pub fn from_app(app_name: &str, version: &str) -> Result<Self> {
        Self::new(HttpClientConfig::new(app_name, version))
    }

    /// Perform a GET request, returning a [`Response`].
    ///
    /// The entire operation is bounded by `config.timeout`.
    ///
    /// # Errors
    ///
    /// - [`Error::Http`] — invalid URL, connection failure, TLS error,
    ///   timeout, or I/O error while reading the response body.
    pub async fn get(&self, url: &str) -> Result<Response> {
        self.request(Method::GET, url, []).await
    }

    /// Perform a POST request with a byte body.
    pub async fn post(&self, url: &str, body: impl AsRef<[u8]>) -> Result<Response> {
        self.request(Method::POST, url, body).await
    }

    /// Perform a PUT request with a byte body.
    pub async fn put(&self, url: &str, body: impl AsRef<[u8]>) -> Result<Response> {
        self.request(Method::PUT, url, body).await
    }

    /// Perform a PATCH request with a byte body.
    pub async fn patch(&self, url: &str, body: impl AsRef<[u8]>) -> Result<Response> {
        self.request(Method::PATCH, url, body).await
    }

    /// Perform a DELETE request with an empty body.
    pub async fn delete(&self, url: &str) -> Result<Response> {
        self.request(Method::DELETE, url, []).await
    }

    /// Perform an HTTP request with a byte body.
    ///
    /// Use [`send`](Self::send) when custom headers are required.
    pub async fn request(
        &self,
        method: Method,
        url: &str,
        body: impl AsRef<[u8]>,
    ) -> Result<Response> {
        let uri: hyper::Uri = url.parse().map_err(HttpError::InvalidUrl)?;
        let req = hyper::Request::builder()
            .method(method)
            .uri(&uri)
            .body(Bytes::copy_from_slice(body.as_ref()))
            .map_err(HttpError::RequestBuild)?;

        self.send(req).await
    }

    /// Send a pre-built HTTP request.
    ///
    /// This is the escape hatch for custom headers. The configured user-agent
    /// is inserted when the request does not already contain one.
    #[tracing::instrument(
        skip(self, request),
        fields(method = %request.method(), url = %request.uri())
    )]
    pub async fn send(&self, mut request: Request<Bytes>) -> Result<Response> {
        if !request.headers().contains_key(hyper::header::USER_AGENT) {
            let user_agent = hyper::header::HeaderValue::from_str(&self.config.user_agent)
                .map_err(HttpError::InvalidHeaderValue)?;
            request
                .headers_mut()
                .insert(hyper::header::USER_AGENT, user_agent);
        }

        let (parts, body) = request.into_parts();
        let request = Request::from_parts(parts, Full::new(body));

        let whole_request = async {
            let retryable_method =
                self.config.retry_policy.all_methods || is_idempotent(request.method());
            let mut remaining = self.config.retry_policy.max_retries;
            let mut next_delay = Duration::from_millis(50);

            loop {
                let attempt = clone_request(&request)?;
                match self.inner.clone().oneshot(attempt).await {
                    Ok(response) => {
                        let status = response.status();
                        tracing::debug!(status = status.as_u16(), "response received");

                        if retryable_method && status.is_server_error() && remaining > 0 {
                            discard_body(response.into_body()).await;
                            wait_to_retry(&mut remaining, &mut next_delay).await;
                            continue;
                        }

                        match read_body(response.into_body(), self.config.max_response_size).await {
                            Ok(body) => {
                                return Ok(Response {
                                    status: status.as_u16(),
                                    body,
                                });
                            }
                            Err(ReadBodyError::Body(error))
                                if retryable_method
                                    && remaining > 0
                                    && error.is::<hyper::Error>() =>
                            {
                                wait_to_retry(&mut remaining, &mut next_delay).await;
                            }
                            Err(ReadBodyError::Body(error)) => {
                                return Err(HttpError::Body(error).into());
                            }
                            Err(ReadBodyError::TooLarge) => {
                                return Err(HttpError::ResponseTooLarge {
                                    maximum: self.config.max_response_size,
                                }
                                .into());
                            }
                        }
                    }
                    Err(error)
                        if retryable_method
                            && remaining > 0
                            && error.is::<hyper_util::client::legacy::Error>() =>
                    {
                        wait_to_retry(&mut remaining, &mut next_delay).await;
                    }
                    Err(error) => return Err(map_service_error(error).into()),
                }
            }
        };

        tokio::time::timeout(self.config.timeout, whole_request)
            .await
            .map_err(|_| {
                HttpError::Io(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    format!("request timed out after {:?}", self.config.timeout),
                ))
            })?
    }

    /// Returns a reference to the client configuration.
    pub const fn config(&self) -> &HttpClientConfig {
        &self.config
    }

    /// Return this client's cookie jar, if cookie handling was enabled.
    #[cfg(feature = "http-cookies")]
    pub const fn cookie_jar(&self) -> Option<&CookieJar> {
        self.cookie_jar.as_ref()
    }
}

fn clone_request(request: &Request<RequestBody>) -> Result<Request<RequestBody>> {
    let mut clone = Request::builder()
        .method(request.method().clone())
        .uri(request.uri().clone())
        .version(request.version())
        .body(request.body().clone())
        .map_err(HttpError::RequestBuild)?;
    *clone.headers_mut() = request.headers().clone();
    *clone.extensions_mut() = request.extensions().clone();
    Ok(clone)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct RequestMarker(&'static str);

    #[test]
    fn cloned_requests_preserve_extensions() {
        let mut request = Request::new(Full::new(Bytes::from_static(b"replay me")));
        request.extensions_mut().insert(RequestMarker("preserved"));

        let clone = clone_request(&request).expect("request should be cloneable");

        assert_eq!(
            clone.extensions().get::<RequestMarker>(),
            Some(&RequestMarker("preserved"))
        );
    }
}

enum ReadBodyError {
    Body(BoxError),
    TooLarge,
}

async fn read_body(
    mut body: ResponseBody,
    maximum: usize,
) -> std::result::Result<Vec<u8>, ReadBodyError> {
    let mut output = Vec::new();
    while let Some(frame) = body.frame().await {
        let frame = frame.map_err(ReadBodyError::Body)?;
        let Ok(data) = frame.into_data() else {
            continue;
        };
        let Some(new_size) = output.len().checked_add(data.len()) else {
            return Err(ReadBodyError::TooLarge);
        };
        if maximum > 0 && new_size > maximum {
            return Err(ReadBodyError::TooLarge);
        }
        output.extend_from_slice(&data);
    }
    Ok(output)
}

async fn discard_body(mut body: ResponseBody) {
    while let Some(frame) = body.frame().await {
        if frame.is_err() {
            break;
        }
    }
}

async fn wait_to_retry(remaining: &mut usize, next_delay: &mut Duration) {
    *remaining -= 1;
    let delay = *next_delay;
    *next_delay = next_delay.saturating_mul(2).min(Duration::from_secs(1));
    tracing::debug!(?delay, remaining = *remaining, "retrying HTTP request");
    tokio::time::sleep(delay).await;
}

fn map_service_error(error: BoxError) -> HttpError {
    if error.is::<RedirectError>() {
        return match *error
            .downcast::<RedirectError>()
            .expect("type checked before downcast")
        {
            RedirectError::Loop => HttpError::RedirectLoop,
            RedirectError::TooMany { maximum } => HttpError::TooManyRedirects { maximum },
        };
    }
    HttpError::Request(error)
}

// ─── Response ───────────────────────────────────────────────────────

/// HTTP response returned by [`HttpClient`].
#[derive(Debug)]
pub struct Response {
    /// HTTP status code.
    pub status: u16,
    body: Vec<u8>,
}

impl Response {
    /// Attempt to decode the body as UTF-8 text.
    ///
    /// This clones the body. Use [`into_text`](Self::into_text) when you
    /// no longer need the `Response`.
    ///
    /// # Errors
    ///
    /// Returns [`std::string::FromUtf8Error`] if the body is not valid UTF-8.
    pub fn text(&self) -> std::result::Result<String, std::string::FromUtf8Error> {
        String::from_utf8(self.body.clone())
    }

    /// Consume the response and decode the body as UTF-8 text.
    ///
    /// # Errors
    ///
    /// Returns [`std::string::FromUtf8Error`] if the body is not valid UTF-8.
    pub fn into_text(self) -> std::result::Result<String, std::string::FromUtf8Error> {
        String::from_utf8(self.body)
    }

    /// Borrow the body as a UTF-8 string slice without copying.
    ///
    /// # Errors
    ///
    /// Returns [`std::str::Utf8Error`] if the body is not valid UTF-8.
    pub fn text_ref(&self) -> std::result::Result<&str, std::str::Utf8Error> {
        std::str::from_utf8(&self.body)
    }

    /// Deserialize the response body as JSON.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Http`] if the body is not valid JSON or cannot
    /// be deserialized into `T`.
    pub fn json<T: serde::de::DeserializeOwned>(&self) -> crate::Result<T> {
        serde_json::from_slice(&self.body).map_err(|e| HttpError::Json(e).into())
    }

    /// Return the raw response body bytes.
    pub fn bytes(&self) -> &[u8] {
        &self.body
    }

    /// Returns `true` for 2xx status codes.
    pub const fn is_success(&self) -> bool {
        self.status >= 200 && self.status < 300
    }
}
