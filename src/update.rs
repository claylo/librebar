//! Update notifications from pluggable release sources.
//!
//! [`UpdateChecker`] compares the current version with a [`ReleaseSource`].
//! [`GitHubReleaseSource`] provides GitHub Releases support, including optional
//! bearer authentication. Results are cached for 24 hours by default. Set
//! `{APP}_NO_UPDATE_CHECK=1` to suppress checks entirely.
//!
//! # Example
//!
//! ```no_run
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let checker = librebar::update::UpdateChecker::github("myapp", "0.1.0", "owner/repo")?;
//! if let Some(update) = checker.check().await? {
//!     eprintln!("{}", update.message());
//! }
//! # Ok(())
//! # }
//! ```

use std::borrow::Cow;
use std::time::Duration;

use crate::error::{BoxError, boxed_error};
use crate::http::{Bytes, HeaderValue, HttpClient, Method, Request, StatusCode, Uri, header};

/// Re-export of [`mod@async_trait`], used to implement [`ReleaseSource`].
pub use async_trait::async_trait;

const CACHE_TTL: Duration = Duration::from_secs(86400); // 24 hours
const CACHE_KEY: &str = "latest-release";

/// Information returned by a release source.
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[non_exhaustive]
pub struct ReleaseInfo {
    /// Latest available version.
    pub version: String,
    /// URL where the release can be viewed or installed.
    pub url: String,
}

impl ReleaseInfo {
    /// Create release information.
    pub fn new(version: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            version: version.into(),
            url: url.into(),
        }
    }

    /// Create release information with validated version and URL.
    pub fn try_new(
        version: impl Into<String>,
        url: impl Into<String>,
    ) -> std::result::Result<Self, ReleaseInfoError> {
        let version = ReleaseVersion::new(version)?;
        let url = ReleaseUrl::new(url)?;
        Ok(Self {
            version: version.0,
            url: url.0,
        })
    }
}

/// Errors from constructing a [`ReleaseInfo`] with validation.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ReleaseInfoError {
    /// Version string is empty or does not start with a digit or `v`.
    #[error("invalid release version: {0:?}")]
    InvalidVersion(String),
    /// URL is not a parseable HTTPS URL.
    #[error("invalid release URL: {0:?}")]
    InvalidUrl(String),
}

/// A validated release version string.
///
/// Must be non-empty and start with a digit or `v`.
#[derive(Clone, Debug)]
pub struct ReleaseVersion(String);

impl ReleaseVersion {
    /// Create a validated release version.
    pub fn new(version: impl Into<String>) -> std::result::Result<Self, ReleaseInfoError> {
        let version = version.into();
        if version.is_empty() || !version.starts_with(|c: char| c.is_ascii_digit() || c == 'v') {
            return Err(ReleaseInfoError::InvalidVersion(version));
        }
        Ok(Self(version))
    }

    /// Return the version string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A validated release URL.
///
/// Must be a parseable HTTPS URL.
#[derive(Clone, Debug)]
pub struct ReleaseUrl(String);

impl ReleaseUrl {
    /// Create a validated release URL.
    pub fn new(url: impl Into<String>) -> std::result::Result<Self, ReleaseInfoError> {
        let url = url.into();
        if !release_url_is_valid(&url) {
            return Err(ReleaseInfoError::InvalidUrl(url));
        }
        Ok(Self(url))
    }

    /// Return the URL string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn release_url_is_valid(url: &str) -> bool {
    url.parse::<Uri>()
        .is_ok_and(|uri| uri.scheme_str() == Some("https") && uri.authority().is_some())
}

fn release_info_is_valid(release: &ReleaseInfo) -> bool {
    semver::Version::parse(&release.version).is_ok() && release_url_is_valid(&release.url)
}

fn escape_terminal_controls(value: &str) -> Cow<'_, str> {
    if !value.chars().any(char::is_control) {
        return Cow::Borrowed(value);
    }

    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        if character.is_control() {
            escaped.extend(character.escape_default());
        } else {
            escaped.push(character);
        }
    }
    Cow::Owned(escaped)
}

/// Backend that discovers the latest available release.
#[async_trait]
pub trait ReleaseSource: Send + Sync {
    /// Fetch the latest release.
    async fn latest_release(&self) -> std::result::Result<ReleaseInfo, BoxError>;
}

/// GitHub Releases backend.
pub struct GitHubReleaseSource {
    repo: String,
    client: HttpClient,
    api_base: String,
    bearer_token: Option<String>,
}

impl GitHubReleaseSource {
    /// Create a GitHub backend for an `owner/repo` repository.
    pub fn new(repo: impl Into<String>, client: HttpClient) -> Self {
        Self {
            repo: repo.into(),
            client,
            api_base: "https://api.github.com".to_string(),
            bearer_token: None,
        }
    }

