//! Knowledge repository port — source-grounded record persistence.

use async_trait::async_trait;
use engram_domain::*;
use engram_runtime::{CoreError, CoreResult};

/// Persistence port for source-grounded knowledge records.
///
/// Implementations store corpus-derived sources, documents, chunks, entities,
/// and relationships without turning them into agent memories. A backend may be
/// document-oriented, relational, graph-native, or process-local, but it must
/// preserve provenance, policy, and scope so retrieval can compose knowledge
/// with memory safely.
#[allow(clippy::double_must_use)]
// async_trait codegen stamps #[must_use] on the boxed-future helper; CoreResult is already must_use
#[async_trait]
pub trait KnowledgeRepository: Send + Sync {
    /// Stores or updates a registered knowledge source.
    async fn put_source(&self, source: KnowledgeSource) -> CoreResult<KnowledgeSource>;

    /// Looks up a source inside the caller-provided scope boundary.
    async fn get_source(
        &self,
        _id: &SourceId,
        _scope: &Scope,
    ) -> CoreResult<Option<KnowledgeSource>> {
        Ok(None)
    }

    /// Lists all source records visible to `scope` for a complete knowledge inventory.
    async fn list_sources(&self, _scope: &Scope) -> CoreResult<Vec<KnowledgeSource>> {
        Err(CoreError::CapabilityUnsupported {
            capability: "knowledge-source-list".into(),
            reason: "this store does not enumerate knowledge sources".into(),
        })
    }

    /// Lists all document records visible to `scope` for a complete knowledge inventory.
    async fn list_documents(&self, _scope: &Scope) -> CoreResult<Vec<SourceDocument>> {
        Err(CoreError::CapabilityUnsupported {
            capability: "knowledge-document-list".into(),
            reason: "this store does not enumerate knowledge documents".into(),
        })
    }

    /// Stores a versioned document extracted from a source.
    async fn put_document(&self, document: SourceDocument) -> CoreResult<SourceDocument>;

    /// Stores the smallest retrievable source-grounded unit.
    async fn put_chunk(&self, chunk: KnowledgeChunk) -> CoreResult<KnowledgeChunk>;

    /// Looks up a chunk by ID inside the caller-provided scope boundary.
    async fn get_chunk(&self, id: &ChunkId, scope: &Scope) -> CoreResult<Option<KnowledgeChunk>>;

    /// Stores or updates an extracted graph entity.
    async fn put_entity(&self, entity: KnowledgeEntity) -> CoreResult<KnowledgeEntity> {
        Err(CoreError::Adapter {
            adapter: "knowledge_repository".to_owned(),
            message: format!("entity writes are not supported for {}", entity.id),
        })
    }

    /// Stores or updates an extracted graph relationship.
    async fn put_relationship(
        &self,
        relationship: KnowledgeRelationship,
    ) -> CoreResult<KnowledgeRelationship> {
        Err(CoreError::Adapter {
            adapter: "knowledge_repository".to_owned(),
            message: format!(
                "relationship writes are not supported for {}",
                relationship.id
            ),
        })
    }

    /// Looks up an entity by ID inside the caller-provided scope boundary.
    async fn get_entity(
        &self,
        _id: &EntityId,
        _scope: &Scope,
    ) -> CoreResult<Option<KnowledgeEntity>> {
        Ok(None)
    }

    /// Looks up a relationship by ID inside the caller-provided scope boundary.
    async fn get_relationship(
        &self,
        _id: &RelationshipId,
        _scope: &Scope,
    ) -> CoreResult<Option<KnowledgeRelationship>> {
        Ok(None)
    }

    /// Deletes an entity by ID within the caller-provided scope boundary.
    ///
    /// Returns `true` if a row was deleted, `false` if the entity was not found
    /// or the caller's scope does not match the record's scope (hard delete; no
    /// tombstone). Default implementation returns a not-supported error.
    async fn delete_entity(&self, _id: &EntityId, _scope: &Scope) -> CoreResult<bool> {
        Err(CoreError::Adapter {
            adapter: "knowledge_repository".to_owned(),
            message: "entity deletes are not supported".to_owned(),
        })
    }

