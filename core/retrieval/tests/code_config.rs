// STUB: AC1 — Rust-owned immutable code profiles; generic defaults unchanged.
use engram_domain::{CodeIntelligenceProfile, RerankStrategy};
use engram_retrieval::{CodeIntelligenceConfig, RecallFusionConfig};

#[test]
fn balanced_and_fast_have_independent_candidate_and_output_budgets() {
    let balanced = CodeIntelligenceConfig::for_profile(CodeIntelligenceProfile::BalancedV1);
    let fast = CodeIntelligenceConfig::for_profile(CodeIntelligenceProfile::FastV1);
    assert_eq!(balanced.index.body_token_target, 384);
    assert_eq!(balanced.index.context_token_budget, 64);
    assert_eq!(balanced.retrieval.candidates_per_lane, 50);
    assert_eq!(balanced.retrieval.result_limit, 10);
    assert_eq!(balanced.local_scorer.max_candidates, 50);
    assert_eq!(
        balanced.fusion.rerank.unwrap().strategy,
        RerankStrategy::CrossEncoder
    );
    assert_eq!(fast.retrieval.candidates_per_lane, 20);
    assert_eq!(fast.retrieval.graph_hops, 0);
    assert_eq!(fast.fusion.rerank.unwrap().strategy, RerankStrategy::None);
    assert!(RecallFusionConfig::default().rerank.is_none());
}

#[test]
fn code_profiles_reject_unsupported_strategies_and_impossible_token_budgets() {
    let mut c = CodeIntelligenceConfig::for_profile(CodeIntelligenceProfile::BalancedV1);
    assert!(c.validate().is_ok());
    assert!(c.validate_model_windows(512, 2, 8192, 3).is_ok());
    c.fusion.rerank.as_mut().unwrap().strategy = RerankStrategy::Mmr;
    assert!(c.validate().is_err());
    c.fusion.rerank.as_mut().unwrap().strategy = RerankStrategy::CrossEncoder;
    c.index.body_token_target = 500;
    assert!(c.validate_model_windows(512, 2, 8192, 3).is_err());
    c.index.body_token_target = 384;
    c.local_scorer.pair_token_limit = 512;
    assert!(c.validate_model_windows(512, 2, 8192, 3).is_err());
}

#[test]
fn effective_snapshot_is_complete_and_nested_fusion_is_strict() {
    let value = serde_json::to_value(CodeIntelligenceConfig::default()).unwrap();
    let decoded: CodeIntelligenceConfig = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(decoded, CodeIntelligenceConfig::default());
    let mut extra = value.clone();
    extra["fusion"]["unknown"] = serde_json::json!(true);
    assert!(serde_json::from_value::<CodeIntelligenceConfig>(extra).is_err());
    let mut incomplete = value;
    incomplete["fusion"]
        .as_object_mut()
        .unwrap()
        .remove("rerank");
    assert!(serde_json::from_value::<CodeIntelligenceConfig>(incomplete).is_err());
    let mut embedded = CodeIntelligenceConfig::default();
    embedded.fusion.rerank = None;
    assert!(
        embedded.validate().is_err(),
        "typed SDK cannot emit null effective rerank"
    );
}

#[test]
fn operator_bounds_require_usable_candidates_and_expansion() {
    let mut c = CodeIntelligenceConfig::default();
    c.retrieval.result_limit = 51;
    assert!(c.validate().is_err());
    c.retrieval.result_limit = 10;
    c.local_scorer.max_candidates = 9;
    assert!(c.validate().is_err());
    c.local_scorer.max_candidates = 50;
    c.retrieval.graph_neighbor_limit = 0;
    assert!(c.validate().is_err());
    c.retrieval.graph_hops = 0;
    assert!(c.validate().is_ok());
    c.fusion.default_source_weight = 0.0;
    assert!(c.validate().is_err());
    c.fusion.source_weights.insert("lexical".into(), 1.0);
    assert!(
        c.validate().is_ok(),
        "zero vector weight still leaves its lane enabled"
    );
    assert_eq!(c.retrieval.enabled_lanes.len(), 2);
    c.fusion.source_weights.insert("lexical".into(), f32::NAN);
    assert!(c.validate().is_err());
}
