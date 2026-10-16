//! Shared code profiles and deterministic validation. Hosts own file/env loading.
mod fusion_wire;
mod overlay;
mod profiles;
pub use overlay::*;
mod validation;

use crate::RecallFusionConfig;
use engram_domain::CodeIntelligenceProfile;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodeIntelligenceConfig {
    pub schema_version: u32,
    pub profile: CodeIntelligenceProfile,
    pub index: CodeIndexConfig,
    pub query: CodeQueryConfig,
    pub retrieval: CodeRetrievalConfig,
    #[serde(deserialize_with = "fusion_wire::deserialize")]
    pub fusion: RecallFusionConfig,
    pub local_scorer: CodeLocalScorerConfig,
    pub evidence: CodeEvidenceConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodeIndexConfig {
    pub body_token_target: u32,
    pub context_token_budget: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CodeQueryInstruction {
    #[serde(rename = "legacy-v1")]
    LegacyV1,
    #[serde(rename = "none")]
    None,
    #[serde(rename = "bge-retrieval-v1")]
    BgeRetrievalV1,
}
impl CodeQueryInstruction {
    pub fn prefix(self) -> &'static str {
        match self {
            Self::LegacyV1 => "query: ",
            Self::None => "",
            Self::BgeRetrievalV1 => "Represent this sentence for searching relevant passages: ",
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodeQueryConfig {
    pub instruction: CodeQueryInstruction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodeRetrievalLane {
    Vector,
    Lexical,
    Graph,
    AssociativeGraph,
    CommunitySummary,
}
impl CodeRetrievalLane {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Vector => "vector",
            Self::Lexical => "lexical",
            Self::Graph => "graph",
            Self::AssociativeGraph => "associative_graph",
            Self::CommunitySummary => "community_summary",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodeRetrievalConfig {
    pub candidates_per_lane: u32,
    pub result_limit: u32,
    pub enabled_lanes: Vec<CodeRetrievalLane>,
    pub graph_hops: u32,
    pub graph_neighbor_limit: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodeScorerFailurePolicy {
    Degrade,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodeLocalScorerConfig {
    pub model_id: String,
    pub query_token_limit: u32,
    pub pair_token_limit: u32,
    #[serde(default = "profiles::default_max_parents")]
    pub max_parents: u32,
    pub max_candidates: u32,
    pub batch_size: u32,
    pub on_failure: CodeScorerFailurePolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodeEvidenceConfig {
    #[serde(default = "profiles::default_weak_parent_order")]
    pub weak_parent_order: CodeWeakParentOrder,
    pub token_budget: u32,
    pub max_supporting_units: u32,
    pub max_per_file: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodeWeakParentOrder {
    BestLaneRank,
    Scorer,
}
