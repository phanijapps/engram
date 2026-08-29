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
mod dependencies;
mod graph;
mod hierarchy;
mod maintenance;
mod ontology;
mod ownership;
mod predict;
mod procedures;
mod protocol;
mod protocols;
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
             [--db-file <name>] [--backend sqlite|pgvector] \
             [--pg-connection-string <url>] [--tools core|full|all] [--no-vector]"
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
        storage_dir: std::path::PathBuf::from(&config.storage_path),
    };

    let mut registry: ToolRegistry<App> = ToolRegistry::new();
    register_all(&mut registry, &config.tool_profile);
    server::run(registry, &app);
}

/// Tool profiles — restrict which MCP tools are registered to reduce agent
/// call count. An agent that sees 37 tools explores many; one that sees 8
/// stays focused. Pass via `--tools <profile>`.
fn tool_profile_set(profile: &str) -> Option<&'static [&'static str]> {
    match profile {
        // Consolidated surface: 6 rich verbs. DEFAULT — the daily driver.
        "core" | "" => Some(CORE_TOOLS),
        // Consolidated + specialist: 10 tools for maintenance work.
        "full" => Some(FULL_TOOLS),
        // Legacy granular surface: all 44 tools. Superseded — kept for
        // backward compatibility; new integrations should use core/full.
        "all" => None,
        // Legacy granular profiles — superseded by core/full. Still work
        // for existing deployments but are deprecated.
        "investigate" | "read" | "scan" | "write" | "maintain" => {
            eprintln!(
                "engram-mcp: --tools {profile} is superseded by --tools core or --tools full; \
                 using the consolidated core surface"
            );
            Some(CORE_TOOLS)
        }
        other => {
            eprintln!(
                "engram-mcp: unknown --tools profile '{other}'; using core (6 tools). \
                 Options: core | full | all"
            );
            Some(CORE_TOOLS)
        }
    }
}

/// The 6 core consolidated tools — the 80/20 agent surface.
pub static CORE_TOOLS: &[&str] = &[
    "remember", "recall", "code", "scan", "graph", "forget", "maintain",
];

/// The 10 full tools — core + specialist for maintenance/analysis work.
pub static FULL_TOOLS: &[&str] = &[
    "remember",
    "recall",
    "code",
    "scan",
    "graph",
    "forget",
    "maintain",
    "beliefs",
    "procedures",
    "hierarchy",
];

/// Register every tool the server exposes, filtered by the active profile.
fn register_all(registry: &mut ToolRegistry<App>, profile: &str) {
    match tool_profile_set(profile) {
        Some(tools) if tools.contains(&"remember") && tools.len() <= 7 => {
            // Consolidated core surface: 6 rich verbs dispatching to
            // existing handlers. The granular tools are NOT registered —
            // the dispatch fns call the Rust handlers directly.
            register_core_tools(registry);
        }
        Some(tools) if tools.contains(&"remember") => {
            // Consolidated full surface: core + specialist.
            register_specialist_tools(registry);
        }
        allowed => {
            // Granular surface (all/investigate/read/scan/write/maintain):
            // register everything, filter by the allowed list.
            register_all_tools(registry);
            registry.retain(allowed);
        }
    }
}

