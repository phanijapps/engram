//! SQLite implementation of the [`ProvenanceQuery`] port (engram-host-sdk
//! brief, S2).
//!
//! [`SqlProvenanceQuery`] composes an [`Arc<SqlKnowledgeStore>`] and reads the
//! `Provenance` / `EvidenceRef` already embedded in stored entity /
//! relationship / source records. It is engine-specific (it names `Sql*` and
//! holds the knowledge adapter directly), which is why it lives here in the
//! adapters layer rather than in the engine-neutral port crate
//! (`core/integration`). v1 backs the knowledge-graph core — entity,
//! relationship, source; every other [`EvidenceTargetType`] returns
//! [`CoreError::CapabilityUnsupported`] until its scope-safe listing is wired.
//!
//! No schema change: the impl reuses the existing scope-column listings and
//! deserializes each `record_json`, filtering in Rust.
//!
//! ADR-0022: only this adapter crate may name `Sql*`; the port it implements
//! stays engine-neutral.

use std::sync::Arc;

use async_trait::async_trait;
use engram_domain::{
    EntityId, EvidenceRef, EvidenceTargetType, KnowledgeEntity, KnowledgeRelationship, Provenance,
    RelationshipId, Scope, SourceId,
};
use engram_integration::{ProvenanceEntry, ProvenanceQuery, TimeWindow};
use engram_knowledge::KnowledgeRepository;
use engram_runtime::{CoreError, CoreResult};
use engram_store_knowledge_sqlite::SqlKnowledgeStore;

/// SQLite-backed [`ProvenanceQuery`]: reads embedded provenance / evidence from
/// the knowledge-graph records an [`SqlKnowledgeStore`] already holds.
///
/// Construct with [`SqlProvenanceQuery::new`] from a shared store handle. The
/// query is read-only and carries no mutable state.
pub struct SqlProvenanceQuery {
    knowledge: Arc<SqlKnowledgeStore>,
}

impl SqlProvenanceQuery {
    /// Wraps a shared knowledge-store handle to expose provenance / evidence
    /// reads.
    pub fn new(knowledge: Arc<SqlKnowledgeStore>) -> Self {
        Self { knowledge }
    }

    /// The `CapabilityUnsupported` error returned for target kinds not backed
    /// in v1 (memory, belief, document, chunk, concept, event, url).
    fn unsupported_kind() -> CoreError {
        CoreError::CapabilityUnsupported {
            capability: "episodes_evidence".to_string(),
            reason: "target kind not backed in v1".to_string(),
        }
    }
}

#[async_trait]
impl ProvenanceQuery for SqlProvenanceQuery {
    async fn provenance_for(
        &self,
        target: EvidenceTargetType,
        id: &str,
        scope: &Scope,
    ) -> CoreResult<Option<Provenance>> {
        match target {
            EvidenceTargetType::Entity => Ok(self
                .knowledge
                .get_entity(&EntityId::from(id), scope)
                .await?
                .map(|entity| entity.provenance)),
            EvidenceTargetType::Relationship => Ok(self
                .knowledge
                .get_relationship(&RelationshipId::from(id), scope)
                .await?
                .map(|relationship| relationship.provenance)),
            EvidenceTargetType::Source => {
                let source = source_by_id(&self.knowledge, id, scope).await?;
                Ok(source.map(|s| s.provenance))
            }
            // v1-unsupported target kinds: memory, belief, document, chunk,
            // concept, event, url. Each needs a scope-safe listing before it
            // can be wired; until then they fail honestly with a typed error
            // rather than a silent empty.
            EvidenceTargetType::Memory
            | EvidenceTargetType::Belief
            | EvidenceTargetType::Document
            | EvidenceTargetType::Chunk
            | EvidenceTargetType::Concept
            | EvidenceTargetType::Event
            | EvidenceTargetType::Url => Err(Self::unsupported_kind()),
        }
    }