    /// Override the GitHub API base URL.
    #[must_use]
    pub fn with_api_base(mut self, api_base: impl Into<String>) -> Self {
        self.api_base = api_base.into().trim_end_matches('/').to_string();
        self
    }

    /// Authenticate GitHub requests with a bearer token.
    #[must_use]
    pub fn with_bearer_token(mut self, token: impl Into<String>) -> Self {
        self.bearer_token = Some(token.into());
        self
    }
}

impl std::fmt::Debug for GitHubReleaseSource {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("GitHubReleaseSource")
            .field("repo", &self.repo)
            .field("api_base", &self.api_base)
            .field(
                "bearer_token",
                &self.bearer_token.as_ref().map(|_| "[REDACTED]"),
            )
            .finish_non_exhaustive()
    }
}

#[derive(Debug, thiserror::Error)]
enum GitHubReleaseError {
    #[error("could not build the GitHub release request")]
    BuildRequest(#[source] BoxError),
    #[error("GitHub release request failed")]
    Request(#[source] crate::Error),
    #[error("GitHub release API returned {0}")]
    Status(StatusCode),
    #[error("GitHub release API returned invalid JSON")]
    Decode(#[source] serde_json::Error),
    #[error("GitHub release API returned invalid release metadata")]
    InvalidMetadata,
}

#[derive(serde::Deserialize)]
struct GitHubRelease {
    tag_name: String,
    html_url: String,
}

#[async_trait]
impl ReleaseSource for GitHubReleaseSource {
    async fn latest_release(&self) -> std::result::Result<ReleaseInfo, BoxError> {
        let url = format!("{}/repos/{}/releases/latest", self.api_base, self.repo);
        let mut request = Request::builder()
            .method(Method::GET)
            .uri(url)
            .body(Bytes::new())
            .map_err(|error| GitHubReleaseError::BuildRequest(boxed_error(error)))?;
        if let Some(token) = &self.bearer_token {
            let mut value = HeaderValue::from_str(&format!("Bearer {token}"))
                .map_err(|error| GitHubReleaseError::BuildRequest(boxed_error(error)))?;
            value.set_sensitive(true);
            request.headers_mut().insert(header::AUTHORIZATION, value);
        }

        let response = self
            .client
            .send(request)
            .await
            .map_err(GitHubReleaseError::Request)?;
        if !response.is_success() {
            return Err(Box::new(GitHubReleaseError::Status(response.status())));
        }
        let release: GitHubRelease =
            serde_json::from_slice(response.bytes()).map_err(GitHubReleaseError::Decode)?;
        let version = release
            .tag_name
            .strip_prefix('v')
            .unwrap_or(&release.tag_name);
        let release = ReleaseInfo::new(version, release.html_url);
        if !release_info_is_valid(&release) {
            return Err(Box::new(GitHubReleaseError::InvalidMetadata));
        }
        Ok(release)
    }
}

/// Failure while checking for an update.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum UpdateError {
    /// The default GitHub HTTP client could not be constructed.
    #[error("could not build the update HTTP client")]
    Client(#[source] BoxError),
    /// The configured release source could not determine the latest release.
    #[error("release source failed")]
    Source(#[source] BoxError),
}

/// Information about an available update.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct UpdateInfo {
    /// Currently running version.
    pub current: String,
    /// Latest available version.
    pub latest: String,
    /// URL to the release page.
    pub url: String,
}

impl UpdateInfo {
    /// Create update information.
    pub fn new(
        current: impl Into<String>,
        latest: impl Into<String>,
        url: impl Into<String>,
    ) -> Self {
        Self {
            current: current.into(),
            latest: latest.into(),
            url: url.into(),
        }
    }

    /// Format a user-friendly update notification.
    pub fn message(&self) -> String {
        format!(
            "Update available: {} -> {} ({})",
            escape_terminal_controls(&self.current),
            escape_terminal_controls(&self.latest),
            escape_terminal_controls(&self.url)
        )
    }
}

/// Checks a release source for new versions.
pub struct UpdateChecker {
    app_name: String,
    current_version: String,
    env_suppress: String,
    source: Box<dyn ReleaseSource>,
    cache: Option<crate::cache::Cache>,
}

impl std::fmt::Debug for UpdateChecker {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("UpdateChecker")
            .field("app_name", &self.app_name)
            .field("current_version", &self.current_version)
            .field("env_suppress", &self.env_suppress)
            .field("source", &"<release source>")
            .field("cache_enabled", &self.cache.is_some())
            .finish()
    }
}

impl UpdateChecker {
    /// Create an update checker using an explicit release source.
    pub fn new(
        app_name: &str,
        current_version: &str,
        source: impl ReleaseSource + 'static,
    ) -> Self {
        let prefix = app_name.to_uppercase().replace('-', "_");
        Self {
            app_name: app_name.to_string(),
            current_version: current_version.to_string(),
            env_suppress: format!("{prefix}_NO_UPDATE_CHECK"),
            source: Box::new(source),
            cache: crate::cache::Cache::default_for(app_name),
        }
    }

    /// Create an update checker using the GitHub Releases API.
    ///
    /// `repo` is the GitHub `owner/repo` string.
    pub fn github(
        app_name: &str,
        current_version: &str,
        repo: &str,
    ) -> std::result::Result<Self, UpdateError> {
        let client = HttpClient::from_app(app_name, current_version)
            .map_err(|error| UpdateError::Client(boxed_error(error)))?;
        Ok(Self::new(
            app_name,
            current_version,
            GitHubReleaseSource::new(repo, client),
        ))
    }

    /// Disable update-result caching.
    #[must_use]
    pub fn without_cache(mut self) -> Self {
        self.cache = None;
        self
    }

    /// Use an explicit cache for update results.
    #[must_use]
    pub fn with_cache(mut self, cache: crate::cache::Cache) -> Self {
        self.cache = Some(cache);
        self
    }

    /// Check if update checking is suppressed by environment variable.
    pub fn is_suppressed(&self) -> bool {
        std::env::var(&self.env_suppress)
            .ok()
            .is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true"))
    }

    /// Application name.
    pub fn app_name(&self) -> &str {
        &self.app_name
    }

    /// Current version string.
    pub fn current_version(&self) -> &str {
        &self.current_version
    }

    /// Check for updates.
    ///
    /// Returns `Ok(Some(UpdateInfo))` when a newer version is available.
    /// Suppressed checks and up-to-date releases return `Ok(None)`. Release
    /// source failures return [`UpdateError`]. Cache failures are logged at
    /// debug level and fall back to the configured source.
    #[tracing::instrument(skip(self), fields(app = %self.app_name, current = %self.current_version))]
    pub async fn check(&self) -> std::result::Result<Option<UpdateInfo>, UpdateError> {
        if self.is_suppressed() {
            tracing::debug!("update check suppressed by env");
            return Ok(None);
        }

        if let Some(release) = self.cached_release().await {
            tracing::debug!(latest_version = %release.version, "using cached update check");
            return Ok(self.compare_versions_with_url(&release.version, &release.url));
        }

        let release = self
            .source
            .latest_release()
            .await
            .map_err(UpdateError::Source)?;
        if !release_info_is_valid(&release) {
            tracing::debug!("ignored invalid release metadata");
            return Ok(None);
        }
        self.cache_release(&release).await;
        Ok(self.compare_versions_with_url(&release.version, &release.url))
    }

    async fn cached_release(&self) -> Option<ReleaseInfo> {
        let cache = self.cache.clone()?;
        match crate::cache::run_io(move || cache.get(CACHE_KEY)).await {
            Ok(Some(bytes)) => match serde_json::from_slice(&bytes) {
                Ok(release) if release_info_is_valid(&release) => Some(release),
                Ok(_) => {
                    tracing::debug!("ignored invalid update cache metadata");
                    None
                }
                Err(error) => {
                    tracing::debug!(error = %error, "ignored invalid update cache entry");
                    None
                }
            },
            Ok(None) => None,
            Err(error) => {
                tracing::debug!(error = %error, "update cache read failed");
                None
            }
        }
    }

    async fn cache_release(&self, release: &ReleaseInfo) {
        let Some(cache) = self.cache.clone() else {
            return;
        };
        let Ok(bytes) = serde_json::to_vec(release) else {
            tracing::debug!("could not encode update cache entry");
            return;
        };
        if let Err(error) =
            crate::cache::run_io(move || cache.set(CACHE_KEY, &bytes, CACHE_TTL)).await
        {
            tracing::debug!(error = %error, "update cache write failed");
        }
    }

    fn compare_versions_with_url(&self, latest: &str, url: &str) -> Option<UpdateInfo> {
        if release_url_is_valid(url) && is_newer(&self.current_version, latest) {
            Some(UpdateInfo::new(&self.current_version, latest, url))
        } else {
            None
        }
    }
}

/// Compare two semantic version strings.
///
/// Returns `true` if both strings are valid semantic versions and `latest` is
/// newer than `current`. Malformed versions are never treated as updates.
pub fn is_newer(current: &str, latest: &str) -> bool {
    let Ok(current) = semver::Version::parse(current) else {
        return false;
    };
    let Ok(latest) = semver::Version::parse(latest) else {
        return false;
    };
    latest > current
}