/// Register ALL tools unconditionally (internal — called by register_all).
fn register_all_tools(registry: &mut ToolRegistry<App>) {
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
        name: "predict_context",
        description: "Derive proactive retrieval hints from the agent's current state \
                      (task + recent_queries + recent_target_ids). Returns predicted queries \
                      + still-relevant target ids to feed into recall / get_context. \
                      Deterministic baseline (no model).",
        input_schema: json!({
            "type": "object",
            "properties": {
                "task": { "type": "string", "description": "Current task label (optional)." },
                "recent_queries": { "type": "array", "items": { "type": "string" }, "description": "Recent explicit queries (optional)." },
                "recent_target_ids": { "type": "array", "items": { "type": "string" }, "description": "Recently retrieved target ids (optional)." }
            }
        }),
        handler: predict::predict_context,
    });
    registry.register(ToolRecord {
        name: "recall",
        description: "General evidence discovery: fuses vector + lexical + graph + temporal + belief \
                      lanes over ALL entity kinds (code symbols, docs, memories, beliefs, concepts). \
                      The right FIRST call for any question — not just code. Returns fused + ranked \
                      items with per-lane provenance (raw lane score + fused RRF score). Parameters: \
                      `query` (natural language), optional `lanes` filter (memory|knowledge|docs|beliefs), \
                      `limit` (default 10, max 100). Use `search` for exact-identifier code lookup; \
                      use `recall` for broad discovery across every knowledge kind.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Natural-language query. Matched across all lanes + entity kinds." },
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
        description: "Treesitter-index a code repository into the project workspace (code lane). \
                      Incremental: a per-root manifest is persisted under <storage>/scan-manifests/ \
                      so re-scans skip unchanged files; pass force=true to re-ingest everything. \
                      Honors an optional scan config: pass `scan_config` (path to a JSON file) or \
                      drop one at `<repo>/.engram/scan.json` to tune the concept-link filter \
                      (blocklist/allowlist/min_name_length) and the file denylist (dirs/extensions). \
                      Missing/malformed config soft-fails to the builtin filter.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Repository root path." },
                "force": { "type": "boolean", "description": "Skip the incremental manifest and re-ingest every file (default false)." },
                "scan_config": { "type": "string", "description": "Optional path to a scan-filter JSON config (overrides <repo>/.engram/scan.json)." }
            },
            "required": ["path"]
        }),
        handler: codegraph::scan_repo,
    });
    registry.register(ToolRecord {
        name: "reindex",
        description: "Drain the vector-embed backlog (PS5): embeds un-embedded chunks in the \\
                      scope regardless of source — the backfill path for old-source leftovers \\
                      and fresh stores after a backend switch (vectors start empty). Keyed on \\
                      the vector index's embedded-set. Capped per call (`limit`, default 256, \\
                      max 1024) in deterministic chunk-id order; re-run to continue. Reports \\
                      embedded count + remaining. Requires the vector lane (fastembed feature, \\
                      no --no-vector).",
        input_schema: json!({
            "type": "object",
            "properties": {
                "limit": { "type": "number", "description": "Max chunks to embed this call (default 256, max 1024)." },
                "scope": { "type": "object", "description": "Optional scope override; defaults to the launch scope." }
            }
        }),
        handler: codegraph::reindex,
    });
    registry.register(ToolRecord {
        name: "scan_protocols",
        description: "Post-index scan: extract HTTP protocol boundaries (client fetch calls + \
                      server route registrations), normalize route patterns, create endpoint \
                      entities, and link callers → endpoints → handlers. Run AFTER scan_repo.",
        input_schema: json!({
            "type": "object",
            "properties": { "path": { "type": "string", "description": "Repository root path." } },
            "required": ["path"]
        }),
        handler: protocols::scan_protocols,
    });
    registry.register(ToolRecord {
        name: "scan_dependencies",
        description: "Post-index scan: extract package/crate dependencies from Cargo.toml + \
                      package.json into Module entities + depends_on edges (multi-package / \
                      multi-repo dependency view). Run AFTER scan_repo.",
        input_schema: json!({
            "type": "object",
            "properties": { "path": { "type": "string", "description": "Repository root path." } },
            "required": ["path"]
        }),
        handler: dependencies::scan_dependencies,
    });
    registry.register(ToolRecord {
        name: "scan_ownership",
        description: "Post-index scan: extract CODEOWNERS rules into Organization/Person \
                      entities + owns edges to path Modules (who-owns-what view). Missing \
                      CODEOWNERS is a no-op. Run AFTER scan_repo.",
        input_schema: json!({
            "type": "object",
            "properties": { "path": { "type": "string", "description": "Repository root path." } },
            "required": ["path"]
        }),
        handler: ownership::scan_ownership,
    });
    registry.register(ToolRecord {
        name: "search",
        description: "Ranked code search over indexed symbols (entities) + code text (chunks). \
                      Fuses lexical (BM25) + graph + associative-graph + community-summary lanes. \
                      By default returns BOTH entity hits (symbol name + path) AND chunk hits \
                      (code text excerpt — surfaces credential strings, constants, request \
                      construction that live in function bodies). Set include_chunks=false for \
                      entity-only results.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "query": { "type": "string" },
                "limit": { "type": "integer" },
                "repository": {
                    "type": "string",
                    "description": "Optional repository filter (e.g. \"phanijapps/engram\", \
                                    \"github.com/phanijapps/engram\"). When set, results are \
                                    narrowed to that repository by provenance, eliminating \
                                    cross-repo contamination in a shared-scope DB. Absent = \
                                    all repositories."
                },
                "min_score": {
                    "type": "number",
                    "description": "Minimum fused score (default 0.01). Items below this are \
                                    dropped as noise. Lower to surface more; raise to tighten."
                },
                "include_chunks": {
                    "type": "boolean",
                    "description": "When true (default), include code Chunk results alongside \
                                    Entity results. Each chunk hit includes a 500-char excerpt \
                                    of the code text. When false, only Entity (symbol) results \
                                    are returned."
                },
                "diagnostics": {
                    "type": "boolean",
                    "description": "When true, emit the full format per result \
                                    (\"name (kind) — org/repo, path [retriever, score=X.XX]\"). \
                                    Default is compact: \"name — repo, path [X.XX]\"."
                }
            },
            "required": ["query"]
        }),
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
        name: "list_maintenance_candidates",
        description: "Deterministic graph-maintenance candidate detection \
                      (orphan/low-confidence/unsupported/duplicate). No LLM. \
                      Returns [MaintenanceCandidate, …].",
        input_schema: json!({
            "type": "object",
            "properties": {
                "scope": { "type": "object" },
                "graphId": { "type": "string" },
                "policy": { "type": "object" }
            },
            "required": ["scope"]
        }),
        handler: maintenance::list_maintenance_candidates,
    });
    registry.register(ToolRecord {
        name: "build_maintenance_plan",
        description: "Build a dry-run maintenance plan (fills before/after previews; \
                      never mutates). Takes a MaintenancePlanRequest, returns a \
                      MaintenancePlan with previews.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "scope": { "type": "object" },
                "mutations": { "type": "array" },
                "policy": { "type": "object" },
                "actor": { "type": "object" }
            },
            "required": ["scope", "mutations", "actor"]
        }),
        handler: maintenance::build_maintenance_plan,
    });
    registry.register(ToolRecord {
        name: "apply_maintenance_plan",
        description: "Apply (mode: \"apply\") or preview (mode: \"preview\") a reviewed \
                      maintenance plan. Apply commits inside one backend transaction with \
                      a referential-integrity verify; preview stages without committing.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "plan": { "type": "object" },
                "mode": { "type": "string", "enum": ["preview", "apply"] }
            },
            "required": ["plan", "mode"]
        }),
        handler: maintenance::apply_maintenance_plan,
    });
    registry.register(ToolRecord {
        name: "graph_health",
        description: "Point-in-time graph-health aggregates: orphan/low-confidence/\
                      unsupported/duplicate candidate counts + archived entity/relationship \
                      counts, per scope (+ optional graphId).",
        input_schema: json!({
            "type": "object",
            "properties": {
                "scope": { "type": "object" },
                "graphId": { "type": "string" }
            },
            "required": ["scope"]
        }),
        handler: maintenance::graph_health,
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
        name: "contradiction_list",
        description: "List open contradiction review records (tension between beliefs) in the \
                      project scope.",
        input_schema: json!({ "type": "object", "properties": {} }),
        handler: belief::contradiction_list,
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
        description: "Callers, callees, and community for one symbol — or, when `symbol` is an array, for each symbol in one call (batch).",
        input_schema: json!({ "type": "object", "properties": { "symbol": { "oneOf": [ { "type": "string" }, { "type": "array", "items": { "type": "string" } } ], "description": "A single symbol name OR an array of symbol names to resolve in one call." }, "depth": { "type": "integer" }, "cap": { "type": "integer" } }, "required": ["symbol"] }),
        handler: codegraph::symbol_context,
    });
    registry.register(ToolRecord {
        name: "change_impact",
        description: "Blast radius + dependency paths from a change site — or, when `target` is an array, for each target in one call (batch).",
        input_schema: json!({ "type": "object", "properties": { "target": { "oneOf": [ { "type": "string" }, { "type": "array", "items": { "type": "string" } } ], "description": "A single target symbol OR an array of target symbols to resolve in one call." }, "depth": { "type": "integer" }, "cap": { "type": "integer" }, "to": { "type": "string" } }, "required": ["target"] }),
        handler: codegraph::change_impact,
    });
    registry.register(ToolRecord {
        name: "file_dependencies",
        description: "File-level import graph from `imports` edges (RFC-0020 Phase 2): each scanned file → the module paths it imports, resolved to defining files by stem suffix where the scanned source contains them.",
        input_schema: json!({ "type": "object", "properties": {} }),
        handler: codegraph::file_dependencies,
    });
    registry.register(ToolRecord {
        name: "explore",
        description: "Natural-language entry point (RFC-0020 Phase 2): identifier-shaped tokens in the query seed entity matches; matched seeds expand over calls/contains edges into a bounded, relevance-ordered subgraph (defaults: depth 2, 24 nodes, 64 edges).",
        input_schema: json!({ "type": "object", "properties": { "query": { "type": "string", "description": "Natural-language question, e.g. 'how does parse_config flow into validate?'" }, "depth": { "type": "integer" }, "max_nodes": { "type": "integer" }, "max_edges": { "type": "integer" } }, "required": ["query"] }),
        handler: codegraph::explore,
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
        description: "Compose a task-aware context packet: fused recall + code neighborhood. `focus` accepts a string OR an array of anchors (multi-anchor mode).",
        input_schema: json!({
            "type": "object",
            "properties": {
                "focus": {
                    "oneOf": [
                        { "type": "string" },
                        { "type": "array", "items": { "type": "string" } }
                    ],
                    "description": "Symbol, file, concept, or free-text. OR a JSON array of \
                                    anchors: the first drives the [Code]/[Graph] sections and \
                                    recall searches for all terms in one fused pass. \
                                    e.g. \"loginAnthropic\" or [\"loginAnthropic\", \"resolveStoredOAuth\"]."
                },
                "depth": { "type": "integer" },
                "limit": { "type": "integer" },
                "mode": {
                    "type": "string",
                    "enum": ["evidence", "discovery", "compact"],
                    "description": "Recall rendering mode (default \"evidence\"). \
                                    \"evidence\" — return content excerpts (up to 2000 chars/item). \
                                    \"discovery\" (alias \"compact\") — return ONLY result headers \
                                    (name, kind, repo, path, score), ~80 chars/item. Use discovery \
                                    to scan what exists before paying for content."
                },
                "repository": {
                    "type": "string",
                    "description": "Optional repository filter (e.g. \"phanijapps/engram\", \
                                    \"github.com/phanijapps/engram\"). When set, the [Recall], \
                                    [Graph], and [Code] sections are narrowed to that repository \
                                    by provenance, eliminating cross-repo contamination in a \
                                    shared-scope DB. Absent = all repositories."
                }
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

// ===== CONSOLIDATED TOOL SURFACE (research: agent-memory-and-tool-consolidation.md) =====
// 10 rich verbs with discriminators replacing 44 granular tools.
// Each dispatches to the existing handler — zero behavior change, backward compat preserved.

/// Registers the 6 CORE consolidated tools (the 80/20 agent surface).
/// These are aliases over existing handlers — the granular tools still exist
/// under `--tools all` for backward compatibility.
pub fn register_core_tools(registry: &mut ToolRegistry<App>) {
    // 1. remember — ONE write surface for all durable records
    registry.register(ToolRecord {
        name: "remember",
        description: "Store what you learned. ONE write surface for every record type. \
                      kind=memory: persist an observation/episode/fact. \
                      kind=entity: add a knowledge-graph entity. \
                      kind=relationship: add a graph edge. \
                      kind=belief: assert a derived stance. \
                      kind=procedure: store a replayable runbook. \
                      kind=knowledge: bulk distill-write (facts + entities + edges).",
        input_schema: json!({
            "type": "object",
            "properties": {
                "kind": { "type": "string", "enum": ["memory", "entity", "relationship", "belief", "procedure", "knowledge"],
                          "description": "What kind of record to store (default: memory)." },
                "content": { "type": "string", "description": "The text/content to store (memory, belief statement, procedure text)." },
                "name": { "type": "string", "description": "Entity/procedure/belief subject name." },
                "kind_detail": { "type": "string", "description": "Memory kind: observation|fact|preference|episode|procedure. Entity kind: function|class|module|…" },
                "confidence": { "type": "number", "description": "Belief confidence 0-1." },
                "subject": { "type": "string", "description": "Belief subject key." },
                "predicate": { "type": "string", "description": "Relationship predicate (calls|extends|…)." },
                "object": { "type": "string", "description": "Relationship object entity name." },
                "steps": { "type": "array", "items": { "type": "string" }, "description": "Procedure steps." },
                "data": { "type": "object", "description": "Bulk knowledge payload (kind=knowledge)." }
            },
            "required": ["kind"]
        }),
        handler: consolidated_remember,
    });

    // 2. recall — ONE retrieval surface for everything
    registry.register(ToolRecord {
        name: "recall",
        description: "What do I know about X? ONE retrieval surface. \
                      mode=fused (default): multi-lane hybrid (vector+graph+lexical+temporal+beliefs). \
                      mode=keyword: BM25 lexical search. \
                      mode=context: task-aware context packet with graph/code neighborhoods. \
                      mode=predict: proactive retrieval hints from recent context.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "The search query or topic." },
                "mode": { "type": "string", "enum": ["fused", "keyword", "context", "predict"],
                          "description": "Retrieval mode (default: fused)." },
                "limit": { "type": "number", "description": "Max results." },
                "focus": { "type": "array", "items": { "type": "string" }, "description": "Symbol anchors for context mode." }
            },
            "required": ["query"]
        }),
        handler: consolidated_recall,
    });

    // 3. code — ONE code intelligence surface
    registry.register(ToolRecord {
        name: "code",
        description: "Understand this codebase. ONE code-intel surface. \
                      op=search: ranked code search by query text. \
                      op=context: callers/callees/community for a symbol. \
                      op=impact: blast radius + dependency path from a change. \
                      op=health: dead code + repository stats. \
                      op=architecture: central symbols, bridges, communities. \
                      op=changed: temporal recency + impact scoring. \
                      op=explore: identifier-seeded neighborhood expansion. \
                      op=dependencies: file-level import graph.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "op": { "type": "string", "enum": ["search", "context", "impact", "health", "architecture", "changed", "explore", "dependencies"],
                        "description": "Code intelligence operation." },
                "symbol": { "type": "string", "description": "Target symbol (context/impact modes)." },
                "query": { "type": "string", "description": "Search query (explore mode)." },
                "depth": { "type": "number", "description": "Traversal depth (default 1-2)." },
                "limit": { "type": "number", "description": "Result cap." },
                "to": { "type": "string", "description": "Dependency path endpoint (impact mode)." }
            },
            "required": ["op"]
        }),
        handler: consolidated_code,
    });

    // 4. scan — ONE ingestion surface
    registry.register(ToolRecord {
        name: "scan",
        description: "Learn from a source. ONE ingestion surface. \
                      target=repo: tree-sitter index a code repository (13 languages). \
                      target=protocols: extract HTTP API boundaries. \
                      target=dependencies: extract package/crate dependencies. \
                      target=ownership: extract CODEOWNERS. \
                      target=docs: chunk a Markdown/text document.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "target": { "type": "string", "enum": ["repo", "protocols", "dependencies", "ownership", "docs"],
                            "description": "What to scan (default: repo)." },
                "path": { "type": "string", "description": "Repository or file path." },
                "content": { "type": "string", "description": "Document content (target=docs)." },
                "force": { "type": "boolean", "description": "Bypass incremental manifest (default false)." }
            },
            "required": ["path"]
        }),
        handler: consolidated_scan,
    });

    // 5. graph — ONE graph query surface
    registry.register(ToolRecord {
        name: "graph",
        description: "Explore the knowledge graph. ONE graph query surface. \
                      op=neighbors: directly connected entities. \
                      op=subgraph: breadth-first subgraph around a node. \
                      op=resolve: resolve a name to its entity.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "op": { "type": "string", "enum": ["neighbors", "subgraph", "resolve"],
                        "description": "Graph operation." },
                "name": { "type": "string", "description": "Entity name or ID." },
                "depth": { "type": "number", "description": "Subgraph depth (default 2)." },
                "limit": { "type": "number", "description": "Result cap." }
            },
            "required": ["op", "name"]
        }),
        handler: consolidated_graph,
    });

    // 6. forget — ONE deletion surface
    registry.register(ToolRecord {
        name: "forget",
        description: "Remove what's wrong or outdated. ONE deletion surface. \
                      kind=memory: delete/redact/tombstone/archive a memory. \
                      kind=belief: retract a belief by subject.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "kind": { "type": "string", "enum": ["memory", "belief"],
                          "description": "What to forget (default: memory)." },
                "id": { "type": "string", "description": "Memory ID to forget." },
                "mode": { "type": "string", "enum": ["delete", "redact", "tombstone", "archive"],
                          "description": "Deletion mode (default: tombstone)." },
                "subject": { "type": "string", "description": "Belief subject to retract." }
            },
            "required": []
        }),
        handler: consolidated_forget,
    });

    // 7. maintain — ONE operations surface
    registry.register(ToolRecord {
        name: "maintain",
        description: "Keep the store healthy. ONE operations surface. \
                      op=consolidate: run reflection + decay → derived beliefs. \
                      op=reindex: drain the vector-embed backlog (content-hash dedup). \
                      op=health: graph-health aggregates (orphans, duplicates). \
                      op=candidates: list maintenance candidates. \
                      op=plan: build a dry-run maintenance plan. \
                      op=apply: apply a reviewed plan. \
                      op=capabilities: report which capabilities are wired. \
                      op=increment: bump a procedure counter (id + outcome=success|failure).",
        input_schema: json!({
            "type": "object",
            "properties": {
                "op": { "type": "string", "enum": ["consolidate", "reindex", "health", "candidates", "plan", "apply", "capabilities", "increment"],
                        "description": "Maintenance operation." },
                "limit": { "type": "number", "description": "Reindex chunk cap / candidate limit." },
                "plan": { "type": "object", "description": "Maintenance plan JSON (op=apply)." },
                "mode": { "type": "string", "enum": ["preview", "apply"], "description": "Plan mode." }
            },
            "required": ["op"]
        }),
        handler: consolidated_maintain,
    });
}

