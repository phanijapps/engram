//! Unified Engram MCP server.
//!
//! One stdio JSON-RPC 2.0 server exposing engram's generic memory,
//! knowledge-graph, and (Phase 2+) code-intelligence capabilities to AI agents
//! over a single [`EngramProvider`]. Phase 1 wires the transport + tool
//! registry; real tools replace the placeholder in later tasks.
//!
//! See `docs/specs/engram-mcp-core/spec.md` (RFC-0015, Phase 1).
//!
//! [`EngramProvider`]: engram_integration::EngramProvider

mod protocol;
mod registry;
mod server;

use registry::{ToolRecord, ToolRegistry};
use serde_json::{Value, json};

fn main() {
    let mut registry = ToolRegistry::new();
    register_all(&mut registry);
    server::run(registry);
}

/// Register every tool the server exposes. Phase 1 ships a single placeholder
/// (`ping`) so the transport is exercisable end-to-end; T2+ replaces it with
/// the real tool set.
fn register_all(registry: &mut ToolRegistry) {
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

fn ping(_args: &Value) -> Value {
    protocol::text_content("pong")
}
