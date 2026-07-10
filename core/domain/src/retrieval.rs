//! Retrieval request and result contracts.
//!
//! Retrieval is modeled as a policy-checked pipeline: candidate sources produce
//! results, fusion/reranking explains score changes, and context composition
//! reports omissions or degraded sources instead of hiding partial failure.

use serde::{Deserialize, Serialize};

use crate::{
    ConceptId, EntityId, KnowledgeChunkKind, MemoryKind, Metadata, Policy, Provenance, Requester,
    Scope, SourceKind, Timestamp,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetrievalMode {
    Temporal,
    Cue,
    Hierarchical,
    Semantic,
    Graph,
    Keyword,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryFilter {
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub memory_kinds: Vec<MemoryKind>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub source_kinds: Vec<SourceKind>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub chunk_kinds: Vec<KnowledgeChunkKind>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub concept_ids: Vec<ConceptId>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub entity_ids: Vec<EntityId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since: Option<Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub until: Option<Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_confidence: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_archived: Option<bool>,
    /// Bi-temporal "as of" filter — returns only entity versions valid at this
    /// instant (valid_from <= as_of < valid_until). Distinct from `since`/`until`
    /// which filter on observed time. ADR-0021.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub as_of: Option<Timestamp>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CueOperator {
    Equals,
    Contains,
    StartsWith,
    EndsWith,
    Exists,
    In,
    Range,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Cue {
    pub slot: String,
    pub value: crate::Scalar,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operator: Option<CueOperator>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weight: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextBudget {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_items: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_bytes: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetrievalRequest {
    pub query: String,
    pub scope: Scope,
    pub requester: Requester,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub modes: Vec<RetrievalMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filters: Option<QueryFilter>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub cues: Vec<Cue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub budget: Option<ContextBudget>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_explanations: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetrievalTargetType {
    Memory,
    Event,
    Chunk,
    Document,
    Entity,
    Relationship,
    Concept,
    Belief,
    Contradiction,
    HierarchyNode,
    HierarchyRelation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetrievalScore {
    pub total: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relevance: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recency: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cue_match: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hierarchical_fit: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy_fit: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetrievalExplanation {
    pub reason: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub matched_cues: Vec<Cue>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub matched_terms: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub path: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_summary: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FusionStrategy {
    None,
    WeightedSum,
    ReciprocalRankFusion,
    MaxScore,
    LearnedRanker,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RerankStrategy {
    None,
    Mmr,
    CrossEncoder,
    LlmJudge,
    PolicyPriority,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FusionTrace {
    /// Unique identifier for this retrieval query.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query_id: Option<String>,

    /// Name of the vector index used (if applicable).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vector_index: Option<String>,

    /// Time taken to generate embeddings in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding_time_ms: Option<u64>,

    /// Time taken for vector search in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_time_ms: Option<u64>,

    /// Source that produced this result.
    pub source: String,

    /// Rank of this result in the source output.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_rank: Option<u32>,

    /// Score from the source.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_score: Option<f32>,

    /// Final score after fusion and reranking.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<f32>,

    /// Final rank after fusion and reranking.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rank: Option<u32>,

    /// Fusion strategy used.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fusion_strategy: Option<FusionStrategy>,

    /// Score after fusion.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fusion_score: Option<f32>,

    /// Rerank strategy used.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rerank_strategy: Option<RerankStrategy>,

    /// Score after reranking.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rerank_score: Option<f32>,

    /// Reason why this result was discarded (if applicable).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discard_reason: Option<String>,

    /// IDs of results this was deduplicated with.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub deduplicated_with: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetrievalResult {
    pub id: String,
    pub target_type: RetrievalTargetType,
    pub target_id: String,
    pub content: String,
    pub score: RetrievalScore,
    pub provenance: Provenance,
    pub policy: Policy,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explanation: Option<RetrievalExplanation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fusion_trace: Option<FusionTrace>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OmittedReason {
    PolicyDenied,
    BudgetExceeded,
    LowScore,
    Expired,
    Redacted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OmittedResult {
    pub target_type: RetrievalTargetType,
    pub target_id: String,
    pub reason: OmittedReason,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceFailureSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetrievalSourceFailure {
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<RetrievalMode>,
    pub severity: SourceFailureSeverity,
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    pub degraded: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextPayload {
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub items: Vec<RetrievalResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub budget: Option<ContextBudget>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub omitted: Vec<OmittedResult>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub source_failures: Vec<RetrievalSourceFailure>,
    pub created_at: Timestamp,
}
