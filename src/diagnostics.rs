//! Doctor command framework and debug bundles.
//!
//! Provides a check registration and execution framework for "doctor"
//! commands, plus a debug bundle builder that collects diagnostic
//! information into a tar.zst archive.
//!
//! # Example
//!
//! ```
//! use librebar::diagnostics::{DoctorCheck, DoctorRunner, CheckResult, CheckStatus};
//!
//! struct ConfigCheck;
//!
//! impl DoctorCheck for ConfigCheck {
//!     fn name(&self) -> &str { "config" }
//!     fn category(&self) -> &str { "configuration" }
//!     fn run(&self) -> CheckResult {
//!         CheckResult::new(CheckStatus::Ok, "Config valid")
//!     }
//! }
//!
//! let mut runner = DoctorRunner::new();
//! runner.add(ConfigCheck);
//! let results = runner.run_all();
//! let report = DoctorRunner::format_report(&results);
//! assert!(report.contains("config"));
//! ```

use std::fmt;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

// ─── Doctor Framework ──────────────────────────────────────────────

/// Trait for doctor checks. Implement for each diagnostic check.
pub trait DoctorCheck {
    /// Short name for the check (e.g., "config", "permissions").
    fn name(&self) -> &str;

    /// Category for grouping in output (e.g., "configuration", "network").
    fn category(&self) -> &str;

    /// Run the check and return a result.
    fn run(&self) -> CheckResult;
}

/// Result of a single doctor check.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct CheckResult {
    /// Status of the check.
    pub status: CheckStatus,
    /// Human-readable message describing the result.
    pub message: String,
}

impl CheckResult {
    /// Create a doctor check result.
    pub fn new(status: CheckStatus, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }
}

/// Status of a doctor check.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckStatus {
    /// Check passed.
    Ok,
    /// Check passed with a warning.
    Warn,
    /// Check failed.
    Error,
}

impl CheckStatus {
    /// Returns true if the status is `Ok`.
    pub const fn is_ok(self) -> bool {
        matches!(self, Self::Ok)
    }
}

/// Named check result (name + category + result).
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct NamedResult {
    /// Check name.
    pub name: String,
    /// Check category.
    pub category: String,
    /// Check result.
    pub result: CheckResult,
}

/// Summary of doctor check results.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct DoctorSummary {
    /// Number of checks that passed.
    pub passed: usize,
    /// Number of checks that warned.
    pub warned: usize,
    /// Number of checks that failed.
    pub failed: usize,
}

/// Collects and runs doctor checks.
pub struct DoctorRunner {
    checks: Vec<Box<dyn DoctorCheck>>,
}

impl DoctorRunner {
    /// Create a new empty runner.
    pub fn new() -> Self {
        Self { checks: Vec::new() }
    }

    /// Register a check.
    pub fn add(&mut self, check: impl DoctorCheck + 'static) {
        self.checks.push(Box::new(check));
    }

    /// Number of registered checks.
    pub fn check_count(&self) -> usize {
        self.checks.len()
    }

    /// Run all checks and return named results.
    pub fn run_all(&self) -> Vec<NamedResult> {
        self.checks
            .iter()
            .map(|check| {
                let name = check.name().to_string();
                let category = check.category().to_string();
                tracing::debug!(check = %name, category = %category, "running doctor check");
                let result = check.run();
                tracing::debug!(check = %name, status = ?result.status, "check complete");
                NamedResult {
                    name,
                    category,
                    result,
                }
            })
            .collect()
    }

    /// Summarize a set of check results.
    pub fn summarize(results: &[NamedResult]) -> DoctorSummary {
        let mut summary = DoctorSummary::default();
        for r in results {
            match r.result.status {
                CheckStatus::Ok => summary.passed += 1,
                CheckStatus::Warn => summary.warned += 1,
                CheckStatus::Error => summary.failed += 1,
            }
        }
        summary
    }

