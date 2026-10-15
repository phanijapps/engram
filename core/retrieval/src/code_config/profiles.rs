use super::*;
use crate::{RecallFusionConfig, RerankConfig};
use engram_domain::{CodeIntelligenceProfile, RerankStrategy};

impl CodeIntelligenceConfig {
    /// Profile expansion is the sole default definition; overlays do not default fields.
    pub fn for_profile(profile: CodeIntelligenceProfile) -> Self {
        let fast = profile == CodeIntelligenceProfile::FastV1;
        Self {
            schema_version: 1,
            profile,
            index: CodeIndexConfig {
                body_token_target: 384,
                context_token_budget: 64,
            },
            query: CodeQueryConfig {
                instruction: CodeQueryInstruction::None,
            },
            retrieval: CodeRetrievalConfig {
                candidates_per_lane: if fast { 20 } else { 50 },
                result_limit: 10,
                enabled_lanes: vec![CodeRetrievalLane::Vector, CodeRetrievalLane::Lexical],
                graph_hops: if fast { 0 } else { 1 },
                graph_neighbor_limit: 20,
            },
            fusion: RecallFusionConfig {
                rerank: Some(RerankConfig {
                    strategy: if fast {
                        RerankStrategy::None
                    } else {
                        RerankStrategy::CrossEncoder
                    },
                    lambda: 0.5,
                }),
                ..RecallFusionConfig::default()
            },
            local_scorer: CodeLocalScorerConfig {
                model_id: "jinaai/jina-reranker-v1-turbo-en".into(),
                query_token_limit: 128,
                pair_token_limit: 1024,
                max_candidates: 50,
                batch_size: 8,
                on_failure: CodeScorerFailurePolicy::Degrade,
            },
            evidence: CodeEvidenceConfig {
                token_budget: 6000,
                max_supporting_units: 20,
                max_per_file: 4,
            },
        }
    }
}
impl Default for CodeIntelligenceConfig {
    fn default() -> Self {
        Self::for_profile(CodeIntelligenceProfile::BalancedV1)
    }
}
