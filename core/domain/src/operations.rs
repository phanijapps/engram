//! Operation payloads exchanged with services and adapters.
//!
//! These types describe write, retrieve, forget, ingest, and consolidation
//! requests without choosing a transport. Service implementations may add
//! internal commands, but public APIs should preserve these contract shapes.

use serde::{Deserialize, Serialize};

use crate::{
    ConsolidationRunId, ContextPayload, DeleteMode, EvidenceRef, MemoryContent, MemoryEvent,
    MemoryKind, MemoryLink, MemoryRecord, Policy, Provenance, Requester, Scope, SourceId,
    Timestamp,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteMemoryRequest {
    pub kind: MemoryKind,
    pub content: MemoryContent,
    pub scope: Scope,
    pub requester: Requester,
    pub provenance: Provenance,
    pub policy: Policy,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub links: Vec<MemoryLink>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteMemoryResponse {
    pub record: MemoryRecord,
    pub event: MemoryEvent,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deduplicated: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ForgetTargetType {
    Memory,
    Event,
    Source,
    Document,
    Chunk,
    Entity,
    Concept,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForgetRequest {
    pub target_type: ForgetTargetType,
    pub target_id: String,
    pub scope: Scope,
    pub requester: Requester,
    pub mode: DeleteMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ForgetStatus {
    Deleted,
    Redacted,
    Tombstoned,
    Archived,
    Denied,
    NotFound,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForgetResult {
    pub target_type: String,
    pub target_id: String,
    pub status: ForgetStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event: Option<MemoryEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsolidationStrategy {
    Manual,
    TimeWindow,
    EventCount,
    RetrievalFailure,
    Hybrid,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsolidationRequest {
    pub scope: Scope,
    pub requester: Requester,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since: Option<Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub until: Option<Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strategy: Option<ConsolidationStrategy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dry_run: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsolidationTrigger {
    Scheduled,
    OnDemand,
    WriteThreshold,
    EventThreshold,
    RetrievalFailure,
    PolicyExpiration,
    ManualReview,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsolidationRunStatus {
    Running,
    Completed,
    CompletedWithErrors,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsolidationTaskKind {
    Compaction,
    MemorySynthesis,
    BeliefSynthesis,
    BeliefContradictionDetection,
    BeliefPropagation,
    HierarchyBuild,
    TaxonomyEvolution,
    SemanticDriftDetection,
    ConflictResolution,
    Decay,
    Pruning,
    OrphanArchival,
    ProcedureExtraction,
    Evaluation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsolidationTaskStatus {
    Skipped,
    Completed,
    CompletedWithErrors,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsolidationError {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task: Option<ConsolidationTaskKind>,
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_id: Option<String>,
    pub recoverable: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsolidationTaskResult {
    pub task: ConsolidationTaskKind,
    pub status: ConsolidationTaskStatus,
    pub started_at: Timestamp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items_read: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items_written: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items_updated: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items_skipped: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_calls: Option<u64>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub errors: Vec<ConsolidationError>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub output_refs: Vec<EvidenceRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsolidationStats {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memories_read: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memories_written: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub beliefs_synthesized: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contradictions_detected: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hierarchy_nodes_created: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hierarchy_relations_created: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub records_decayed: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub records_pruned: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_calls: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsolidationRun {
    pub id: ConsolidationRunId,
    pub scope: Scope,
    pub requester: Requester,
    pub trigger: ConsolidationTrigger,
    pub status: ConsolidationRunStatus,
    pub started_at: Timestamp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<Timestamp>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub tasks: Vec<ConsolidationTaskResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stats: Option<ConsolidationStats>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub errors: Vec<ConsolidationError>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<crate::Metadata>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IngestMode {
    Full,
    Incremental,
    ChangedOnly,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IngestRequest {
    pub source_id: SourceId,
    pub scope: Scope,
    pub requester: Requester,
    pub mode: IngestMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dry_run: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetrieveResponse {
    pub context: ContextPayload,
}
