//! Manifest-level regression tests for the published Cargo feature contract.

use std::process::Command;

#[test]
fn manifest_enables_the_application_foundation_by_default() {
    let output = Command::new(env!("CARGO"))
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .output()
        .expect("cargo metadata should run");
    assert!(
        output.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let metadata: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("cargo metadata should emit JSON");
    let package = metadata["packages"]
        .as_array()
        .and_then(|packages| {
            packages
                .iter()
                .find(|package| package["name"] == env!("CARGO_PKG_NAME"))
        })
        .expect("librebar package should be present");
    let defaults = package["features"]["default"]
        .as_array()
        .expect("default feature list should be present")
        .iter()
        .map(|feature| feature.as_str().expect("feature names should be strings"))
        .collect::<Vec<_>>();

    assert_eq!(
        defaults,
        ["cli", "config", "logging", "crash", "cache", "diagnostics"]
    );
}
