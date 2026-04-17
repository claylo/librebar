//! HTTP/HTTPS client with tracing integration.
//!
//! Provides a thin wrapper around hyper with:
//! - TLS via rustls (Mozilla CA roots, no system OpenSSL dependency)
//! - HTTP/2 with HTTP/1.1 fallback
//! - Configurable user-agent and timeout
//! - `#[tracing::instrument]` on every request
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

use std::time::Duration;

use http_body_util::{BodyExt, Empty};
use hyper::body::Bytes;
use hyper_util::rt::TokioExecutor;

use crate::Result;
use crate::error::HttpError;

// ─── Config ─────────────────────────────────────────────────────────

/// Configuration for [`HttpClient`].
#[derive(Debug)]
pub struct HttpClientConfig {
    /// Value sent as the `User-Agent` header on every request.
    pub user_agent: String,
    /// Per-request timeout. Requests that exceed this are cancelled.
    pub timeout: Duration,
}

impl HttpClientConfig {
    /// Build a config with a `"name/version"` user-agent and 30 s timeout.
    pub fn new(app_name: &str, version: &str) -> Self {
        Self {
            user_agent: format!("{app_name}/{version}"),
            timeout: Duration::from_secs(30),
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

// ─── Client ─────────────────────────────────────────────────────────

/// HTTPS connector type used by the client.
type HttpsConnector =
    hyper_rustls::HttpsConnector<hyper_util::client::legacy::connect::HttpConnector>;

/// HTTP/HTTPS client with tracing and timeout support.
///
/// Uses rustls for TLS with Mozilla's CA root certificates.
/// HTTP/2 with HTTP/1.1 fallback. Connection pooling handled
/// automatically.
pub struct HttpClient {
    inner: hyper_util::client::legacy::Client<HttpsConnector, Empty<Bytes>>,
    config: HttpClientConfig,
}

impl HttpClient {
    /// Create a new client from an explicit [`HttpClientConfig`].
    pub fn new(config: HttpClientConfig) -> Result<Self> {
        let https = hyper_rustls::HttpsConnectorBuilder::new()
            .with_provider_and_webpki_roots(rustls::crypto::ring::default_provider())
            .map_err(HttpError::Tls)?
            .https_or_http()
            .enable_all_versions()
            .build();

        let inner = hyper_util::client::legacy::Client::builder(TokioExecutor::new()).build(https);
        Ok(Self { inner, config })
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
    #[tracing::instrument(skip(self), fields(url = %url))]
    pub async fn get(&self, url: &str) -> Result<Response> {
        let uri: hyper::Uri = url.parse().map_err(HttpError::InvalidUrl)?;

        let req = hyper::Request::builder()
            .method(hyper::Method::GET)
            .uri(&uri)
            .header(hyper::header::USER_AGENT, &self.config.user_agent)
            .body(Empty::<Bytes>::new())
            .map_err(HttpError::RequestBuild)?;

        let whole_request = async {
            let resp = self.inner.request(req).await.map_err(HttpError::Request)?;

            let status = resp.status().as_u16();
            tracing::debug!(status, "response received");

            let body = resp
                .into_body()
                .collect()
                .await
                .map_err(HttpError::Body)?
                .to_bytes();

            Ok(Response {
                status,
                body: body.to_vec(),
            })
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
}

// ─── Response ───────────────────────────────────────────────────────

/// HTTP response returned by [`HttpClient::get`].
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