/// Registers the 4 SPECIALIST consolidated tools (loaded via --tools full).
pub fn register_specialist_tools(registry: &mut ToolRegistry<App>) {
    register_core_tools(registry);

    // 8. beliefs — belief queries
    registry.register(ToolRecord {
        name: "beliefs",
        description: "Query beliefs and contradictions. \
                      op=get: the live belief for a subject. \
                      op=stale: beliefs flagged stale. \
                      op=contradictions: open contradiction review records.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "op": { "type": "string", "enum": ["get", "stale", "contradictions"],
                        "description": "Belief query." },
                "subject": { "type": "string", "description": "Belief subject (op=get)." }
            },
            "required": ["op"]
        }),
        handler: consolidated_beliefs,
    });

    // 9. procedures — procedure queries
    registry.register(ToolRecord {
        name: "procedures",
        description: "Query and update procedures (runbooks). \
                      op=list: all procedures in scope. \
                      op=increment: bump success or failure counter.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "op": { "type": "string", "enum": ["list", "increment"],
                        "description": "Procedure operation." },
                "id": { "type": "string", "description": "Procedure ID (op=increment)." },
                "outcome": { "type": "string", "enum": ["success", "failure"], "description": "Counter to bump." }
            },
            "required": ["op"]
        }),
        handler: consolidated_procedures,
    });

    // 10. hierarchy — hierarchy operations
    registry.register(ToolRecord {
        name: "hierarchy",
        description: "Build and navigate the hierarchy (Louvain communities + layers). \
                      op=build: cluster the knowledge graph via Louvain + persist nodes. \
                      op=path: navigation path (LCA + nodes + relations) between two symbols.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "op": { "type": "string", "enum": ["build", "path"],
                        "description": "Hierarchy operation." },
                "from": { "type": "string", "description": "Path start (op=path)." },
                "to": { "type": "string", "description": "Path end (op=path)." }
            },
            "required": ["op"]
        }),
        handler: consolidated_hierarchy,
    });
}

