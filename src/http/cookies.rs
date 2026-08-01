use std::fs::File;
use std::future::Future;
use std::io::{BufReader, BufWriter, Write};
use std::path::Path;
use std::pin::Pin;
use std::sync::{Arc, RwLock};
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
#[derive(Clone, Debug, Default)]
pub struct CookieJar {
    inner: Arc<RwLock<cookie_store::CookieStore>>,
}

impl CookieJar {
    pub(super) fn load_from(path: &Path) -> Result<Self> {
        let file = File::open(path).map_err(|source| cookie_error("load", path, source))?;
        let store = cookie_store::serde::json::load(BufReader::new(file))
            .map_err(|source| cookie_box_error("load", path, source))?;
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
            let store = self.inner.read().map_err(|_| {
                cookie_error(
                    "save",
                    path,
                    std::io::Error::other("cookie jar lock is poisoned"),
                )
            })?;
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
        file.sync_all()
            .map_err(|source| cookie_error("save", path, source))?;
        file.commit()
            .map_err(|source| cookie_error("save", path, source))?;
        Ok(())
    }

    fn request_header(&self, url: &Url) -> Option<hyper::header::HeaderValue> {
        let value = {
            let store = self.inner.read().ok()?;
            store
                .get_request_values(url)
                .map(|(name, value)| format!("{name}={value}"))
                .collect::<Vec<_>>()
                .join("; ")
        };
        if value.is_empty() {
            None
        } else {
            hyper::header::HeaderValue::from_str(&value).ok()
        }
    }

    pub(super) fn apply_to_request<B>(&self, request: &mut Request<B>) {
        if request.headers().contains_key(COOKIE) {
            return;
        }
        if let Ok(url) = Url::parse(&request.uri().to_string())
            && let Some(value) = self.request_header(&url)
        {
            request.headers_mut().insert(COOKIE, value);
        }
    }

    fn store_response(&self, url: &Url, response: &Response<ResponseBody>) {
        let cookies = response
            .headers()
            .get_all(SET_COOKIE)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .filter_map(|value| cookie_store::RawCookie::parse(value.to_owned()).ok())
            .map(cookie_store::RawCookie::into_owned);
        if let Ok(mut store) = self.inner.write() {
            store.store_response_cookies(cookies, url);
        }
    }
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
        let url = Url::parse(&request.uri().to_string());
        jar.apply_to_request(&mut request);

        Box::pin(async move {
            let response = inner.call(request).await?;
            if let Ok(url) = url {
                jar.store_response(&url, &response);
            }
            Ok(response)
        })
    }
}
