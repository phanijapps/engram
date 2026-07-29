//! Code-intelligence tools (RFC-0015 Phase 2): `scan_repo` + the composites +
//! `search`. `scan_repo` uses a fan-in adapter so treesitter ingestion routes
//! through the provider's separate knowledge + graph handles (no direct
//! `SqlKnowledgeStore`, unlike the old `codegraph/mcp-server`).

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use engram_domain::{
    ChunkId, EntityId, KnowledgeChunk, KnowledgeEntity, KnowledgeGraph, KnowledgeGraphId,
    KnowledgeRelationship, KnowledgeSource, RelationshipId, Scope, SourceDocument,
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
/// runs against the provider-backed stores — no direct `SqlKnowledgeStore`.
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
            let _ = block_on(feed.upsert_batch(&entries));
        }
    }

    Ok(protocol::text_content(format!("{summary:?}")))
}