// ===== Dispatch handlers (thin routers over existing handlers) =====

fn consolidated_remember(app: &App, args: &Value) -> Result<Value, ToolError> {
    let kind = args["kind"].as_str().unwrap_or("memory");
    match kind {
        "entity" => crate::tools::put_entity(
            app,
            &json!({
                "name": args["name"], "kind": args.get("kind_detail").cloned().unwrap_or(json!("concept"))
            }),
        ),
        "relationship" => crate::tools::put_relationship(
            app,
            &json!({
                "subject": args["name"], "predicate": args["predicate"], "object": args["object"]
            }),
        ),
        "belief" => crate::belief::belief_put(
            app,
            &json!({
                "subject": args["subject"], "statement": args["content"], "confidence": args["confidence"]
            }),
        ),
        "procedure" => crate::procedures::procedure_put(
            app,
            &json!({
                "name": args.get("name").and_then(|v| v.as_str()).unwrap_or("unnamed-procedure"),
                "steps": args.get("steps").and_then(|v| v.as_array()).map(|a| json!(a)).unwrap_or_else(|| json!([args["content"].clone()]))
            }),
        ),
        "knowledge" => crate::tools::store_knowledge(app, args),
        _ => crate::tools::write_memory(
            app,
            &json!({
                "content": args["content"], "kind": args.get("kind_detail").cloned().unwrap_or(json!("observation"))
            }),
        ),
    }
}

