//! Code-intelligence tools (RFC-0015 Phase 2 + 3): `scan_repo` + the composites
//! + `search` + `get_context`. `scan_repo` uses a fan-in adapter so treesitter
//! ingestion routes through the provider's handles (no engine-store bypass).

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use engram_domain::{
    ChunkId, EntityId, KnowledgeChunk, KnowledgeEntity, KnowledgeGraph, KnowledgeGraphId,
    KnowledgeRelationship, KnowledgeSource, RelationshipId, RetrievalRequest, Scope,
    SourceDocument,
};
use engram_ingest::{ScanOptions, scan_repository};
use engram_knowledge::{CoreResult, KnowledgeGraphRepository, KnowledgeRepository};
use futures::executor::block_on;
use serde_json::Value;

use crate::app::App;
use crate::protocol;
use crate::registry::ToolError;
use crate::tools::{internal, policy, req_str, system_actor};

/// Fan-in adapter combining the provider's separate knowledge + graph handles
/// into one type implementing both traits, so [`scan_repository`] (which needs a
/// single `R: KnowledgeRepository + KnowledgeGraphRepository + Send + Sync`)
/// runs against the provider-backed stores — no direct concrete-store access.
pub(crate) struct KnowledgeRepoGraph {
    knowledge: Arc<dyn KnowledgeRepository>,
    graph: Arc<dyn KnowledgeGraphRepository>,
}

impl KnowledgeRepoGraph {
    pub(crate) fn new(
        knowledge: Arc<dyn KnowledgeRepository>,
        graph: Arc<dyn KnowledgeGraphRepository>,
    ) -> Self {
        Self { knowledge, graph }
    }
}

#[async_trait]
impl KnowledgeRepository for KnowledgeRepoGraph {
    async fn put_source(&self, source: KnowledgeSource) -> CoreResult<KnowledgeSource> {
        self.knowledge.put_source(source).await
    }
    async fn put_document(&self, document: SourceDocument) -> CoreResult<SourceDocument> {
        self.knowledge.put_document(document).await
    }
    async fn put_chunk(&self, chunk: KnowledgeChunk) -> CoreResult<KnowledgeChunk> {
        self.knowledge.put_chunk(chunk).await
    }
    async fn get_chunk(&self, id: &ChunkId, scope: &Scope) -> CoreResult<Option<KnowledgeChunk>> {
        self.knowledge.get_chunk(id, scope).await
    }
    async fn put_entity(&self, entity: KnowledgeEntity) -> CoreResult<KnowledgeEntity> {
        self.knowledge.put_entity(entity).await
    }
    async fn put_relationship(
        &self,
        relationship: KnowledgeRelationship,
    ) -> CoreResult<KnowledgeRelationship> {
        self.knowledge.put_relationship(relationship).await
    }
    async fn get_entity(
        &self,
        id: &EntityId,
        scope: &Scope,
    ) -> CoreResult<Option<KnowledgeEntity>> {
        self.knowledge.get_entity(id, scope).await
    }
    async fn get_relationship(
        &self,
        id: &RelationshipId,
        scope: &Scope,
    ) -> CoreResult<Option<KnowledgeRelationship>> {
        self.knowledge.get_relationship(id, scope).await
    }
    async fn delete_entity(&self, id: &EntityId, scope: &Scope) -> CoreResult<bool> {
        self.knowledge.delete_entity(id, scope).await
    }
    async fn delete_relationship(&self, id: &RelationshipId, scope: &Scope) -> CoreResult<bool> {
        self.knowledge.delete_relationship(id, scope).await
    }
}

#[async_trait]
impl KnowledgeGraphRepository for KnowledgeRepoGraph {
    async fn put_graph(&self, graph: KnowledgeGraph) -> CoreResult<KnowledgeGraph> {
        self.graph.put_graph(graph).await
    }
    async fn get_graph(
        &self,
        id: &KnowledgeGraphId,
        scope: &Scope,
    ) -> CoreResult<Option<KnowledgeGraph>> {
        self.graph.get_graph(id, scope).await
    }
    async fn neighbors(
        &self,
        graph_id: &KnowledgeGraphId,
        node_id: &EntityId,
        scope: &Scope,
        limit: Option<u32>,
    ) -> CoreResult<Vec<KnowledgeRelationship>> {
        self.graph.neighbors(graph_id, node_id, scope, limit).await
    }
    async fn delete_graph(&self, id: &KnowledgeGraphId, scope: &Scope) -> CoreResult<bool> {
        self.graph.delete_graph(id, scope).await
    }
    async fn list_graphs_by_source(
        &self,
        scope: &Scope,
        stable_source_key: &str,
    ) -> CoreResult<Vec<KnowledgeGraph>> {
        self.graph
            .list_graphs_by_source(scope, stable_source_key)
            .await
    }
}

