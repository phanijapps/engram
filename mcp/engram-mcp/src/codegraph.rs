//! Code-intelligence tools (RFC-0015 Phase 2 + 3): `scan_repo` + the composites
//! + `search` + `get_context`.
//!
//! `scan_repo` uses a fan-in adapter so treesitter ingestion routes through
//! the provider's handles (no engine-store bypass).

use engram_domain::{
    KnowledgeEntity, KnowledgeRelationship, RetrievalRequest, RetrievalTargetType,
};
use engram_ingest::{
    KnowledgeRepoGraph, ScanFilter, ScanFilterConfig, ScanOptions, scan_repository,
};
use futures::executor::block_on;
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

use crate::app::App;
use crate::protocol;
use crate::registry::ToolError;
use crate::tools::{internal, policy, req_str, requester, system_actor};

/// Default per-direction visited cap for bounded code-graph neighborhoods. The
/// depth defaults (`symbol_context`=1, `change_impact`=2) are the primary flood
/// bound; this cap is the safety net for raised-depth or super-hub queries.
const DEFAULT_NEIGHBORHOOD_CAP: usize = 64;

/// `scan_repo`: treesitter-index a code repository into the project workspace,
/// routed through the provider via the fan-in adapter. Feeds code-symbol names
/// to the lexical lane so `search`/`recall` find them.
pub fn scan_repo(app: &App, args: &Value) -> Result<Value, ToolError> {
    let path = req_str(args, "path")?;
    let knowledge = app.provider.require_knowledge().map_err(internal)?.clone();
    let graph = app.provider.require_graph().map_err(internal)?.clone();
    let repo = KnowledgeRepoGraph::new(knowledge, graph);

    let (scan_filter, filter_note) = resolve_scan_filter(path, args);
    let opts = ScanOptions {
        scope: app.scope.clone(),
        policy: policy(),
        actor: system_actor(),
        source_name: "engram-mcp-scan".to_owned(),
        max_bytes: 0,
        manifest: HashMap::new(),
        scan_filter,
    };
    let (summary, _manifest) =
        scan_repository(std::path::Path::new(path), &opts, &repo, |_| ()).map_err(internal)?;

    // Feed code-symbol names to the lexical lane so keyword search finds them.
    if let (Ok(query), Ok(feed)) = (
        app.provider.require_knowledge_query(),
        app.provider.require_lexical_feed(),
    ) {
        let entries: Vec<(String, String)> = block_on(query.list_entities(&app.scope))
            .unwrap_or_default()
            .into_iter()
            .map(|e| (e.id.to_string(), format!("{} {:?}", e.name, e.kind)))
            .collect();
        if !entries.is_empty() {
            if let Err(e) = block_on(feed.upsert_batch(&entries)) {
                // Non-fatal: search may return no code symbols, but the scan itself succeeded.
                eprintln!("engram-mcp: lexical feed warning: {e}");
            }
        }
    }

    // Embed chunks into the vector index (when fastembed is wired).
    //
    // Incremental + batched (indexing-embed-performance): only chunks whose id
    // is NOT already in the vector index are embedded, and the embedding work
    // runs in batches through `embed_batch` (one FastEmbed model call per batch
    // instead of one per chunk). Re-scanning an unchanged repo embeds ~0 chunks;
    // adding a repo to a populated DB embeds only the new repo's chunks.
    let mut embedded = 0usize;
    if let (Ok(query), Some(embedder), Ok(vector_index)) = (
        app.provider.require_knowledge_query(),
        app.provider.embedding_provider(),
        app.provider.require_vectors(),
    ) {
        let chunks = block_on(query.list_chunks(&app.scope)).unwrap_or_default();
        let space = embedder.embedding_space();

        // Skip chunks that already have a vector — the incremental win. A
        // failed listing degrades to "embed everything" (current behavior).
        let have: std::collections::HashSet<engram_domain::Id> =
            block_on(vector_index.embedded_ids()).unwrap_or_default();
        let pending: Vec<&engram_domain::KnowledgeChunk> = chunks
            .iter()
            .filter(|c| !c.text.is_empty() && !have.contains(&c.id))
            .collect();

        const EMBED_BATCH_SIZE: usize = 64;
        for batch in pending.chunks(EMBED_BATCH_SIZE) {
            let texts: Vec<String> = batch.iter().map(|c| c.text.clone()).collect();
            // Skip empty-text chunks defensively (the filter above already
            // dropped them) and warn per-chunk on insert errors, as before.
            match embedder.embed_batch(&texts) {
                Ok(vectors) => {
                    for (chunk, vector) in batch.iter().zip(vectors.into_iter()) {
                        if vector.is_empty() {
                            // Mirrors the skip-empty-text behavior for slots
                            // the batch path left empty.
                            continue;
                        }
                        if let Err(e) = block_on(vector_index.insert(&chunk.id, &space, vector)) {
                            eprintln!("engram-mcp: embed warning for {}: {e}", chunk.id);
                        } else {
                            embedded += 1;
                        }
                    }
                }
                Err(e) => {
                    // Whole batch failed — do not abort the scan; warn and move
                    // on so a single bad batch never blocks indexing.
                    for chunk in batch {
                        eprintln!("engram-mcp: embed error for {}: {e}", chunk.id);
                    }
                }
            }
        }
    }

    Ok(protocol::text_content(format!(
        "{summary:?}\n{filter_note}\nembedded {embedded} chunks"
    )))
}

