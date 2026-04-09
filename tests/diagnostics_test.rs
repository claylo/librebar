#![allow(missing_docs)]
#![cfg(feature = "diagnostics")]

use rebar::diagnostics::{CheckResult, CheckStatus, DebugBundle, DoctorCheck, DoctorRunner};
use tempfile::TempDir;

struct AlwaysPassCheck;

impl DoctorCheck for AlwaysPassCheck {
    fn name(&self) -> &str {
        "always-pass"
    }

    fn category(&self) -> &str {
        "test"
    }

    fn run(&self) -> CheckResult {
        CheckResult {
            status: CheckStatus::Ok,
            message: "Everything is fine".to_string(),
        }
    }
}

struct AlwaysFailCheck;

impl DoctorCheck for AlwaysFailCheck {
    fn name(&self) -> &str {
        "always-fail"
    }

    fn category(&self) -> &str {
        "test"
    }

    fn run(&self) -> CheckResult {
        CheckResult {
            status: CheckStatus::Error,
            message: "Something is wrong".to_string(),
        }
    }
}

#[test]
fn runner_registers_checks() {
    let mut runner = DoctorRunner::new();
    runner.add(Box::new(AlwaysPassCheck));
    runner.add(Box::new(AlwaysFailCheck));
    assert_eq!(runner.check_count(), 2);
}

#[test]
fn runner_executes_all_checks() {
    let mut runner = DoctorRunner::new();
    runner.add(Box::new(AlwaysPassCheck));
    runner.add(Box::new(AlwaysFailCheck));
    let results = runner.run_all();
    assert_eq!(results.len(), 2);
}

#[test]
fn runner_reports_pass_and_fail() {
    let mut runner = DoctorRunner::new();
    runner.add(Box::new(AlwaysPassCheck));
    runner.add(Box::new(AlwaysFailCheck));
    let results = runner.run_all();
    let summary = DoctorRunner::summarize(&results);
    assert_eq!(summary.passed, 1);
    assert_eq!(summary.failed, 1);
}

#[test]
fn debug_bundle_creates_archive() {
    let tmp = TempDir::new().unwrap();
    let mut bundle = DebugBundle::new("test-app", tmp.path());

    bundle.add_text("info.txt", "test content").unwrap();
    let archive_path = bundle.finish().unwrap();
    assert!(archive_path.exists());
    assert!(archive_path.to_string_lossy().ends_with(".tar.gz"));
}

#[test]
fn check_status_is_ok() {
    assert!(CheckStatus::Ok.is_ok());
    assert!(!CheckStatus::Error.is_ok());
    assert!(!CheckStatus::Warn.is_ok());
}
