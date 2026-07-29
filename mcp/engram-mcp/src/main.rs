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
mod tools;

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
    registry.register(ToolRecord {
        name: "write_memory",
        description: "Persist an observation or episode to the memory layer.",
        input_schema: json!({
            "type": "object",
            "properties": { "content": { "type": "string" } },
            "required": ["content"]
        }),
        handler: tools::write_memory,
    });
    registry.register(ToolRecord {
        name: "forget",
        description: "Delete, redact, tombstone, or archive a memory by target id.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "target_id": { "type": "string" },
                "mode": { "type": "string", "description": "delete | redact | tombstone | archive" }
            },
            "required": ["target_id"]
        }),
        handler: tools::forget,
    });
    registry.register(ToolRecord {
        name: "put_entity",
        description: "Add an entity to the knowledge graph (upsert by name; honors the kind arg).",
        input_schema: json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" },
                "kind": { "type": "string", "description": "Entity kind (concept, api, function, …); defaults to concept. Unknown kinds are rejected." }
            },
            "required": ["name"]
        }),
        handler: tools::put_entity,
    });
    registry.register(ToolRecord {
        name: "put_relationship",
        description: "Add a (subject, predicate, object) edge to the knowledge graph.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "subject": { "type": "string" },
                "predicate": { "type": "string" },
                "object": { "type": "string" }
            },
            "required": ["subject", "predicate", "object"]
        }),
        handler: tools::put_relationship,
    });
    registry.register(ToolRecord {
        name: "recall",
        description: "Fused retrieval across memory + knowledge + beliefs for the project.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "query": { "type": "string" },
                "lanes": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Restrict to source lanes: memory | knowledge | docs | beliefs. Absent or empty fuses all."
                },
                "limit": { "type": "integer", "minimum": 1, "maximum": 100, "description": "Max items (default 10)." }
            },
            "required": ["query"]
        }),
        handler: tools::recall,
    });
    registry.register(ToolRecord {
        name: "consolidate",
        description: "Run consolidation (reflection + decay) over the project scope.",
        input_schema: json!({
            "type": "object",
            "properties": { "dry_run": { "type": "boolean" } }
        }),
        handler: tools::consolidate,
    });
    registry.register(ToolRecord {
        name: "store_knowledge",
        description: "Bulk distill-write: write extracted facts + entities + relationships in one \
                      best-effort batch (NOT ACID). Surfaces per-step status. Entries missing a \
                      required field are skipped (reported in the result).",
        input_schema: json!({
            "type": "object",
            "properties": {
                "facts": { "type": "array", "items": { "type": "object", "properties": { "content": { "type": "string" } }, "required": ["content"] } },
                "entities": { "type": "array", "items": { "type": "object", "properties": { "name": { "type": "string" }, "kind": { "type": "string" } }, "required": ["name"] } },
                "relationships": { "type": "array", "items": { "type": "object", "properties": { "subject": { "type": "string" }, "predicate": { "type": "string" }, "object": { "type": "string" } }, "required": ["subject", "predicate", "object"] } },
                "idempotency_key": { "type": "string", "description": "Omit only if you do not need re-send dedup; otherwise supply a stable caller-chosen key." }
            }
        }),
        handler: tools::store_knowledge,
    });
}

fn ping(_app: &App, _args: &Value) -> Result<Value, ToolError> {
    Ok(protocol::text_content("pong"))
}
