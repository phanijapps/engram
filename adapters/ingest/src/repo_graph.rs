//! Fan-in adapter combining a provider's separate knowledge + graph handles into
//! one type implementing both traits.
//!
//! [`crate::scan_repository`] requires a single
//! `R: KnowledgeRepository + KnowledgeGraphRepository + Send + Sync`. A wired
//! [`EngramProvider`](engram_integration::EngramProvider) exposes those as two
//! separate `Arc<dyn ...>` handles; this adapter fans them in so scan (and any
//! other caller that needs both) runs against the provider-backed stores — no
//! direct concrete-store access, no engine bypass.

use std::sync::Arc;

use async_trait::async_trait;
use engram_domain::{
    ChunkId, EntityId, KnowledgeChunk, KnowledgeEntity, KnowledgeGraph, KnowledgeGraphId,
    KnowledgeRelationship, KnowledgeSource, RelationshipId, Scope, SourceDocument,
};
use engram_knowledge::{CoreResult, KnowledgeGraphRepository, KnowledgeRepository};

/// Combines `KnowledgeRepository` + `KnowledgeGraphRepository` handles behind a
/// single type that implements both — the shape `scan_repository` consumes.
pub struct KnowledgeRepoGraph {
    knowledge: Arc<dyn KnowledgeRepository>,
    graph: Arc<dyn KnowledgeGraphRepository>,
}

impl KnowledgeRepoGraph {
    pub fn new(
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
