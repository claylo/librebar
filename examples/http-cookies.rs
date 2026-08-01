//! Stateful HTTP example — explicit cookie capture and persistence.
//!
//! This uses httpbin's redirecting cookie endpoint to demonstrate that a
//! `Set-Cookie` response is applied to the redirected request, then saves the
//! jar—including session cookies—for the next invocation.
//!
//! ```sh
//! cargo run --example http-cookies --features http-cookies -- /tmp/librebar-cookies.json
//! ```
#![allow(missing_docs)]

use std::path::PathBuf;

use anyhow::{Context, Result};
use librebar::http::HttpClient;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let jar_path = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("librebar-cookies.json"));

    let builder = HttpClient::builder("librebar-http-cookies-example", env!("CARGO_PKG_VERSION"));
    let client = if jar_path.exists() {
        builder.with_cookie_jar_from(&jar_path).build()?
    } else {
        builder.with_cookie_jar().build()?
    };

    let response = client
        .get("https://httpbin.org/cookies/set/session/librebar")
        .await
        .context("cookie request failed")?;
    println!("{}", response.text_ref()?);

    client
        .cookie_jar()
        .context("cookie jar was not enabled")?
        .save_to(&jar_path)?;
    println!("saved persistent cookies to {}", jar_path.display());
    Ok(())
}
