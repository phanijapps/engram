//! Canonical v1 export over typed, scope-bounded storage inventories.
//!
//! Export preserves the existing v1 fields and omissions. Archive restore is
//! unsupported; successful dry-run validation does not imply restore support.
use crate::storage_composition::{ArchiveInventory, ArchiveSnapshot};
use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use engram_domain::{
    Belief, Concept, ConceptScheme, HierarchyNode, KnowledgeChunk, KnowledgeEntity,
    KnowledgeRelationship, KnowledgeSource, MemoryRecord, Scope, SourceDocument,
};
use engram_runtime::CoreResult;

use crate::{
    BeliefImportRecord, ConceptImportRecord, ConceptSchemeImportRecord, ExportImport,
    HierarchyNodeImportRecord, ImportData, KnowledgeChunkImportRecord,
    KnowledgeDocumentImportRecord, KnowledgeEntityImportRecord, KnowledgeRelationshipImportRecord,
    KnowledgeSourceImportRecord, MemoryImportRecord,
};

/// Shared export implementation; recipes contribute scoped storage inventory.
pub struct ArchiveExport {
    inventory: Arc<dyn ArchiveInventory>,
}
impl ArchiveExport {
    pub fn new(inventory: Arc<dyn ArchiveInventory>) -> Self {
        Self { inventory }
    }
}
#[async_trait]
impl ExportImport for ArchiveExport {
    async fn export(&self, scope: &Scope) -> CoreResult<ImportData> {
        export_snapshot(self.inventory.snapshot(scope).await?, scope)
    }
}

/// Maps existing typed families to the unchanged archive-v1 representation.
/// Scope is checked before conversion, including each inherited source/scheme.
pub fn export_snapshot(mut snapshot: ArchiveSnapshot, scope: &Scope) -> CoreResult<ImportData> {
    let visible = |stored: &Scope| engram_memory::scope::scope_allows(stored, scope);
    snapshot.memories.retain(|record| visible(&record.scope));
    snapshot.sources.retain(|record| visible(&record.scope));
    snapshot.entities.retain(|record| visible(&record.scope));
    snapshot
        .relationships
        .retain(|record| visible(&record.scope));
    snapshot
        .concept_schemes
        .retain(|record| visible(&record.scope));
    snapshot.beliefs.retain(|record| visible(&record.scope));
    snapshot
        .hierarchy_nodes
        .retain(|record| visible(&record.scope));
    let source_scopes: HashMap<String, Scope> = snapshot
        .sources
        .iter()
        .map(|source| (source.id.to_string(), source.scope.clone()))
        .collect();
    let scheme_scopes: HashMap<String, Scope> = snapshot
        .concept_schemes
        .iter()
        .map(|scheme| (scheme.id.to_string(), scheme.scope.clone()))
        .collect();
    snapshot
        .documents
        .retain(|record| source_scopes.contains_key(record.source_id.as_str()));
    snapshot
        .chunks
        .retain(|record| source_scopes.contains_key(record.source_id.as_str()));
    snapshot.concepts.retain(|(record, inherited_scope)| {
        visible(inherited_scope)
            && scheme_scopes.get(record.scheme_id.as_str()) == Some(inherited_scope)
    });
    Ok(ImportData {
        memories: snapshot.memories.iter().map(memory_record).collect(),
        knowledge_sources: snapshot.sources.iter().map(source_record).collect(),
        knowledge_documents: snapshot
            .documents
            .iter()
            .map(|record| document_record(record, &source_scopes))
            .collect(),
        knowledge_chunks: snapshot
            .chunks
            .iter()
            .map(|record| chunk_record(record, &source_scopes))
            .collect(),
        knowledge_entities: snapshot.entities.iter().map(entity_record).collect(),
        knowledge_relationships: snapshot
            .relationships
            .iter()
            .map(relationship_record)
            .collect(),
        concept_schemes: snapshot
            .concept_schemes
            .iter()
            .map(concept_scheme_record)
            .collect(),
        concepts: snapshot
            .concepts
            .iter()
            .map(|(record, inherited_scope)| concept_record(record, inherited_scope))
            .collect(),
        beliefs: snapshot.beliefs.iter().map(belief_record).collect(),
        hierarchy_nodes: snapshot
            .hierarchy_nodes
            .iter()
            .map(hierarchy_node_record)
            .collect(),
        vectors: Vec::new(),
    })
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

fn belief_record(belief: &Belief) -> BeliefImportRecord {
    // The record's `start_time` is the belief's validity start when set, falling
    // back to the record's creation time (a belief without a `valid_from` still
    // carries a creation provenance timestamp).
    let start_time = belief
        .valid_from
        .map(|ts| ts.timestamp())
        .unwrap_or_else(|| belief.created_at.timestamp());
    BeliefImportRecord {
        id: belief.id.to_string(),
        scope: scope_json(&belief.scope),
        content: belief.content.clone(),
        start_time,
        end_time: belief.valid_until.map(|ts| ts.timestamp()),
        metadata: metadata_json(&belief.metadata),
    }
}

fn hierarchy_node_record(node: &HierarchyNode) -> HierarchyNodeImportRecord {
    // The record's `kind` is the serde string form of `HierarchyNodeKind`
    // (e.g. `base`/`aggregate`); an empty serialize normalizes to `None`
    // (defensive — the enum always serializes to a non-empty string).
    let kind = enum_string(&node.kind);
    HierarchyNodeImportRecord {
        id: node.id.to_string(),
        scope: scope_json(&node.scope),
        label: node.name.clone(),
        kind: if kind.is_empty() { None } else { Some(kind) },
        metadata: metadata_json(&node.metadata),
    }
}

fn concept_scheme_record(scheme: &ConceptScheme) -> ConceptSchemeImportRecord {
    // ConceptScheme carries no inline `description` or free-form `metadata`
    // field in this model, so those import-record fields normalize to absent.
    // The record's `title` maps from the scheme's `name`.
    ConceptSchemeImportRecord {
        id: scheme.id.to_string(),
        scope: scope_json(&scheme.scope),
        title: scheme.name.clone(),
        description: None,
        metadata: "{}".to_string(),
    }
}

fn concept_record(concept: &Concept, scheme_scope: &Scope) -> ConceptImportRecord {
    // Concepts inherit scope from their owning scheme (Concept carries no scope
    // of its own); the preferred label is the import record's `label`.
    ConceptImportRecord {
        id: concept.id.to_string(),
        scope: scope_json(scheme_scope),
        scheme_id: concept.scheme_id.to_string(),
        label: concept.pref_label.value.clone(),
        metadata: "{}".to_string(),
    }
}
