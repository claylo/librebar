//! MCP server helpers wrapping rmcp.
//!
//! Re-exports key rmcp types and provides a convenience function for
//! the common pattern of serving an MCP server on stdio.
//!
//! # Usage
//!
//! ```no_run
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // `transport_stdio()` hides the concrete Tokio I/O types used by rmcp.
//! // Pass it to your rmcp `ServerHandler::serve(...)` call, then await the
//! // returned service's `.waiting()` future to block until the client
//! // disconnects. See the `mcp-server` example for a full implementation.
//! let transport = librebar::mcp::transport_stdio();
//! # let _ = transport;
//! # Ok(())
//! # }
//! ```

/// Re-export of [`rmcp`], Librebar's MCP extension API.
pub use rmcp;

// Keep the common imports available at their existing short paths.
pub use rmcp::ServiceExt;
pub use rmcp::handler;
pub use rmcp::model;

/// Create a stdio transport for MCP communication.
///
/// Returns an opaque server transport suitable for passing to
/// [`ServiceExt::serve`]. Tokio's concrete stdio types remain private.
///
/// This is the standard transport for CLI-based MCP servers.
pub fn transport_stdio() -> impl rmcp::transport::IntoTransport<
    rmcp::RoleServer,
    std::io::Error,
    rmcp::transport::async_rw::TransportAdapterAsyncRW,
> {
    rmcp::transport::io::stdio()
}
