use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodeParentDecisionReason {
    Selected,
    BelowThreshold,
    ScorerPool,
    ResultLimit,
    PerFileLimit,
    EvidenceBudget,
    InvalidEvidence,
    SerializedByteLimit,
}

/// Bounded metadata for an already authorized implementation parent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodeParentDecision {
    pub parent_unit_id: String,
    pub reason: CodeParentDecisionReason,
    pub fusion_score: f32,
    #[serde(deserialize_with = "super::required_nullable")]
    pub rerank_score: Option<f32>,
    #[serde(deserialize_with = "super::required_nullable")]
    pub applied_threshold: Option<f32>,
    pub retrieved_segments: u32,
    pub scored_segments: u32,
}

pub(super) fn is_zero(value: &u32) -> bool {
    *value == 0
}
