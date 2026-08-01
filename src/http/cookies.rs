use std::collections::BTreeMap;
use std::fs::File;
use std::future::Future;
use std::io::{BufReader, BufWriter, Write};
use std::path::Path;
use std::pin::Pin;
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::task::{Context, Poll};

use atomic_write_file::AtomicWriteFile;
use http::header::{COOKIE, SET_COOKIE};
use http::{Request, Response};
use tower::{BoxError, Service};
use url::Url;

use super::{RequestBody, ResponseBody};
use crate::Result;
use crate::error::HttpError;

/// Resource ceilings for an HTTP cookie jar.
///
/// Defaults to 4,096 bytes per cookie name and value, 50 live cookies per
/// domain, and 3,000 live cookies in total. A zero ceiling rejects or evicts
/// every cookie in that category.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CookieLimits {
    max_cookie_bytes: usize,
    max_cookies_per_domain: usize,
    max_cookies_total: usize,
}

impl CookieLimits {
    /// Set the maximum combined byte length of a cookie name and value.
    #[must_use]
    pub const fn max_cookie_bytes(mut self, maximum: usize) -> Self {
        self.max_cookie_bytes = maximum;
        self
    }

    /// Set the maximum number of live cookies retained for one domain.
    #[must_use]
    pub const fn max_cookies_per_domain(mut self, maximum: usize) -> Self {
        self.max_cookies_per_domain = maximum;
        self
    }

    /// Set the maximum number of live cookies retained across all domains.
    #[must_use]
    pub const fn max_cookies_total(mut self, maximum: usize) -> Self {
        self.max_cookies_total = maximum;
        self
    }

    /// Return the maximum combined byte length of a cookie name and value.
    pub const fn cookie_bytes(&self) -> usize {
        self.max_cookie_bytes
    }

    /// Return the maximum number of live cookies retained for one domain.
    pub const fn cookies_per_domain(&self) -> usize {
        self.max_cookies_per_domain
    }

    /// Return the maximum number of live cookies retained across all domains.
    pub const fn cookies_total(&self) -> usize {
        self.max_cookies_total
    }
}

impl Default for CookieLimits {
    fn default() -> Self {
        Self {
            max_cookie_bytes: 4_096,
            max_cookies_per_domain: 50,
            max_cookies_total: 3_000,
        }
    }
}

/// A shareable RFC 6265 cookie jar.
#[derive(Clone, Debug)]
pub struct CookieJar {
    inner: Arc<RwLock<cookie_store::CookieStore>>,
    limits: CookieLimits,
}

impl Default for CookieJar {
    fn default() -> Self {
        Self::with_limits(CookieLimits::default())
    }
}

impl CookieJar {
    pub(super) fn with_limits(limits: CookieLimits) -> Self {
        Self {
            inner: Arc::new(RwLock::new(
                cookie_store::CookieStore::new_with_public_suffix(Some(public_suffix_list())),
            )),
            limits,
        }
    }

    pub(super) fn load_from(path: &Path, limits: CookieLimits) -> Result<Self> {
        let file = File::open(path).map_err(|source| cookie_error("load", path, source))?;
        let store = cookie_store::serde::json::load(BufReader::new(file))
            .map_err(|source| cookie_box_error("load", path, source))?
            .with_suffix_list(public_suffix_list());
        let jar = Self {
            inner: Arc::new(RwLock::new(store)),
            limits,
        };
        {
            let mut store = jar.write_store();
            jar.enforce_limits(&mut store);
        }
        Ok(jar)
    }

