//! MCP server example — stdio JSON-RPC with a single `greet` tool.
//!
//! Exercises librebar's `mcp` feature end-to-end. Implements `ServerHandler`
//! manually (without rmcp's `#[tool]` / `#[tool_router]` macros, which
//! librebar doesn't pull in) so readers see the actual trait surface —
//! `get_info`, `list_tools`, `call_tool` — that the macros ordinarily
//! generate.
//!
//! # Run
//!
//! ```sh
//! cargo run --example mcp-server \
//!     --features "cli,config,logging,mcp" \
//!     -- -C examples run
//! ```
//!
//! The server reads JSON-RPC on stdin and writes on stdout, so don't try
//! to pipe the subcommand through a shell — use a real MCP client.
//!
//! # Inspect interactively
//!
//! The `mcp-inspector` utility (npm: `@modelcontextprotocol/inspector`)
//! speaks stdio MCP and provides a local web UI for browsing capabilities
//! and calling tools:
//!
//! ```sh
//! npx @modelcontextprotocol/inspector \
//!     cargo run --example mcp-server \
//!     --features "cli,config,logging,mcp" \
//!     -- run
//! ```
//!
//! Point Claude Desktop at the built binary the same way by adding an
//! entry to its `mcpServers` config with `"command"` pointing at
//! `target/debug/examples/mcp-server`.
//!
//! # Why the logging stays out of stdout
//!
//! librebar's `logging` layer writes JSONL to a file under the platform
//! log dir — nothing is emitted to stdout or stderr. That matters here:
//! the stdio transport owns the JSON-RPC framing on stdout, and any log
//! noise would desync the protocol. `-v` / `-vv` flags still work and
//! just raise the file-layer filter.
#![allow(missing_docs)]

use anyhow::Result;
use clap::{Parser, Subcommand};
use librebar::mcp::ServiceExt;
use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler,
    model::{
        CallToolRequestParams, CallToolResult, Content, ListToolsResult, PaginatedRequestParams,
        ServerCapabilities, ServerInfo, Tool,
    },
    service::RequestContext,
};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::sync::Arc;

#[derive(Debug, Deserialize, Serialize)]
#[serde(default)]
struct Config {
    /// Log level used as the baseline when no `-q`/`-v` flag is passed.
    log_level: librebar::config::LogLevel,
    /// Prefix the `greet` tool uses when formatting its response.
    greeting: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            log_level: librebar::config::LogLevel::Info,
            greeting: "hello".to_string(),
        }
    }
}

#[derive(Parser)]
#[command(
    name = "mcp-server",
    about = "Example MCP server exposing a single `greet` tool over stdio"
)]
struct Cli {
    #[command(flatten)]
    common: librebar::cli::CommonArgs,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Serve on stdio until the client disconnects.
    Run,
    /// Report app state and the configured greeting.
    Info,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    cli.common.apply_color();
    cli.common.apply_chdir()?;

    let app = librebar::init("mcp-server")
        .with_version(env!("CARGO_PKG_VERSION"))
        .with_cli(cli.common)
        .config::<Config>()
        .logging()
        .start()?;

    match cli.command.unwrap_or(Command::Info) {
        Command::Run => run_server(&app).await,
        Command::Info => {
            print_info(&app);
            Ok(())
        }
    }
}

async fn run_server(app: &librebar::App<Config>) -> Result<()> {
    let server = GreetServer {
        greeting: app.config().greeting.clone(),
    };
    tracing::info!("mcp-server starting on stdio");

    let service = server.serve(librebar::mcp::transport_stdio()).await?;
    service.waiting().await?;

    tracing::info!("mcp-server client disconnected; exiting");
    Ok(())
}

fn print_info(app: &librebar::App<Config>) {
    let config = app.config();
    println!("app:      {} v{}", app.app_name(), app.version());
    println!("sources:  {:?}", app.config_sources());
    println!("greeting: {}", config.greeting);
    println!("tools:    greet");
    println!(
        "log dir:  {:?}",
        librebar::logging::platform_log_dir(app.app_name())
    );
    println!();
    println!("Run with `run` to serve on stdio (expects a connected MCP client).");
}

// ─── Server ─────────────────────────────────────────────────────────

#[derive(Clone)]
struct GreetServer {
    greeting: String,
}

impl ServerHandler for GreetServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions("A minimal librebar example exposing a single `greet` tool.")
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        // Inline JSON Schema — librebar doesn't depend on schemars, and
        // rmcp's `#[tool]` macros (which would generate this from a typed
        // param struct) aren't enabled. For real servers, enable rmcp's
        // `macros` feature in your own Cargo.toml and use the derive
        // pattern instead of hand-rolling schemas.
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Who to greet",
                }
            },
            "required": ["name"],
        });
        let schema_obj = schema.as_object().cloned().unwrap_or_default();

        let tool = Tool::new(
            Cow::Borrowed("greet"),
            Cow::Borrowed("Greet someone by name with the configured prefix"),
            Arc::new(schema_obj),
        );

        Ok(ListToolsResult::with_all_items(vec![tool]))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        if request.name != "greet" {
            return Err(McpError::invalid_params(
                format!("unknown tool: {}", request.name),
                None,
            ));
        }

        let name = request
            .arguments
            .as_ref()
            .and_then(|args| args.get("name"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| McpError::invalid_params("missing required arg: name", None))?;

        let output = format!("{}, {}!", self.greeting, name);
        tracing::info!(name = %name, "greet tool invoked");
        Ok(CallToolResult::success(vec![Content::text(output)]))
    }
}
