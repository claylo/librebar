use std::fs::File;
use std::future::Future;
use std::io::{BufReader, BufWriter, Write};
use std::path::Path;
use std::pin::Pin;
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::task::{Context, Poll};

use atomic_write_file::AtomicWriteFile;
use hyper::header::{COOKIE, SET_COOKIE};
use hyper::{Request, Response};
use tower::{BoxError, Service};
use url::Url;

use super::{RequestBody, ResponseBody};
use crate::Result;
use crate::error::HttpError;

/// A shareable RFC 6265 cookie jar.
#[derive(Clone, Debug)]
pub struct CookieJar {
    inner: Arc<RwLock<cookie_store::CookieStore>>,
}

impl Default for CookieJar {
    fn default() -> Self {
        Self {
            inner: Arc::new(RwLock::new(
                cookie_store::CookieStore::new_with_public_suffix(Some(public_suffix_list())),
            )),
        }
    }
}

impl CookieJar {
    pub(super) fn load_from(path: &Path) -> Result<Self> {
        let file = File::open(path).map_err(|source| cookie_error("load", path, source))?;
        let store = cookie_store::serde::json::load(BufReader::new(file))
            .map_err(|source| cookie_box_error("load", path, source))?
            .with_suffix_list(public_suffix_list());
        Ok(Self {
            inner: Arc::new(RwLock::new(store)),
        })
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

    fn request_header(&self, url: &Url) -> Option<hyper::header::HeaderValue> {
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
            .map(cookie_store::RawCookie::into_owned);
        self.write_store().store_response_cookies(cookies, url);
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

fn cookie_url(uri: &hyper::Uri) -> Option<Url> {
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
) -> Option<hyper::header::HeaderValue> {
    let value = values
        .into_iter()
        .filter_map(|(name, value)| {
            let pair = format!("{name}={value}");
            match hyper::header::HeaderValue::from_str(&pair) {
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
    match hyper::header::HeaderValue::from_str(&value) {
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
        let jar = CookieJar::load_from(file.path()).unwrap();

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
