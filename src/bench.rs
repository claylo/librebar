//! Benchmark harness helpers.
//!
//! Supports two harnesses via separate feature flags:
//!
//! - **`bench`** — [divan] for wall-clock benchmarks (any platform)
//! - **`bench-gungraun`** — [gungraun] for instruction-count benchmarks
//!   via Valgrind/Callgrind (Linux/Intel)
//!
//! # divan (wall-clock)
//!
//! ```ignore
//! fn main() {
//!     divan::main();
//! }
//!
//! #[divan::bench]
//! fn my_benchmark() { /* ... */ }
//! ```
//!
//! # gungraun (instruction-count)
//!
//! ```ignore
//! use librebar::bench::gungraun;
//!
//! #[gungraun::library_benchmark]
//! #[bench::args(1000)]
//! fn sort_vec(n: usize) {
//!     let mut v: Vec<u32> = (0..n as u32).rev().collect();
//!     v.sort();
//! }
//!
//! gungraun::library_benchmark_group!(name = sorting; benchmarks = sort_vec);
//! gungraun::main!(library_benchmark_groups = sorting);
//! ```

#[cfg(feature = "bench")]
pub use divan;

#[cfg(feature = "bench-gungraun")]
pub use gungraun;

/// Configuration for wall-clock benchmark runs (divan).
#[cfg(feature = "bench")]
#[derive(Clone, Debug)]
pub struct BenchConfig {
    /// Minimum number of iterations per benchmark.
    pub min_iterations: u32,
    /// Maximum time per benchmark in seconds.
    pub max_time_secs: u64,
}

#[cfg(feature = "bench")]
impl Default for BenchConfig {
    fn default() -> Self {
        Self {
            min_iterations: 100,
            max_time_secs: 5,
        }
    }
}
