//! Manifest-level regression tests for the published Cargo feature contract.

use std::process::Command;

fn package_metadata() -> serde_json::Value {
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
    metadata["packages"]
        .as_array()
        .and_then(|packages| {
            packages
                .iter()
                .find(|package| package["name"] == env!("CARGO_PKG_NAME"))
        })
        .cloned()
        .expect("librebar package should be present")
}

#[test]
fn manifest_enables_the_application_foundation_by_default() {
    let package = package_metadata();
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

#[test]
fn manifest_separates_cache_and_cookie_persistence_dependencies() {
    let package = package_metadata();
    let features = package["features"]
        .as_object()
        .expect("feature map should be present");
    let feature_dependencies = |feature: &str| {
        features[feature]
            .as_array()
            .expect("feature dependencies should be present")
            .iter()
            .map(|dependency| dependency.as_str().expect("dependency should be a string"))
            .collect::<Vec<_>>()
    };

    let cache = feature_dependencies("cache");
    assert!(cache.contains(&"dep:tempfile"));
    assert!(!cache.contains(&"dep:atomic-write-file"));

    let cookies = feature_dependencies("http-cookies");
    assert!(cookies.contains(&"dep:atomic-write-file"));
    assert!(!cookies.contains(&"dep:tempfile"));
}
