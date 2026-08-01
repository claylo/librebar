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
//! - RFC-aware private GET caching behind the `http-cache` feature
//! - `#[tracing::instrument]` on every request
//! - GET, POST, PUT, PATCH, DELETE, and arbitrary [`Request`] support
//! - [`Response`] metadata with lossless repeated headers and trailers
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
//! `Response` retains the final status, protocol version, repeated headers,
//! trailers, and decoded body. Use `headers().get_all(name)` when field order
//! and multiplicity matter.
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
//!
//! Conditional requests preserve opaque ETag and Last-Modified validators:
//!
//! ```no_run
//! use librebar::http::{ConditionalResponse, HttpClient};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let client = HttpClient::from_app("my-app", "1.0.0")?;
//! let initial = client.get("https://example.com/data").await?;
//! if let Some(validator) = initial.validator() {
//!     let _metadata_only = client.check_modified("https://example.com/data", &validator).await?;
//!     match client.get_if_modified("https://example.com/data", &validator).await? {
//!         ConditionalResponse::Modified(response) => println!("{}", response.text_ref()?),
//!         ConditionalResponse::NotModified(_) => println!("unchanged"),
//!         ConditionalResponse::Indeterminate(response) => {
//!             eprintln!("origin returned {}", response.status());
//!         }
//!     }
//! }
//! # Ok(())
//! # }
//! ```
//!
//! With `http-cache`, HTTP policy controls freshness and revalidation while
//! the caller retains explicit ownership of the filesystem cache and key:
//!
//! ```no_run
//! use librebar::cache::Cache;
//! use librebar::http::HttpClient;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let cache = Cache::default_for("my-app").ok_or("cache directory unavailable")?;
//! let client = HttpClient::from_app("my-app", "1.0.0")?;
//! let response = client
//!     .get_cached(&cache, "widgets", "https://example.com/widgets")
//!     .await?;
//! println!("{:?}: {}", response.cache_status(), response.text_ref()?);
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

mod response;
pub use hyper::header::{HeaderMap, HeaderValue};
pub use hyper::{Method, Request, StatusCode, Version};
#[cfg(feature = "http-cache")]
pub use response::CacheStatus;
pub use response::{ConditionalResponse, ModificationCheck, Response, ResponseMetadata, Validator};

#[cfg(feature = "http-cache")]
#[path = "http/cache.rs"]
mod http_cache;

use crate::Result;
use crate::error::HttpError;

