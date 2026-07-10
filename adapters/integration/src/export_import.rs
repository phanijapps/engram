//! SQLite implementation of the [`ExportImport`] port (engram-host-sdk brief, S5).
//!
//! [`SqlExportImport`] composes the wired [`SqlKnowledgeStore`] and
//! [`SqlMemoryService`] and reads a scope's semantic state into one
//! [`ImportData`] payload by calling the existing **concrete store listing
//! methods** (`list_sources`, `list_documents`, `list_chunks`, `list_entities`,
//! `list_relationships`, `list_memories_in_scope`). Export is the read half of
//! scope-to-scope movement; import stays on the existing [`MigrationService`]
//! handle (`dry_run_import` + `apply_import`), which this impl does not
//! reimplement or wrap.
//!
//! The exported [`ImportData`] reuses the existing `*ImportRecord` types, so
//! `export` → `MigrationService::dry_run_import` → `apply_import` round-trips
//! for single-backend movement.
//!
//! # v1 coverage
//!
//! v1 exports knowledge (sources, documents, chunks, entities, relationships)
//! and memory — the families whose concrete stores expose scope-wide listing
//! methods. Concept schemes/concepts, hierarchy, belief, and vectors are
//! deferred: the concrete stores expose no scope-wide list method for them in
//! v1 (see `docs/backlog.md`, `export-import-hierarchy-belief`). `SourceDocument`
//! and `KnowledgeChunk` carry no `scope` field of their own, so their scope is
//! resolved from their owning `KnowledgeSource` (the same visibility rule the
//! store applies when listing).
//!
//! No schema change: the impl reuses the existing per-store reads. It is
//! engine-specific (it names `Sql*` and holds the adapters directly), which is
//! why it lives here rather than in the engine-neutral port crate.
//!
//! ADR-0022: only this adapter crate may name `Sql*`; the port it implements
//! stays engine-neutral.
//!
//! [`MigrationService`]: engram_integration::MigrationService

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use engram_domain::{
    KnowledgeChunk, KnowledgeEntity, KnowledgeRelationship, KnowledgeSource, MemoryRecord, Scope,
    SourceDocument,
};
use engram_integration::{
    ExportImport, ImportData, KnowledgeChunkImportRecord, KnowledgeDocumentImportRecord,
    KnowledgeEntityImportRecord, KnowledgeRelationshipImportRecord, KnowledgeSourceImportRecord,
    MemoryImportRecord,
};
use engram_runtime::CoreResult;
use engram_store_knowledge_sqlite::SqlKnowledgeStore;
use engram_store_sql::SqlMemoryService;

/// SQLite-backed [`ExportImport`]: reads a scope's semantic state from the wired
/// concrete stores into one [`ImportData`] payload.
///
/// Construct with [`SqlExportImport::new`] from the shared knowledge + memory
/// store handles. The export carries no mutable state; each `export` reads the
/// stores independently.
pub struct SqlExportImport {
    knowledge: Arc<SqlKnowledgeStore>,
    memory: Arc<SqlMemoryService>,
}

impl SqlExportImport {
    /// Wraps the shared knowledge + memory store handles to expose scope export.
    pub fn new(knowledge: Arc<SqlKnowledgeStore>, memory: Arc<SqlMemoryService>) -> Self {
        Self { knowledge, memory }
    }
}

#[async_trait]
impl ExportImport for SqlExportImport {
    async fn export(&self, scope: &Scope) -> CoreResult<ImportData> {
        // ---- Knowledge family: read via the concrete listing methods --------
        let sources = self.knowledge.list_sources(scope).await?;
        let documents = self.knowledge.list_documents(scope).await?;
        let chunks = self.knowledge.list_chunks(scope).await?;
        let entities = self.knowledge.list_entities(scope).await?;
        let relationships = self.knowledge.list_relationships(scope).await?;

        // Documents and chunks carry no `scope` field of their own; their scope
        // is inherited from their owning source. Build a source-id -> scope map
        // from the already-listed (scope-visible) sources so the exported
        // records carry a faithful scope string.
        let source_scopes: HashMap<String, Scope> = sources
            .iter()
            .map(|s| (s.id.to_string(), s.scope.clone()))
            .collect();

        // ---- Memory family --------------------------------------------------
        let memories = self.memory.list_memories_in_scope(scope)?;

        Ok(ImportData {
            memories: memories.iter().map(memory_record).collect(),
            knowledge_sources: sources.iter().map(source_record).collect(),
            knowledge_documents: documents
                .iter()
                .map(|d| document_record(d, &source_scopes))
                .collect(),
            knowledge_chunks: chunks
                .iter()
                .map(|c| chunk_record(c, &source_scopes))
                .collect(),
            knowledge_entities: entities.iter().map(entity_record).collect(),
            knowledge_relationships: relationships.iter().map(relationship_record).collect(),
            // Concept schemes/concepts, hierarchy, belief, and vectors are
            // deferred — no scope-wide concrete list method in v1.
            concept_schemes: Vec::new(),
            concepts: Vec::new(),
            beliefs: Vec::new(),
            hierarchy_nodes: Vec::new(),
            vectors: Vec::new(),
        })
    }
}

