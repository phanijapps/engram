//! Source-grounded knowledge contracts.
//!
//! Knowledge is distinct from agent memory: sources, documents, chunks,
//! entities, and relationships remain tied to external corpora such as code
//! repositories or uploaded documents. Embeddings are represented by references
//! only; vector bytes and index-specific metadata belong in adapters.

use serde::{Deserialize, Serialize};

use crate::{
    ChunkId, ConceptRef, DocumentId, EntityId, EntityRef, EvidenceRef, KnowledgeGraphId, Metadata,
    OntologyClassId, OntologyRef, Policy, Provenance, RelationshipId, Scope, SourceId, Timestamp,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    Filesystem,
    GitRepository,
    Url,
    Upload,
    Database,
    Api,
    Generated,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeSource {
    pub id: SourceId,
    pub kind: SourceKind,
    pub scope: Scope,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub policy: Policy,
    pub provenance: Provenance,
    pub created_at: Timestamp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeGraph {
    pub id: KnowledgeGraphId,
    pub scope: Scope,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub ontology_refs: Vec<OntologyRef>,
    pub policy: Policy,
    pub provenance: Provenance,
    pub created_at: Timestamp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceDocumentKind {
    Text,
    Markdown,
    Html,
    Pdf,
    Code,
    Notebook,
    Image,
    Audio,
    Video,
    StructuredData,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceDocument {
    pub id: DocumentId,
    pub source_id: SourceId,
    pub kind: SourceDocumentKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub content_hash: String,
    pub provenance: Provenance,
    pub policy: Policy,
    pub created_at: Timestamp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeChunkKind {
    DocumentSection,
    Paragraph,
    Table,
    CodeBlock,
    CodeSymbol,
    File,
    DiffHunk,
    ApiReference,
    TranscriptSegment,
    StructuredRecord,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceLocation {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_offset: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_offset: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anchor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeChunk {
    pub id: ChunkId,
    pub document_id: DocumentId,
    pub source_id: SourceId,
    pub kind: KnowledgeChunkKind,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<SourceLocation>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub entities: Vec<EntityRef>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub concepts: Vec<ConceptRef>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub embedding_refs: Vec<EmbeddingRef>,
    pub content_hash: String,
    pub provenance: Provenance,
    pub policy: Policy,
    pub created_at: Timestamp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityKind {
    Person,
    Organization,
    Project,
    Repository,
    File,
    Module,
    Class,
    Function,
    Method,
    Variable,
    Struct,
    Interface,
    Trait,
    TypeAlias,
    Enum,
    Endpoint,
    Api,
    Concept,
    ValueStream,
    Requirement,
    Task,
    Tool,
    Artifact,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeEntity {
    pub id: EntityId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub graph_id: Option<KnowledgeGraphId>,
    pub kind: EntityKind,
    pub name: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub aliases: Vec<String>,
    pub scope: Scope,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub source_refs: Vec<EvidenceRef>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub concept_refs: Vec<ConceptRef>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub ontology_class_refs: Vec<OntologyClassId>,
    pub provenance: Provenance,
    pub created_at: Timestamp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_from: Option<Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_until: Option<Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeRelationship {
    pub id: RelationshipId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub graph_id: Option<KnowledgeGraphId>,
    pub subject: EntityRef,
    pub predicate: String,
    pub object: EntityRef,
    pub scope: Scope,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub evidence: Vec<EvidenceRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
    pub provenance: Provenance,
    pub created_at: Timestamp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<Timestamp>,
}

// ── Knowledge-graph identity and consolidation (RFC-0014) ───────────────────

/// Current normalization scheme version.
pub const NORMALIZATION_VERSION: &str = "1";

/// Caller-selected identity policy for entity resolution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "mode")]
pub enum EntityIdentityMode {
    /// ID-only: no identity resolution (existing behavior, default).
    IdOnly,
    /// Caller-supplied stable key that survives renames.
    StableKey { key: String },
    /// Scope + kind + normalized name (opt-in; never crosses scope/kind/graph
    /// unless the caller explicitly broadens the boundary).
    ScopedKindAndNormalizedName {
        normalization_version: String,
        include_graph: bool,
        match_aliases: bool,
    },
}

/// How conflicting scalar values are resolved during a merge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictStrategy {
    /// Report conflicts but do not auto-resolve (caller decides).
    Report,
    /// Canonical entity's value wins.
    PreferCanonical,
    /// Earliest-created entity's value wins.
    PreferEarliest,
}

/// Controls how entity fields merge during identity resolution or consolidation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityMergePolicy {
    pub conflict_strategy: ConflictStrategy,
}

impl Default for EntityMergePolicy {
    fn default() -> Self {
        Self {
            conflict_strategy: ConflictStrategy::Report,
        }
    }
}

/// A write request with a declared identity policy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityWriteRequest {
    pub entity: KnowledgeEntity,
    pub identity: EntityIdentityMode,
    #[serde(default)]
    pub merge_policy: EntityMergePolicy,
}

/// The outcome of an identity-aware entity write.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "outcome")]
pub enum EntityWriteOutcome {
    Created {
        entity: KnowledgeEntity,
    },
    Matched {
        entity: KnowledgeEntity,
    },
    Merged {
        entity: KnowledgeEntity,
        changed_fields: Vec<String>,
        conflicts: Vec<EntityMergeConflict>,
    },
}

/// A conflicting field value discovered during a merge.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityMergeConflict {
    pub field: String,
    pub canonical_value: String,
    pub duplicate_value: String,
}

/// A request to consolidate duplicate entity IDs into a canonical entity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityMergeRequest {
    pub canonical_id: EntityId,
    pub duplicate_ids: Vec<EntityId>,
    pub scope: Scope,
    #[serde(default)]
    pub policy: EntityMergePolicy,
}

/// The result of a transactional entity consolidation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityMergeResult {
    pub canonical_entity: KnowledgeEntity,
    pub redirected_relationships: usize,
    pub coalesced_relationships: usize,
    pub deleted_entities: usize,
    pub conflicts: Vec<EntityMergeConflict>,
    pub audit_id: String,
}

/// A group of entities that share an identity key under a declared policy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollisionGroup {
    pub identity_key: String,
    pub entity_ids: Vec<EntityId>,
}

// ── End identity types ──────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmbeddingTargetType {
    Memory,
    Chunk,
    Entity,
    Concept,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddingRef {
    pub id: String,
    pub model: String,
    pub dimensions: u32,
    pub target_type: EmbeddingTargetType,
    pub target_id: String,
    pub content_hash: String,
    pub created_at: Timestamp,
}