fn consolidated_recall(app: &App, args: &Value) -> Result<Value, ToolError> {
    let mode = args["mode"].as_str().unwrap_or("fused");
    match mode {
        "keyword" => {
            // Phase 1.2: keyword mode was misrouting to code search (lexical
            // code chunks) — now routes to memory recall with the memory lane
            // so keyword queries find FACTS and MEMORIES, not code chunks.
            crate::tools::recall(
                app,
                &json!({
                    "mode": "hybrid", "query": args["query"],
                    "limit": args.get("limit").cloned().unwrap_or(json!(10))
                }),
            )
        }
        "context" => crate::codegraph::get_context(
            app,
            &json!({
                "query": args["query"], "focus": args.get("focus").cloned().unwrap_or(json!([]))
            }),
        ),
        "predict" => crate::predict::predict_context(app, &json!({"query": args["query"]})),
        _ => crate::tools::recall(
            app,
            &json!({
                "mode": "hybrid", "query": args["query"], "limit": args.get("limit").cloned().unwrap_or(json!(10))
            }),
        ),
    }
}

fn consolidated_code(app: &App, args: &Value) -> Result<Value, ToolError> {
    let op = args["op"].as_str().unwrap_or("context");
    match op {
        "search" => crate::codegraph::search(
            app,
            &json!({
                "query": args.get("query")
                    .or_else(|| args.get("symbol").filter(|v| !v.is_null()))
                    .unwrap_or(&json!("")),
                "limit": args.get("limit").cloned().unwrap_or(json!(5))
            }),
        ),
        "impact" => crate::codegraph::change_impact(
            app,
            &json!({
                "target": args["symbol"], "depth": args.get("depth").cloned().unwrap_or(json!(2)),
                "to": args.get("to").cloned().unwrap_or(json!(null))
            }),
        ),
        "health" => crate::codegraph::code_health(app, args),
        "architecture" => crate::codegraph::architecture(
            app,
            &json!({
                "limit": args.get("limit").cloned().unwrap_or(json!(10))
            }),
        ),
        "changed" => crate::codegraph::whats_changed(app, args),
        "explore" => crate::codegraph::explore(
            app,
            &json!({
                "query": args["query"], "maxNodes": args.get("limit").cloned().unwrap_or(json!(25))
            }),
        ),
        "dependencies" => crate::codegraph::file_dependencies(app, args),
        _ => crate::codegraph::symbol_context(
            app,
            &json!({
                "symbol": args["symbol"], "depth": args.get("depth").cloned().unwrap_or(json!(1))
            }),
        ),
    }
}

