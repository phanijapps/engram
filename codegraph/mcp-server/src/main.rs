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
use engram_codegraph_temporal as cgt;
use engram_domain::*;
use engram_ingest::{ScanOptions, scan_repository};
use engram_store_knowledge_sqlite::SqlKnowledgeStore;
use engram_store_lexical::LexicalIndex;

use futures::executor::block_on;
use serde_json::{Value, json};
use std::sync::Mutex;

/// Per-path manifest for incremental re-indexing (rel_path → content_hash).
type ManifestStore = Mutex<HashMap<String, HashMap<String, String>>>;

/// Per-path directional change summary (added/removed/modified file counts)
/// captured at scan time by diffing the prior vs new manifest. The `temporal_*
/// tools read it. `novel` is intentionally absent — it needs per-symbol version
/// history, which the current hard-delete retraction (ADR-0018) discards.
type DirectionalStore = Mutex<HashMap<String, cgt::DirectionalResult>>;

/// Classifies the file-level change between two manifests as
/// added / removed / modified. `prior` empty (first scan) → everything added.
fn manifest_diff(
    prior: &HashMap<String, String>,
    current: &HashMap<String, String>,
) -> cgt::DirectionalResult {
    let added = current.keys().filter(|k| !prior.contains_key(*k)).count();
    let removed = prior.keys().filter(|k| !current.contains_key(*k)).count();
    let modified = current
        .iter()
        .filter(|(k, v)| prior.get(*k).is_some_and(|pv| pv != *v))
        .count();
    cgt::DirectionalResult {
        added,
        removed,
        modified,
    }
}

fn main() {
    let store = match std::env::args().nth(1) {
        Some(path) => SqlKnowledgeStore::open_file(&path),
        None => SqlKnowledgeStore::open_in_memory(),
    }
    .expect("open knowledge store");

    let lexical = Mutex::new(LexicalIndex::new().expect("open lexical index"));
    let manifest: ManifestStore = Mutex::new(HashMap::new());
    let directional: DirectionalStore = Mutex::new(HashMap::new());

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
                let text = handle_tool(
                    name,
                    args,
                    &store,
                    &lexical,
                    &manifest,
                    &directional,
                    &scope,
                );
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
            "search_code",
            "BM25 keyword search over indexed symbols (find functions/classes by name or keyword).",
            obj(&[("query", "string"), ("limit", "integer")]),
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
        tool(
            "temporal_recent",
            "Rank symbols by recency (most recently introduced first). Needs valid_from timestamps on entities.",
            obj(&[]),
        ),
        tool(
            "temporal_impact",
            "Rank symbols by blast-radius-weighted impact (in_degree^0.7 × (1+out_degree)^0.3).",
            obj(&[]),
        ),
        tool(
            "temporal_compound",
            "Rank symbols by a blend of recency + impact (the 'what matters most right now' view).",
            obj(&[]),
        ),
        tool(
            "temporal_overview",
            "Summarize community structure: community count + largest cluster size (architectural shape).",
            obj(&[]),
        ),
        tool(
            "temporal_directional",
            "File-level change direction since the previous scan: +added ~modified -removed (growth vs churn). Pass {path} to pick a scanned repo.",
            obj(&[("path", "string")]),
        ),
        tool(
            "capability_report",
            "Report which tools are available and whether the graph has data indexed.",
            obj(&[]),
        ),
        tool(
            "most_complex",
            "Rank source-text snippets by cyclomatic complexity. Pass an array of {name, source} pairs.",
            obj(&[("sources", "string")]),
        ),
        tool(
            "match_api_topology",
            "Match HTTP endpoints to call sites — cross-service API topology. Pass endpoints + calls arrays.",
            obj(&[("endpoints", "string"), ("calls", "string")]),
        ),
    ]
}

