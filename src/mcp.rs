//! MCP server helpers wrapping rmcp.
//!
//! Re-exports key rmcp types and provides a convenience function for
//! the common pattern of serving an MCP server on stdio.
//!
//! # Usage
//!
//! ```no_run
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // `transport_stdio()` pairs tokio stdin/stdout for rmcp's JSON-RPC loop.
//! // Pass it to your rmcp `ServerHandler::serve(...)` call, then await the
//! // returned service's `.waiting()` future to block until the client
//! // disconnects. See the `mcp-server` example for a full implementation.
//! let transport = librebar::mcp::transport_stdio();
//! # let _ = transport;
//! # Ok(())
//! # }
//! ```

// Re-export key types consumers need
pub use rmcp::ServiceExt;
pub use rmcp::handler;
pub use rmcp::model;

/// Create a stdio transport for MCP communication.
///
/// Returns a `(Stdin, Stdout)` pair suitable for passing to
/// [`ServiceExt::serve`].
///
/// This is the standard transport for CLI-based MCP servers.
pub fn transport_stdio() -> (tokio::io::Stdin, tokio::io::Stdout) {
    rmcp::transport::io::stdio()
}
