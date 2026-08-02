#![allow(missing_docs)]
#![cfg(feature = "dispatch")]

use librebar::dispatch;

#[cfg(unix)]
const DISPATCH_PROBE: &str = "LIBREBAR_DISPATCH_PROBE";
#[cfg(unix)]
const EXPECTED_BINARY: &str = "LIBREBAR_EXPECTED_DISPATCH_BINARY";

#[test]
fn find_subcommand_binary_name() {
    let name = dispatch::subcommand_binary("myapp", "serve");
    assert_eq!(name, "myapp-serve");
}

#[test]
fn resolve_returns_none_for_missing_command() {
    let result = dispatch::resolve("librebar-test-nonexistent-42", "fakecmd");
    assert!(result.is_none());
}

#[cfg(unix)]
#[test]
fn resolve_ignores_empty_and_relative_path_entries() {
    use std::fs;
    use std::os::unix::fs::PermissionsExt as _;
    use std::process::Command;

    let tmp = tempfile::TempDir::new().unwrap();
    let cwd = tmp.path().join("cwd");
    let relative_bin = cwd.join("relative-bin");
    let trusted_bin = tmp.path().join("trusted-bin");
    fs::create_dir_all(&relative_bin).unwrap();
    fs::create_dir(&trusted_bin).unwrap();

    let write_executable = |directory: &std::path::Path| {
        let path = directory.join("myapp-deploy");
        fs::write(&path, "#!/bin/sh\nexit 0\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path
    };
    write_executable(&cwd);
    write_executable(&relative_bin);
    let expected = write_executable(&trusted_bin);

    for path in [
        format!(":{}", trusted_bin.display()),
        format!("relative-bin:{}", trusted_bin.display()),
    ] {
        let output = Command::new(std::env::current_exe().unwrap())
            .arg("--exact")
            .arg("resolve_probe_prefers_absolute_path_entries")
            .arg("--nocapture")
            .current_dir(&cwd)
            .env("PATH", path)
            .env(DISPATCH_PROBE, "1")
            .env(EXPECTED_BINARY, &expected)
            .output()
            .unwrap();

        assert!(
            output.status.success(),
            "dispatch probe failed:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[cfg(unix)]
#[test]
fn resolve_probe_prefers_absolute_path_entries() {
    if std::env::var_os(DISPATCH_PROBE).is_none() {
        return;
    }

    let expected = std::path::PathBuf::from(std::env::var_os(EXPECTED_BINARY).unwrap());
    assert_eq!(dispatch::resolve("myapp", "deploy"), Some(expected));
}