fn handle_tool(
    name: &str,
    args: &Value,
    store: &SqlKnowledgeStore,
    lexical: &Mutex<LexicalIndex>,
    manifest: &ManifestStore,
    directional: &DirectionalStore,
    scope: &Scope,
) -> String {
    // Build the entity-name lookup once for all store-based queries.
    let names = entity_lookup(store, scope);

    match name {
        "scan_repo" => {
            let path = args["path"].as_str().unwrap_or(".").to_owned();
            // Load the prior manifest for this path (incremental skip-unchanged).
            let prior_manifest = {
                let guard = manifest.lock().unwrap_or_else(|e| e.into_inner());
                guard.get(&path).cloned().unwrap_or_default()
            };
            let opts = ScanOptions {
                manifest: prior_manifest.clone(),
                ..scan_options(scope)
            };
            match scan_repository(Path::new(&path), &opts, store, |_| ()) {
                Ok((summary, new_manifest)) => {
                    // Classify the file-level change (prior vs new manifest) and
                    // cache it for the `temporal_directional` tool.
                    let diff = manifest_diff(&prior_manifest, &new_manifest);
                    {
                        let mut guard = directional.lock().unwrap_or_else(|e| e.into_inner());
                        guard.insert(path.clone(), diff.clone());
                    }
                    // Persist the new manifest for incremental re-scans.
                    {
                        let mut guard = manifest.lock().unwrap_or_else(|e| e.into_inner());
                        guard.insert(path.clone(), new_manifest);
                    }
                    // Populate the lexical index with entity names for BM25 search.
                    let entities = block_on(store.list_entities(scope)).unwrap_or_default();
                    if let Ok(idx) = lexical.lock() {
                        for entity in &entities {
                            let searchable = format!("{} {:?}", entity.name, entity.kind);
                            let _ = idx.upsert(&entity.id.to_string(), &searchable);
                        }
                    }
                    format!(
                        "Indexed {}: {} files ({}), {} entities, {} relationships | change: +{} ~{} -{}",
                        path,
                        summary.ingested,
                        if summary.ingested == 0 {
                            "unchanged"
                        } else {
                            "re-indexed"
                        },
                        summary.entities,
                        summary.relationships,
                        diff.added,
                        diff.modified,
                        diff.removed
                    )
                }
                Err(e) => format!("Error indexing {path}: {e}"),
            }
        }
        "search_code" => {
            let query = args["query"].as_str().unwrap_or("");
            let limit = args["limit"].as_u64().unwrap_or(20) as usize;
            let results = match lexical.lock() {
                Ok(idx) => idx.search(query, limit).unwrap_or_default(),
                Err(_) => Vec::new(),
            };
            let readable: Vec<Value> = results
                .iter()
                .enumerate()
                .map(|(i, (id, score))| {
                    let mut entry = resolve_symbol(id, &names);
                    entry["rank"] = json!(i + 1);
                    entry["score"] = json!(score);
                    entry
                })
                .collect();
            envelope(&readable)
        }

        // --- Store-based queries (need prior scan_repo) ---
        "dead_code" => {
            let rels = relationships(store, scope);
            let dead = cgq::dead_code(&rels);
            let mut entry_points = 0usize;
            let mut tests = 0usize;
            let mut candidates = 0usize;
            let results: Vec<Value> = dead
                .iter()
                .map(|id| {
                    let mut entry = resolve_symbol(id, &names);
                    let category = dead_code_class(entry["name"].as_str().unwrap_or(""));
                    match category {
                        "entry_point" => entry_points += 1,
                        "test" => tests += 1,
                        _ => candidates += 1,
                    }
                    entry["category"] = json!(category);
                    entry
                })
                .collect();
            json_pretty(&json!({
                "total": results.len(),
                "candidates": candidates,
                "entry_points": entry_points,
                "tests": tests,
                "results": results,
            }))
        }
        "blast_radius" => {
            let target = args["target"].as_str().unwrap_or("");
            let depth = args["depth"].as_u64().unwrap_or(5) as usize;
            let rels = relationships(store, scope);
            let mut callers: Vec<Value> = cgq::blast_radius(&rels, target, depth)
                .into_iter()
                .map(|id| resolve_symbol(&id, &names))
                .collect();
            callers.sort_by_key(|v| v["name"].as_str().unwrap_or("").to_owned());
            json_pretty(&json!({
                "target": resolve_symbol(target, &names)["name"],
                "depth": depth,
                "caller_count": callers.len(),
                "callers": callers,
            }))
        }
        "dependency_path" => {
            let from = args["from"].as_str().unwrap_or("");
            let to = args["to"].as_str().unwrap_or("");
            let rels = relationships(store, scope);
            let from_name = resolve_symbol(from, &names)["name"].clone();
            let to_name = resolve_symbol(to, &names)["name"].clone();
            match cgq::dependency_path(&rels, from, to) {
                Some(path) => {
                    let readable: Vec<Value> =
                        path.iter().map(|id| resolve_symbol(id, &names)).collect();
                    json_pretty(&json!({
                        "from": from_name,
                        "to": to_name,
                        "found": true,
                        "hops": path.len().saturating_sub(1),
                        "path": readable,
                    }))
                }
                None => json_pretty(&json!({
                    "from": from_name,
                    "to": to_name,
                    "found": false,
                })),
            }
        }
        "central_symbols" => {
            let limit = args["limit"].as_u64().unwrap_or(20) as usize;
            let rels = relationships(store, scope);
            let ranked = cgq::central_symbols(&rels, limit);
            let readable: Vec<Value> = ranked
                .iter()
                .filter(|(id, _)| names.contains_key(id))
                .enumerate()
                .map(|(i, (id, score))| {
                    let mut entry = resolve_symbol(id, &names);
                    entry["rank"] = json!(i + 1);
                    entry["score"] = json!(score);
                    entry
                })
                .collect();
            envelope(&readable)
        }
        "bridge_symbols" => {
            let limit = args["limit"].as_u64().unwrap_or(20) as usize;
            let rels = relationships(store, scope);
            let ranked = cgq::bridge_symbols(&rels, limit);
            let readable: Vec<Value> = ranked
                .iter()
                .filter(|(id, _)| names.contains_key(id))
                .enumerate()
                .map(|(i, (id, score))| {
                    let mut entry = resolve_symbol(id, &names);
                    entry["rank"] = json!(i + 1);
                    entry["score"] = json!(score);
                    entry
                })
                .collect();
            envelope(&readable)
        }
        "call_communities" => {
            let max_passes = args["maxPasses"].as_u64().unwrap_or(10) as usize;
            let rels = relationships(store, scope);
            let labels = cgq::call_communities(&rels, max_passes);
            // Group symbols by community label so an agent sees the architectural
            // clusters (and their size) rather than a flat symbol→label table.
            let mut groups: HashMap<usize, Vec<Value>> = HashMap::new();
            for (id, label) in &labels {
                let entry = resolve_symbol(id, &names);
                groups
                    .entry(*label)
                    .or_default()
                    .push(json!({ "name": entry["name"], "kind": entry["kind"] }));
            }
            let mut communities: Vec<Value> = groups
                .into_iter()
                .map(|(label, mut members)| {
                    members.sort_by_key(|v| v["name"].as_str().unwrap_or("").to_owned());
                    json!({ "label": label, "size": members.len(), "members": members })
                })
                .collect();
            communities.sort_by(|a, b| {
                b["size"]
                    .as_u64()
                    .unwrap_or(0)
                    .cmp(&a["size"].as_u64().unwrap_or(0))
            });
            json_pretty(&json!({
                "community_count": communities.len(),
                "communities": communities,
            }))
        }
        "symbol_context" => {
            let symbol = args["symbol"].as_str().unwrap_or("");
            let depth = args["depth"].as_u64().unwrap_or(5) as usize;
            let rels = relationships(store, scope);
            let ctx = cgq::symbol_context(&rels, symbol, depth);
            let callers: Vec<Value> = ctx
                .callers
                .iter()
                .map(|id| resolve_symbol(id, &names))
                .collect();
            let callees: Vec<Value> = ctx
                .callees
                .iter()
                .map(|id| resolve_symbol(id, &names))
                .collect();
            json_pretty(&json!({
                "symbol": resolve_symbol(symbol, &names),
                "callers": callers,
                "callees": callees,
                "community": ctx.community
            }))
        }
        "process_flow" => {
            let entry = args["entryPoint"].as_str().unwrap_or("");
            let max_depth = args["maxDepth"].as_u64().unwrap_or(10) as usize;
            let rels = relationships(store, scope);
            let flow = cgq::process_flow(&rels, entry, max_depth);
            let readable: Vec<Value> = flow.iter().map(|id| resolve_symbol(id, &names)).collect();
            json_pretty(&readable)
        }
        "repository_stats" => {
            let rels = relationships(store, scope);
            let stats = cgq::repository_stats(&rels);
            json_pretty(&stats)
        }

        // --- Temporal tools (need prior scan_repo) ---
        "temporal_recent" => {
            let versions = build_versions(store, scope);
            let now = chrono::Utc::now();
            let ranked = cgt::recent(&versions, now, 86400.0); // 1-day half-life
            let readable: Vec<Value> = ranked
                .iter()
                .take(20)
                .map(|(id, score)| {
                    let mut entry = resolve_symbol(id, &names);
                    entry["score"] = json!(score);
                    entry
                })
                .collect();
            json_pretty(&readable)
        }
        "temporal_impact" => {
            let versions = build_versions(store, scope);
            let ranked = cgt::impact(&versions);
            let readable: Vec<Value> = ranked
                .iter()
                .take(20)
                .map(|(id, score)| {
                    let mut entry = resolve_symbol(id, &names);
                    entry["score"] = json!(score);
                    entry
                })
                .collect();
            json_pretty(&readable)
        }
        "temporal_compound" => {
            let versions = build_versions(store, scope);
            let now = chrono::Utc::now();
            let ranked = cgt::compound(&versions, now, 86400.0);
            let readable: Vec<Value> = ranked
                .iter()
                .take(20)
                .map(|(id, score)| {
                    let mut entry = resolve_symbol(id, &names);
                    entry["score"] = json!(score);
                    entry
                })
                .collect();
            json_pretty(&readable)
        }
        "temporal_overview" => {
            let rels = relationships(store, scope);
            let labels = cgq::call_communities(&rels, 10);
            let stats = cgt::overview(&labels);
            json_pretty(&json!({
                "community_count": stats.community_count,
                "largest_community_size": stats.largest_community_size,
                "classified_symbols": labels.len()
            }))
        }
        "temporal_directional" => {
            let path = args["path"].as_str().unwrap_or(".").to_owned();
            let result = {
                let guard = directional.lock().unwrap_or_else(|e| e.into_inner());
                guard.get(&path).cloned().unwrap_or_default()
            };
            json_pretty(&json!({
                "added": result.added,
                "modified": result.modified,
                "removed": result.removed,
                "net_change": result.added as i64 - result.removed as i64,
                "note": "file-level change direction since the previous scan of this path"
            }))
        }
        "capability_report" => {
            let rels = relationships(store, scope);
            let stats = cgq::repository_stats(&rels);
            json_pretty(&json!({
                "server": "engram-codegraph",
                "version": "0.1.0",
                "tools_available": 23,
                "graph_indexed": stats.edge_count > 0,
                "node_count": stats.node_count,
                "edge_count": stats.edge_count,
                "tool_groups": {
                    "indexing": ["scan_repo"],
                    "impact": ["dead_code", "blast_radius", "dependency_path"],
                    "architecture": ["central_symbols", "bridge_symbols", "call_communities", "symbol_context"],
                    "quality": ["cyclomatic_complexity", "most_complex"],
                    "api_topology": ["find_endpoints", "find_api_calls", "match_api_topology"],
                    "processes": ["find_entry_points", "process_flow"],
                    "temporal": ["temporal_recent", "temporal_impact", "temporal_compound", "temporal_overview", "temporal_directional"],
                    "stats": ["repository_stats", "capability_report"]
                },
                "deferred": {
                    "temporal_novel": "needs per-symbol version history, blocked on the ADR-0018 retraction-mode decision (soft-delete version preservation)"
                }
            }))
        }
        "most_complex" => {
            let sources_val = &args["sources"];
            let sources: Vec<(String, String)> = if sources_val.is_array() {
                sources_val
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter_map(|item| {
                        let name = item["name"].as_str()?;
                        let source = item["source"].as_str()?;
                        Some((name.to_owned(), source.to_owned()))
                    })
                    .collect()
            } else {
                Vec::new()
            };
            let limit = args["limit"].as_u64().unwrap_or(20) as usize;
            json_pretty(&cgq::most_complex(&sources, limit))
        }
        "match_api_topology" => {
            let endpoints: Vec<cgq::HttpEndpoint> =
                serde_json::from_value(args["endpoints"].clone()).unwrap_or_default();
            let calls: Vec<String> =
                serde_json::from_value(args["calls"].clone()).unwrap_or_default();
            let matches = cgq::match_api_topology(&endpoints, &calls);
            let readable: Vec<Value> = matches
                .iter()
                .map(|(call, target)| json!({ "call": call, "endpoint": target }))
                .collect();
            json_pretty(&readable)
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

/// Builds VersionedSymbol list from entities + their graph degree for temporal scoring.
fn build_versions(store: &SqlKnowledgeStore, scope: &Scope) -> Vec<cgt::VersionedSymbol> {
    let entities = block_on(store.list_entities(scope)).unwrap_or_default();
    let rels = relationships(store, scope);
    let edges = cgq::call_edges(&rels);
    let in_deg = engram_graph_analytics::in_degree(&edges);
    let out_deg: HashMap<String, usize> = {
        let mut m = HashMap::new();
        for (src, _) in &edges {
            *m.entry(src.clone()).or_default() += 1;
        }
        m
    };
    entities
        .iter()
        .map(|e| {
            let id = e.id.to_string();
            cgt::VersionedSymbol {
                key: id,
                valid_from: e.valid_from,
                valid_until: e.valid_until,
                in_degree: *in_deg.get(e.name.as_str()).unwrap_or(&0),
                out_degree: *out_deg.get(e.name.as_str()).unwrap_or(&0),
            }
        })
        .collect()
}

/// Cached entity info for human-readable MCP output.
struct EntityInfo {
    name: String,
    kind: String,
    file: String,
    start_line: u32,
    end_line: u32,
}

/// Loads all entities and builds an ID → EntityInfo lookup with name, kind,
/// and file:line from source_refs.
fn entity_lookup(store: &SqlKnowledgeStore, scope: &Scope) -> HashMap<String, EntityInfo> {
    block_on(store.list_entities(scope))
        .unwrap_or_default()
        .iter()
        .map(|e| {
            let loc = e.source_refs.iter().find_map(|r| r.location.as_ref());
            let info = EntityInfo {
                name: e.name.clone(),
                kind: format!("{:?}", e.kind).to_lowercase(),
                file: loc.and_then(|l| l.path.clone()).unwrap_or_default(),
                start_line: loc.and_then(|l| l.start_line).unwrap_or(0),
                end_line: loc.and_then(|l| l.end_line).unwrap_or(0),
            };
            (e.id.to_string(), info)
        })
        .collect()
}

/// Resolves an entity ID to a human-readable JSON object with file:line.
fn resolve_symbol(id: &str, lookup: &HashMap<String, EntityInfo>) -> Value {
    match lookup.get(id) {
        Some(info) => {
            let mut entry = json!({
                "name": info.name,
                "kind": info.kind,
                "id": id,
            });
            if !info.file.is_empty() {
                entry["file"] = json!(info.file);
                if info.start_line > 0 {
                    entry["line"] = json!(info.start_line);
                    if info.end_line > info.start_line {
                        entry["endLine"] = json!(info.end_line);
                    }
                }
            }
            entry
        }
        None => json!({ "name": id, "kind": "unknown", "id": id }),
    }
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

/// Wraps a result list in a uniform `{count, results}` envelope so callers can
/// branch on magnitude/emptiness the same way for every list-returning tool.
fn envelope(results: &[Value]) -> String {
    json_pretty(&json!({ "count": results.len(), "results": results }))
}

/// Classifies a zero-caller symbol to help filter dead-code false positives.
/// A symbol named like an entry point or test is *probably not genuinely dead* —
/// it is reached via dynamic dispatch, framework wiring, or the test runner,
/// none of which appear as static `calls` edges. This collapses the dead-code
/// skill's manual "cross-reference find_entry_points" step into one call.
fn dead_code_class(name: &str) -> &'static str {
    match name {
        "main" | "run" | "start" | "handler" | "__main__" => "entry_point",
        _ if name.starts_with("test_")
            || name.starts_with("tests")
            || name.starts_with("it_")
            || name.starts_with("should_") =>
        {
            "test"
        }
        _ => "candidate",
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(items: &[(&str, &str)]) -> HashMap<String, String> {
        items
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn manifest_diff_first_scan_is_all_added() {
        // No prior manifest → every current file is "added".
        let prior = manifest(&[]);
        let current = manifest(&[("a.rs", "h1"), ("b.rs", "h2")]);
        let d = manifest_diff(&prior, &current);
        assert_eq!(
            d,
            cgt::DirectionalResult {
                added: 2,
                removed: 0,
                modified: 0
            }
        );
    }

    #[test]
    fn manifest_diff_unchanged_scan_is_zero() {
        // Same hashes → no change (the incremental skip-unchanged case).
        let prior = manifest(&[("a.rs", "h1"), ("b.rs", "h2")]);
        let current = manifest(&[("a.rs", "h1"), ("b.rs", "h2")]);
        assert_eq!(
            manifest_diff(&prior, &current),
            cgt::DirectionalResult::default()
        );
    }

    #[test]
    fn manifest_diff_classifies_add_remove_modify() {
        let prior = manifest(&[("a.rs", "h1"), ("b.rs", "h2"), ("c.rs", "h3")]);
        // a.rs unchanged, b.rs modified (hash differs), c.rs removed, d.rs added.
        let current = manifest(&[("a.rs", "h1"), ("b.rs", "h2-modified"), ("d.rs", "h4")]);
        let d = manifest_diff(&prior, &current);
        assert_eq!(
            d,
            cgt::DirectionalResult {
                added: 1,
                removed: 1,
                modified: 1
            }
        );
    }
}