fn consolidated_scan(app: &App, args: &Value) -> Result<Value, ToolError> {
    let target = args["target"].as_str().unwrap_or("repo");
    match target {
        "protocols" => crate::protocols::scan_protocols(app, &json!({"path": args["path"]})),
        "dependencies" => {
            crate::dependencies::scan_dependencies(app, &json!({"path": args["path"]}))
        }
        "ownership" => crate::ownership::scan_ownership(app, &json!({"path": args["path"]})),
        "docs" => crate::tools::index_docs(
            app,
            &json!({
                "content": args.get("content").cloned().unwrap_or(json!(""))
            }),
        ),
        _ => crate::codegraph::scan_repo(app, &json!({"path": args["path"]})),
    }
}

fn consolidated_graph(app: &App, args: &Value) -> Result<Value, ToolError> {
    let op = args["op"].as_str().unwrap_or("neighbors");
    match op {
        "subgraph" => crate::graph::graph_subgraph(
            app,
            &json!({
                "name": args["name"], "depth": args.get("depth").cloned().unwrap_or(json!(2)),
                "limit": args.get("limit").cloned().unwrap_or(json!(100))
            }),
        ),
        "resolve" => crate::graph::resolve_entity(app, &json!({"name": args["name"]})),
        _ => crate::graph::graph_neighbors(
            app,
            &json!({
                "name": args["name"], "limit": args.get("limit").cloned().unwrap_or(json!(20))
            }),
        ),
    }
}

