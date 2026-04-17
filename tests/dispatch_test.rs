#![allow(missing_docs)]
#![cfg(feature = "dispatch")]

use librebar::dispatch;

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
