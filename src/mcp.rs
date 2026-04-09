//! MCP server helpers wrapping rmcp.
//!
//! Re-exports key rmcp types and provides a convenience function for
//! the common pattern of serving an MCP server on stdio.
//!
//! # Usage
//!
//! ```ignore
//! use rebar::mcp::ServiceExt;
//!
//! let server = MyServer::new();
//! let service = server.serve(rebar::mcp::transport_stdio()).await?;
//! service.waiting().await?;
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