fn consolidated_forget(app: &App, args: &Value) -> Result<Value, ToolError> {
    let kind = args["kind"].as_str().unwrap_or("memory");
    match kind {
        "belief" => {
            let subject = args["subject"].as_str();
            if subject.is_none() || subject == Some("") {
                return Ok(protocol::text_content(
                    "forget: kind=belief requires a `subject` (the belief's subject key). \\
n                     Use recall first to find the belief, then forget by subject.",
                ));
            }
            crate::belief::belief_retract(app, &json!({"subject": args["subject"]}))
        }
        _ => {
            let id = args["id"].as_str();
            if id.is_none() || id == Some("") {
                return Ok(protocol::text_content(
                    "forget: kind=memory requires an `id` (the memory record ID). \\
                     Use recall or graph to find the ID first, then forget by id. \\\n                     kind=belief requires a `subject` instead.",
                ));
            }
            crate::tools::forget(
                app,
                &json!({
                    "id": args["id"], "mode": args.get("mode").cloned().unwrap_or(json!("tombstone"))
                }),
            )
        }
    }
}

fn consolidated_maintain(app: &App, args: &Value) -> Result<Value, ToolError> {
    let op = args["op"].as_str().unwrap_or("health");
    match op {
        "consolidate" => crate::tools::consolidate(app, args),
        "reindex" => crate::codegraph::reindex(
            app,
            &json!({
                "limit": args.get("limit").cloned().unwrap_or(json!(256))
            }),
        ),
        "candidates" => crate::maintenance::list_maintenance_candidates(app, args),
        "plan" => crate::maintenance::build_maintenance_plan(app, args),
        "apply" => crate::maintenance::apply_maintenance_plan(
            app,
            &json!({
                "plan": args["plan"], "mode": args.get("mode").cloned().unwrap_or(json!("preview"))
            }),
        ),
        "capabilities" => crate::codegraph::capability_report(app, args),
        "increment" => crate::procedures::procedure_increment(
            app,
            &json!({
                "id": args["id"], "outcome": args.get("outcome").cloned().unwrap_or(json!("success"))
            }),
        ),
        _ => crate::maintenance::graph_health(app, args),
    }
}