// ---------------- record mapping helpers ----------------------------------

/// Serializes a [`Scope`] to a faithful JSON string (preserves every scope
/// dimension). Falls back to the tenant if serialization fails.
fn scope_json(scope: &Scope) -> String {
    serde_json::to_string(scope).unwrap_or_else(|_| scope.tenant.clone())
}

/// Serializes an enum to its serde string form (e.g. `SourceKind::Filesystem`
/// -> `filesystem`). Empty string on serialization failure.
fn enum_string<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

/// Serializes an optional [`engram_domain::Metadata`] map to a JSON string.
/// `None` and failures normalize to `{}`.
fn metadata_json(meta: &Option<engram_domain::Metadata>) -> String {
    meta.as_ref()
        .and_then(|m| serde_json::to_string(m).ok())
        .unwrap_or_else(|| "{}".to_string())
}

fn source_record(source: &KnowledgeSource) -> KnowledgeSourceImportRecord {
    KnowledgeSourceImportRecord {
        id: source.id.to_string(),
        scope: scope_json(&source.scope),
        source_type: enum_string(&source.kind),
        uri: source.uri.clone().unwrap_or_default(),
        metadata: metadata_json(&source.metadata),
    }
}

fn document_record(
    document: &SourceDocument,
    source_scopes: &HashMap<String, Scope>,
) -> KnowledgeDocumentImportRecord {
    // SourceDocument carries no inline content text in this model — document
    // text is chunked and exported separately as KnowledgeChunk records. The
    // document record carries its identity, title, and metadata.
    let scope = document.source_id.to_string();
    KnowledgeDocumentImportRecord {
        id: document.id.to_string(),
        scope: source_scopes
            .get(&scope)
            .map(scope_json)
            // Fall back to the owning source id when the source was not in the
            // exported set (defensive; the listing already filters by source
            // scope, so this branch should not fire for a consistent store).
            .unwrap_or_default(),
        source_id: document.source_id.to_string(),
        title: document.title.clone().unwrap_or_default(),
        content: String::new(),
        metadata: metadata_json(&document.metadata),
    }
}

fn chunk_record(
    chunk: &KnowledgeChunk,
    source_scopes: &HashMap<String, Scope>,
) -> KnowledgeChunkImportRecord {
    // KnowledgeChunk carries no `scope` or `sequence` field of its own; its
    // scope is inherited from its owning source, and ordering in this model is
    // by id (not a per-document sequence number).
    KnowledgeChunkImportRecord {
        id: chunk.id.to_string(),
        scope: source_scopes
            .get(&chunk.source_id.to_string())
            .map(scope_json)
            .unwrap_or_default(),
        document_id: chunk.document_id.to_string(),
        sequence: 0,
        content: chunk.text.clone(),
        metadata: metadata_json(&chunk.metadata),
    }
}

fn entity_record(entity: &KnowledgeEntity) -> KnowledgeEntityImportRecord {
    KnowledgeEntityImportRecord {
        id: entity.id.to_string(),
        scope: scope_json(&entity.scope),
        kind: enum_string(&entity.kind),
        name: entity.name.clone(),
        metadata: metadata_json(&entity.metadata),
    }
}

fn relationship_record(relationship: &KnowledgeRelationship) -> KnowledgeRelationshipImportRecord {
    KnowledgeRelationshipImportRecord {
        id: relationship.id.to_string(),
        scope: scope_json(&relationship.scope),
        source_id: relationship
            .subject
            .id
            .as_ref()
            .map(|id| id.to_string())
            .unwrap_or_default(),
        target_id: relationship
            .object
            .id
            .as_ref()
            .map(|id| id.to_string())
            .unwrap_or_default(),
        kind: relationship.predicate.clone(),
        // KnowledgeRelationship carries no free-form metadata field; it has
        // evidence/confidence/provenance, which are not in the import record.
        metadata: "{}".to_string(),
    }
}

fn memory_record(record: &MemoryRecord) -> MemoryImportRecord {
    MemoryImportRecord {
        id: record.id.to_string(),
        scope: scope_json(&record.scope),
        content: record.content.text.clone(),
        timestamp: record.created_at.timestamp(),
        policy: serde_json::to_string(&record.policy).unwrap_or_else(|_| "{}".to_string()),
    }
}

#[cfg(test)]
mod tests {
    //! The SqlExportImport integration tests live in
    //! `adapters/integration/tests/export_import.rs` so they can share the
    //! fixture helpers and the block_on driving style. This module is reserved
    //! for any future inline unit tests that do not require a store.
}
