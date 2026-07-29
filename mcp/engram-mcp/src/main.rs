//! Unified Engram MCP server.
//!
//! One stdio JSON-RPC 2.0 server exposing engram's generic memory,
//! knowledge-graph, and (Phase 2+) code-intelligence capabilities to AI agents
//! over a single [`EngramProvider`]. Phase 1 wires the transport, tool
//! registry, provider bootstrap, fused-per-project scope, and multi-layer
//! ontology/taxonomy configuration; the write/recall tools and the
//! `engram-distill` skill arrive in later tasks.
//!
//! See `docs/specs/engram-mcp-core/spec.md` (RFC-0015, Phase 1).
//!
//! [`EngramProvider`]: engram_integration::EngramProvider

mod app;
mod bootstrap;
mod config;
mod ontology;
mod protocol;
mod registry;
mod scope;
mod server;

use app::App;
use registry::{ToolError, ToolRecord, ToolRegistry};
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

    // Resolve the multi-layer ontology + taxonomy config (file or baked-in
    // default). Persistence into the ontology/taxonomy repositories lands in T4b.
    let ontology = ontology::resolve_ontology_config(config.ontology_path.as_deref())
        .unwrap_or_else(|message| {
            eprintln!("engram-mcp: {message}");
            std::process::exit(1);
        });
    let taxonomy = ontology::resolve_taxonomy_config(config.taxonomy_path.as_deref())
        .unwrap_or_else(|message| {
            eprintln!("engram-mcp: {message}");
            std::process::exit(1);
        });

    let app = App {
        provider,
        scope: scope::project_scope(&config.project, "default"),
        ontology,
        taxonomy,
    };

    let mut registry: ToolRegistry<App> = ToolRegistry::new();
    register_all(&mut registry);
    server::run(registry, &app);
}

/// Register every tool the server exposes. Phase 1 ships the placeholder `ping`
/// plus the ontology/taxonomy read tools; T5+ adds the write/recall tools.
fn register_all(registry: &mut ToolRegistry<App>) {
    registry.register(ToolRecord {
        name: "ping",
        description: "Transport health check. Returns \"pong\". (Phase-1 placeholder.)",
        input_schema: json!({ "type": "object", "properties": {}, "additionalProperties": false }),
        handler: ping,
    });
    registry.register(ToolRecord {
        name: "ontology_read",
        description: "Return the active multi-layer ontology configuration: layers, classes, \
                      and within/across predicates.",
        input_schema: json!({ "type": "object", "properties": {} }),
        handler: app::ontology_read,
    });
    registry.register(ToolRecord {
        name: "taxonomy_read",
        description: "Return the active taxonomy configuration: concept scheme name + concepts.",
        input_schema: json!({ "type": "object", "properties": {} }),
        handler: app::taxonomy_read,
    });
}

fn ping(_app: &App, _args: &Value) -> Result<Value, ToolError> {
    Ok(protocol::text_content("pong"))
}