    /// Save unexpired cookies, including session cookies, to a JSON file.
    pub fn save_to(&self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        let mut options = AtomicWriteFile::options();
        #[cfg(unix)]
        {
            use atomic_write_file::unix::OpenOptionsExt as _;
            use std::os::unix::fs::OpenOptionsExt as _;
            options.mode(0o600).preserve_mode(false);
        }
        let mut file = options
            .open(path)
            .map_err(|source| cookie_error("save", path, source))?;

        let cookies = {
            let store = self.read_store();
            store.iter_unexpired().cloned().collect::<Vec<_>>()
        };
        {
            let mut writer = BufWriter::new(&mut file);
            serde_json::to_writer_pretty(&mut writer, &cookies)
                .map_err(|source| cookie_error("save", path, source))?;
            writer
                .write_all(b"\n")
                .map_err(|source| cookie_error("save", path, source))?;
            writer
                .flush()
                .map_err(|source| cookie_error("save", path, source))?;
        }
        file.commit()
            .map_err(|source| cookie_error("save", path, source))?;
        Ok(())
    }

    fn request_header(&self, url: &Url) -> Option<http::header::HeaderValue> {
        let store = self.read_store();
        cookie_header_value(store.get_request_values(url))
    }

    pub(super) fn apply_to_request<B>(&self, request: &mut Request<B>) {
        if request.headers().contains_key(COOKIE) {
            return;
        }
        let Some(url) = cookie_url(request.uri()) else {
            return;
        };
        if let Some(value) = self.request_header(&url) {
            request.headers_mut().insert(COOKIE, value);
        }
    }

    fn store_response(&self, url: &Url, response: &Response<ResponseBody>) {
        let cookies = response
            .headers()
            .get_all(SET_COOKIE)
            .iter()
            .filter_map(|value| match value.to_str() {
                Ok(value) => Some(value),
                Err(error) => {
                    tracing::warn!(%error, "dropping non-text Set-Cookie header");
                    None
                }
            })
            .filter_map(
                |value| match cookie_store::RawCookie::parse(value.to_owned()) {
                    Ok(cookie) => Some(cookie),
                    Err(error) => {
                        tracing::warn!(%error, "dropping malformed Set-Cookie header");
                        None
                    }
                },
            )
            .map(cookie_store::RawCookie::into_owned)
            .filter(|cookie| {
                let size = cookie.name().len() + cookie.value().len();
                if size <= self.limits.max_cookie_bytes {
                    return true;
                }
                tracing::warn!(
                    cookie_name = cookie.name(),
                    cookie_domain = cookie.domain().or_else(|| url.host_str()).unwrap_or(""),
                    size,
                    limit = self.limits.max_cookie_bytes,
                    "dropping cookie that exceeds configured size limit"
                );
                false
            });
        let mut store = self.write_store();
        store.store_response_cookies(cookies, url);
        self.enforce_limits(&mut store);
        drop(store);
    }

    fn enforce_limits(&self, store: &mut cookie_store::CookieStore) {
        let oversized = stored_cookie_keys(store)
            .into_iter()
            .filter(|key| key.size > self.limits.max_cookie_bytes)
            .collect::<Vec<_>>();
        for key in oversized {
            if store.remove(&key.domain, &key.path, &key.name).is_some() {
                tracing::warn!(
                    cookie_name = %key.name,
                    cookie_domain = %key.domain,
                    size = key.size,
                    limit = self.limits.max_cookie_bytes,
                    "dropping stored cookie that exceeds configured size limit"
                );
            }
        }

        let mut by_domain = BTreeMap::<String, Vec<StoredCookieKey>>::new();
        for key in stored_cookie_keys(store) {
            by_domain.entry(key.domain.clone()).or_default().push(key);
        }
        for cookies in by_domain.values_mut() {
            cookies.sort_unstable();
            let excess = cookies
                .len()
                .saturating_sub(self.limits.max_cookies_per_domain);
            evict_cookies(
                store,
                cookies.iter().take(excess),
                "per-domain cookie count",
                self.limits.max_cookies_per_domain,
            );
        }

        let mut cookies = stored_cookie_keys(store);
        cookies.sort_unstable();
        let excess = cookies.len().saturating_sub(self.limits.max_cookies_total);
        evict_cookies(
            store,
            cookies.iter().take(excess),
            "total cookie count",
            self.limits.max_cookies_total,
        );
    }

