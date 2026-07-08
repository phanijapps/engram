//! Source-grounded knowledge contracts.
//!
//! Knowledge is distinct from agent memory: sources, documents, chunks,
//! entities, and relationships remain tied to external corpora such as code
//! repositories or uploaded documents. Embeddings are represented by references
//! only; vector bytes and index-specific metadata belong in adapters.

use serde::{Deserialize, Serialize};

use crate::{
    ChunkId, ConceptRef, DocumentId, EntityId, EntityRef, EvidenceRef, KnowledgeGraphId, Metadata,
    OntologyRef, Policy, Provenance, RelationshipId, Scope, SourceId, Timestamp,
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