pub use hyper::body::Bytes;

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
    /// How long stale HTTP entries remain available for revalidation.
    #[cfg(feature = "http-cache")]
    pub http_cache_stale_retention: Duration,
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
            #[cfg(feature = "http-cache")]
            http_cache_stale_retention: Duration::from_secs(7 * 24 * 60 * 60),
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

    /// Set how long stale HTTP entries remain available for revalidation.
    #[cfg(feature = "http-cache")]
    #[must_use]
    pub const fn http_cache_stale_retention(mut self, retention: Duration) -> Self {
        self.config.http_cache_stale_retention = retention;
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
    /// - [`Error::Http`](crate::Error::Http) — invalid URL, connection failure, TLS error,
    ///   timeout, or I/O error while reading the response body.
    pub async fn get(&self, url: &str) -> Result<Response> {
        self.request(Method::GET, url, []).await
    }

    /// Perform an RFC-aware cached GET using an explicit cache and key.
    ///
    /// A key identifies one stored representation. Include tenant, locale, or
    /// media-type distinctions in the key when callers intentionally need
    /// multiple variants. Cache files can contain complete API responses and
    /// are therefore written as private data.
    ///
    /// # Errors
    ///
    /// Returns an HTTP or cache error when the request or cache read fails.
    #[cfg(feature = "http-cache")]
    pub async fn get_cached(
        &self,
        cache: &crate::cache::Cache,
        key: &str,
        url: &str,
    ) -> Result<Response> {
        http_cache::get_cached(self, cache, key, url).await
    }

    /// Perform a conditional GET using an origin-supplied validator.
    ///
    /// ETag takes precedence over Last-Modified when both are available.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Http`](crate::Error::Http) when the request cannot be
    /// built or completed.
    pub async fn get_if_modified(
        &self,
        url: &str,
        validator: &Validator,
    ) -> Result<ConditionalResponse> {
        let request = conditional_request(Method::GET, url, validator)?;
        let response = self.send(request).await?;
        if response.status() == StatusCode::NOT_MODIFIED {
            let (metadata, _) = response.into_parts();
            Ok(ConditionalResponse::NotModified(metadata))
        } else if response.status().is_success() {
            Ok(ConditionalResponse::Modified(response))
        } else {
            Ok(ConditionalResponse::Indeterminate(response))
        }
    }

    /// Check whether a representation changed using a conditional HEAD.
    ///
    /// This method never falls back to GET when HEAD is unsupported.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Http`](crate::Error::Http) when the request cannot be
    /// built or completed.
    pub async fn check_modified(
        &self,
        url: &str,
        validator: &Validator,
    ) -> Result<ModificationCheck> {
        let request = conditional_request(Method::HEAD, url, validator)?;
        let response = self.send(request).await?;
        let status = response.status();
        let (metadata, _) = response.into_parts();
        if status == StatusCode::NOT_MODIFIED {
            Ok(ModificationCheck::NotModified(metadata))
        } else if status.is_success() {
            Ok(ModificationCheck::Modified(metadata))
        } else {
            Ok(ModificationCheck::Indeterminate(metadata))
        }
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
        fields(method = %request.method(), url = %sanitized_uri(request.uri()))
    )]
    pub async fn send(&self, mut request: Request<Bytes>) -> Result<Response> {
        self.prepare_request(&mut request)?;

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

                        let (parts, response_body) = response.into_parts();
                        match read_body(response_body, self.config.max_response_size).await {
                            Ok((body, trailers)) => {
                                let metadata = ResponseMetadata::new(
                                    parts.status,
                                    parts.version,
                                    parts.headers,
                                    trailers,
                                );
                                return Ok(Response::new(metadata, body));
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

    pub(super) fn prepare_request(&self, request: &mut Request<Bytes>) -> Result<()> {
        if !request.headers().contains_key(hyper::header::USER_AGENT) {
            let user_agent = hyper::header::HeaderValue::from_str(&self.config.user_agent)
                .map_err(HttpError::InvalidHeaderValue)?;
            request
                .headers_mut()
                .insert(hyper::header::USER_AGENT, user_agent);
        }
        #[cfg(feature = "http-cookies")]
        if let Some(jar) = &self.cookie_jar {
            jar.apply_to_request(request);
        }
        Ok(())
    }
}

fn conditional_request(method: Method, url: &str, validator: &Validator) -> Result<Request<Bytes>> {
    let uri: hyper::Uri = url.parse().map_err(HttpError::InvalidUrl)?;
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(etag) = validator.etag() {
        builder = builder.header(hyper::header::IF_NONE_MATCH, etag);
    } else if let Some(last_modified) = validator.last_modified() {
        builder = builder.header(hyper::header::IF_MODIFIED_SINCE, last_modified);
    }
    builder
        .body(Bytes::new())
        .map_err(HttpError::RequestBuild)
        .map_err(Into::into)
}

fn sanitized_uri(uri: &hyper::Uri) -> String {
    let mut sanitized = String::new();

    if let Some(scheme) = uri.scheme_str() {
        sanitized.push_str(scheme);
        sanitized.push_str("://");
    }
    if let Some(host) = uri.host() {
        sanitized.push_str(host);
        if let Some(port) = uri.port_u16() {
            sanitized.push(':');
            sanitized.push_str(&port.to_string());
        }
    }
    sanitized.push_str(uri.path());

    sanitized
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
    use std::collections::VecDeque;
    use std::convert::Infallible;
    use std::pin::Pin;
    use std::task::{Context, Poll};

    use hyper::body::{Body, Frame};

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

    struct FrameBody(VecDeque<std::result::Result<Frame<Bytes>, Infallible>>);

    impl Body for FrameBody {
        type Data = Bytes;
        type Error = Infallible;

        fn poll_frame(
            mut self: Pin<&mut Self>,
            _context: &mut Context<'_>,
        ) -> Poll<Option<std::result::Result<Frame<Self::Data>, Self::Error>>> {
            Poll::Ready(self.0.pop_front())
        }
    }

    #[tokio::test]
    async fn body_collection_appends_multiple_trailer_frames() {
        let mut first = HeaderMap::new();
        first.append("x-trailer", HeaderValue::from_static("one"));
        let mut second = HeaderMap::new();
        second.append("x-trailer", HeaderValue::from_static("two"));
        let body = FrameBody(VecDeque::from([
            Ok(Frame::data(Bytes::from_static(b"body"))),
            Ok(Frame::trailers(first)),
            Ok(Frame::trailers(second)),
        ]))
        .map_err(|never| -> BoxError { match never {} })
        .boxed_unsync();

        let (bytes, trailers) = read_body(body, 1024).await.unwrap();

        assert_eq!(bytes, b"body");
        assert_eq!(
            trailers
                .unwrap()
                .get_all("x-trailer")
                .iter()
                .map(|value| value.to_str().unwrap())
                .collect::<Vec<_>>(),
            ["one", "two"]
        );
    }
}

#[derive(Debug)]
enum ReadBodyError {
    Body(BoxError),
    TooLarge,
}

async fn read_body(
    mut body: ResponseBody,
    maximum: usize,
) -> std::result::Result<(Vec<u8>, Option<HeaderMap>), ReadBodyError> {
    let mut output = Vec::new();
    let mut trailers: Option<HeaderMap> = None;
    while let Some(frame) = body.frame().await {
        let frame = frame.map_err(ReadBodyError::Body)?;
        match frame.into_data() {
            Ok(data) => append_bounded(&mut output, &data, maximum)?,
            Err(frame) => {
                if let Ok(next) = frame.into_trailers() {
                    let stored = trailers.get_or_insert_with(HeaderMap::new);
                    for (name, value) in next {
                        if let Some(name) = name {
                            stored.append(name, value);
                        }
                    }
                }
            }
        }
    }
    Ok((output, trailers))
}

fn append_bounded(
    output: &mut Vec<u8>,
    data: &[u8],
    maximum: usize,
) -> std::result::Result<(), ReadBodyError> {
    let new_size = output
        .len()
        .checked_add(data.len())
        .ok_or(ReadBodyError::TooLarge)?;
    if maximum > 0 && new_size > maximum {
        return Err(ReadBodyError::TooLarge);
    }
    output.extend_from_slice(data);
    Ok(())
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