/// Load + merge a scan filter from a JSON config file.
fn try_load_scan_filter(path: &Path) -> Result<ScanFilter, String> {
    let json =
        std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let cfg =
        ScanFilterConfig::from_json(&json).map_err(|e| format!("parse {}: {e}", path.display()))?;
    Ok(ScanFilter::merge(&cfg))
}

/// Resolve the scan filter via the discovery ladder:
///   1. an explicit `scan_config` path arg, else
///   2. `<repo>/.engram/scan.json` if it exists, else
///   3. the builtin defaults.
///
/// Returns the filter + a short note for the result text. Never errors — a
/// missing/malformed config soft-fails to the builtin so a bad config file
/// never aborts a scan.
fn resolve_scan_filter(repo_root: &str, args: &Value) -> (ScanFilter, String) {
    // (1) explicit arg wins.
    if let Some(explicit) = args.get("scan_config").and_then(|v| v.as_str()) {
        return match try_load_scan_filter(Path::new(explicit)) {
            Ok(filter) => (filter, format!("scan_config applied: {explicit}")),
            Err(e) => (
                ScanFilter::default(),
                format!("scan_config ignored ({e}); builtin filter"),
            ),
        };
    }
    // (2) repo-local <root>/.engram/scan.json. Read directly (no `exists()`
    // probe) so a transient removal between probe and read can't produce a
    // misleading "ignored" note; NotFound simply means no config.
    let discovered = Path::new(repo_root).join(".engram").join("scan.json");
    match std::fs::read_to_string(&discovered) {
        Ok(json) => match ScanFilterConfig::from_json(&json) {
            Ok(cfg) => {
                return (
                    ScanFilter::merge(&cfg),
                    ".engram/scan.json applied".to_owned(),
                );
            }
            Err(e) => {
                return (
                    ScanFilter::default(),
                    format!(".engram/scan.json ignored (parse {e}); builtin filter"),
                );
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => { /* no config; fall through */ }
        Err(e) => {
            return (
                ScanFilter::default(),
                format!(".engram/scan.json ignored (read {e}); builtin filter"),
            );
        }
    }
    // (3) builtin.
    (
        ScanFilter::default(),
        "no scan config; builtin filter".to_owned(),
    )
}

// --- composites (T2–T8) ------------------------------------------------------

/// Fetch all relationships in the project scope. Errors (rather than returning
/// an empty vec) when the knowledge-query capability is unwired or the store
/// fails — so graph tools fail loudly instead of masquerading as "no relations".
pub(crate) fn fetch_rels(app: &App) -> Result<Vec<KnowledgeRelationship>, ToolError> {
    let query = app
        .provider
        .require_knowledge_query()
        .map_err(|_| internal("knowledge query capability not configured"))?;
    block_on(query.list_relationships(&app.scope))
        .map_err(|e| internal(format!("knowledge query failed: {e}")))
}

/// `search`: ranked code-symbol search over indexed entities.
///
/// Routes through the unified (hybrid) recall — lexical (BM25) + graph +
/// associative-graph + community-summary lanes fuse over weighted RRF — so
/// multi-term and natural-language queries (e.g. `"reciprocal rank fusion"`)
/// return ranked symbol hits. The lexical lane resolves entity-id BM25 hits to
/// their code symbol (the resolver is entity-aware), so symbols indexed by
/// `scan_repo` are reachable. Replaces the prior whole-string `.contains()`
/// loop, which missed any query that was not a verbatim substring of
/// `"{name} {kind}"` (the §6.3 defect: `search "reciprocal rank fusion"` →
/// "No results").
///
/// Backward compatible: when recall is unavailable, search degrades to the
/// direct entity-list scan (the old path) rather than erroring.
pub fn search(app: &App, args: &Value) -> Result<Value, ToolError> {
    let query = req_str(args, "query")?;
    let limit = args["limit"]
        .as_u64()
        .map(|n| n as usize)
        .unwrap_or(10)
        .clamp(1, 100);

    // Prefer the hybrid-recall path. Each recall item carries the fused rank;
    // entity (code-symbol) hits are kept and rendered as `name (kind)`.
    let hits: Vec<String> = match app.provider.require_recall() {
        Ok(handle) => {
            let request = RetrievalRequest {
                query: query.to_owned(),
                scope: app.scope.clone(),
                requester: requester(),
                modes: Vec::new(),
                filters: None,
                cues: Vec::new(),
                limit: Some(limit as u32),
                budget: None,
                include_explanations: Some(true),
            };
            let payload = block_on(handle.recall(request)).map_err(internal)?;
            // Resolve target_ids back to entities for a consistent `name (kind)`
            // rendering regardless of which lane produced the hit (lexical
            // content = "name Kind"; graph content = name). Falls back to the
            // item's resolved content if the entity is no longer present.
            let by_id = entity_lookup(app);
            payload
                .items
                .iter()
                .filter(|i| i.target_type == RetrievalTargetType::Entity)
                .filter_map(|i| match by_id.get(&i.target_id) {
                    Some(e) => Some(format!("{} ({:?})", e.name, e.kind)),
                    None if !i.content.is_empty() => Some(i.content.clone()),
                    None => None,
                })
                .take(limit)
                .collect()
        }
        Err(_) => {
            // Degrade to a direct scan when recall is not wired (e.g. a
            // capability-check failure) so search still works without hybrid.
            substring_symbol_scan(app, query, limit)
        }
    };

    let body = if hits.is_empty() {
        "No results.".to_owned()
    } else {
        hits.join("\n")
    };
    Ok(protocol::text_content(body))
}

/// Builds an `entity_id → KnowledgeEntity` lookup over the project scope for
/// consistent symbol rendering. Empty when the knowledge-query capability is
/// unavailable (callers fall back to recall item content).
fn entity_lookup(app: &App) -> HashMap<String, KnowledgeEntity> {
    app.provider
        .require_knowledge_query()
        .ok()
        .and_then(|q| block_on(q.list_entities(&app.scope)).ok())
        .unwrap_or_default()
        .into_iter()
        .map(|e| (e.id.to_string(), e))
        .collect()
}

/// Fallback symbol scan (the pre-hybrid path): lists entities and keeps those
/// whose `"{name} {kind}"` contains the query as a substring. Used only when
/// unified recall is unavailable.
fn substring_symbol_scan(app: &App, query: &str, limit: usize) -> Vec<String> {
    let knowledge_query = match app.provider.require_knowledge_query() {
        Ok(q) => q,
        Err(_) => return Vec::new(),
    };
    let entities = block_on(knowledge_query.list_entities(&app.scope)).unwrap_or_default();
    let needle = query.to_lowercase();
    entities
        .iter()
        .filter(|e| {
            format!("{} {:?}", e.name, e.kind)
                .to_lowercase()
                .contains(&needle)
        })
        .take(limit)
        .map(|e| format!("{} ({:?})", e.name, e.kind))
        .collect()
}

/// `symbol_context`: callers, callees, and community for one symbol.
pub fn symbol_context(app: &App, args: &Value) -> Result<Value, ToolError> {
    let symbol = req_str(args, "symbol")?;
    let depth = args["depth"].as_u64().unwrap_or(1) as usize;
    let cap = args["cap"]
        .as_u64()
        .unwrap_or(DEFAULT_NEIGHBORHOOD_CAP as u64) as usize;
    let rels = fetch_rels(app)?;
    let ctx = engram_codegraph_queries::symbol_context_bounded(&rels, symbol, depth, cap);
    Ok(protocol::text_content(format!("{ctx:?}")))
}

/// `change_impact`: blast radius + dependency path from a change site.
pub fn change_impact(app: &App, args: &Value) -> Result<Value, ToolError> {
    let target = req_str(args, "target")?;
    let depth = args["depth"].as_u64().unwrap_or(2) as usize;
    let cap = args["cap"]
        .as_u64()
        .unwrap_or(DEFAULT_NEIGHBORHOOD_CAP as u64) as usize;
    let rels = fetch_rels(app)?;
    let radius = engram_codegraph_queries::blast_radius_bounded(&rels, target, depth, cap);
    let path = args["to"]
        .as_str()
        .and_then(|to| engram_codegraph_queries::dependency_path(&rels, target, to));
    Ok(protocol::text_content(format!(
        "Blast radius ({depth} hops, cap {cap}): {radius:?}\nDependency path: {path:?}"
    )))
}

/// `code_health`: dead code (zero-caller symbols) + repository stats.
pub fn code_health(app: &App, _args: &Value) -> Result<Value, ToolError> {
    let rels = fetch_rels(app)?;
    let dead = engram_codegraph_queries::dead_code(&rels);
    let stats = engram_codegraph_queries::repository_stats(&rels);
    Ok(protocol::text_content(format!(
        "Dead code ({} symbols): {dead:?}\nStats: {stats:?}",
        dead.len()
    )))
}

/// `architecture`: central symbols, bridges, communities, stats — one map.
pub fn architecture(app: &App, args: &Value) -> Result<Value, ToolError> {
    let limit = args["limit"].as_u64().unwrap_or(10) as usize;
    let rels = fetch_rels(app)?;
    let central = engram_codegraph_queries::central_symbols(&rels, limit);
    let bridges = engram_codegraph_queries::bridge_symbols(&rels, limit);
    let communities = engram_codegraph_queries::call_communities(&rels, 3);
    let stats = engram_codegraph_queries::repository_stats(&rels);
    Ok(protocol::text_content(format!(
        "Central: {central:?}\nBridges: {bridges:?}\nCommunities: {communities:?}\nStats: {stats:?}"
    )))
}

/// `whats_changed`: temporal recency + impact + compound + overview.
/// (`directional` deferred — needs scan-baseline retention.)
pub fn whats_changed(app: &App, _args: &Value) -> Result<Value, ToolError> {
    let query = app.provider.require_knowledge_query().map_err(internal)?;
    let entities = block_on(query.list_entities(&app.scope)).unwrap_or_default();
    let rels = fetch_rels(app)?;
    let versions: Vec<engram_codegraph_temporal::VersionedSymbol> = entities
        .iter()
        .map(|e| {
            let name = e.name.as_str();
            engram_codegraph_temporal::VersionedSymbol {
                key: name.to_owned(),
                valid_from: e.valid_from,
                valid_until: e.valid_until,
                in_degree: rels
                    .iter()
                    .filter(|r| r.object.name.as_deref() == Some(name))
                    .count(),
                out_degree: rels
                    .iter()
                    .filter(|r| r.subject.name.as_deref() == Some(name))
                    .count(),
            }
        })
        .collect();
    let now = chrono::Utc::now();
    let recent = engram_codegraph_temporal::recent(&versions, now, 14.0);
    let impact = engram_codegraph_temporal::impact(&versions);
    let compound = engram_codegraph_temporal::compound(&versions, now, 14.0);
    let communities = engram_codegraph_queries::call_communities(&rels, 3);
    let overview = engram_codegraph_temporal::overview(&communities);
    Ok(protocol::text_content(format!(
        "Recent: {recent:?}\nImpact: {impact:?}\nCompound: {compound:?}\nOverview: {overview:?}"
    )))
}

// --- Phase 3 -----------------------------------------------------------------

/// `get_context`: compose a task-aware context packet for a focus (symbol, file,
/// concept, or free-text). Fuses recall (docs + memories + beliefs) with the
/// code neighborhood (callers/callees/community) — a pragmatic first version of
/// RFC-0013's `ContextSubgraph` packet, built on existing capabilities.
pub fn get_context(app: &App, args: &Value) -> Result<Value, ToolError> {
    let focus = req_str(args, "focus")?;
    let depth = args["depth"].as_u64().unwrap_or(2) as usize;
    let limit = args["limit"].as_u64().unwrap_or(10).clamp(1, 50) as u32;

    // 1. Fused recall (docs + memories + beliefs matching the focus).
    let recall_text = match app.provider.require_recall() {
        Ok(handle) => {
            let req = RetrievalRequest {
                query: focus.to_owned(),
                scope: app.scope.clone(),
                requester: crate::tools::requester(),
                modes: Vec::new(),
                filters: None,
                cues: Vec::new(),
                limit: Some(limit),
                budget: None,
                include_explanations: Some(true),
            };
            let payload = block_on(handle.recall(req)).map_err(internal)?;
            let items: Vec<&str> = payload
                .items
                .iter()
                .take(limit as usize)
                .map(|i| i.content.as_str())
                .collect();
            if items.is_empty() {
                String::new()
            } else {
                items.join("\n---\n")
            }
        }
        Err(_) => String::new(),
    };

    // 2. Code neighborhood (callers/callees/community for the focus symbol).
    let (rels, graph_status) = match fetch_rels(app) {
        Ok(r) => (r, String::new()),
        Err(e) => (Vec::new(), format!("(graph unavailable: {})", e.message)),
    };
    let code_ctx = engram_codegraph_queries::symbol_context(&rels, focus, depth);

    // 3. Unified-graph links — doc/concept `describes`/`mentions` edges for the
    //    focus, on top of the code neighborhood. This is the doc↔code connection
    //    surfacing in one context packet.
    let links: Vec<String> = rels
        .iter()
        .filter_map(|r| {
            let (s, o) = (r.subject.name.as_deref()?, r.object.name.as_deref()?);
            if s == focus {
                Some(format!("{focus} -[{}]-> {o}", r.predicate))
            } else if o == focus {
                Some(format!("{s} -[{}]-> {focus}", r.predicate))
            } else {
                None
            }
        })
        .take(20)
        .collect();
    let graph_text = if !graph_status.is_empty() {
        graph_status
    } else if links.is_empty() {
        "(none)".to_owned()
    } else {
        links.join("\n")
    };

    Ok(protocol::text_content(format!(
        "=== Context for '{focus}' ===\n\n[Recall]\n{recall_text}\n\n[Graph]\n{graph_text}\n\n[Code]\n{code_ctx:?}"
    )))
}

/// `capability_report`: report which provider capabilities are wired.
pub fn capability_report(app: &App, _args: &Value) -> Result<Value, ToolError> {
    let caps: Vec<String> = [
        ("memory", app.provider.memory().is_some()),
        ("knowledge", app.provider.knowledge().is_some()),
        ("graph", app.provider.graph().is_some()),
        ("knowledge_query", app.provider.knowledge_query().is_some()),
        ("lexical_feed", app.provider.lexical_feed().is_some()),
        ("recall", app.provider.recall().is_some()),
        ("consolidation", app.provider.consolidation().is_some()),
        ("batch", app.provider.batch().is_some()),
        ("ontology", app.provider.ontology().is_some()),
        ("taxonomy", app.provider.taxonomy().is_some()),
        ("beliefs", app.provider.beliefs().is_some()),
        ("procedures", app.provider.procedures().is_some()),
        ("hierarchy", app.provider.hierarchy().is_some()),
        ("identity", app.provider.identity().is_some()),
    ]
    .into_iter()
    .map(|(name, ok)| format!("  {name}: {}", if ok { "supported" } else { "unsupported" }))
    .collect();
    Ok(protocol::text_content(format!(
        "Server: engram-mcp 0.1.0\nCapabilities:\n{}",
        caps.join("\n")
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tmp_root(label: &str) -> std::path::PathBuf {
        let root =
            std::env::temp_dir().join(format!("engram-scan-cfg-{}-{label}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join(".engram")).unwrap();
        root
    }

    /// `api` is 3 chars → builtin rejects it. An allowlist entry forces it on,
    /// which is the observable signal that a config was applied.
    const ALLOW_API_CFG: &str = r#"{ "concepts": { "allowlist": ["api"] } }"#;

    #[test]
    fn resolve_uses_explicit_scan_config_arg() {
        let root = tmp_root("explicit");
        let cfg_path = root.join("my-scan.json");
        std::fs::write(&cfg_path, ALLOW_API_CFG).unwrap();

        let args = json!({ "scan_config": cfg_path.to_string_lossy() });
        let (filter, note) = resolve_scan_filter(root.to_str().unwrap(), &args);

        assert!(filter.should_link_concept("api"), "allowlist applied");
        assert!(note.contains("scan_config applied"), "note: {note}");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn resolve_discovers_repo_local_config() {
        let root = tmp_root("discovered");
        // No `scan_config` arg, but <root>/.engram/scan.json exists.
        std::fs::write(root.join(".engram").join("scan.json"), ALLOW_API_CFG).unwrap();

        let args = json!({});
        let (filter, note) = resolve_scan_filter(root.to_str().unwrap(), &args);

        assert!(
            filter.should_link_concept("api"),
            "discovered config applied"
        );
        assert!(note.contains(".engram/scan.json applied"), "note: {note}");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn resolve_falls_back_to_builtin_when_absent() {
        let root = tmp_root("builtin");
        let args = json!({});
        let (filter, note) = resolve_scan_filter(root.to_str().unwrap(), &args);

        // Builtin rejects short names like "api".
        assert!(!filter.should_link_concept("api"));
        assert!(note.contains("builtin filter"), "note: {note}");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn resolve_soft_fails_on_malformed_config() {
        let root = tmp_root("malformed");
        std::fs::write(root.join(".engram").join("scan.json"), "{ not json").unwrap();

        let args = json!({});
        let (filter, note) = resolve_scan_filter(root.to_str().unwrap(), &args);

        // Soft-fail → builtin (rejects "api"), note explains the ignore.
        assert!(!filter.should_link_concept("api"));
        assert!(note.contains("ignored"), "note: {note}");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn resolve_explicit_arg_overrides_discovered() {
        // Both an explicit arg AND a discovered file exist; the arg wins and the
        // discovered file's content must NOT leak into the filter. The
        // discovered file is given a VALID sentinel (an allowlist entry that
        // would force-link "api" if it leaked) so any leak fails the test —
        // using a broken discovered file would not prove read-order priority.
        let root = tmp_root("override");
        std::fs::write(
            root.join(".engram").join("scan.json"),
            r#"{ "concepts": { "allowlist": ["api"] } }"#,
        )
        .unwrap();
        let cfg_path = root.join("explicit.json");
        // Blocklist a name the builtin accepts, to prove the explicit file loaded.
        std::fs::write(
            &cfg_path,
            r#"{ "concepts": { "blocklist": ["RetrievalIndex"] } }"#,
        )
        .unwrap();

        let args = json!({ "scan_config": cfg_path.to_string_lossy() });
        let (filter, note) = resolve_scan_filter(root.to_str().unwrap(), &args);

        assert!(
            !filter.should_link_concept("RetrievalIndex"),
            "explicit blocklist applied"
        );
        assert!(
            !filter.should_link_concept("api"),
            "discovered allowlist must not leak (api stays builtin-rejected)"
        );
        assert!(note.contains("scan_config applied"), "note: {note}");

        let _ = std::fs::remove_dir_all(&root);
    }
}
