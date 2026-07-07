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
#[async_trait]
pub trait KnowledgeRepository: Send + Sync {
    /// Stores or updates a registered knowledge source.
    async fn put_source(&self, source: KnowledgeSource) -> CoreResult<KnowledgeSource>;

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
}
