//! SQLite-backed [`KnowledgeQuery`] over [`SqlKnowledgeStore`]'s inherent list
//! methods.
//!
//! Engine-specific (names `SqlKnowledgeStore`, gated behind the `sqlite`
//! feature). The [`KnowledgeQuery`] trait itself (in the parent crate's
//! [`knowledge_query`](crate::knowledge_query) module) stays engine-neutral.

use async_trait::async_trait;
use engram_domain::{DocumentId, KnowledgeChunk, Scope};
use engram_runtime::CoreResult;
use engram_store_sqlite::SqlKnowledgeStore;

use crate::knowledge_query::{ChunkRef, KnowledgeQuery};

#[async_trait]
impl KnowledgeQuery for SqlKnowledgeStore {
    async fn list_entities(
        &self,
        scope: &Scope,
    ) -> CoreResult<Vec<engram_domain::KnowledgeEntity>> {
        SqlKnowledgeStore::list_entities(self, scope).await
    }

    async fn list_relationships(
        &self,
        scope: &Scope,
    ) -> CoreResult<Vec<engram_domain::KnowledgeRelationship>> {
        SqlKnowledgeStore::list_relationships(self, scope).await
    }

    async fn list_chunks(&self, scope: &Scope) -> CoreResult<Vec<KnowledgeChunk>> {
        SqlKnowledgeStore::list_chunks(self, scope).await
    }

    /// Lean override: id + source + text-nonemptiness without materializing
    /// record JSON or text (the store's in-engine json_extract path).
    async fn list_chunk_refs(&self, scope: &Scope) -> CoreResult<Vec<ChunkRef>> {
        Ok(SqlKnowledgeStore::list_chunk_refs(self, scope)
            .await?
            .into_iter()
            .map(|r| ChunkRef {
                id: r.id,
                source: r.source,
                has_text: r.has_text,
                content_hash: r.content_hash,
            })
            .collect())
    }

    async fn list_graphs(&self, scope: &Scope) -> CoreResult<Vec<engram_domain::KnowledgeGraph>> {
        SqlKnowledgeStore::list_graphs(self, scope).await
    }

    async fn list_chunks_by_document(
        &self,
        document_id: &DocumentId,
        scope: &Scope,
    ) -> CoreResult<Vec<KnowledgeChunk>> {
        SqlKnowledgeStore::list_chunks_by_document(self, document_id, scope).await
    }
}
