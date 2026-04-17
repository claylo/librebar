#![allow(missing_docs)]
#![cfg(feature = "bench")]

#[test]
fn bench_module_compiles() {
    // Verify the module is accessible
    let _ = std::any::type_name::<librebar::bench::BenchConfig>();
}
