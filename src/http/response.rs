use std::fmt;

use crate::error::{HttpError, boxed_error};
use crate::http::{AsHeaderName, HeaderMap, HeaderValue, StatusCode, Version, header};

/// Metadata collected from an HTTP response.
#[derive(Clone)]
pub struct ResponseMetadata {
    pub(super) status: StatusCode,
    pub(super) version: Version,
    pub(super) headers: HeaderMap,
    pub(super) trailers: Option<HeaderMap>,
}

struct HeaderNames<'a>(&'a HeaderMap);

impl fmt::Debug for HeaderNames<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_list().entries(self.0.keys()).finish()
    }
}

impl fmt::Debug for ResponseMetadata {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ResponseMetadata")
            .field("status", &self.status)
            .field("version", &self.version)
            .field("header_names", &HeaderNames(&self.headers))
            .field("trailer_names", &self.trailers.as_ref().map(HeaderNames))
            .finish()
    }
}

impl ResponseMetadata {
    pub(super) const fn new(
        status: StatusCode,
        version: Version,
        headers: HeaderMap,
        trailers: Option<HeaderMap>,
    ) -> Self {
        Self {
            status,
            version,
            headers,
            trailers,
        }
    }

    /// Return the HTTP status code.
    pub const fn status(&self) -> StatusCode {
        self.status
    }

    /// Return the HTTP protocol version.
    pub const fn version(&self) -> Version {
        self.version
    }

    /// Return the response headers.
    pub const fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    /// Return the first value for a response header.
    pub fn header<K: AsHeaderName>(&self, name: K) -> Option<&HeaderValue> {
        self.headers.get(name)
    }

    /// Return response trailers, when the response supplied any.
    pub const fn trailers(&self) -> Option<&HeaderMap> {
        self.trailers.as_ref()
    }
}

/// HTTP response returned by [`super::HttpClient`].
pub struct Response {
    pub(super) metadata: ResponseMetadata,
    pub(super) body: Vec<u8>,
    #[cfg(feature = "http-cache")]
    pub(super) cache_status: Option<CacheStatus>,
}

impl fmt::Debug for Response {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut response = formatter.debug_struct("Response");
        response
            .field("metadata", &self.metadata)
            .field("body_len", &self.body.len());
        #[cfg(feature = "http-cache")]
        response.field("cache_status", &self.cache_status);
        response.finish()
    }
}

impl Response {
    pub(super) const fn new(metadata: ResponseMetadata, body: Vec<u8>) -> Self {
        Self {
            metadata,
            body,
            #[cfg(feature = "http-cache")]
            cache_status: None,
        }
    }

    /// Return the HTTP status code.
    pub const fn status(&self) -> StatusCode {
        self.metadata.status()
    }

    /// Return the HTTP protocol version.
    pub const fn version(&self) -> Version {
        self.metadata.version()
    }

    /// Return the response headers.
    pub const fn headers(&self) -> &HeaderMap {
        self.metadata.headers()
    }

    /// Return the first value for a response header.
    pub fn header<K: AsHeaderName>(&self, name: K) -> Option<&HeaderValue> {
        self.metadata.header(name)
    }

    /// Return response trailers, when the response supplied any.
    pub const fn trailers(&self) -> Option<&HeaderMap> {
        self.metadata.trailers()
    }

    /// Consume the response into its metadata and body.
    pub fn into_parts(self) -> (ResponseMetadata, Vec<u8>) {
        (self.metadata, self.body)
    }

    /// Return the validators supplied by the origin, when present.
    pub fn validator(&self) -> Option<Validator> {
        Validator::from_headers(self.headers())
    }

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
    /// Returns [`crate::Error::Http`] if the body is not valid JSON or cannot
    /// be deserialized into `T`.
    pub fn json<T: serde::de::DeserializeOwned>(&self) -> crate::Result<T> {
        serde_json::from_slice(&self.body)
            .map_err(|error| HttpError::Json(boxed_error(error)).into())
    }

    /// Return the raw response body bytes.
    pub fn bytes(&self) -> &[u8] {
        &self.body
    }

    /// Returns `true` for 2xx status codes.
    pub fn is_success(&self) -> bool {
        self.status().is_success()
    }
}

/// How a response was obtained by [`super::HttpClient::get_cached`].
#[cfg(feature = "http-cache")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CacheStatus {
    /// The request used a fresh cached representation without network I/O.
    Hit,
    /// The response came from the network.
    Miss,
    /// A `304 Not Modified` response refreshed the cached representation.
    Revalidated,
}

#[cfg(feature = "http-cache")]
impl Response {
    /// Return how this response was obtained, when HTTP caching was involved.
    pub const fn cache_status(&self) -> Option<CacheStatus> {
        self.cache_status
    }

    pub(super) const fn with_cache_status(mut self, status: CacheStatus) -> Self {
        self.cache_status = Some(status);
        self
    }
}

/// Opaque server validators for conditional HTTP requests.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Validator {
    etag: Option<HeaderValue>,
    last_modified: Option<HeaderValue>,
}

impl Validator {
    /// Construct an ETag validator.
    pub const fn from_etag(value: HeaderValue) -> Self {
        Self {
            etag: Some(value),
            last_modified: None,
        }
    }

    /// Construct a Last-Modified validator.
    pub const fn from_last_modified(value: HeaderValue) -> Self {
        Self {
            etag: None,
            last_modified: Some(value),
        }
    }

    /// Extract all available validators from response headers.
    pub fn from_headers(headers: &HeaderMap) -> Option<Self> {
        let etag = headers.get(header::ETAG).cloned();
        let last_modified = headers.get(header::LAST_MODIFIED).cloned();
        (etag.is_some() || last_modified.is_some()).then_some(Self {
            etag,
            last_modified,
        })
    }

    /// Return the opaque ETag value.
    pub const fn etag(&self) -> Option<&HeaderValue> {
        self.etag.as_ref()
    }

    /// Return the opaque Last-Modified value.
    pub const fn last_modified(&self) -> Option<&HeaderValue> {
        self.last_modified.as_ref()
    }
}

/// Result of a conditional GET request.
#[derive(Debug)]
pub enum ConditionalResponse {
    /// The origin returned a new successful representation.
    Modified(Response),
    /// The origin confirmed that the supplied validator still matches.
    NotModified(ResponseMetadata),
    /// The response did not establish whether the representation changed.
    Indeterminate(Response),
}

/// Result of a conditional HEAD request.
#[derive(Debug)]
pub enum ModificationCheck {
    /// The origin returned successful metadata for a changed representation.
    Modified(ResponseMetadata),
    /// The origin confirmed that the supplied validator still matches.
    NotModified(ResponseMetadata),
    /// The response did not establish whether the representation changed.
    Indeterminate(ResponseMetadata),
}
