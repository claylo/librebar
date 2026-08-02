//! Wall-clock benchmarks for filesystem cache reads and writes.

use std::time::Duration;

use librebar::bench::{BenchConfig, divan};
use librebar::cache::Cache;
use tempfile::TempDir;

const CACHE_TTL: Duration = Duration::from_secs(60);

fn main() {
    let config = BenchConfig::default();
    divan::Divan::from_args()
        .sample_count(config.min_iterations)
        .max_time(Duration::from_secs(config.max_time_secs))
        .main();
}

#[divan::bench(args = [1_024, 65_536, 1_048_576])]
fn set(bencher: divan::Bencher, bytes: usize) {
    let temp = TempDir::new().expect("create benchmark cache directory");
    let cache = Cache::new(temp.path());
    let value = vec![b'x'; bytes];

    bencher
        .counter(divan::counter::BytesCount::from(bytes))
        .bench_local(|| {
            cache
                .set("benchmark", divan::black_box(&value), CACHE_TTL)
                .expect("write benchmark cache entry");
        });
}

#[divan::bench(args = [1_024, 65_536, 1_048_576])]
fn get(bencher: divan::Bencher, bytes: usize) {
    let temp = TempDir::new().expect("create benchmark cache directory");
    let cache = Cache::new(temp.path());
    let value = vec![b'x'; bytes];
    cache
        .set("benchmark", &value, CACHE_TTL)
        .expect("seed benchmark cache entry");

    bencher
        .counter(divan::counter::BytesCount::from(bytes))
        .bench_local(|| {
            cache
                .get(divan::black_box("benchmark"))
                .expect("read benchmark cache entry")
                .expect("benchmark cache entry exists")
        });
}
