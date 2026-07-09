//! Codegraph MCP server — exposes the codegraph query operations over MCP
//! (JSON-RPC over stdio). Any MCP client (Claude Code, Cursor, Codex) can
//! spawn this binary and query the structural code graph.
//!
//! Usage: `engram-codegraph-mcp` (stdio transport, in-memory store).
//!        `engram-codegraph-mcp /path/to/store.db` (file-backed store).

use std::collections::HashMap;
use std::io::{self, BufRead, Write};
use std::path::Path;

use engram_codegraph_queries as cgq;
use engram_domain::*;
use engram_ingest::{ScanOptions, scan_repository};

use engram_store_knowledge_sqlite::SqlKnowledgeStore;
use futures::executor::block_on;
use serde_json::{Value, json};

fn main() {
    let store = match std::env::args().nth(1) {
        Some(path) => SqlKnowledgeStore::open_file(&path),
        None => SqlKnowledgeStore::open_in_memory(),
    }
    .expect("open knowledge store");

    let scope = Scope {
        tenant: "default".to_owned(),
        subject: None,
        workspace: Some("codegraph".to_owned()),
        session: None,
        environment: None,
    };

    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = stdout.lock();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };
        let request: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let id = request.get("id").cloned();
        let method = request["method"].as_str().unwrap_or("");
        let params = &request["params"];

        let result: Option<Value> = match method {
            "initialize" => Some(json!({
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "engram-codegraph", "version": "0.1.0" }
            })),
            "notifications/initialized" => None,
            "tools/list" => Some(json!({ "tools": tool_list() })),
            "tools/call" => {
                let name = params["name"].as_str().unwrap_or("");
                let args = &params["arguments"];
                let text = handle_tool(name, args, &store, &scope);
                Some(json!({ "content": [{ "type": "text", "text": text }] }))
            }
            _ => Some(json!({
                "error": { "code": -32601, "message": format!("method not found: {method}") }
            })),
        };

        if let Some(result) = result {
            let response = json!({ "jsonrpc": "2.0", "id": id, "result": result });
            writeln!(out, "{response}").unwrap();
            out.flush().unwrap();
        }
    }
}

fn tool_list() -> Vec<Value> {
    vec![
        tool(
            "scan_repo",
            "Index a repository into the codegraph. Returns file/entity/relationship counts.",
            obj(&[("path", "string")]),
        ),
        tool(
            "dead_code",
            "Find symbols defined but never called (zero in-degree on calls edges).",
            obj(&[]),
        ),
        tool(
            "blast_radius",
            "Find transitive callers of a symbol (who breaks if I change X?).",
            obj(&[("target", "string"), ("depth", "integer")]),
        ),
        tool(
            "dependency_path",
            "Find the shortest call path from one symbol to another.",
            obj(&[("from", "string"), ("to", "string")]),
        ),
        tool(
            "central_symbols",
            "Rank symbols by PageRank centrality (most-depended-on).",
            obj(&[("limit", "integer")]),
        ),
        tool(
            "bridge_symbols",
            "Rank symbols by betweenness centrality (chokepoints).",
            obj(&[("limit", "integer")]),
        ),
        tool(
            "call_communities",
            "Detect communities (Louvain) — tightly-coupled clusters.",
            obj(&[("maxPasses", "integer")]),
        ),
        tool(
            "symbol_context",
            "360-degree view of a symbol: callers, callees, community.",
            obj(&[("symbol", "string"), ("depth", "integer")]),
        ),
        tool(
            "cyclomatic_complexity",
            "Estimate cyclomatic complexity from source text.",
            obj(&[("source", "string")]),
        ),
        tool(
            "find_endpoints",
            "Detect HTTP endpoints from source text (Express/FastAPI/Actix/etc.).",
            obj(&[("source", "string")]),
        ),
        tool(
            "find_api_calls",
            "Detect HTTP call sites from source text (fetch/axios/requests).",
            obj(&[("source", "string")]),
        ),
        tool(
            "find_entry_points",
            "Detect entry-point functions (main, handler, __main__).",
            obj(&[("source", "string")]),
        ),
        tool(
            "process_flow",
            "Trace the execution flow from an entry point through the call graph.",
            obj(&[("entryPoint", "string"), ("maxDepth", "integer")]),
        ),
        tool(
            "repository_stats",
            "Node + edge counts for the indexed call graph.",
            obj(&[]),
        ),
    ]
}