    fn read_store(&self) -> RwLockReadGuard<'_, cookie_store::CookieStore> {
        match self.inner.read() {
            Ok(store) => store,
            Err(error) => {
                tracing::warn!("recovering poisoned cookie jar read lock");
                self.inner.clear_poison();
                error.into_inner()
            }
        }
    }

    fn write_store(&self) -> RwLockWriteGuard<'_, cookie_store::CookieStore> {
        match self.inner.write() {
            Ok(store) => store,
            Err(error) => {
                tracing::warn!("recovering poisoned cookie jar write lock");
                self.inner.clear_poison();
                error.into_inner()
            }
        }
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct StoredCookieKey {
    expiration_class: u8,
    expires_at: i128,
    domain: String,
    path: String,
    name: String,
    size: usize,
}

fn stored_cookie_keys(store: &cookie_store::CookieStore) -> Vec<StoredCookieKey> {
    store
        .iter_unexpired()
        .filter_map(|cookie| {
            let domain = cookie.domain.as_cow()?.into_owned();
            let (expiration_class, expires_at) = match cookie.expires {
                cookie_store::CookieExpiration::AtUtc(expires_at) => {
                    (0, expires_at.unix_timestamp_nanos())
                }
                cookie_store::CookieExpiration::SessionEnd => (1, 0),
            };
            Some(StoredCookieKey {
                expiration_class,
                expires_at,
                domain,
                path: cookie.path.as_ref().to_owned(),
                name: cookie.name().to_owned(),
                size: cookie.name().len() + cookie.value().len(),
            })
        })
        .collect()
}

fn evict_cookies<'a>(
    store: &mut cookie_store::CookieStore,
    cookies: impl IntoIterator<Item = &'a StoredCookieKey>,
    reason: &'static str,
    limit: usize,
) {
    for cookie in cookies {
        if store
            .remove(&cookie.domain, &cookie.path, &cookie.name)
            .is_some()
        {
            tracing::warn!(
                cookie_name = %cookie.name,
                cookie_domain = %cookie.domain,
                reason,
                limit,
                "evicting cookie at configured resource limit"
            );
        }
    }
}

fn cookie_url(uri: &http::Uri) -> Option<Url> {
    match Url::parse(&uri.to_string()) {
        Ok(url) => Some(url),
        Err(error) => {
            tracing::warn!(%error, "cookie jar ignored an unparseable request URI");
            None
        }
    }
}

fn cookie_header_value<'a>(
    values: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Option<http::header::HeaderValue> {
    let value = values
        .into_iter()
        .filter_map(|(name, value)| {
            let pair = format!("{name}={value}");
            match http::header::HeaderValue::from_str(&pair) {
                Ok(_) => Some(pair),
                Err(error) => {
                    tracing::warn!(
                        cookie_name = name,
                        %error,
                        "dropping cookie with invalid HTTP header encoding"
                    );
                    None
                }
            }
        })
        .collect::<Vec<_>>()
        .join("; ");
    if value.is_empty() {
        return None;
    }
    match http::header::HeaderValue::from_str(&value) {
        Ok(value) => Some(value),
        Err(error) => {
            tracing::warn!(%error, "failed to encode validated cookie header");
            None
        }
    }
}

fn public_suffix_list() -> publicsuffix::List {
    publicsuffix::List::from_bytes(include_bytes!("public_suffix_list.dat"))
        .expect("embedded public suffix list must be valid")
}

fn cookie_error(
    operation: &'static str,
    path: &Path,
    source: impl std::error::Error + Send + Sync + 'static,
) -> crate::Error {
    cookie_box_error(operation, path, Box::new(source))
}

fn cookie_box_error(operation: &'static str, path: &Path, source: BoxError) -> crate::Error {
    HttpError::CookieJar {
        operation,
        path: path.display().to_string(),
        source,
    }
    .into()
}

