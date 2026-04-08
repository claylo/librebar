//! Rebar: Rust application foundation crate.
//!
//! Feature-gated modules for CLI, config, logging, and more.
//! Use the builder to orchestrate initialization:
//!
//! ```ignore
//! let app = rebar::init(env!("CARGO_PKG_NAME"))
//!     .with_cli(cli.common)
//!     .config::<Config>()
//!     .logging()
//!     .start()?;
//! ```
#![deny(unsafe_code)]

pub mod error;

#[cfg(feature = "cli")]
pub mod cli;

#[cfg(feature = "config")]
pub mod config;

#[cfg(feature = "logging")]
pub mod logging;

pub use error::{Error, Result};
