//! Presence-preserving operator overlays; omission never resets profile defaults.
use super::{CodeQueryInstruction, CodeRetrievalLane, CodeScorerFailurePolicy};
use engram_domain::{CodeIntelligenceProfile, RerankStrategy};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::BTreeMap;

fn present<'de, D, T>(de: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(de).map(Some)
}

macro_rules! field_admission {
    ($name:ident, $path:literal) => {
        fn $name<'de, D, T>(de: D) -> Result<Option<T>, D::Error>
        where
            D: Deserializer<'de>,
            T: Deserialize<'de>,
        {
            present(de).map_err(|e| serde::de::Error::custom(format!("{}: {e}", $path)))
        }
    };
}
field_admission!(present_schema_version, "schema_version");
field_admission!(present_profile, "profile");
field_admission!(present_index, "index");
field_admission!(present_query, "query");
field_admission!(present_retrieval, "retrieval");
field_admission!(present_fusion, "fusion");
field_admission!(present_local_scorer, "local_scorer");
field_admission!(present_evidence, "evidence");
field_admission!(present_body_token_target, "index.body_token_target");
field_admission!(present_context_token_budget, "index.context_token_budget");
field_admission!(present_instruction, "query.instruction");
field_admission!(present_candidates_per_lane, "retrieval.candidates_per_lane");
field_admission!(present_result_limit, "retrieval.result_limit");
field_admission!(present_enabled_lanes, "retrieval.enabled_lanes");
field_admission!(present_graph_hops, "retrieval.graph_hops");
field_admission!(
    present_graph_neighbor_limit,
    "retrieval.graph_neighbor_limit"
);
field_admission!(present_rrf_k, "fusion.rrf_k");
field_admission!(
    present_default_source_weight,
    "fusion.default_source_weight"
);
field_admission!(present_source_weights, "fusion.source_weights");
field_admission!(present_rerank, "fusion.rerank");
field_admission!(present_strategy, "fusion.rerank.strategy");
field_admission!(present_lambda, "fusion.rerank.lambda");
field_admission!(present_model_id, "local_scorer.model_id");
field_admission!(present_query_token_limit, "local_scorer.query_token_limit");
field_admission!(present_pair_token_limit, "local_scorer.pair_token_limit");
field_admission!(present_max_candidates, "local_scorer.max_candidates");
field_admission!(present_batch_size, "local_scorer.batch_size");
field_admission!(present_on_failure, "local_scorer.on_failure");
field_admission!(present_token_budget, "evidence.token_budget");
field_admission!(
    present_max_supporting_units,
    "evidence.max_supporting_units"
);
field_admission!(present_max_per_file, "evidence.max_per_file");

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodeIntelligenceConfigOverlay {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_schema_version"
    )]
    pub schema_version: Option<u32>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_profile"
    )]
    pub profile: Option<CodeIntelligenceProfile>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_index"
    )]
    pub index: Option<CodeIndexOverlay>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_query"
    )]
    pub query: Option<CodeQueryOverlay>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_retrieval"
    )]
    pub retrieval: Option<CodeRetrievalOverlay>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_fusion"
    )]
    pub fusion: Option<CodeFusionOverlay>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_local_scorer"
    )]
    pub local_scorer: Option<CodeScorerOverlay>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_evidence"
    )]
    pub evidence: Option<CodeEvidenceOverlay>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodeIndexOverlay {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_body_token_target"
    )]
    pub body_token_target: Option<u32>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_context_token_budget"
    )]
    pub context_token_budget: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodeQueryOverlay {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_instruction"
    )]
    pub instruction: Option<CodeQueryInstruction>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodeRetrievalOverlay {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_candidates_per_lane"
    )]
    pub candidates_per_lane: Option<u32>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_result_limit"
    )]
    pub result_limit: Option<u32>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_enabled_lanes"
    )]
    pub enabled_lanes: Option<Vec<CodeRetrievalLane>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_graph_hops"
    )]
    pub graph_hops: Option<u32>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_graph_neighbor_limit"
    )]
    pub graph_neighbor_limit: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodeFusionOverlay {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_rrf_k"
    )]
    pub rrf_k: Option<u32>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_default_source_weight"
    )]
    pub default_source_weight: Option<f32>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_source_weights"
    )]
    pub source_weights: Option<BTreeMap<String, f32>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_rerank"
    )]
    pub rerank: Option<CodeRerankOverlay>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodeRerankOverlay {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_strategy"
    )]
    pub strategy: Option<RerankStrategy>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_lambda"
    )]
    pub lambda: Option<f32>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodeScorerOverlay {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_model_id"
    )]
    pub model_id: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_query_token_limit"
    )]
    pub query_token_limit: Option<u32>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_pair_token_limit"
    )]
    pub pair_token_limit: Option<u32>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_max_candidates"
    )]
    pub max_candidates: Option<u32>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_batch_size"
    )]
    pub batch_size: Option<u32>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_on_failure"
    )]
    pub on_failure: Option<CodeScorerFailurePolicy>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodeEvidenceOverlay {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_token_budget"
    )]
    pub token_budget: Option<u32>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_max_supporting_units"
    )]
    pub max_supporting_units: Option<u32>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_max_per_file"
    )]
    pub max_per_file: Option<u32>,
}
