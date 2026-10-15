use super::{CodeEvidenceSourceRef, CodeTokenCoverage};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CodeIntelligenceProfile {
    #[serde(rename = "balanced-v1")]
    BalancedV1,
    #[serde(rename = "fast-v1")]
    FastV1,
}
impl CodeIntelligenceProfile {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::BalancedV1 => "balanced-v1",
            Self::FastV1 => "fast-v1",
        }
    }
}
use crate::{FusionTrace, RetrievalSourceFailure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodeSourceCategory {
    Implementation,
    Test,
    Generated,
    Documentation,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodeEvidenceKind {
    Code,
    Document,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodePrimaryKind {
    Code,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodePrimaryCategory {
    Implementation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodeEvidenceReranker {
    None,
    CrossEncoder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodeContextKind {
    Path,
    Symbol,
    Signature,
    Parent,
    Import,
    UnresolvedImport,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodeExcerptPart {
    pub source: CodeEvidenceSourceRef,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodeContextFact {
    pub kind: CodeContextKind,
    pub value: String,
    pub sources: Vec<CodeEvidenceSourceRef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodeSegmentEvidence {
    pub segment_id: String,
    pub sources: Vec<CodeEvidenceSourceRef>,
    pub fusion_traces: Vec<FusionTrace>,
    pub fusion_score: f32,
    #[serde(deserialize_with = "crate::code_evidence::required_nullable")]
    pub rerank_score: Option<f32>,
    pub embedding_input: CodeTokenCoverage,
    #[serde(deserialize_with = "crate::code_evidence::required_nullable")]
    pub rerank_input: Option<CodeTokenCoverage>,
    /// Exact successful scorer-view inputs; original embedding ranges remain
    /// in `sources`. Absent for disabled/failed scoring.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rerank_sources: Option<Vec<CodeEvidenceSourceRef>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rerank_context: Option<Vec<CodeContextFact>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodeEvidenceItem {
    /// Canonical descriptor-unit id, not the canonical chunk lookup id.
    pub parent_unit_id: String,
    pub name: String,
    pub kind: CodePrimaryKind,
    pub source_category: CodePrimaryCategory,
    pub excerpts: Vec<CodeExcerptPart>,
    pub segments: Vec<CodeSegmentEvidence>,
    pub context: Vec<CodeContextFact>,
    pub partial: bool,
    pub fusion_score: f32,
    #[serde(deserialize_with = "crate::code_evidence::required_nullable")]
    pub rerank_score: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodeSupportingUnit {
    pub unit_id: String,
    pub name: String,
    pub kind: CodeEvidenceKind,
    pub source_category: CodeSourceCategory,
    pub excerpts: Vec<CodeExcerptPart>,
    pub partial: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodeEvidenceStatus {
    CandidatesAvailable,
    InsufficientEvidence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodeScorerState {
    Ready,
    Disabled,
    Unavailable,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodeCalibrationState {
    Validated,
    Unvalidated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodeScoreSpace {
    Rrf,
    CrossEncoder,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodeEvidenceDiagnostics {
    pub config_hash: String,
    pub profile: CodeIntelligenceProfile,
    pub requested_reranker: CodeEvidenceReranker,
    pub effective_reranker: CodeEvidenceReranker,
    pub scorer_state: CodeScorerState,
    #[serde(deserialize_with = "crate::code_evidence::required_nullable")]
    pub scorer_manifest_id: Option<String>,
    pub calibration_state: CodeCalibrationState,
    pub candidate_counts: BTreeMap<String, u32>,
    pub omitted_units: u32,
    pub truncated: bool,
    pub reindex_required: bool,
    pub config_origins: BTreeMap<String, String>,
    pub query_representation_id: String,
    pub phase_ms: BTreeMap<String, f64>,
    pub omitted_segments: u32,
    pub omitted_parents: u32,
    pub scored_pairs: u32,
    #[serde(deserialize_with = "crate::code_evidence::required_nullable")]
    pub calibration_id: Option<String>,
    #[serde(deserialize_with = "crate::code_evidence::required_nullable")]
    pub threshold_score_space: Option<CodeScoreSpace>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodeEvidencePacket {
    pub items: Vec<CodeEvidenceItem>,
    pub evidence_status: CodeEvidenceStatus,
    pub diagnostics: CodeEvidenceDiagnostics,
    pub source_failures: Vec<RetrievalSourceFailure>,
    pub supporting_units: Vec<CodeSupportingUnit>,
}
