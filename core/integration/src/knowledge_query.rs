//! Engine-neutral query port for listing knowledge-graph records in a scope.
//!
//! [`KnowledgeRepository`](engram_knowledge::KnowledgeRepository) and
//! [`KnowledgeGraphRepository`](engram_knowledge::KnowledgeGraphRepository) are
//! write/lookup ports (put / get / delete / neighbors); they intentionally
//! expose no "list everything in scope" method. Code-intelligence composition —
//! and any caller that needs the full entity/edge set for a project — goes
//! through this port so it can route through the
//! [`EngramProvider`](crate::EngramProvider) instead of reaching into a concrete
//! store (the old `codegraph/mcp-server` reached into the concrete store
//! directly; this port removes that need).

use async_trait::async_trait;
use engram_domain::{
    ChunkId, DocumentId, KnowledgeChunk, KnowledgeEntity, KnowledgeGraph, KnowledgeRelationship,
    Scope,
};
use engram_runtime::CoreResult;

/// Read port: list the entities / relationships / chunks visible to a scope.
/// Lean chunk reference for embed/index paths (see
/// [`KnowledgeQuery::list_chunk_refs`]).
#[derive(Debug, Clone)]
pub struct ChunkRef {
    pub id: ChunkId,
    /// The chunk's provenance source string (scan delta filtering).
    pub source: String,
    /// `false` for empty-text chunks — excluded from embedding without
    /// needing the text itself.
    pub has_text: bool,
}

#[async_trait]
pub trait KnowledgeQuery: Send + Sync {
    /// All entities in `scope`.
    async fn list_entities(&self, scope: &Scope) -> CoreResult<Vec<KnowledgeEntity>>;

    /// All relationships in `scope`.
    async fn list_relationships(&self, scope: &Scope) -> CoreResult<Vec<KnowledgeRelationship>>;

    /// All chunks in `scope` (for embedding/indexing). Default: empty (not supported).
    async fn list_chunks(&self, _scope: &Scope) -> CoreResult<Vec<KnowledgeChunk>> {
        Ok(Vec::new())
    }

    /// Lean chunk listing for embed/index paths: id + provenance source +
    /// text-nonemptiness per chunk in scope — no text transfer, no full
    /// record deserialization. The embed pending-set computation needs
    /// exactly this; materializing full chunk records (text included) for
    /// 90k+ chunks cost ~1.4s and hundreds of MB per call. Default: derived
    /// from [`Self::list_chunks`] (correct everywhere, efficient where the
    /// store overrides it).
    async fn list_chunk_refs(&self, scope: &Scope) -> CoreResult<Vec<ChunkRef>> {
        Ok(self
            .list_chunks(scope)
            .await?
            .into_iter()
            .map(|c| ChunkRef {
                id: c.id,
                source: c.provenance.source,
                has_text: !c.text.is_empty(),
            })
            .collect())
    }

    /// All graphs in `scope`. Default: empty (not supported).
    async fn list_graphs(&self, _scope: &Scope) -> CoreResult<Vec<KnowledgeGraph>> {
        Ok(Vec::new())
    }

    /// The chunks of one `document_id` within `scope` (bounded per-document read,
    /// used by the extract-knowledge op). Default: empty (not supported).
    async fn list_chunks_by_document(
        &self,
        _document_id: &DocumentId,
        _scope: &Scope,
    ) -> CoreResult<Vec<KnowledgeChunk>> {
        Ok(Vec::new())
    }
}
