//! Update notifications via GitHub releases API.
//!
//! Checks for new versions by querying the GitHub releases API. Results
//! are cached for 24 hours (when the `cache` feature is also enabled)
//! to avoid repeated network hits. Respects `{APP}_NO_UPDATE_CHECK=1`
//! to suppress checks entirely.
//!
//! # Example
//!
//! ```no_run
//! # async fn example() {
//! let checker = librebar::update::UpdateChecker::new("myapp", "0.1.0", "owner/repo");
//! if let Some(update) = checker.check().await {
//!     eprintln!("{}", update.message());
//! }
//! # }
//! ```

use std::time::Duration;

const CACHE_TTL: Duration = Duration::from_secs(86400); // 24 hours
const CACHE_KEY: &str = "latest-version";

/// Information about an available update.
#[derive(Clone, Debug)]
pub struct UpdateInfo {
    /// Currently running version.
    pub current: String,
    /// Latest available version.
    pub latest: String,
    /// URL to the release page.
    pub url: String,
}

impl UpdateInfo {
    /// Format a user-friendly update notification.
    pub fn message(&self) -> String {
        format!(
            "Update available: {} -> {} ({})",
            self.current, self.latest, self.url
        )
    }
}

/// Checks GitHub releases for new versions.
#[derive(Debug)]
pub struct UpdateChecker {
    app_name: String,
    current_version: String,
    repo: String,
    env_suppress: String,
}

impl UpdateChecker {
    /// Create a new update checker.
    ///
    /// `repo` is the GitHub `owner/repo` string.
    pub fn new(app_name: &str, current_version: &str, repo: &str) -> Self {
        let prefix = app_name.to_uppercase().replace('-', "_");
        Self {
            app_name: app_name.to_string(),
            current_version: current_version.to_string(),
            repo: repo.to_string(),
            env_suppress: format!("{prefix}_NO_UPDATE_CHECK"),
        }
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

    /// Check for updates. Returns `Some(UpdateInfo)` if a newer version
    /// is available, `None` otherwise.
    ///
    /// This is non-blocking and best-effort. Network errors, GitHub rate
    /// limits, and parse failures are logged at debug level and return `None`.
    #[tracing::instrument(skip(self), fields(app = %self.app_name, current = %self.current_version))]
    pub async fn check(&self) -> Option<UpdateInfo> {
        if self.is_suppressed() {
            tracing::debug!("update check suppressed by env");
            return None;
        }

        // Check cache first
        if let Some(cache) = crate::cache::Cache::default_for(&self.app_name)
            && let Ok(Some(cached)) = cache.get(CACHE_KEY)
            && let Ok(version) = String::from_utf8(cached)
        {
            tracing::debug!(cached_version = %version, "using cached version check");
            return self.compare_versions(&version);
        }

        // Fetch from GitHub
        let url = format!("https://api.github.com/repos/{}/releases/latest", self.repo);
        let client =
            crate::http::HttpClient::from_app(&self.app_name, &self.current_version).ok()?;

        let resp = match client.get(&url).await {
            Ok(r) => r,
            Err(e) => {
                tracing::debug!(error = %e, "update check failed");
                return None;
            }
        };

        if !resp.is_success() {
            tracing::debug!(status = resp.status, "GitHub API returned non-200");
            return None;
        }

        let json: serde_json::Value = match resp.json() {
            Ok(v) => v,
            Err(e) => {
                tracing::debug!(error = %e, "failed to parse release response");
                return None;
            }
        };
        let tag = json.get("tag_name")?.as_str()?;
        let latest = tag.strip_prefix('v').unwrap_or(tag);
        let html_url = json.get("html_url")?.as_str().unwrap_or("");

        // Cache the result (best-effort: stale cache just means another API call)
        if let Some(cache) = crate::cache::Cache::default_for(&self.app_name) {
            let _ = cache.set(CACHE_KEY, latest.as_bytes(), CACHE_TTL);
        }

        self.compare_versions_with_url(latest, html_url)
    }

    fn compare_versions(&self, latest: &str) -> Option<UpdateInfo> {
        let url = format!("https://github.com/{}/releases/tag/v{}", self.repo, latest);
        self.compare_versions_with_url(latest, &url)
    }

    fn compare_versions_with_url(&self, latest: &str, url: &str) -> Option<UpdateInfo> {
        if is_newer(&self.current_version, latest) {
            Some(UpdateInfo {
                current: self.current_version.clone(),
                latest: latest.to_string(),
                url: url.to_string(),
            })
        } else {
            None
        }
    }
}

/// Compare two semver-ish version strings.
///
/// Returns `true` if `latest` is newer than `current`.
/// Handles `major.minor.patch` format. Non-numeric segments
/// are treated as 0.
pub fn is_newer(current: &str, latest: &str) -> bool {
    let parse = |v: &str| -> Vec<u64> { v.split('.').map(|s| s.parse().unwrap_or(0)).collect() };

    let curr = parse(current);
    let lat = parse(latest);

    for i in 0..curr.len().max(lat.len()) {
        let c = curr.get(i).copied().unwrap_or(0);
        let l = lat.get(i).copied().unwrap_or(0);
        if l > c {
            return true;
        }
        if l < c {
            return false;
        }
    }
    false
}
