//! RFC-aware persistent HTTP cache example.
//!
//! ```sh
//! cargo run --example http-cache --features http-cache
//! cargo run --example http-cache --features http-cache -- https://example.com/data my-key
//! ```
#![allow(missing_docs)]

use anyhow::{Context, Result};
use librebar::cache::Cache;
use librebar::http::HttpClient;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let url = args
        .next()
        .unwrap_or_else(|| "https://httpbin.org/cache/60".to_owned());
    let key = args.next().unwrap_or_else(|| "example-response".to_owned());

    let cache = Cache::default_for("librebar-http-cache-example")
        .context("platform cache directory is unavailable")?;
    let client = HttpClient::from_app("librebar-http-cache-example", env!("CARGO_PKG_VERSION"))?;
    let response = client.get_cached(&cache, &key, &url).await?;

    println!(
        "{:?} {} {} bytes",
        response.cache_status(),
        response.status(),
        response.bytes().len()
    );
    if let Some(validator) = response.validator() {
        if let Some(etag) = validator.etag() {
            println!("etag: {etag:?}");
        }
        if let Some(last_modified) = validator.last_modified() {
            println!("last-modified: {last_modified:?}");
        }
    }
    println!("{}", response.text_ref()?);
    Ok(())
}