fn handle_tool(name: &str, args: &Value, store: &SqlKnowledgeStore, scope: &Scope) -> String {
    match name {
        "scan_repo" => {
            let path = args["path"].as_str().unwrap_or(".");
            let opts = scan_options(scope);
            match scan_repository(Path::new(path), &opts, store, |_| ()) {
                Ok((summary, _)) => format!(
                    "Indexed {}: {} files, {} entities, {} relationships",
                    path, summary.ingested, summary.entities, summary.relationships
                ),
                Err(e) => format!("Error indexing {path}: {e}"),
            }
        }

        // --- Store-based queries (need prior scan_repo) ---
        "dead_code" => {
            let rels = relationships(store, scope);
            json_pretty(&cgq::dead_code(&rels))
        }
        "blast_radius" => {
            let target = args["target"].as_str().unwrap_or("");
            let depth = args["depth"].as_u64().unwrap_or(5) as usize;
            let rels = relationships(store, scope);
            let mut callers: Vec<String> = cgq::blast_radius(&rels, target, depth)
                .into_iter()
                .collect();
            callers.sort();
            json_pretty(&callers)
        }
        "dependency_path" => {
            let from = args["from"].as_str().unwrap_or("");
            let to = args["to"].as_str().unwrap_or("");
            let rels = relationships(store, scope);
            json_pretty(&cgq::dependency_path(&rels, from, to))
        }
        "central_symbols" => {
            let limit = args["limit"].as_u64().unwrap_or(20) as usize;
            let rels = relationships(store, scope);
            json_pretty(&cgq::central_symbols(&rels, limit))
        }
        "bridge_symbols" => {
            let limit = args["limit"].as_u64().unwrap_or(20) as usize;
            let rels = relationships(store, scope);
            json_pretty(&cgq::bridge_symbols(&rels, limit))
        }
        "call_communities" => {
            let max_passes = args["maxPasses"].as_u64().unwrap_or(10) as usize;
            let rels = relationships(store, scope);
            json_pretty(&cgq::call_communities(&rels, max_passes))
        }
        "symbol_context" => {
            let symbol = args["symbol"].as_str().unwrap_or("");
            let depth = args["depth"].as_u64().unwrap_or(5) as usize;
            let rels = relationships(store, scope);
            json_pretty(&cgq::symbol_context(&rels, symbol, depth))
        }
        "process_flow" => {
            let entry = args["entryPoint"].as_str().unwrap_or("");
            let max_depth = args["maxDepth"].as_u64().unwrap_or(10) as usize;
            let rels = relationships(store, scope);
            json_pretty(&cgq::process_flow(&rels, entry, max_depth))
        }
        "repository_stats" => {
            let rels = relationships(store, scope);
            let stats = cgq::repository_stats(&rels);
            json_pretty(&stats)
        }

        // --- Source-text tools (agent passes source code) ---
        "cyclomatic_complexity" => {
            let source = args["source"].as_str().unwrap_or("");
            cgq::cyclomatic_complexity(source).to_string()
        }
        "find_endpoints" => {
            let source = args["source"].as_str().unwrap_or("");
            json_pretty(&cgq::find_endpoints(source))
        }
        "find_api_calls" => {
            let source = args["source"].as_str().unwrap_or("");
            json_pretty(&cgq::find_api_calls(source))
        }
        "find_entry_points" => {
            let source = args["source"].as_str().unwrap_or("");
            json_pretty(&cgq::find_entry_points(source))
        }

        _ => format!("Unknown tool: {name}"),
    }
}

// --- helpers ---

fn relationships(
    store: &SqlKnowledgeStore,
    scope: &Scope,
) -> Vec<engram_domain::KnowledgeRelationship> {
    block_on(store.list_relationships(scope)).unwrap_or_default()
}

fn scan_options(scope: &Scope) -> ScanOptions {
    ScanOptions {
        scope: scope.clone(),
        policy: Policy {
            visibility: Visibility::Workspace,
            retention: Retention::Durable,
            sensitivity: None,
            allowed_uses: vec![AllowedUse::Retrieval],
            expires_at: None,
            delete_mode: None,
        },
        actor: Actor {
            id: Id::from("mcp-server"),
            kind: ActorKind::Agent,
            display_name: Some("MCP Server".to_owned()),
            metadata: None,
        },
        source_name: "mcp-scan".to_owned(),
        max_bytes: 0,
        manifest: HashMap::new(),
    }
}

fn json_pretty<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| "null".to_owned())
}

fn tool(name: &str, description: &str, schema: Value) -> Value {
    json!({ "name": name, "description": description, "inputSchema": schema })
}

fn obj(props: &[(&str, &str)]) -> Value {
    let properties: HashMap<&str, Value> = props
        .iter()
        .map(|(k, t)| (*k, json!({ "type": t })))
        .collect();
    let required: Vec<&str> = props.iter().map(|(k, _)| *k).collect();
    json!({ "type": "object", "properties": properties, "required": required })
}