    /// Format results as a human-readable report.
    pub fn format_report(results: &[NamedResult]) -> String {
        let mut buf = String::new();
        let mut current_category = "";

        for r in results {
            if r.category != current_category {
                if !buf.is_empty() {
                    buf.push('\n');
                }
                buf.push_str(&r.category);
                buf.push_str(":\n");
                current_category = &r.category;
            }

            let icon = match r.result.status {
                CheckStatus::Ok => "OK",
                CheckStatus::Warn => "WARN",
                CheckStatus::Error => "FAIL",
            };
            buf.push_str(&format!("  [{icon}] {}: {}\n", r.name, r.result.message));
        }

        let summary = Self::summarize(results);
        buf.push_str(&format!(
            "\n{} passed, {} warnings, {} failed\n",
            summary.passed, summary.warned, summary.failed
        ));

        buf
    }
}

impl Default for DoctorRunner {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Debug Bundle ──────────────────────────────────────────────────

const REDACTED: &str = "[REDACTED]";

/// Redacts one prospective debug-bundle entry before it is retained.
///
/// Implement this trait when an application has schema-specific secrets or
/// needs to inspect binary formats. [`DebugBundle`] uses [`SecretRedactor`]
/// unless a replacement is installed with [`DebugBundle::with_redactor`].
pub trait Redactor: Send + Sync {
    /// Return the content that may be written to the named archive entry.
    fn redact(&self, name: &str, data: &[u8]) -> Vec<u8>;
}

/// Default redactor for common structured-text secret fields.
///
/// JSON, JSON Lines, TOML, and YAML are parsed before matching keys. Other
/// UTF-8 content is treated as assignment-style text, including dotenv files.
/// Values whose keys identify passwords, secrets, tokens, authorization,
/// credentials, connection strings, database URLs, or private keys become
/// `[REDACTED]`. A second pass detects recognizable credential values such as
/// provider tokens, JWTs, inline URL credentials, and private keys without
/// broadly removing diagnostic identifiers such as email and IP addresses.
///
/// Non-UTF-8 data is returned unchanged; applications that bundle opaque
/// binary formats should install a format-aware [`Redactor`].
pub struct SecretRedactor {
    values: leakguard::Redactor,
}

impl fmt::Debug for SecretRedactor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretRedactor")
    }
}

impl Default for SecretRedactor {
    fn default() -> Self {
        use leakguard::{Kind, Mask};

        let values = leakguard::Redactor::only(&[
            Kind::Jwt,
            Kind::AwsAccessKey,
            Kind::UrlCredentials,
            Kind::GitHubToken,
            Kind::SlackToken,
            Kind::StripeKey,
            Kind::GoogleApiKey,
            Kind::OpenAiKey,
            Kind::PrivateKey,
            Kind::AzureConnectionString,
            Kind::TelegramToken,
            Kind::DiscordToken,
        ])
        .mask(Mask::fixed(REDACTED));

        Self { values }
    }
}

impl Redactor for SecretRedactor {
    fn redact(&self, name: &str, data: &[u8]) -> Vec<u8> {
        let Ok(text) = std::str::from_utf8(data) else {
            return data.to_vec();
        };

        let redacted =
            redact_structured_text(name, text).unwrap_or_else(|| redact_assignment_text(text));
        self.values.clean(&redacted).into_bytes()
    }
}

fn redact_structured_text(name: &str, text: &str) -> Option<String> {
    let extension = Path::new(name)
        .extension()
        .and_then(|value| value.to_str())?
        .to_ascii_lowercase();

    match extension.as_str() {
        "json" => {
            let mut value: serde_json::Value = serde_json::from_str(text).ok()?;
            if !redact_value(&mut value) {
                return Some(text.to_string());
            }
            serde_json::to_string_pretty(&value).ok()
        }
        "jsonl" => Some(redact_json_lines(text)),
        "toml" => {
            let mut value: serde_json::Value = toml::from_str(text).ok()?;
            if !redact_value(&mut value) {
                return Some(text.to_string());
            }
            toml::to_string_pretty(&value).ok()
        }
        "yaml" | "yml" => {
            let mut value: serde_json::Value = serde_saphyr::from_str(text).ok()?;
            if !redact_value(&mut value) {
                return Some(text.to_string());
            }
            // JSON is a YAML 1.2 subset. Emitting it avoids enabling
            // serde-saphyr's serializer and its unsafe-by-default base64 path.
            serde_json::to_string_pretty(&value).ok()
        }
        _ => None,
    }
}