    async fn evidence_for(
        &self,
        target: EvidenceTargetType,
        id: &str,
        scope: &Scope,
    ) -> CoreResult<Vec<EvidenceRef>> {
        match target {
            EvidenceTargetType::Entity => Ok(self
                .knowledge
                .get_entity(&EntityId::from(id), scope)
                .await?
                .map(|entity| entity.provenance.evidence)
                .unwrap_or_default()),
            EvidenceTargetType::Relationship => {
                // A relationship carries both its own `evidence` links and a
                // `Provenance.evidence` list; surface both (dedup is not
                // required for v1 — they are distinct slots that may overlap
                // by reference).
                Ok(self
                    .knowledge
                    .get_relationship(&RelationshipId::from(id), scope)
                    .await?
                    .map(|relationship| {
                        let mut combined = relationship.evidence.clone();
                        combined.extend(relationship.provenance.evidence.clone());
                        combined
                    })
                    .unwrap_or_default())
            }
            EvidenceTargetType::Source => Ok(source_by_id(&self.knowledge, id, scope)
                .await?
                .map(|s| s.provenance.evidence)
                .unwrap_or_default()),
            EvidenceTargetType::Memory
            | EvidenceTargetType::Belief
            | EvidenceTargetType::Document
            | EvidenceTargetType::Chunk
            | EvidenceTargetType::Concept
            | EvidenceTargetType::Event
            | EvidenceTargetType::Url => Err(Self::unsupported_kind()),
        }
    }

    async fn provenance_by_source(
        &self,
        stable_source_key: &str,
        scope: &Scope,
        window: TimeWindow,
    ) -> CoreResult<Vec<ProvenanceEntry>> {
        // `stable_source_key` is the source-grouping key (typically the source
        // URI / repo URL supplied at ingest), NOT the `KnowledgeSource.id`. The
        // knowledge adapter indexes graphs by the `stableSourceKey` lifted from
        // graph metadata, and `list_*_by_source` resolve records through that
        // column — so this is the identifier those listings accept.
        let mut entries = Vec::new();
        for entity in self
            .knowledge
            .list_entities_by_source(scope, stable_source_key)
            .await?
        {
            if window.contains(entity.provenance.observed_at) {
                entries.push(entity_entry(entity));
            }
        }
        for relationship in self
            .knowledge
            .list_relationships_by_source(scope, stable_source_key)
            .await?
        {
            if window.contains(relationship.provenance.observed_at) {
                entries.push(relationship_entry(relationship));
            }
        }
        Ok(entries)
    }

    async fn evidence_by_scope(
        &self,
        scope: &Scope,
        window: TimeWindow,
        limit: usize,
    ) -> CoreResult<Vec<ProvenanceEntry>> {
        let mut entries = Vec::new();
        for entity in self.knowledge.list_entities(scope).await? {
            if entries.len() >= limit {
                break;
            }
            if window.contains(entity.provenance.observed_at) {
                entries.push(entity_entry(entity));
            }
        }
        for relationship in self.knowledge.list_relationships(scope).await? {
            if entries.len() >= limit {
                break;
            }
            if window.contains(relationship.provenance.observed_at) {
                entries.push(relationship_entry(relationship));
            }
        }
        // Source is a v1-supported target — include its provenance too, so a
        // scope-wide evidence query is not silently incomplete.
        for source in self.knowledge.list_sources(scope).await? {
            if entries.len() >= limit {
                break;
            }
            if window.contains(source.provenance.observed_at) {
                entries.push(source_entry(source));
            }
        }
        Ok(entries)
    }
}

/// Looks up a single source by id within `scope` by filtering the scope's
/// source listing (the knowledge adapter exposes no `get_source` by id).
async fn source_by_id(
    knowledge: &Arc<SqlKnowledgeStore>,
    id: &str,
    scope: &Scope,
) -> CoreResult<Option<engram_domain::KnowledgeSource>> {
    let wanted = SourceId::from(id);
    Ok(knowledge
        .list_sources(scope)
        .await?
        .into_iter()
        .find(|source| source.id == wanted))
}

fn entity_entry(entity: KnowledgeEntity) -> ProvenanceEntry {
    ProvenanceEntry {
        target: EvidenceTargetType::Entity,
        target_id: entity.id.to_string(),
        provenance: entity.provenance,
    }
}

fn relationship_entry(relationship: KnowledgeRelationship) -> ProvenanceEntry {
    ProvenanceEntry {
        target: EvidenceTargetType::Relationship,
        target_id: relationship.id.to_string(),
        provenance: relationship.provenance,
    }
}

fn source_entry(source: engram_domain::KnowledgeSource) -> ProvenanceEntry {
    ProvenanceEntry {
        target: EvidenceTargetType::Source,
        target_id: source.id.to_string(),
        provenance: source.provenance,
    }
}

#[cfg(test)]
mod tests {
    //! The SqlProvenanceQuery integration tests live in
    //! `adapters/integration/tests/provenance_query.rs` so they can share the
    //! fixture helpers. This module is reserved for any future inline unit
    //! tests that do not require a store.
}