fn consolidated_beliefs(app: &App, args: &Value) -> Result<Value, ToolError> {
    let op = args["op"].as_str().unwrap_or("get");
    match op {
        "stale" => crate::belief::belief_stale_list(app, args),
        "contradictions" => crate::belief::contradiction_list(app, args),
        _ => crate::belief::belief_get(app, &json!({"subject": args["subject"]})),
    }
}

fn consolidated_procedures(app: &App, args: &Value) -> Result<Value, ToolError> {
    let op = args["op"].as_str().unwrap_or("list");
    match op {
        "increment" => crate::procedures::procedure_increment(
            app,
            &json!({
                "id": args["id"], "outcome": args.get("outcome").cloned().unwrap_or(json!("success"))
            }),
        ),
        _ => crate::procedures::procedure_list(app, args),
    }
}

fn consolidated_hierarchy(app: &App, args: &Value) -> Result<Value, ToolError> {
    let op = args["op"].as_str().unwrap_or("path");
    match op {
        "build" => crate::hierarchy::hierarchy_build(app, args),
        _ => crate::hierarchy::hierarchy_path(
            app,
            &json!({
                "from": args["from"], "to": args["to"]
            }),
        ),
    }
}

#[cfg(test)]
mod parity_tests {
    use super::*;

    /// Cross-server tool-list parity (engram-code spec, ADR-0022): the
    /// shared fixture `tests/tool_names.txt` is asserted against BOTH this
    /// server's registry and the TS HTTP server's tool table — the two
    /// transports must never drift apart.
    #[test]
    fn registry_matches_shared_tool_name_fixture() {
        let fixture = std::fs::read_to_string("tests/tool_names.txt").expect("fixture");
        let expected: Vec<String> = fixture
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_owned)
            .collect();
        let mut registry: ToolRegistry<App> = ToolRegistry::new();
        register_all_tools(&mut registry);
        let mut actual: Vec<String> = registry
            .list()
            .iter()
            .filter_map(|t| t["name"].as_str().map(str::to_owned))
            .collect();
        actual.sort();
        let mut expected = expected;
        expected.sort();
        assert_eq!(
            actual, expected,
            "Rust stdio registry drifted from the shared fixture — update \
             tests/tool_names.txt (and the TS HTTP tool table) together"
        );
    }
}