fn redact_json_lines(text: &str) -> String {
    let mut redacted = String::with_capacity(text.len());

    for chunk in text.split_inclusive('\n') {
        let (line, newline) = chunk
            .strip_suffix('\n')
            .map_or((chunk, ""), |line| (line, "\n"));
        let output = serde_json::from_str::<serde_json::Value>(line)
            .ok()
            .and_then(|mut value| {
                redact_value(&mut value)
                    .then(|| serde_json::to_string(&value).ok())
                    .flatten()
            })
            .unwrap_or_else(|| redact_assignment_line(line));
        redacted.push_str(&output);
        redacted.push_str(newline);
    }

    redacted
}

fn redact_value(value: &mut serde_json::Value) -> bool {
    match value {
        serde_json::Value::Object(values) => {
            let mut changed = false;
            for (key, value) in values {
                if is_sensitive_key(key) {
                    *value = serde_json::Value::String(REDACTED.to_string());
                    changed = true;
                } else {
                    changed |= redact_value(value);
                }
            }
            changed
        }
        serde_json::Value::Array(values) => {
            let mut changed = false;
            for value in values {
                changed |= redact_value(value);
            }
            changed
        }
        _ => false,
    }
}

fn redact_assignment_text(text: &str) -> String {
    let mut redacted = String::with_capacity(text.len());

    for chunk in text.split_inclusive('\n') {
        let (line, newline) = chunk
            .strip_suffix('\n')
            .map_or((chunk, ""), |line| (line, "\n"));
        redacted.push_str(&redact_assignment_line(line));
        redacted.push_str(newline);
    }

    redacted
}

fn redact_assignment_line(line: &str) -> String {
    let Some((separator_index, separator)) = line
        .char_indices()
        .find(|(_, character)| matches!(character, ':' | '='))
    else {
        return line.to_string();
    };

    let key = line[..separator_index]
        .trim()
        .trim_start_matches('-')
        .trim()
        .trim_matches(['\'', '"']);
    if !is_sensitive_key(key) {
        return line.to_string();
    }

    let has_trailing_comma = line[separator_index + separator.len_utf8()..]
        .trim_end()
        .ends_with(',');
    format!(
        "{}{} \"{REDACTED}\"{}",
        &line[..separator_index],
        separator,
        if has_trailing_comma { "," } else { "" }
    )
}

fn is_sensitive_key(key: &str) -> bool {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut previous_was_lowercase = false;

    for character in key.chars() {
        if !character.is_ascii_alphanumeric() {
            if !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
            previous_was_lowercase = false;
            continue;
        }

        if character.is_ascii_uppercase() && previous_was_lowercase && !current.is_empty() {
            words.push(std::mem::take(&mut current));
        }
        current.push(character.to_ascii_lowercase());
        previous_was_lowercase = character.is_ascii_lowercase();
    }
    if !current.is_empty() {
        words.push(current);
    }

    if words.iter().any(|word| {
        matches!(
            word.as_str(),
            "authorization"
                | "credential"
                | "credentials"
                | "passwd"
                | "password"
                | "secret"
                | "token"
        )
    }) {
        return true;
    }

    if words.last().is_some_and(|word| word == "key") {
        return true;
    }

    let compact = words.concat();
    matches!(compact.as_str(), "connectionstring" | "databaseurl")
}

fn create_private_file(path: &Path) -> std::io::Result<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }

    let file = options.open(path)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }

    Ok(file)
}

/// Builder for diagnostic debug bundles (tar.zst archives).
pub struct DebugBundle {
    app_name: String,
    dir: PathBuf,
    entries: Vec<DebugBundleEntry>,
    redactor: Box<dyn Redactor>,
}

#[derive(Debug)]
enum DebugBundleEntry {
    Buffered { name: String, data: Vec<u8> },
    SanitizedFile { name: String, path: PathBuf },
}