/// `scan_repo`: treesitter-index a code repository into the project workspace,
/// routed through the provider via the fan-in adapter. Feeds code-symbol names
/// to the lexical lane so `search`/`recall` find them.
pub fn scan_repo(app: &App, args: &Value) -> Result<Value, ToolError> {
    let path = req_str(args, "path")?;
    let knowledge = app.provider.require_knowledge().map_err(internal)?.clone();
    let graph = app.provider.require_graph().map_err(internal)?.clone();
    let repo = KnowledgeRepoGraph::new(knowledge, graph);

    let opts = ScanOptions {
        scope: app.scope.clone(),
        policy: policy(),
        actor: system_actor(),
        source_name: "engram-mcp-scan".to_owned(),
        max_bytes: 0,
        manifest: HashMap::new(),
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

    Ok(protocol::text_content(format!("{summary:?}")))
}

// --- composites (T2–T8) ------------------------------------------------------

/// Fetch all relationships in the project scope (returns empty vec on error).
pub(crate) fn fetch_rels(app: &App) -> Vec<KnowledgeRelationship> {
    app.provider
        .require_knowledge_query()
        .ok()
        .and_then(|q| block_on(q.list_relationships(&app.scope)).ok())
        .unwrap_or_default()
}

/// `search`: keyword search over indexed code symbols by entity name/kind.
/// Uses `KnowledgeQuery` directly (list entities, filter by substring) — no
/// dependency on the lexical resolver (which is chunk-based; entity-ID hits
/// would be dropped). The `LexicalFeed` remains wired for future BM25 ranking
/// once an entity-id resolver lane is added.
pub fn search(app: &App, args: &Value) -> Result<Value, ToolError> {
    let query = req_str(args, "query")?;
    let limit = args["limit"]
        .as_u64()
        .map(|n| n as usize)
        .unwrap_or(10)
        .clamp(1, 100);
    let knowledge_query = app.provider.require_knowledge_query().map_err(internal)?;
    let entities = block_on(knowledge_query.list_entities(&app.scope)).unwrap_or_default();
    let needle = query.to_lowercase();
    let matches: Vec<String> = entities
        .iter()
        .filter(|e| {
            let haystack = format!("{} {:?}", e.name, e.kind).to_lowercase();
            haystack.contains(&needle)
        })
        .take(limit)
        .map(|e| format!("{} ({:?})", e.name, e.kind))
        .collect();
    let body = if matches.is_empty() {
        "No results.".to_owned()
    } else {
        matches.join("\n")
    };
    Ok(protocol::text_content(body))
}

/// `symbol_context`: callers, callees, and community for one symbol.
pub fn symbol_context(app: &App, args: &Value) -> Result<Value, ToolError> {
    let symbol = req_str(args, "symbol")?;
    let depth = args["depth"].as_u64().unwrap_or(2) as usize;
    let rels = fetch_rels(app);
    let ctx = engram_codegraph_queries::symbol_context(&rels, symbol, depth);
    Ok(protocol::text_content(format!("{ctx:?}")))
}

/// `change_impact`: blast radius + dependency path from a change site.
pub fn change_impact(app: &App, args: &Value) -> Result<Value, ToolError> {
    let target = req_str(args, "target")?;
    let depth = args["depth"].as_u64().unwrap_or(3) as usize;
    let rels = fetch_rels(app);
    let radius = engram_codegraph_queries::blast_radius(&rels, target, depth);
    let path = args["to"]
        .as_str()
        .and_then(|to| engram_codegraph_queries::dependency_path(&rels, target, to));
    Ok(protocol::text_content(format!(
        "Blast radius ({depth} hops): {radius:?}\nDependency path: {path:?}"
    )))
}

/// `code_health`: dead code (zero-caller symbols) + repository stats.
pub fn code_health(app: &App, _args: &Value) -> Result<Value, ToolError> {
    let rels = fetch_rels(app);
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
    let rels = fetch_rels(app);
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
    let rels = fetch_rels(app);
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
    let rels = fetch_rels(app);
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
    let graph_text = if links.is_empty() {
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
