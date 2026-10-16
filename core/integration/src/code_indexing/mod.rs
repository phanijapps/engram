//! Focused derived-code preparation and bounded embedding maintenance.
mod context;
mod derivation;
mod input;
mod maintenance;
pub(crate) use maintenance::backfill;

pub(crate) fn index_config(provider: &crate::EngramProvider) -> engram_retrieval::CodeIndexConfig {
    provider
        .code_configuration()
        .map(|c| c.effective.index.clone())
        .unwrap_or_else(|| {
            engram_retrieval::CodeIntelligenceConfig::for_profile(
                engram_domain::CodeIntelligenceProfile::BalancedV1,
            )
            .index
        })
}