impl fmt::Debug for DebugBundle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DebugBundle")
            .field("app_name", &self.app_name)
            .field("dir", &self.dir)
            .field("entries", &self.entries)
            .field("redactor", &"<redactor>")
            .finish()
    }
}

impl DebugBundle {
    /// Create a new debug bundle builder.
    ///
    /// The archive will be written to `dir`.
    pub fn new(app_name: &str, dir: &Path) -> Self {
        Self {
            app_name: app_name.to_string(),
            dir: dir.to_path_buf(),
            entries: Vec::new(),
            redactor: Box::new(SecretRedactor::default()),
        }
    }

    /// Replace the default redactor.
    ///
    /// Install the redactor before adding entries. Existing entries have
    /// already been processed by the redactor that was active when added.
    pub fn with_redactor(mut self, redactor: impl Redactor + 'static) -> Self {
        self.redactor = Box::new(redactor);
        self
    }

    /// Add a text file to the bundle after redaction.
    #[must_use]
    pub fn add_text(self, name: &str, content: &str) -> Self {
        self.add_bytes(name, content.as_bytes())
    }

    /// Add binary content to the bundle after redaction.
    ///
    /// Passing an owned [`Vec<u8>`] moves the caller's buffer into this method
    /// instead of cloning it at the API boundary.
    #[must_use]
    pub fn add_bytes(mut self, name: &str, data: impl Into<Vec<u8>>) -> Self {
        let data = data.into();
        let data = self.redactor.redact(name, &data);
        self.entries.push(DebugBundleEntry::Buffered {
            name: name.to_string(),
            data,
        });
        self
    }

    /// Add a file that the caller has already sanitized.
    ///
    /// The file path is retained and its content is streamed into the archive
    /// by [`Self::finish`], so the source must remain available and sanitized
    /// until then. This method deliberately does not run the configured
    /// [`Redactor`]; use [`Self::add_text`] or [`Self::add_bytes`] for content
    /// that has not already crossed a trusted redaction boundary.
    #[must_use]
    pub fn add_sanitized_file(mut self, name: &str, path: &Path) -> Self {
        self.entries.push(DebugBundleEntry::SanitizedFile {
            name: name.to_string(),
            path: path.to_path_buf(),
        });
        self
    }

    /// Add doctor results to the bundle.
    #[must_use]
    pub fn add_doctor_results(self, results: &[NamedResult]) -> Self {
        let report = DoctorRunner::format_report(results);
        self.add_text("doctor-report.txt", &report)
    }

    /// Write the tar.zst archive and return its path.
    pub fn finish(self) -> Result<PathBuf> {
        use std::io::Write as _;

        std::fs::create_dir_all(&self.dir).map_err(Error::Diagnostic)?;

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let filename = format!("{}-debug-{timestamp}.tar.zst", self.app_name);
        let path = self.dir.join(&filename);

        let mut archive = tar::Builder::new(Vec::new());

        for entry in &self.entries {
            match entry {
                DebugBundleEntry::Buffered { name, data } => {
                    let mut header = tar::Header::new_gnu();
                    header.set_size(data.len() as u64);
                    header.set_mode(0o600);
                    header.set_cksum();
                    archive
                        .append_data(&mut header, name, data.as_slice())
                        .map_err(Error::Diagnostic)?;
                }
                DebugBundleEntry::SanitizedFile { name, path } => {
                    let file = std::fs::File::open(path).map_err(Error::Diagnostic)?;
                    let mut header = tar::Header::new_gnu();
                    header.set_size(file.metadata().map_err(Error::Diagnostic)?.len());
                    header.set_mode(0o600);
                    header.set_cksum();
                    archive
                        .append_data(&mut header, name, file)
                        .map_err(Error::Diagnostic)?;
                }
            }
        }

        let tar_bytes = archive.into_inner().map_err(Error::Diagnostic)?;
        let compressed = rust_zstd::compress(&tar_bytes, 3);

        let mut file = create_private_file(&path).map_err(Error::Diagnostic)?;
        file.write_all(&compressed).map_err(Error::Diagnostic)?;

        tracing::info!(path = %path.display(), "debug bundle created");
        Ok(path)
    }
}
