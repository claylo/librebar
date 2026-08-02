#![allow(missing_docs)]
#![cfg(feature = "diagnostics")]

use librebar::diagnostics::{
    CheckResult, CheckStatus, DebugBundle, DoctorCheck, DoctorRunner, Redactor,
};
use std::cell::Cell;
use std::io::Read;
use std::path::Path;
use std::rc::Rc;
use tempfile::TempDir;

fn read_archive_entry(archive_path: &Path, name: &str) -> (Vec<u8>, u32) {
    let compressed = std::fs::read(archive_path).unwrap();
    let tar_bytes = rust_zstd::decompress(&compressed).unwrap();
    let cursor = std::io::Cursor::new(tar_bytes);
    let mut archive = tar::Archive::new(cursor);

    for entry in archive.entries().unwrap() {
        let mut entry = entry.unwrap();
        if entry.path().unwrap().as_ref() != Path::new(name) {
            continue;
        }

        let mode = entry.header().mode().unwrap();
        let mut content = Vec::new();
        entry.read_to_end(&mut content).unwrap();
        return (content, mode);
    }

    panic!("archive entry not found: {name}");
}

struct AlwaysPassCheck;

impl DoctorCheck for AlwaysPassCheck {
    fn name(&self) -> &str {
        "always-pass"
    }

    fn category(&self) -> &str {
        "test"
    }

    fn run(&self) -> CheckResult {
        CheckResult::new(CheckStatus::Ok, "Everything is fine")
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
        CheckResult::new(CheckStatus::Error, "Something is wrong")
    }
}

struct LocalCheck {
    runs: Rc<Cell<usize>>,
}

impl DoctorCheck for LocalCheck {
    fn name(&self) -> &str {
        "local"
    }

    fn category(&self) -> &str {
        "test"
    }

    fn run(&self) -> CheckResult {
        self.runs.set(self.runs.get() + 1);
        CheckResult::new(CheckStatus::Ok, "Local check ran")
    }
}

struct FixedRedactor;

impl Redactor for FixedRedactor {
    fn redact(&self, _name: &str, _data: &[u8]) -> Vec<u8> {
        b"custom-redaction".to_vec()
    }
}

#[test]
fn runner_registers_checks() {
    let mut runner = DoctorRunner::new();
    runner.add(AlwaysPassCheck);
    runner.add(AlwaysFailCheck);
    assert_eq!(runner.check_count(), 2);
}

#[test]
fn runner_accepts_an_unboxed_non_send_check() {
    let runs = Rc::new(Cell::new(0));
    let mut runner = DoctorRunner::new();
    runner.add(LocalCheck {
        runs: Rc::clone(&runs),
    });

    let results = runner.run_all();

    assert_eq!(results.len(), 1);
    assert_eq!(runs.get(), 1);
}

#[test]
fn runner_executes_all_checks() {
    let mut runner = DoctorRunner::new();
    runner.add(AlwaysPassCheck);
    runner.add(AlwaysFailCheck);
    let results = runner.run_all();
    assert_eq!(results.len(), 2);
}

#[test]
fn runner_reports_pass_and_fail() {
    let mut runner = DoctorRunner::new();
    runner.add(AlwaysPassCheck);
    runner.add(AlwaysFailCheck);
    let results = runner.run_all();
    let summary = DoctorRunner::summarize(&results);
    assert_eq!(summary.passed, 1);
    assert_eq!(summary.failed, 1);
}

#[test]
fn debug_bundle_can_be_finished_from_a_chain() {
    let tmp = TempDir::new().unwrap();
    let archive_path = DebugBundle::new("test-app", tmp.path())
        .add_text("info.txt", "test content")
        .finish()
        .unwrap();

    assert!(archive_path.exists());
    assert!(archive_path.to_string_lossy().ends_with(".tar.zst"));
}

