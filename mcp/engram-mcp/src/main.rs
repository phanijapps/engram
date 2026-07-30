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
mod belief;
mod bootstrap;
mod codegraph;
mod config;
mod graph;
mod hierarchy;
mod ontology;
mod procedures;
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
             [--org <name> --domain <name> [--subdomain <name>]] \
             [--ontology <path>] [--taxonomy <path>] [--layout single|multi] \
             [--db-file <name>]"
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
        scope: scope::resolve_scope(
            config.org.as_deref(),
            config.domain.as_deref(),
            config.subdomain.as_deref(),
            &config.project,
        ),
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
    registry.register(ToolRecord {
        name: "index_docs",
        description: "Chunk a Markdown (or text) document into retrievable sections and persist \
                      them (docs lane). Use for docs/notes the agent wants recallable.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "content": { "type": "string", "description": "The document text (Markdown)." },
                "path": { "type": "string", "description": "Optional source path (provenance)." },
                "kind": { "type": "string", "description": "markdown | text (default markdown)." }
            },
            "required": ["content"]
        }),
        handler: tools::index_docs,
    });
    registry.register(ToolRecord {
        name: "scan_repo",
        description: "Treesitter-index a code repository into the project workspace (code lane).",
        input_schema: json!({
            "type": "object",
            "properties": { "path": { "type": "string", "description": "Repository root path." } },
            "required": ["path"]
        }),
        handler: codegraph::scan_repo,
    });
    registry.register(ToolRecord {
        name: "search",
        description: "Keyword search over indexed code symbols.",
        input_schema: json!({ "type": "object", "properties": { "query": { "type": "string" }, "limit": { "type": "integer" } }, "required": ["query"] }),
        handler: codegraph::search,
    });
    registry.register(ToolRecord {
        name: "graph_neighbors",
        description: "Entities directly connected to a node (any kind) and the edges between \
                      them. Bidirectional — e.g. a concept describes a function, or a function \
                      calls another.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" },
                "limit": { "type": "integer", "description": "Max edges (default 100)." }
            },
            "required": ["name"]
        }),
        handler: graph::graph_neighbors,
    });
    registry.register(ToolRecord {
        name: "graph_subgraph",
        description: "Breadth-first subgraph around a node up to `depth` hops (default 2). Edges \
                      are labelled with their natural direction; explores doc↔code links.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" },
                "depth": { "type": "integer", "description": "Hop limit (default 2)." },
                "limit": { "type": "integer", "description": "Max edges (default 100)." }
            },
            "required": ["name"]
        }),
        handler: graph::graph_subgraph,
    });
    registry.register(ToolRecord {
        name: "resolve_entity",
        description: "Resolve a name to its entity (exact, else first substring): kind, id, graph, \
                      source-ref count, aliases. The \"is X in the graph?\" lookup.",
        input_schema: json!({
            "type": "object",
            "properties": { "name": { "type": "string" } },
            "required": ["name"]
        }),
        handler: graph::resolve_entity,
    });
    registry.register(ToolRecord {
        name: "belief_get",
        description: "Read the live belief for a subject (valid at `as_of`, default now). The \
                      \"what do we believe about X?\" lookup.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "subject": { "type": "string" },
                "as_of": { "type": "string", "description": "Optional RFC3339 timestamp; defaults to now." }
            },
            "required": ["subject"]
        }),
        handler: belief::belief_get,
    });
    registry.register(ToolRecord {
        name: "belief_put",
        description: "Assert or update a belief (a new valid-time version) for a subject.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "subject": { "type": "string" },
                "statement": { "type": "string" },
                "confidence": { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "Default 0.8." }
            },
            "required": ["subject", "statement"]
        }),
        handler: belief::belief_put,
    });
    registry.register(ToolRecord {
        name: "belief_retract",
        description: "Retract a belief by id (closes its valid interval).",
        input_schema: json!({
            "type": "object",
            "properties": { "id": { "type": "string" } },
            "required": ["id"]
        }),
        handler: belief::belief_retract,
    });
    registry.register(ToolRecord {
        name: "belief_stale_list",
        description: "List beliefs flagged stale in the project scope (need review).",
        input_schema: json!({ "type": "object", "properties": {} }),
        handler: belief::belief_stale_list,
    });
    registry.register(ToolRecord {
        name: "hierarchy_build",
        description: "Cluster the knowledge graph via Louvain communities into hierarchy nodes \
                      (layer 0) with entity members + inter-cluster relations. After building, \
                      hierarchy_path returns navigation results.",
        input_schema: json!({
            "type": "object",
            "properties": { "max_passes": { "type": "integer", "description": "Louvain passes (default 3)." } }
        }),
        handler: hierarchy::hierarchy_build,
    });
    registry.register(ToolRecord {
        name: "hierarchy_path",
        description: "Navigation path (LCA + nodes + relations) for seed entity ids over the \
                      clustered hierarchy. Empty until a hierarchy is built for the scope.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "seeds": { "type": "array", "items": { "type": "string" } },
                "max_layer": { "type": "integer" }
            },
            "required": ["seeds"]
        }),
        handler: hierarchy::hierarchy_path,
    });
    registry.register(ToolRecord {
        name: "procedure_put",
        description: "Assert or update a replayable procedure (runbook steps + optional trigger).",
        input_schema: json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" },
                "steps": { "type": "array", "items": { "type": "string" } },
                "trigger": { "type": "string" }
            },
            "required": ["name"]
        }),
        handler: procedures::procedure_put,
    });
    registry.register(ToolRecord {
        name: "procedure_list",
        description: "List procedures (runbooks) in the project scope with step counts + \
                      success/failure tallies.",
        input_schema: json!({ "type": "object", "properties": {} }),
        handler: procedures::procedure_list,
    });
    registry.register(ToolRecord {
        name: "procedure_increment",
        description: "Bump the success or failure counter for a procedure by id.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "id": { "type": "string" },
                "outcome": { "type": "string", "description": "success | failure" }
            },
            "required": ["id", "outcome"]
        }),
        handler: procedures::procedure_increment,
    });
    registry.register(ToolRecord {
        name: "symbol_context",
        description: "Callers, callees, and community for one symbol.",
        input_schema: json!({ "type": "object", "properties": { "symbol": { "type": "string" }, "depth": { "type": "integer" } }, "required": ["symbol"] }),
        handler: codegraph::symbol_context,
    });
    registry.register(ToolRecord {
        name: "change_impact",
        description: "Blast radius + dependency paths from a change site.",
        input_schema: json!({ "type": "object", "properties": { "target": { "type": "string" }, "depth": { "type": "integer" }, "to": { "type": "string" } }, "required": ["target"] }),
        handler: codegraph::change_impact,
    });
    registry.register(ToolRecord {
        name: "code_health",
        description: "Dead code + repository stats.",
        input_schema: json!({ "type": "object", "properties": {} }),
        handler: codegraph::code_health,
    });
    registry.register(ToolRecord {
        name: "architecture",
        description: "Central symbols, bridges, communities, stats — one map.",
        input_schema: json!({ "type": "object", "properties": { "limit": { "type": "integer" } } }),
        handler: codegraph::architecture,
    });
    registry.register(ToolRecord {
        name: "whats_changed",
        description: "Temporal recency + impact + compound + overview.",
        input_schema: json!({ "type": "object", "properties": {} }),
        handler: codegraph::whats_changed,
    });
    registry.register(ToolRecord {
        name: "get_context",
        description: "Compose a task-aware context packet: fused recall + code neighborhood.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "focus": { "type": "string", "description": "Symbol, file, concept, or free-text." },
                "depth": { "type": "integer" },
                "limit": { "type": "integer" }
            },
            "required": ["focus"]
        }),
        handler: codegraph::get_context,
    });
    registry.register(ToolRecord {
        name: "capability_report",
        description: "Report which provider capabilities are wired.",
        input_schema: json!({ "type": "object", "properties": {} }),
        handler: codegraph::capability_report,
    });
}

fn ping(_app: &App, _args: &Value) -> Result<Value, ToolError> {
    Ok(protocol::text_content("pong"))
}
