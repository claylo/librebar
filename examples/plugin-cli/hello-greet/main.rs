//! Standalone plugin binary resolved as `plugin-cli-hello-greet` on PATH.
//!
//! Deliberately minimal and deliberately librebar-free: a plugin is just a
//! binary whose name follows the `{app}-{subcommand}` convention. Any tool
//! that can read argv and write stdout qualifies. If you wanted shared
//! logging, shared config discovery, or shared exit conventions, you'd add
//! them — but nothing in librebar's dispatch requires it.
//!
//! When invoked via `plugin-cli hello-greet --name Clay`, librebar's
//! dispatch forwards the trailing args unchanged, so this binary only
//! sees `--name Clay`.
#![allow(missing_docs)]

use clap::Parser;

#[derive(Parser)]
#[command(
    name = "plugin-cli-hello-greet",
    about = "Plugin subcommand: greet by name"
)]
struct Args {
    /// Person to greet.
    #[arg(long, default_value = "world")]
    name: String,
}

fn main() {
    let args = Args::parse();
    println!("hello, {}!", args.name);
}