#[test]
fn debug_bundle_streams_a_pre_sanitized_file_at_finish() {
    let tmp = TempDir::new().unwrap();
    let source = tmp.path().join("application.log");
    std::fs::write(&source, "before finish").unwrap();

    let bundle = DebugBundle::new("test-app", tmp.path())
        .add_sanitized_file("logs/application.log", &source);

    std::fs::write(&source, "content read at finish").unwrap();
    let archive_path = bundle.finish().unwrap();
    let (content, mode) = read_archive_entry(&archive_path, "logs/application.log");

    assert_eq!(content, b"content read at finish");
    assert_eq!(mode & 0o777, 0o600);
}

#[test]
fn debug_bundle_redacts_sensitive_values_from_text_formats() {
    let tmp = TempDir::new().unwrap();
    let mut bundle = DebugBundle::new("test-app", tmp.path());
    let cases = [
        (
            "config.json",
            r#"{"items":[{"api_key":"json-secret"},{"token":"json-secret"}],"safe":"json-visible"}"#,
            "json-secret",
            "json-visible",
        ),
        (
            "config.toml",
            "password = \"toml-secret\"\nsafe = \"toml-visible\"\n",
            "toml-secret",
            "toml-visible",
        ),
        (
            "config.yaml",
            "nested:\n  client_secret: yaml-secret\nsafe: yaml-visible\n",
            "yaml-secret",
            "yaml-visible",
        ),
        (
            "service.env",
            "AUTHORIZATION=Bearer dotenv-secret\nSAFE=dotenv-visible\n",
            "dotenv-secret",
            "dotenv-visible",
        ),
        (
            "app.jsonl",
            "{\"message\":\"request\",\"authorization\":\"Bearer log-secret\"}\n{\"safe\":\"log-visible\"}\n",
            "log-secret",
            "log-visible",
        ),
        (
            "notes.txt",
            "endpoint=https://alice:hunter2@example.com\nsafe=value-visible\n",
            "alice:hunter2",
            "value-visible",
        ),
    ];

    for (name, content, _, _) in cases {
        bundle = bundle.add_text(name, content);
    }

    let archive_path = bundle.finish().unwrap();
    for (name, _, secret, visible) in cases {
        let (content, _) = read_archive_entry(&archive_path, name);
        let content = String::from_utf8(content).unwrap();
        assert!(
            !content.contains(secret),
            "{name} leaked {secret}: {content}"
        );
        assert!(content.contains("[REDACTED]"), "{name}: {content}");
        assert!(content.contains(visible), "{name}: {content}");
        if name.ends_with(".yaml") {
            let parsed: serde_json::Value = serde_saphyr::from_str(&content).unwrap();
            assert_eq!(parsed["safe"], "yaml-visible");
        }
    }
}

#[test]
fn debug_bundle_accepts_a_custom_redactor() {
    let tmp = TempDir::new().unwrap();
    let bundle = DebugBundle::new("test-app", tmp.path())
        .with_redactor(FixedRedactor)
        .add_bytes("opaque.bin", b"private bytes".to_vec());

    let archive_path = bundle.finish().unwrap();
    let (content, _) = read_archive_entry(&archive_path, "opaque.bin");
    assert_eq!(content, b"custom-redaction");
}

#[cfg(unix)]
#[test]
fn debug_bundle_archive_is_owner_only() {
    use std::os::unix::fs::PermissionsExt as _;

    let tmp = TempDir::new().unwrap();
    let bundle = DebugBundle::new("test-app", tmp.path()).add_text("info.txt", "safe content");

    let archive_path = bundle.finish().unwrap();
    let mode = std::fs::metadata(archive_path)
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
}

#[test]
fn debug_bundle_entries_are_owner_only() {
    let tmp = TempDir::new().unwrap();
    let bundle = DebugBundle::new("test-app", tmp.path()).add_text("info.txt", "safe content");

    let archive_path = bundle.finish().unwrap();
    let (_, mode) = read_archive_entry(&archive_path, "info.txt");
    assert_eq!(mode & 0o777, 0o600);
}

#[test]
fn check_status_is_ok() {
    assert!(CheckStatus::Ok.is_ok());
    assert!(!CheckStatus::Error.is_ok());
    assert!(!CheckStatus::Warn.is_ok());
}
