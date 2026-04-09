//! Doctor command framework and debug bundles.
//!
//! Provides a check registration and execution framework for "doctor"
//! commands, plus a debug bundle builder that collects diagnostic
//! information into a tar.gz archive.
//!
//! # Example
//!
//! ```ignore
//! use rebar::diagnostics::{DoctorCheck, DoctorRunner, CheckResult, CheckStatus};
//!
//! struct ConfigCheck;
//!
//! impl DoctorCheck for ConfigCheck {
//!     fn name(&self) -> &str { "config" }
//!     fn category(&self) -> &str { "configuration" }
//!     fn run(&self) -> CheckResult {
//!         CheckResult { status: CheckStatus::Ok, message: "Config valid".into() }
//!     }
//! }
//!
//! let mut runner = DoctorRunner::new();
//! runner.add(Box::new(ConfigCheck));
//! let results = runner.run_all();
//! println!("{}", DoctorRunner::format_report(&results));
//! ```

use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

// ─── Doctor Framework ──────────────────────────────────────────────

/// Trait for doctor checks. Implement for each diagnostic check.
pub trait DoctorCheck: Send {
    /// Short name for the check (e.g., "config", "permissions").
    fn name(&self) -> &str;

    /// Category for grouping in output (e.g., "configuration", "network").
    fn category(&self) -> &str;

    /// Run the check and return a result.
    fn run(&self) -> CheckResult;
}

/// Result of a single doctor check.
#[derive(Clone, Debug)]
pub struct CheckResult {
    /// Status of the check.
    pub status: CheckStatus,
    /// Human-readable message describing the result.
    pub message: String,
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
    pub fn add(&mut self, check: Box<dyn DoctorCheck>) {
        self.checks.push(check);
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

/// Builder for diagnostic debug bundles (tar.gz archives).
#[derive(Debug)]
pub struct DebugBundle {
    app_name: String,
    dir: PathBuf,
    files: Vec<(String, Vec<u8>)>,
}

impl DebugBundle {
    /// Create a new debug bundle builder.
    ///
    /// The archive will be written to `dir`.
    pub fn new(app_name: &str, dir: &Path) -> Self {
        Self {
            app_name: app_name.to_string(),
            dir: dir.to_path_buf(),
            files: Vec::new(),
        }
    }

    /// Add a text file to the bundle.
    pub fn add_text(&mut self, name: &str, content: &str) -> &mut Self {
        self.files
            .push((name.to_string(), content.as_bytes().to_vec()));
        self
    }

    /// Add a binary file to the bundle.
    pub fn add_bytes(&mut self, name: &str, data: &[u8]) -> &mut Self {
        self.files.push((name.to_string(), data.to_vec()));
        self
    }

    /// Add doctor results to the bundle.
    pub fn add_doctor_results(&mut self, results: &[NamedResult]) -> &mut Self {
        let report = DoctorRunner::format_report(results);
        self.add_text("doctor-report.txt", &report)
    }

    /// Write the tar.gz archive and return its path.
    pub fn finish(self) -> Result<PathBuf> {
        std::fs::create_dir_all(&self.dir).map_err(Error::Diagnostic)?;

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let filename = format!("{}-debug-{timestamp}.tar.gz", self.app_name);
        let path = self.dir.join(&filename);

        let file = std::fs::File::create(&path).map_err(Error::Diagnostic)?;
        let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        let mut archive = tar::Builder::new(encoder);

        for (name, data) in &self.files {
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            archive
                .append_data(&mut header, name, data.as_slice())
                .map_err(Error::Diagnostic)?;
        }

        archive
            .into_inner()
            .map_err(Error::Diagnostic)?
            .finish()
            .map_err(Error::Diagnostic)?;

        tracing::info!(path = %path.display(), "debug bundle created");
        Ok(path)
    }
}
