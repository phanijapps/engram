//! Unified Engram MCP server.
//!
//! One stdio JSON-RPC 2.0 server exposing engram's generic memory,
//! knowledge-graph, and (Phase 2+) code-intelligence capabilities to AI agents
//! over a single [`EngramProvider`]. Phase 1 wires the transport, tool
//! registry, and provider bootstrap; real tools replace the placeholder in
//! later tasks.
//!
//! See `docs/specs/engram-mcp-core/spec.md` (RFC-0015, Phase 1).
//!
//! [`EngramProvider`]: engram_integration::EngramProvider

mod bootstrap;
mod config;
mod protocol;
mod registry;
mod scope;
mod server;

use engram_integration::EngramProvider;
use registry::{ToolRecord, ToolRegistry};
use serde_json::{Value, json};

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let config = config::McpConfig::from_args(&argv).unwrap_or_else(|message| {
        eprintln!("engram-mcp: {message}");
        eprintln!(
            "usage: engram-mcp --storage <path> [--project <name>] \
             [--ontology <path>] [--taxonomy <path>]"
        );
        std::process::exit(2);
    });
    let provider = bootstrap::open_provider(&config).unwrap_or_else(|message| {
        eprintln!("engram-mcp: {message}");
        std::process::exit(1);
    });

    let mut registry: ToolRegistry<EngramProvider> = ToolRegistry::new();
    register_all(&mut registry);
    server::run(registry, &provider);
}

/// Register every tool the server exposes. Phase 1 ships a single placeholder
/// (`ping`) so the transport is exercisable end-to-end; T5+ replaces it with
/// the real tool set.
fn register_all(registry: &mut ToolRegistry<EngramProvider>) {
    registry.register(ToolRecord {
        name: "ping",
        description: "Transport health check. Returns \"pong\". (Phase-1 placeholder.)",
        input_schema: json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        }),
        handler: ping,
    });
}

fn ping(_provider: &EngramProvider, _args: &Value) -> Value {
    protocol::text_content("pong")
}
