#![allow(missing_docs)]
#![cfg(feature = "mcp")]

#[test]
fn transport_stdio_type_is_accessible() {
    // Verify the function exists and compiles
    let _: fn() -> _ = rebar::mcp::transport_stdio;
}

#[test]
fn service_ext_trait_is_accessible() {
    #[allow(unused_imports)]
    use rebar::mcp::ServiceExt as _;
}
