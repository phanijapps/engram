//! Strict effective fusion input while retaining the existing fusion behavior type.
use crate::{RecallFusionConfig, RerankConfig};
use engram_domain::RerankStrategy;
use serde::{Deserialize, Deserializer};
use std::collections::BTreeMap;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FusionWire {
    rrf_k: u32,
    default_source_weight: f32,
    source_weights: BTreeMap<String, f32>,
    rerank: RerankWire,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RerankWire {
    strategy: RerankStrategy,
    lambda: f32,
}

pub(super) fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<RecallFusionConfig, D::Error> {
    let w = FusionWire::deserialize(d)?;
    Ok(RecallFusionConfig {
        rrf_k: w.rrf_k,
        default_source_weight: w.default_source_weight,
        source_weights: w.source_weights,
        rerank: Some(RerankConfig {
            strategy: w.rerank.strategy,
            lambda: w.rerank.lambda,
        }),
    })
}
