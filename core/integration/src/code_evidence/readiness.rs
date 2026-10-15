//! Bootstrap readiness from actual held handles, independent of corpus quality.
use crate::EngramProvider;
use engram_domain::{CapabilityReason, CapabilityState, CodeIntelligenceProfile, RerankStrategy};
use engram_retrieval::{CodeIntelligenceConfig, CodeRetrievalLane};

pub(crate) fn states(provider: &EngramProvider) -> (CapabilityState, CapabilityState) {
    let defaults = CodeIntelligenceConfig::for_profile(CodeIntelligenceProfile::BalancedV1);
    let config = provider
        .code_configuration()
        .map(|c| &c.effective)
        .unwrap_or(&defaults);
    let unavailable = || CapabilityState::Unsupported {
        reason: CapabilityReason::ProviderUnavailable,
    };
    let source = provider.knowledge().is_some_and(|k| {
        k.code_sources().is_some() && k.code_segments().is_some() && k.document_heads().is_some()
    });
    let representation = provider
        .embedding_provider()
        .filter(|e| e.is_local() && e.asset_identity().is_some())
        .and_then(|e| {
            crate::code_representation::code_representation(e.as_ref(), &config.index).ok()
        });
    let evidence = if !source || representation.is_none() {
        unavailable()
    } else if config
        .retrieval
        .enabled_lanes
        .iter()
        .any(|lane| match lane {
            CodeRetrievalLane::Vector => provider.code_vectors().is_none_or(|i| {
                representation
                    .as_ref()
                    .is_none_or(|r| i.embedding_space() != &r.embedding_space)
            }),
            _ => provider.code_retrieval_lane(*lane).is_none(),
        })
        || (config.retrieval.graph_hops > 0 && provider.graph().is_none())
    {
        CapabilityState::Degraded {
            reason: CapabilityReason::ProviderUnavailable,
        }
    } else {
        CapabilityState::Supported
    };
    let scorer = if !config
        .fusion
        .rerank
        .as_ref()
        .is_some_and(|r| r.strategy == RerankStrategy::CrossEncoder)
    {
        CapabilityState::Unsupported {
            reason: CapabilityReason::FeatureDisabled,
        }
    } else if provider
        .code_scorer()
        .filter(|s| s.is_local() && s.supports_code_input())
        .and_then(|s| s.identity())
        .is_some_and(|id| {
            id.model_id == config.local_scorer.model_id
                && !id.manifest_id.is_empty()
                && !id.tokenizer_id.is_empty()
                && !id.input_contract_id.is_empty()
                && id.score_contract_id == "logits-column-0/raw-v1"
                && id.window >= config.local_scorer.pair_token_limit
        })
    {
        CapabilityState::Supported
    } else {
        unavailable()
    };
    (evidence, scorer)
}
