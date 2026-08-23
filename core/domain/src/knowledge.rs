//! Source-grounded knowledge contracts.
//!
//! Knowledge is distinct from agent memory: sources, documents, chunks,
//! entities, and relationships remain tied to external corpora such as code
//! repositories or uploaded documents. Embeddings are represented by references
//! only; vector bytes and index-specific metadata belong in adapters.

use serde::{Deserialize, Serialize};

use crate::{
    ChunkId, ConceptRef, DocumentId, EntityId, EntityRef, EvidenceRef, Id, KnowledgeGraphId,
    Metadata, OntologyClassId, OntologyRef, Policy, Provenance, RelationshipId, Scope, SourceId,
    Timestamp,
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
    /// Soft-delete timestamp (ADR-0027); None = active. Excluded from active reads.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archived_at: Option<Timestamp>,
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
    /// Soft-delete timestamp (ADR-0027); None = active. Excluded from active reads.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archived_at: Option<Timestamp>,
}

// ── Code indexing vocabulary (RFC-0020 Phase 2, ADR-0028) ───────────────────

/// Closed predicate vocabulary for code-extraction edges. The relationship
/// `predicate` stays an open string; this enum is the set the code extractor
/// (`engram-code`) is contracted to emit (`docs/domain-data-model.md`
/// §Code indexing).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodeEdgeKind {
    Calls,
    Imports,
    Contains,
    Extends,
    Implements,
    RoutesTo,
}

impl CodeEdgeKind {
    /// The closed set, in canonical order.
    pub const ALL: [CodeEdgeKind; 6] = [
        CodeEdgeKind::Calls,
        CodeEdgeKind::Imports,
        CodeEdgeKind::Contains,
        CodeEdgeKind::Extends,
        CodeEdgeKind::Implements,
        CodeEdgeKind::RoutesTo,
    ];

    /// Predicate string as persisted on `KnowledgeRelationship.predicate`.
    pub fn as_str(self) -> &'static str {
        match self {
            CodeEdgeKind::Calls => "calls",
            CodeEdgeKind::Imports => "imports",
            CodeEdgeKind::Contains => "contains",
            CodeEdgeKind::Extends => "extends",
            CodeEdgeKind::Implements => "implements",
            CodeEdgeKind::RoutesTo => "routes_to",
        }
    }
}

impl std::fmt::Display for CodeEdgeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for CodeEdgeKind {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        CodeEdgeKind::ALL
            .iter()
            .copied()
            .find(|k| k.as_str() == s)
            .ok_or(())
    }
}

/// Lifecycle of a recorded unresolved cross-file reference. `pending →
/// resolved` happens when a later ingest defines a unique target (the orphan
/// sweep); `pending → failed` only by explicit operator action, never
/// automatically.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnresolvedReferenceStatus {
    Pending,
    Resolved,
    Failed,
}

impl UnresolvedReferenceStatus {
    /// The only legal transitions: pending → resolved (sweep) and pending →
    /// failed (operator). Terminal states never change.
    pub fn can_transition_to(self, next: UnresolvedReferenceStatus) -> bool {
        use UnresolvedReferenceStatus::*;
        matches!((self, next), (Pending, Resolved) | (Pending, Failed))
    }
}

/// A recorded, best-effort-failed cross-file code reference — the honesty
/// ledger for code resolution (`docs/domain-data-model.md` §Code indexing).
/// Rows persist in the knowledge store with the same lifecycle as
/// relationships: retracted with their referring document on re-ingest,
/// re-attempted by the orphan sweep whenever new symbols land.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnresolvedReference {
    pub id: Id,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub graph_id: Option<KnowledgeGraphId>,
    /// Referring entity.
    pub from_entity_id: EntityId,
    /// Name as written at the reference site.
    pub reference_name: String,
    /// Candidate target ids known at record time.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub candidates: Vec<EntityId>,
    pub status: UnresolvedReferenceStatus,
    /// Referring file path (disambiguator).
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    pub scope: Scope,
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

#[cfg(test)]
mod code_indexing_tests {
    use super::*;

    #[test]
    fn code_edge_kind_serde_round_trips_every_variant() {
        for kind in CodeEdgeKind::ALL {
            let json = serde_json::to_string(&kind).expect("serialize");
            let back: CodeEdgeKind = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(back, kind, "round-trip failed for {json}");
        }
    }

    #[test]
    fn code_edge_kind_str_round_trip_is_closed() {
        for kind in CodeEdgeKind::ALL {
            let s = kind.as_str();
            assert_eq!(
                s.parse::<CodeEdgeKind>(),
                Ok(kind),
                "from_str failed for {s}"
            );
        }
        assert_eq!("".parse::<CodeEdgeKind>(), Err(()));
        assert_eq!("describes".parse::<CodeEdgeKind>(), Err(()));
        assert_eq!("CALLS".parse::<CodeEdgeKind>(), Err(()));
    }

    #[test]
    fn code_edge_kind_predicate_strings_are_snake_case() {
        for kind in CodeEdgeKind::ALL {
            let s = kind.as_str();
            assert!(
                s.chars()
                    .all(|c| c.is_ascii_lowercase() || c == '_' || c.is_ascii_digit()),
                "predicate {s} is not snake_case"
            );
        }
        assert_eq!(CodeEdgeKind::RoutesTo.as_str(), "routes_to");
    }

    #[test]
    fn unresolved_status_allows_only_pending_exits() {
        use UnresolvedReferenceStatus::*;
        assert!(Pending.can_transition_to(Resolved));
        assert!(Pending.can_transition_to(Failed));
        assert!(!Pending.can_transition_to(Pending));
        assert!(!Resolved.can_transition_to(Pending));
        assert!(!Resolved.can_transition_to(Failed));
        assert!(!Resolved.can_transition_to(Resolved));
        assert!(!Failed.can_transition_to(Pending));
        assert!(!Failed.can_transition_to(Resolved));
        assert!(!Failed.can_transition_to(Failed));
    }

    #[test]
    fn unresolved_reference_serde_camel_case_round_trip() {
        let record = UnresolvedReference {
            id: Id::from("unref-1"),
            graph_id: Some(KnowledgeGraphId::from("graph-1")),
            from_entity_id: EntityId::from("ent-1"),
            reference_name: "parse_config".to_string(),
            candidates: vec![EntityId::from("ent-2"), EntityId::from("ent-3")],
            status: UnresolvedReferenceStatus::Pending,
            path: "src/main.rs".to_string(),
            line: Some(42),
            scope: Scope {
                tenant: "t".to_string(),
                subject: None,
                workspace: None,
                session: None,
                environment: None,
            },
            created_at: chrono::Utc::now(),
            updated_at: None,
        };
        let json = serde_json::to_string(&record).expect("serialize");
        assert!(
            json.contains("\"fromEntityId\""),
            "camelCase field missing: {json}"
        );
        let back: UnresolvedReference = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, record);
    }
}
