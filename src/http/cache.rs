use std::time::SystemTime;

use base64::Engine as _;
use http_cache_semantics::{AfterResponse, BeforeRequest, CacheOptions, CachePolicy};
use hyper::header::{
    AGE, AUTHORIZATION, CONNECTION, CONTENT_ENCODING, CONTENT_LENGTH, CONTENT_RANGE, DATE,
    HeaderMap, HeaderName, HeaderValue, PROXY_AUTHORIZATION, WARNING,
};
use hyper::{Method, Request, StatusCode, Version};
use sha2::{Digest, Sha256};

use super::{Bytes, CacheStatus, HttpClient, Response, ResponseMetadata};
use crate::cache::Cache;
use crate::error::CacheError;
use crate::{Error, Result};

const SENSITIVE_REQUEST_HEADERS: [HeaderName; 3] =
    [AUTHORIZATION, PROXY_AUTHORIZATION, hyper::header::COOKIE];

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

fn fingerprint_credentials(headers: &mut HeaderMap) {
    for name in &SENSITIVE_REQUEST_HEADERS {
        let values = headers.get_all(name).iter().cloned().collect::<Vec<_>>();
        if values.is_empty() {
            continue;
        }
        headers.remove(name);
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

fn restore_wire_credentials(policy_headers: &mut HeaderMap, wire_headers: &HeaderMap) {
    for name in &SENSITIVE_REQUEST_HEADERS {
        policy_headers.remove(name);
        for value in wire_headers.get_all(name) {
            policy_headers.append(name.clone(), value.clone());
        }
    }
}

fn namespaced_key(caller_key: &str) -> String {
    format!("http:v1:{caller_key}")
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

    let Some(entry) = load_entry(cache, key)? else {
        return fetch_and_maybe_store(client, cache, key, wire_request).await;
    };

    match entry.policy.before_request(&policy_request, now) {
        BeforeRequest::Fresh(parts) => {
            match fresh_response(entry.response, &parts.headers, CacheStatus::Hit) {
                Ok(response) => Ok(response),
                Err(error) => {
                    tracing::warn!(key, error = %error, "discarding corrupt HTTP cache entry");
                    let _ = cache.remove(&namespaced_key(key));
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
            let _ = cache.remove(&namespaced_key(key));
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
    fingerprint_credentials(request.headers_mut());
    Ok(request)
}

fn load_entry(cache: &Cache, key: &str) -> Result<Option<CachedHttpEntry>> {
    let namespaced = namespaced_key(key);
    let bytes = match cache.get(&namespaced) {
        Ok(bytes) => bytes,
        Err(Error::Cache(CacheError::Json(error))) => {
            tracing::warn!(key, error = %error, "discarding corrupt HTTP cache entry");
            let _ = cache.remove(&namespaced);
            return Ok(None);
        }
        Err(Error::Cache(CacheError::Decode(error))) => {
            tracing::warn!(key, error = %error, "discarding corrupt HTTP cache entry");
            let _ = cache.remove(&namespaced);
            return Ok(None);
        }
        Err(error) => return Err(error),
    };
    let Some(bytes) = bytes else {
        return Ok(None);
    };

    let parsed = serde_json::from_slice::<CachedHttpEntry>(&bytes);
    match parsed.and_then(|entry| {
        if entry.format_version != 1 {
            return Err(serde_json::Error::io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                CacheEntryError::UnsupportedFormat(entry.format_version),
            )));
        }
        entry.response.validate().map_err(|error| {
            serde_json::Error::io(std::io::Error::new(std::io::ErrorKind::InvalidData, error))
        })?;
        Ok(entry)
    }) {
        Ok(entry) => Ok(Some(entry)),
        Err(error) => {
            tracing::warn!(key, error = %error, "discarding corrupt HTTP cache entry");
            let _ = cache.remove(&namespaced);
            Ok(None)
        }
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
        persist_entry(client, cache, key, &policy, &response, response_time);
    } else if let Err(error) = cache.remove(&namespaced_key(key)) {
        tracing::warn!(key, error = %error, "failed to remove non-storable HTTP cache entry");
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
    restore_wire_credentials(wire_revalidation.headers_mut(), wire_request.headers());

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
            persist_cached_response(client, cache, key, &policy, &cached, response_time);
            fresh_response(cached, &calculated_parts.headers, CacheStatus::Revalidated)
                .map_err(corrupt_cache_error)
        }
        AfterResponse::Modified(_, _) => {
            if response.status() == StatusCode::NOT_MODIFIED {
                let _ = cache.remove(&namespaced_key(key));
                return Ok(response.with_cache_status(CacheStatus::Miss));
            }
            let policy = CachePolicy::new_options(
                &original_policy_request,
                &policy_response,
                response_time,
                private_cache_options(),
            );
            if policy.is_storable() {
                persist_entry(client, cache, key, &policy, &response, response_time);
            } else if let Err(error) = cache.remove(&namespaced_key(key)) {
                tracing::warn!(key, error = %error, "failed to remove non-storable HTTP cache entry");
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

fn persist_entry(
    client: &HttpClient,
    cache: &Cache,
    key: &str,
    policy: &CachePolicy,
    response: &Response,
    now: SystemTime,
) {
    match CachedResponse::try_from(response) {
        Ok(cached) => persist_cached_response(client, cache, key, policy, &cached, now),
        Err(error) => tracing::warn!(key, error = %error, "failed to encode HTTP cache response"),
    }
}

fn persist_cached_response(
    client: &HttpClient,
    cache: &Cache,
    key: &str,
    policy: &CachePolicy,
    response: &CachedResponse,
    now: SystemTime,
) {
    let entry = CachedHttpEntry {
        format_version: 1,
        policy: policy.clone(),
        response: CachedResponse {
            status: response.status,
            version: response.version,
            headers: response.headers.clone(),
            trailers: response.trailers.clone(),
            body: response.body.clone(),
        },
    };
    let encoded = match serde_json::to_vec(&entry) {
        Ok(encoded) => encoded,
        Err(error) => {
            tracing::warn!(key, error = %error, "failed to serialize HTTP cache entry");
            return;
        }
    };
    let ttl = policy
        .time_to_live(now)
        .saturating_add(client.config().http_cache_stale_retention);
    if let Err(error) = cache.set(&namespaced_key(key), &encoded, ttl) {
        tracing::warn!(key, error = %error, "failed to persist HTTP cache entry");
    }
}

fn corrupt_cache_error(error: impl std::fmt::Display) -> Error {
    Error::Cache(CacheError::Io(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        error.to_string(),
    )))
}

#[cfg(test)]
mod tests {
    use std::time::SystemTime;

    use http_cache_semantics::BeforeRequest;
    use http_cache_semantics::{CacheOptions, CachePolicy};
    use hyper::header::{CACHE_CONTROL, COOKIE, LINK, SET_COOKIE};
    use hyper::{Request, StatusCode};

    use super::*;

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
    fn policy_view_fingerprints_request_credentials() {
        let mut request = Request::builder()
            .uri("https://example.test/private")
            .header(AUTHORIZATION, "Bearer super-secret")
            .header(PROXY_AUTHORIZATION, "Basic proxy-secret")
            .header(COOKIE, "session=also-secret")
            .body(())
            .unwrap();
        let response = hyper::Response::builder()
            .status(StatusCode::OK)
            .header(CACHE_CONTROL, "private, max-age=60")
            .body(())
            .unwrap();

        fingerprint_credentials(request.headers_mut());
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

        assert!(!serialized.contains("super-secret"));
        assert!(!serialized.contains("proxy-secret"));
        assert!(!serialized.contains("also-secret"));
        assert!(request.headers().contains_key(AUTHORIZATION));
        assert!(request.headers().contains_key(PROXY_AUTHORIZATION));
        assert!(request.headers().contains_key(COOKIE));
    }

    #[test]
    fn wire_credentials_replace_policy_fingerprints_before_send() {
        let wire = Request::builder()
            .uri("https://example.test/private")
            .header(AUTHORIZATION, "Bearer real")
            .header(PROXY_AUTHORIZATION, "Basic proxy-real")
            .header(COOKIE, "session=real")
            .body(())
            .unwrap();
        let mut policy_headers = wire.headers().clone();
        fingerprint_credentials(&mut policy_headers);

        restore_wire_credentials(&mut policy_headers, wire.headers());

        assert_eq!(policy_headers[AUTHORIZATION], "Bearer real");
        assert_eq!(policy_headers[PROXY_AUTHORIZATION], "Basic proxy-real");
        assert_eq!(policy_headers[COOKIE], "session=real");
        assert!(!format!("{policy_headers:?}").contains("sha256:"));
    }

    #[test]
    fn cache_keys_are_namespaced() {
        assert_eq!(namespaced_key("item"), "http:v1:item");
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
