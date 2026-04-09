#![allow(missing_docs)]
#![cfg(feature = "bench-gungraun")]

#[test]
fn gungraun_module_compiles() {
    // Verify the gungraun re-export is accessible
    let _ = std::any::type_name::<rebar::bench::gungraun::LibraryBenchmarkConfig>();
}