    /// Deletes a relationship by ID within the caller-provided scope boundary.
    ///
    /// Returns `true` if a row was deleted, `false` if the relationship was not
    /// found or the caller's scope does not match the record's scope (hard delete;
    /// no tombstone). Default implementation returns a not-supported error.
    async fn delete_relationship(&self, _id: &RelationshipId, _scope: &Scope) -> CoreResult<bool> {
        Err(CoreError::Adapter {
            adapter: "knowledge_repository".to_owned(),
            message: "relationship deletes are not supported".to_owned(),
        })
    }

    /// Lists the entities attached to a graph, within the caller's scope —
    /// the identity carry-over read for re-ingest (temporal discrimination:
    /// unchanged symbols keep their original `validFrom` across graph
    /// replacement). Default: empty (unsupported).
    async fn list_entities_by_graph(
        &self,
        _graph_id: &KnowledgeGraphId,
        _scope: &Scope,
    ) -> CoreResult<Vec<KnowledgeEntity>> {
        Ok(Vec::new())
    }

    /// Lists the chunks belonging to a document, within the caller's scope.
    /// Used by source-reingest retraction to find a prior document's chunks
    /// before deleting them + their embeddings. Default: empty (unsupported).
    async fn list_chunks_by_document(
        &self,
        _document_id: &DocumentId,
        _scope: &Scope,
    ) -> CoreResult<Vec<KnowledgeChunk>> {
        Ok(Vec::new())
    }

    /// Hard-deletes a source document by id within the caller's scope.
    /// Default: not supported (returns `false`). Caller is responsible for
    /// deleting the document's chunks + embeddings first (children before
    /// parents).
    async fn delete_document(&self, _id: &DocumentId, _scope: &Scope) -> CoreResult<bool> {
        Ok(false)
    }

    /// Hard-deletes a single chunk by id within the caller's scope.
    /// Default: not supported (returns `false`).
    async fn delete_chunk(&self, _id: &ChunkId, _scope: &Scope) -> CoreResult<bool> {
        Ok(false)
    }

    /// Lists the entities whose `source_refs` carry a source ref targeting
    /// `source_id` (an `EvidenceRef` with `target_type: Source`), within the
    /// caller's scope — the query-by-source surface for sourced distill
    /// batches. Default: empty (unsupported).
    async fn list_entities_by_source_ref(
        &self,
        _source_id: &str,
        _scope: &Scope,
    ) -> CoreResult<Vec<KnowledgeEntity>> {
        Ok(Vec::new())
    }

    /// Lists the relationships whose `evidence` refs target `source_id`,
    /// within the caller's scope. Default: empty (unsupported).
    async fn list_relationships_by_source_ref(
        &self,
        _source_id: &str,
        _scope: &Scope,
    ) -> CoreResult<Vec<KnowledgeRelationship>> {
        Ok(Vec::new())
    }

    /// Clears `archived_at` on one entity (single-record restore, no cascade).
    /// Returns `true` if an archived row was restored, `false` if the entity
    /// was not found, not archived, or outside the caller's scope. Re-asserted
    /// records of a re-distilled source use this to come back active. Default:
    /// not supported.
    async fn restore_entity(&self, _id: &EntityId, _scope: &Scope) -> CoreResult<bool> {
        Err(CoreError::Adapter {
            adapter: "knowledge_repository".to_owned(),
            message: "entity restores are not supported".to_owned(),
        })
    }

    /// Clears `archived_at` on one relationship. Same semantics as
    /// [`Self::restore_entity`]. Default: not supported.
    async fn restore_relationship(&self, _id: &RelationshipId, _scope: &Scope) -> CoreResult<bool> {
        Err(CoreError::Adapter {
            adapter: "knowledge_repository".to_owned(),
            message: "relationship restores are not supported".to_owned(),
        })
    }
}