#[derive(Clone, Debug)]
pub(super) struct CookieService<S> {
    inner: S,
    jar: CookieJar,
}

impl<S> CookieService<S> {
    pub(super) const fn new(inner: S, jar: CookieJar) -> Self {
        Self { inner, jar }
    }
}

impl<S> Service<Request<RequestBody>> for CookieService<S>
where
    S: Service<Request<RequestBody>, Response = Response<ResponseBody>, Error = BoxError>
        + Clone
        + Send
        + 'static,
    S::Future: Send + 'static,
{
    type Response = Response<ResponseBody>;
    type Error = BoxError;
    type Future =
        Pin<Box<dyn Future<Output = std::result::Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(
        &mut self,
        context: &mut Context<'_>,
    ) -> Poll<std::result::Result<(), Self::Error>> {
        self.inner.poll_ready(context)
    }

    fn call(&mut self, mut request: Request<RequestBody>) -> Self::Future {
        let replacement = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, replacement);
        let jar = self.jar.clone();
        let url = cookie_url(request.uri());
        jar.apply_to_request(&mut request);

        Box::pin(async move {
            let response = inner.call(request).await?;
            if let Some(url) = url {
                jar.store_response(&url, &response);
            }
            Ok(response)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http_body_util::{BodyExt as _, Full};
    use hyper::body::Bytes;

    fn store_cookie(jar: &CookieJar, url: &Url, value: &str) {
        let cookie = cookie_store::RawCookie::parse(value.to_owned())
            .unwrap()
            .into_owned();
        jar.inner
            .write()
            .unwrap()
            .store_response_cookies(std::iter::once(cookie), url);
    }

    fn cookie_response(values: &[&str]) -> Response<ResponseBody> {
        let mut response = Response::builder();
        for value in values {
            response = response.header(SET_COOKIE, *value);
        }
        response
            .body(
                Full::new(Bytes::new())
                    .map_err(|never| -> BoxError { match never {} })
                    .boxed_unsync(),
            )
            .unwrap()
    }

    fn live_cookie_count(jar: &CookieJar) -> usize {
        jar.read_store().iter_unexpired().count()
    }

    fn poison_cookie_jar(jar: &CookieJar) {
        let inner = Arc::clone(&jar.inner);
        let result = std::thread::spawn(move || {
            let _guard = inner.write().unwrap();
            panic!("poison cookie jar for test");
        })
        .join();
        assert!(result.is_err());
        assert!(jar.inner.is_poisoned());
    }

    fn assert_rejects_cross_tenant_cookie(jar: &CookieJar) {
        let attacker = Url::parse("https://attacker.github.io/").unwrap();
        let cookie = cookie_store::RawCookie::parse(
            "session=attacker; Domain=.github.io; Path=/".to_owned(),
        )
        .unwrap()
        .into_owned();
        jar.inner
            .write()
            .unwrap()
            .store_response_cookies(std::iter::once(cookie), &attacker);

        let victim = Url::parse("https://victim.github.io/").unwrap();
        assert!(jar.request_header(&victim).is_none());
    }

    #[test]
    fn default_jar_rejects_public_suffix_domain_cookies() {
        assert_rejects_cross_tenant_cookie(&CookieJar::default());
    }

    #[test]
    fn loaded_jar_rejects_public_suffix_domain_cookies() {
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), b"[]").unwrap();
        let jar = CookieJar::load_from(file.path(), CookieLimits::default()).unwrap();

        assert_rejects_cross_tenant_cookie(&jar);
    }

    #[test]
    fn request_header_recovers_from_a_poisoned_jar() {
        let jar = CookieJar::default();
        let url = Url::parse("https://example.com/").unwrap();
        store_cookie(&jar, &url, "session=secret; Path=/");
        poison_cookie_jar(&jar);

        let header = jar.request_header(&url).unwrap();

        assert_eq!(header, "session=secret");
        assert!(!jar.inner.is_poisoned());
    }

    #[test]
    fn response_cookies_recover_from_a_poisoned_jar() {
        let jar = CookieJar::default();
        let url = Url::parse("https://example.com/").unwrap();
        poison_cookie_jar(&jar);
        let body = Full::new(Bytes::new())
            .map_err(|never| -> BoxError { match never {} })
            .boxed_unsync();
        let response = Response::builder()
            .header(SET_COOKIE, "session=secret; Path=/")
            .body(body)
            .unwrap();

        jar.store_response(&url, &response);

        assert_eq!(jar.request_header(&url).unwrap(), "session=secret");
        assert!(!jar.inner.is_poisoned());
    }

    #[test]
    fn oversized_response_cookie_is_rejected() {
        let jar = CookieJar::with_limits(CookieLimits::default().max_cookie_bytes(5));
        let url = Url::parse("https://example.com/").unwrap();
        let response = cookie_response(&["token=secret; Path=/"]);

        jar.store_response(&url, &response);

        assert!(jar.request_header(&url).is_none());
    }

    #[test]
    fn nearest_expiry_is_evicted_at_the_per_domain_limit() {
        let jar = CookieJar::with_limits(CookieLimits::default().max_cookies_per_domain(2));
        let url = Url::parse("https://example.com/").unwrap();
        let response = cookie_response(&[
            "soon=1; Max-Age=60; Path=/",
            "later=2; Max-Age=120; Path=/",
            "session=3; Path=/",
        ]);

        jar.store_response(&url, &response);

        let header = jar
            .request_header(&url)
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        assert_eq!(live_cookie_count(&jar), 2);
        assert!(!header.contains("soon=1"), "{header}");
        assert!(header.contains("later=2"), "{header}");
        assert!(header.contains("session=3"), "{header}");
    }

    #[test]
    fn nearest_expiry_is_evicted_at_the_total_limit() {
        let jar = CookieJar::with_limits(
            CookieLimits::default()
                .max_cookies_per_domain(3)
                .max_cookies_total(2),
        );
        let soon_url = Url::parse("https://soon.example/").unwrap();
        let later_url = Url::parse("https://later.example/").unwrap();
        let session_url = Url::parse("https://session.example/").unwrap();

        jar.store_response(&soon_url, &cookie_response(&["soon=1; Max-Age=60; Path=/"]));
        jar.store_response(
            &later_url,
            &cookie_response(&["later=2; Max-Age=120; Path=/"]),
        );
        jar.store_response(&session_url, &cookie_response(&["session=3; Path=/"]));

        assert_eq!(live_cookie_count(&jar), 2);
        assert!(jar.request_header(&soon_url).is_none());
        assert!(jar.request_header(&later_url).is_some());
        assert!(jar.request_header(&session_url).is_some());
    }

    #[test]
    fn loaded_cookie_jar_is_pruned_to_configured_limits() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let source = CookieJar::default();
        let url = Url::parse("https://example.com/").unwrap();
        store_cookie(&source, &url, "first=1; Path=/");
        store_cookie(&source, &url, "second=2; Path=/");
        store_cookie(&source, &url, "third=3; Path=/");
        source.save_to(file.path()).unwrap();

        let loaded = CookieJar::load_from(
            file.path(),
            CookieLimits::default().max_cookies_per_domain(2),
        )
        .unwrap();

        assert_eq!(live_cookie_count(&loaded), 2);
    }

    #[test]
    fn invalid_cookie_value_does_not_drop_valid_cookies() {
        let header = cookie_header_value([("valid", "kept"), ("invalid", "line\nbreak")]).unwrap();

        assert_eq!(header, "valid=kept");
    }

    #[test]
    fn relative_uri_is_not_usable_for_cookies() {
        let uri = "/relative".parse().unwrap();

        assert!(cookie_url(&uri).is_none());
    }
}
