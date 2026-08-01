use std::time::{Duration, SystemTime};

use base64::Engine as _;
use http_cache_semantics::{AfterResponse, BeforeRequest, CacheOptions, CachePolicy};
use hyper::header::{
    AGE, CONNECTION, CONTENT_ENCODING, CONTENT_LENGTH, CONTENT_RANGE, DATE, HeaderMap, HeaderName,
    HeaderValue, WARNING,
};
use hyper::{Method, Request, StatusCode, Version};
use sha2::{Digest, Sha256};

use super::{Bytes, CacheStatus, HttpClient, Response, ResponseMetadata};
use crate::cache::Cache;
use crate::error::CacheError;
use crate::{Error, Result};

const HTTP_CACHE_MAGIC: &[u8; 8] = b"LBRHT02\0";
const HTTP_CACHE_FOOTER_LEN: usize = 16;
const HTTP_CACHE_FORMAT_VERSION: u8 = 2;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct StoredHeader {
    name: String,
    value: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct LosslessHeaders(Vec<StoredHeader>);

impl From<&HeaderMap> for LosslessHeaders {
    fn from(headers: &HeaderMap) -> Self {
        Self(
            headers
                .iter()
                .map(|(name, value)| StoredHeader {
                    name: name.as_str().to_owned(),
                    value: base64::engine::general_purpose::URL_SAFE_NO_PAD
                        .encode(value.as_bytes()),
                })
                .collect(),
        )
    }
}

impl TryFrom<LosslessHeaders> for HeaderMap {
    type Error = CacheEntryError;

    fn try_from(stored: LosslessHeaders) -> std::result::Result<Self, Self::Error> {
        let mut headers = Self::new();
        for field in stored.0 {
            let name = HeaderName::from_bytes(field.name.as_bytes())?;
            let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(field.value)?;
            let value = HeaderValue::from_bytes(&bytes)?;
            headers.append(name, value);
        }
        Ok(headers)
    }
}

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
enum StoredVersion {
    Http09,
    Http10,
    Http11,
    Http2,
    Http3,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct CachedHttpEntry {
    format_version: u8,
    policy: http_cache_semantics::CachePolicy,
    response: CachedResponse,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct CachedResponse {
    status: u16,
    version: StoredVersion,
    headers: LosslessHeaders,
    trailers: Option<LosslessHeaders>,
    #[serde(skip)]
    body: Vec<u8>,
}

impl TryFrom<&Response> for CachedResponse {
    type Error = CacheEntryError;

    fn try_from(response: &Response) -> std::result::Result<Self, Self::Error> {
        Ok(Self {
            status: response.status().as_u16(),
            version: response.version().try_into()?,
            headers: LosslessHeaders::from(response.headers()),
            trailers: response.trailers().map(LosslessHeaders::from),
            body: response.body.clone(),
        })
    }
}

impl CachedResponse {
    fn into_response(
        self,
        headers: HeaderMap,
        cache_status: CacheStatus,
    ) -> std::result::Result<Response, CacheEntryError> {
        let trailers = self.trailers.map(HeaderMap::try_from).transpose()?;
        let metadata = ResponseMetadata::new(
            StatusCode::from_u16(self.status)?,
            self.version.into(),
            headers,
            trailers,
        );
        Ok(Response::new(metadata, self.body).with_cache_status(cache_status))
    }

    fn headers(&self) -> std::result::Result<HeaderMap, CacheEntryError> {
        self.headers.clone().try_into()
    }

    fn validate(&self) -> std::result::Result<(), CacheEntryError> {
        StatusCode::from_u16(self.status)?;
        self.headers()?;
        if let Some(trailers) = &self.trailers {
            HeaderMap::try_from(trailers.clone())?;
        }
        Ok(())
    }
}

impl TryFrom<Version> for StoredVersion {
    type Error = CacheEntryError;

    fn try_from(version: Version) -> std::result::Result<Self, Self::Error> {
        match version {
            Version::HTTP_09 => Ok(Self::Http09),
            Version::HTTP_10 => Ok(Self::Http10),
            Version::HTTP_11 => Ok(Self::Http11),
            Version::HTTP_2 => Ok(Self::Http2),
            Version::HTTP_3 => Ok(Self::Http3),
            other => Err(CacheEntryError::UnsupportedVersion(format!("{other:?}"))),
        }
    }
}

impl From<StoredVersion> for Version {
    fn from(version: StoredVersion) -> Self {
        match version {
            StoredVersion::Http09 => Self::HTTP_09,
            StoredVersion::Http10 => Self::HTTP_10,
            StoredVersion::Http11 => Self::HTTP_11,
            StoredVersion::Http2 => Self::HTTP_2,
            StoredVersion::Http3 => Self::HTTP_3,
        }
    }
}

#[derive(Debug, thiserror::Error)]
enum CacheEntryError {
    #[error("invalid cached entry framing: {0}")]
    Format(&'static str),
    #[error("invalid cached entry metadata: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid cached header name: {0}")]
    HeaderName(#[from] hyper::header::InvalidHeaderName),
    #[error("invalid cached header value: {0}")]
    HeaderValue(#[from] hyper::header::InvalidHeaderValue),
    #[error("invalid cached header encoding: {0}")]
    Base64(#[from] base64::DecodeError),
    #[error("invalid cached status: {0}")]
    Status(#[from] hyper::http::status::InvalidStatusCode),
    #[error("unsupported cached HTTP version: {0}")]
    UnsupportedVersion(String),
    #[error("unsupported cache entry format version: {0}")]
    UnsupportedFormat(u8),
}

fn decode_entry(mut bytes: Vec<u8>) -> std::result::Result<CachedHttpEntry, CacheEntryError> {
    if bytes.len() < HTTP_CACHE_FOOTER_LEN {
        return Err(CacheEntryError::Format("truncated footer"));
    }

    let magic_start = bytes.len() - HTTP_CACHE_MAGIC.len();
    if &bytes[magic_start..] != HTTP_CACHE_MAGIC {
        return Err(CacheEntryError::Format("unsupported magic or version"));
    }
    let length_start = magic_start - std::mem::size_of::<u64>();
    let metadata_len = u64::from_be_bytes(
        bytes[length_start..magic_start]
            .try_into()
            .expect("metadata length occupies eight bytes"),
    );
    let metadata_len = usize::try_from(metadata_len)
        .map_err(|_| CacheEntryError::Format("metadata length exceeds platform limits"))?;
    let metadata_start = length_start
        .checked_sub(metadata_len)
        .ok_or(CacheEntryError::Format("metadata length exceeds payload"))?;

    let mut entry: CachedHttpEntry = serde_json::from_slice(&bytes[metadata_start..length_start])?;
    if entry.format_version != HTTP_CACHE_FORMAT_VERSION {
        return Err(CacheEntryError::UnsupportedFormat(entry.format_version));
    }
    bytes.truncate(metadata_start);
    entry.response.body = bytes;
    entry.response.validate()?;
    Ok(entry)
}

fn is_cleartext_policy_header(name: &HeaderName) -> bool {
    matches!(
        name.as_str(),
        "host" | "cache-control" | "pragma" | "if-none-match" | "if-modified-since"
    )
}

fn fingerprint_request_headers(headers: &mut HeaderMap) {
    let names = headers
        .keys()
        .filter(|name| !is_cleartext_policy_header(name))
        .cloned()
        .collect::<Vec<_>>();
    for name in names {
        let values = headers.get_all(&name).iter().cloned().collect::<Vec<_>>();
        headers.remove(&name);
        for value in values {
            let mut hasher = Sha256::new();
            hasher.update(b"librebar-http-cache-credential\0");
            hasher.update(name.as_str().as_bytes());
            hasher.update(b"\0");
            hasher.update(value.as_bytes());
            let fingerprint = format!("sha256:{}:{:x}", name.as_str(), hasher.finalize());
            headers.append(
                name.clone(),
                HeaderValue::from_str(&fingerprint)
                    .expect("a SHA-256 fingerprint is a valid header value"),
            );
        }
    }
}

fn restore_wire_headers(policy_headers: &mut HeaderMap, wire_headers: &HeaderMap) {
    for name in wire_headers
        .keys()
        .filter(|name| !is_cleartext_policy_header(name))
    {
        policy_headers.remove(name);
        for value in wire_headers.get_all(name) {
            policy_headers.append(name.clone(), value.clone());
        }
    }
}

fn namespaced_key(caller_key: &str) -> String {
    format!("http:v2:{caller_key}")
}

fn fresh_response(
    cached: CachedResponse,
    policy_headers: &HeaderMap,
    cache_status: CacheStatus,
) -> std::result::Result<Response, CacheEntryError> {
    let mut headers = cached.headers()?;
    remove_hop_by_hop(&mut headers);

    replace_from_policy(&mut headers, policy_headers, AGE);
    replace_from_policy(&mut headers, policy_headers, DATE);
    refresh_warnings(&mut headers, policy_headers);

    cached.into_response(headers, cache_status)
}

fn replace_from_policy(headers: &mut HeaderMap, policy: &HeaderMap, name: HeaderName) {
    headers.remove(&name);
    for value in policy.get_all(&name) {
        headers.append(name.clone(), value.clone());
    }
}

fn remove_hop_by_hop(headers: &mut HeaderMap) {
    let connection_fields = headers
        .get_all(CONNECTION)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .filter_map(|name| HeaderName::from_bytes(name.trim().as_bytes()).ok())
        .collect::<Vec<_>>();
    for name in connection_fields {
        headers.remove(name);
    }
    for name in [
        "connection",
        "keep-alive",
        "proxy-authenticate",
        "proxy-authorization",
        "te",
        "trailer",
        "transfer-encoding",
        "upgrade",
    ] {
        headers.remove(name);
    }
}

fn refresh_warnings(headers: &mut HeaderMap, policy: &HeaderMap) {
    let retained = headers
        .get_all(WARNING)
        .iter()
        .flat_map(|value| {
            value.to_str().map_or_else(
                |_| vec![value.clone()],
                |value| {
                    value
                        .split(',')
                        .map(str::trim)
                        .filter(|warning| !warning.starts_with('1'))
                        .map(|warning| {
                            HeaderValue::from_str(warning).expect("existing warning is valid")
                        })
                        .collect::<Vec<_>>()
                },
            )
        })
        .collect::<Vec<_>>();
    headers.remove(WARNING);
    for value in retained {
        headers.append(WARNING, value);
    }

    for value in policy.get_all(WARNING) {
        let generated_113 = value.to_str().is_ok_and(|value| {
            value
                .split(',')
                .any(|warning| warning.trim().starts_with("113 "))
        });
        if generated_113
            && !headers
                .get_all(WARNING)
                .iter()
                .any(|existing| existing == value)
        {
            headers.append(WARNING, value.clone());
        }
    }
}

fn merge_304_headers(stored: &HeaderMap, not_modified: &HeaderMap) -> HeaderMap {
    let mut merged = stored.clone();
    for name in not_modified.keys() {
        if [
            CONTENT_LENGTH,
            CONTENT_ENCODING,
            hyper::header::TRANSFER_ENCODING,
            CONTENT_RANGE,
        ]
        .contains(name)
        {
            continue;
        }
        merged.remove(name);
        for value in not_modified.get_all(name) {
            merged.append(name.clone(), value.clone());
        }
    }
    merged
}

pub(super) async fn get_cached(
    client: &HttpClient,
    cache: &Cache,
    key: &str,
    url: &str,
) -> Result<Response> {
    let uri: hyper::Uri = url.parse().map_err(crate::error::HttpError::InvalidUrl)?;
    let mut wire_request = Request::builder()
        .method(Method::GET)
        .uri(uri)
        .body(Bytes::new())
        .map_err(crate::error::HttpError::RequestBuild)?;
    client.prepare_request(&mut wire_request)?;
    let policy_request = policy_request(&wire_request)?;
    let now = SystemTime::now();

    let Some(entry) = load_entry(cache, key).await? else {
        return fetch_and_maybe_store(client, cache, key, wire_request).await;
    };

    match entry.policy.before_request(&policy_request, now) {
        BeforeRequest::Fresh(parts) => {
            match fresh_response(entry.response, &parts.headers, CacheStatus::Hit) {
                Ok(response) => Ok(response),
                Err(error) => {
                    tracing::warn!(key, error = %error, "discarding corrupt HTTP cache entry");
                    evict_entry(cache, key).await;
                    fetch_and_maybe_store(client, cache, key, wire_request).await
                }
            }
        }
        BeforeRequest::Stale { request, matches } if matches => {
            revalidate(
                client,
                cache,
                key,
                entry,
                policy_request,
                request,
                wire_request,
            )
            .await
        }
        BeforeRequest::Stale { .. } => {
            evict_entry(cache, key).await;
            fetch_and_maybe_store(client, cache, key, wire_request).await
        }
    }
}

fn policy_request(wire: &Request<Bytes>) -> Result<Request<()>> {
    let mut request = Request::builder()
        .method(wire.method().clone())
        .uri(wire.uri().clone())
        .version(wire.version())
        .body(())
        .map_err(crate::error::HttpError::RequestBuild)?;
    *request.headers_mut() = wire.headers().clone();
    fingerprint_request_headers(request.headers_mut());
    Ok(request)
}

async fn load_entry(cache: &Cache, key: &str) -> Result<Option<CachedHttpEntry>> {
    let cache = cache.clone();
    let key = key.to_owned();
    crate::cache::run_io(move || load_entry_blocking(&cache, &key)).await
}

fn load_entry_blocking(cache: &Cache, key: &str) -> Result<Option<CachedHttpEntry>> {
    let namespaced = namespaced_key(key);
    let bytes = match cache.get(&namespaced) {
        Ok(bytes) => bytes,
        Err(Error::Cache(CacheError::Format(error))) => {
            tracing::warn!(key, error = %error, "discarding corrupt HTTP cache entry");
            evict_entry_blocking(cache, key);
            return Ok(None);
        }
        Err(error) => return Err(error),
    };
    let Some(bytes) = bytes else {
        return Ok(None);
    };

    match decode_entry(bytes) {
        Ok(entry) => Ok(Some(entry)),
        Err(error) => {
            tracing::warn!(key, error = %error, "discarding corrupt HTTP cache entry");
            evict_entry_blocking(cache, key);
            Ok(None)
        }
    }
}

fn warn_eviction_failure(key: &str, error: &Error) {
    tracing::warn!(key, error = %error, "failed to evict HTTP cache entry");
}

fn evict_entry_blocking(cache: &Cache, key: &str) {
    if let Err(error) = cache.remove(&namespaced_key(key)) {
        warn_eviction_failure(key, &error);
    }
}

async fn evict_entry(cache: &Cache, key: &str) {
    let cache = cache.clone();
    let owned_key = key.to_owned();
    let result = crate::cache::run_io(move || cache.remove(&namespaced_key(&owned_key))).await;
    if let Err(error) = result {
        warn_eviction_failure(key, &error);
    }
}

async fn fetch_and_maybe_store(
    client: &HttpClient,
    cache: &Cache,
    key: &str,
    wire_request: Request<Bytes>,
) -> Result<Response> {
    let request_for_policy = policy_request(&wire_request)?;
    let response = client.send(wire_request).await?;
    let response_time = SystemTime::now();
    let policy_response = response_head(&response)?;
    let policy = CachePolicy::new_options(
        &request_for_policy,
        &policy_response,
        response_time,
        private_cache_options(),
    );

    if policy.is_storable() {
        persist_entry(client, cache, key, &policy, &response, response_time).await;
    } else {
        evict_entry(cache, key).await;
    }
    Ok(response.with_cache_status(CacheStatus::Miss))
}

async fn revalidate(
    client: &HttpClient,
    cache: &Cache,
    key: &str,
    entry: CachedHttpEntry,
    original_policy_request: Request<()>,
    policy_parts: hyper::http::request::Parts,
    wire_request: Request<Bytes>,
) -> Result<Response> {
    let policy_revalidation = Request::from_parts(policy_parts, ());
    let mut wire_revalidation = Request::builder()
        .method(policy_revalidation.method().clone())
        .uri(policy_revalidation.uri().clone())
        .version(policy_revalidation.version())
        .body(Bytes::new())
        .map_err(crate::error::HttpError::RequestBuild)?;
    *wire_revalidation.headers_mut() = policy_revalidation.headers().clone();
    restore_wire_headers(wire_revalidation.headers_mut(), wire_request.headers());

    let response = client.send(wire_revalidation).await?;
    let response_time = SystemTime::now();
    let policy_response = response_head(&response)?;
    match entry
        .policy
        .after_response(&policy_revalidation, &policy_response, response_time)
    {
        AfterResponse::NotModified(_, calculated_parts) => {
            let stored_headers = entry.response.headers().map_err(corrupt_cache_error)?;
            let merged_headers = merge_304_headers(&stored_headers, response.headers());
            let mut cached = entry.response;
            cached.headers = LosslessHeaders::from(&merged_headers);
            let merged_policy_response = cached_response_head(&cached, merged_headers)?;
            let policy = CachePolicy::new_options(
                &original_policy_request,
                &merged_policy_response,
                response_time,
                private_cache_options(),
            );
            let cached = persist_cached_response(
                cache,
                key,
                &policy,
                cached,
                response_time,
                client.config().http_cache_stale_retention,
            )
            .await?;
            fresh_response(cached, &calculated_parts.headers, CacheStatus::Revalidated)
                .map_err(corrupt_cache_error)
        }
        AfterResponse::Modified(_, _) => {
            if response.status() == StatusCode::NOT_MODIFIED {
                evict_entry(cache, key).await;
                return Ok(response.with_cache_status(CacheStatus::Miss));
            }
            let policy = CachePolicy::new_options(
                &original_policy_request,
                &policy_response,
                response_time,
                private_cache_options(),
            );
            if policy.is_storable() {
                persist_entry(client, cache, key, &policy, &response, response_time).await;
            } else {
                evict_entry(cache, key).await;
            }
            Ok(response.with_cache_status(CacheStatus::Miss))
        }
    }
}

fn response_head(response: &Response) -> Result<hyper::Response<()>> {
    let mut head = hyper::Response::builder()
        .status(response.status())
        .version(response.version())
        .body(())
        .map_err(crate::error::HttpError::RequestBuild)?;
    *head.headers_mut() = response.headers().clone();
    Ok(head)
}

fn cached_response_head(
    cached: &CachedResponse,
    headers: HeaderMap,
) -> Result<hyper::Response<()>> {
    let mut head = hyper::Response::builder()
        .status(StatusCode::from_u16(cached.status).map_err(corrupt_cache_error)?)
        .version(cached.version.into())
        .body(())
        .map_err(crate::error::HttpError::RequestBuild)?;
    *head.headers_mut() = headers;
    Ok(head)
}

fn private_cache_options() -> CacheOptions {
    CacheOptions {
        shared: false,
        ..CacheOptions::default()
    }
}

async fn persist_entry(
    client: &HttpClient,
    cache: &Cache,
    key: &str,
    policy: &CachePolicy,
    response: &Response,
    now: SystemTime,
) {
    match CachedResponse::try_from(response) {
        Ok(cached) => {
            if let Err(error) = persist_cached_response(
                cache,
                key,
                policy,
                cached,
                now,
                client.config().http_cache_stale_retention,
            )
            .await
            {
                tracing::warn!(key, error = %error, "failed to persist HTTP cache entry");
            }
        }
        Err(error) => tracing::warn!(key, error = %error, "failed to encode HTTP cache response"),
    }
}

async fn persist_cached_response(
    cache: &Cache,
    key: &str,
    policy: &CachePolicy,
    response: CachedResponse,
    now: SystemTime,
    stale_retention: Duration,
) -> Result<CachedResponse> {
    let cache = cache.clone();
    let key = key.to_owned();
    let policy = policy.clone();
    crate::cache::run_io(move || {
        Ok(persist_cached_response_blocking(
            &cache,
            &key,
            &policy,
            response,
            now,
            stale_retention,
        ))
    })
    .await
}

fn persist_cached_response_blocking(
    cache: &Cache,
    key: &str,
    policy: &CachePolicy,
    response: CachedResponse,
    now: SystemTime,
    stale_retention: Duration,
) -> CachedResponse {
    let entry = CachedHttpEntry {
        format_version: HTTP_CACHE_FORMAT_VERSION,
        policy: policy.clone(),
        response,
    };
    let metadata = match serde_json::to_vec(&entry) {
        Ok(metadata) => metadata,
        Err(error) => {
            tracing::warn!(key, error = %error, "failed to serialize HTTP cache entry");
            return entry.response;
        }
    };
    let metadata_len = (metadata.len() as u64).to_be_bytes();
    let ttl = policy.time_to_live(now).saturating_add(stale_retention);
    let parts: [&[u8]; 4] = [
        entry.response.body.as_slice(),
        metadata.as_slice(),
        &metadata_len,
        HTTP_CACHE_MAGIC,
    ];
    if let Err(error) = cache.set_parts(&namespaced_key(key), &parts, ttl) {
        tracing::warn!(key, error = %error, "failed to persist HTTP cache entry");
    }
    entry.response
}

fn corrupt_cache_error(error: impl std::fmt::Display) -> Error {
    Error::Cache(CacheError::Io(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        error.to_string(),
    )))
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "logging")]
    use std::collections::BTreeMap;
    #[cfg(feature = "logging")]
    use std::sync::{Arc, Mutex};
    use std::time::SystemTime;

    use http_cache_semantics::BeforeRequest;
    use http_cache_semantics::{CacheOptions, CachePolicy};
    use hyper::header::{
        AUTHORIZATION, CACHE_CONTROL, COOKIE, IF_MODIFIED_SINCE, IF_NONE_MATCH, LINK,
        PROXY_AUTHORIZATION, SET_COOKIE,
    };
    use hyper::{Request, StatusCode};
    #[cfg(feature = "logging")]
    use tracing::field::{Field, Visit};
    #[cfg(feature = "logging")]
    use tracing_subscriber::layer::SubscriberExt as _;

    use super::*;

    #[cfg(feature = "logging")]
    #[derive(Clone, Default)]
    struct WarningCapture(Arc<Mutex<Vec<BTreeMap<String, String>>>>);

    #[cfg(feature = "logging")]
    impl<S> tracing_subscriber::Layer<S> for WarningCapture
    where
        S: tracing::Subscriber,
    {
        fn on_event(
            &self,
            event: &tracing::Event<'_>,
            _context: tracing_subscriber::layer::Context<'_, S>,
        ) {
            if *event.metadata().level() != tracing::Level::WARN {
                return;
            }
            let mut visitor = EventVisitor::default();
            event.record(&mut visitor);
            self.0.lock().unwrap().push(visitor.fields);
        }
    }

    #[cfg(feature = "logging")]
    #[derive(Default)]
    struct EventVisitor {
        fields: BTreeMap<String, String>,
    }

    #[cfg(feature = "logging")]
    impl Visit for EventVisitor {
        fn record_str(&mut self, field: &Field, value: &str) {
            self.fields
                .insert(field.name().to_owned(), value.to_owned());
        }

        fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
            self.fields
                .insert(field.name().to_owned(), format!("{value:?}"));
        }
    }

    #[cfg(feature = "logging")]
    #[test]
    fn eviction_failure_is_logged_with_key_and_error() {
        use base64::Engine as _;

        let directory = tempfile::tempdir().unwrap();
        let cache = Cache::new(directory.path());
        let key = "locked";
        let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(namespaced_key(key));
        let entry_path = directory.path().join(format!("v2-{encoded}.cache"));
        std::fs::create_dir(&entry_path).unwrap();

        let capture = WarningCapture::default();
        let events = capture.0.clone();
        let subscriber = tracing_subscriber::registry().with(capture);
        let _subscriber_guard = tracing::subscriber::set_default(subscriber);

        evict_entry_blocking(&cache, key);

        let events = events.lock().unwrap();
        let warning = events
            .iter()
            .find(|fields| {
                fields
                    .get("message")
                    .is_some_and(|message| message.contains("failed to evict HTTP cache entry"))
            })
            .expect("eviction failure should emit a warning");
        assert_eq!(warning.get("key").map(String::as_str), Some(key));
        assert!(warning.get("error").is_some_and(|error| !error.is_empty()));
        drop(events);
    }

    #[test]
    fn lossless_headers_round_trip_duplicates_and_opaque_bytes() {
        let mut headers = HeaderMap::new();
        headers.append("x-many", HeaderValue::from_static("one"));
        headers.append("x-many", HeaderValue::from_static("two"));
        headers.append("x-opaque", HeaderValue::from_bytes(b"\x80value").unwrap());

        let stored = LosslessHeaders::from(&headers);
        let encoded = serde_json::to_vec(&stored).unwrap();
        let decoded: LosslessHeaders = serde_json::from_slice(&encoded).unwrap();
        let rebuilt = HeaderMap::try_from(decoded).unwrap();

        assert_eq!(
            rebuilt
                .get_all("x-many")
                .iter()
                .map(|value| value.to_str().unwrap())
                .collect::<Vec<_>>(),
            ["one", "two"]
        );
        assert_eq!(rebuilt["x-opaque"].as_bytes(), b"\x80value");
    }

    #[test]
    fn policy_view_fingerprints_all_non_policy_request_headers() {
        let mut request = Request::builder()
            .uri("https://example.test/private")
            .header(AUTHORIZATION, "Bearer super-secret")
            .header(PROXY_AUTHORIZATION, "Basic proxy-secret")
            .header(COOKIE, "session=also-secret")
            .header("x-api-key", "api-secret")
            .header("private-token", "private-secret")
            .header("x-opaque-credential", "opaque-secret")
            .body(())
            .unwrap();
        let response = hyper::Response::builder()
            .status(StatusCode::OK)
            .header(CACHE_CONTROL, "private, max-age=60")
            .body(())
            .unwrap();

        fingerprint_request_headers(request.headers_mut());
        let policy = CachePolicy::new_options(
            &request,
            &response,
            SystemTime::now(),
            CacheOptions {
                shared: false,
                ..CacheOptions::default()
            },
        );
        let serialized = serde_json::to_string(&policy).unwrap();

        for secret in [
            "super-secret",
            "proxy-secret",
            "also-secret",
            "api-secret",
            "private-secret",
            "opaque-secret",
        ] {
            assert!(
                !serialized.contains(secret),
                "persisted request secret: {secret}"
            );
        }
    }

    #[test]
    fn policy_view_keeps_cache_semantics_fields_cleartext() {
        let mut headers = HeaderMap::new();
        headers.insert("host", HeaderValue::from_static("example.test"));
        headers.insert(CACHE_CONTROL, HeaderValue::from_static("max-age=30"));
        headers.insert("pragma", HeaderValue::from_static("no-cache"));
        headers.insert(IF_NONE_MATCH, HeaderValue::from_static("\"client-v1\""));
        headers.insert(
            IF_MODIFIED_SINCE,
            HeaderValue::from_static("Wed, 21 Oct 2015 07:28:00 GMT"),
        );

        fingerprint_request_headers(&mut headers);

        assert_eq!(headers["host"], "example.test");
        assert_eq!(headers[CACHE_CONTROL], "max-age=30");
        assert_eq!(headers["pragma"], "no-cache");
        assert_eq!(headers[IF_NONE_MATCH], "\"client-v1\"");
        assert_eq!(headers[IF_MODIFIED_SINCE], "Wed, 21 Oct 2015 07:28:00 GMT");
    }

    #[test]
    fn wire_headers_replace_fingerprints_without_clobbering_policy_validators() {
        let wire = Request::builder()
            .uri("https://example.test/private")
            .header(AUTHORIZATION, "Bearer real")
            .header(PROXY_AUTHORIZATION, "Basic proxy-real")
            .header(COOKIE, "session=real")
            .header("x-api-key", "api-real")
            .header(IF_NONE_MATCH, "\"wire-validator\"")
            .body(())
            .unwrap();
        let mut policy_headers = wire.headers().clone();
        fingerprint_request_headers(&mut policy_headers);
        policy_headers.insert(
            IF_NONE_MATCH,
            HeaderValue::from_static("\"policy-validator\""),
        );

        restore_wire_headers(&mut policy_headers, wire.headers());

        assert_eq!(policy_headers[AUTHORIZATION], "Bearer real");
        assert_eq!(policy_headers[PROXY_AUTHORIZATION], "Basic proxy-real");
        assert_eq!(policy_headers[COOKIE], "session=real");
        assert_eq!(policy_headers["x-api-key"], "api-real");
        assert_eq!(policy_headers[IF_NONE_MATCH], "\"policy-validator\"");
        assert!(!format!("{policy_headers:?}").contains("sha256:"));
    }

    #[test]
    fn vary_matches_fingerprinted_request_headers() {
        let now = SystemTime::now();
        let mut stored_request = Request::builder()
            .uri("https://example.test/private")
            .header("x-api-key", "profile-a")
            .body(())
            .unwrap();
        fingerprint_request_headers(stored_request.headers_mut());
        let response = hyper::Response::builder()
            .status(StatusCode::OK)
            .header(CACHE_CONTROL, "private, max-age=60")
            .header("vary", "x-api-key")
            .body(())
            .unwrap();
        let policy = CachePolicy::new_options(
            &stored_request,
            &response,
            now,
            CacheOptions {
                shared: false,
                ..CacheOptions::default()
            },
        );

        let mut same = Request::builder()
            .uri("https://example.test/private")
            .header("x-api-key", "profile-a")
            .body(())
            .unwrap();
        fingerprint_request_headers(same.headers_mut());
        let mut different = Request::builder()
            .uri("https://example.test/private")
            .header("x-api-key", "profile-b")
            .body(())
            .unwrap();
        fingerprint_request_headers(different.headers_mut());

        assert!(matches!(
            policy.before_request(&same, now),
            BeforeRequest::Fresh(_)
        ));
        assert!(matches!(
            policy.before_request(&different, now),
            BeforeRequest::Stale { matches: false, .. }
        ));
    }

    #[test]
    fn cache_keys_are_namespaced() {
        assert_eq!(namespaced_key("item"), "http:v2:item");
    }

    #[test]
    fn fresh_hit_preserves_repeated_headers() {
        let mut response_headers = HeaderMap::new();
        response_headers.append(CACHE_CONTROL, HeaderValue::from_static("max-age=3600"));
        response_headers.append(LINK, HeaderValue::from_static("</a>"));
        response_headers.append(LINK, HeaderValue::from_static("</b>"));
        response_headers.append(SET_COOKIE, HeaderValue::from_static("a=1"));
        response_headers.append(SET_COOKIE, HeaderValue::from_static("b=2"));
        response_headers.append(CONNECTION, HeaderValue::from_static("x-hop"));
        response_headers.append("x-hop", HeaderValue::from_static("remove me"));
        let request = Request::builder()
            .uri("https://example.test/item")
            .body(())
            .unwrap();
        let policy_response = hyper::Response::builder()
            .status(StatusCode::OK)
            .body(())
            .unwrap();
        let mut policy_response = policy_response;
        *policy_response.headers_mut() = response_headers.clone();
        let policy = CachePolicy::new_options(
            &request,
            &policy_response,
            SystemTime::now(),
            CacheOptions {
                shared: false,
                ..CacheOptions::default()
            },
        );
        let policy_headers = match policy.before_request(&request, SystemTime::now()) {
            BeforeRequest::Fresh(parts) => parts.headers,
            BeforeRequest::Stale { .. } => panic!("policy should be fresh"),
        };
        let metadata =
            ResponseMetadata::new(StatusCode::OK, Version::HTTP_11, response_headers, None);
        let response = Response::new(metadata, b"body".to_vec());
        let cached = CachedResponse::try_from(&response).unwrap();

        let served = fresh_response(cached, &policy_headers, CacheStatus::Hit).unwrap();

        assert_eq!(served.headers().get_all(LINK).iter().count(), 2);
        assert_eq!(served.headers().get_all(SET_COOKIE).iter().count(), 2);
        assert!(!served.headers().contains_key(CONNECTION));
        assert!(!served.headers().contains_key("x-hop"));
        assert_eq!(served.headers().get_all(AGE).iter().count(), 1);
        assert_eq!(served.headers().get_all(DATE).iter().count(), 1);
    }

    #[test]
    fn revalidation_merge_preserves_duplicates_and_body_metadata() {
        let mut stored = HeaderMap::new();
        stored.append(LINK, HeaderValue::from_static("</old-a>"));
        stored.append(LINK, HeaderValue::from_static("</old-b>"));
        stored.insert(CONTENT_LENGTH, HeaderValue::from_static("5"));
        let mut not_modified = HeaderMap::new();
        not_modified.append(LINK, HeaderValue::from_static("</new-a>"));
        not_modified.append(LINK, HeaderValue::from_static("</new-b>"));
        not_modified.insert(CONTENT_LENGTH, HeaderValue::from_static("999"));

        let merged = merge_304_headers(&stored, &not_modified);

        assert_eq!(
            merged
                .get_all(LINK)
                .iter()
                .map(|value| value.to_str().unwrap())
                .collect::<Vec<_>>(),
            ["</new-a>", "</new-b>"]
        );
        assert_eq!(merged[CONTENT_LENGTH], "5");
    }
}
