//! Configuration for deterministic retrieval fusion.
//!
//! Source weights let applications tune candidate contributions without making
//! fusion depend on a concrete index implementation or provider.

use std::collections::BTreeMap;

use engram_domain::RerankStrategy;
use engram_runtime::{CoreError, CoreResult};
use serde::{Deserialize, Serialize};

/// Source weighting configuration for weighted-sum retrieval fusion.
#[derive(Debug, Clone, PartialEq)]
pub struct WeightedFusionConfig {
    default_source_weight: f32,
    source_weights: BTreeMap<String, f32>,
}

impl WeightedFusionConfig {
    /// Creates a config with one default weight and optional source overrides.
    ///
    /// Weights must be finite and non-negative. A zero weight keeps a source
    /// visible for traceability while preventing it from increasing the fused
    /// score.
    pub fn new(
        default_source_weight: f32,
        source_weights: BTreeMap<String, f32>,
    ) -> CoreResult<Self> {
        validate_weight("default", default_source_weight)?;
        for (source, weight) in &source_weights {
            validate_weight(source, *weight)?;
        }
        Ok(Self {
            default_source_weight,
            source_weights,
        })
    }

    /// Returns the configured weight for a candidate source.
    ///
    /// Source names are compared exactly against the configured overrides. When
    /// a source has no override, the default weight is returned so fusion can
    /// keep accepting candidates from new adapters without a config change.
    pub fn source_weight(&self, source: &str) -> f32 {
        self.source_weights
            .get(source)
            .copied()
            .unwrap_or(self.default_source_weight)
    }
}

impl Default for WeightedFusionConfig {
    fn default() -> Self {
        Self {
            default_source_weight: 1.0,
            source_weights: BTreeMap::new(),
        }
    }
}

fn validate_weight(source: &str, weight: f32) -> CoreResult<()> {
    if !weight.is_finite() || weight < 0.0 {
        return Err(CoreError::InvalidRequest {
            reason: format!("source weight must be finite and non-negative: {source}={weight}"),
        });
    }
    Ok(())
}

/// Configuration for reciprocal-rank fusion (RRF).
///
/// Reranking strength is configurable: `k` is the RRF constant (lower ⇒ top
/// ranks dominate more aggressively; higher ⇒ flatter), and per-source weights
/// scale each retriever's contribution so a deployment can bias graph vs vector
/// results (weighted RRF: `weight / (k + rank)`). Defaults apply when config is
/// absent: `k = 60`, equal weights (pure RRF).
#[derive(Debug, Clone, PartialEq)]
pub struct ReciprocalFusionConfig {
    k: u32,
    default_source_weight: f32,
    source_weights: BTreeMap<String, f32>,
}

impl ReciprocalFusionConfig {
    /// Creates a config with RRF constant `k`, a default per-source weight, and
    /// optional per-source overrides.
    ///
    /// `k` must be `>= 1`. Weights must be finite and non-negative; a zero weight
    /// keeps a source visible for traceability while removing its contribution.
    pub fn new(
        k: u32,
        default_source_weight: f32,
        source_weights: BTreeMap<String, f32>,
    ) -> CoreResult<Self> {
        if k == 0 {
            return Err(CoreError::InvalidRequest {
                reason: "RRF k must be greater than zero".to_owned(),
            });
        }
        validate_weight("default", default_source_weight)?;
        for (source, weight) in &source_weights {
            validate_weight(source, *weight)?;
        }
        Ok(Self {
            k,
            default_source_weight,
            source_weights,
        })
    }

    /// The RRF constant.
    pub fn k(&self) -> u32 {
        self.k
    }

    /// The configured weight for a candidate source (override or default).
    pub fn source_weight(&self, source: &str) -> f32 {
        self.source_weights
            .get(source)
            .copied()
            .unwrap_or(self.default_source_weight)
    }
}

impl Default for ReciprocalFusionConfig {
    /// Defaults when config is absent: `k = 60`, equal weights (pure RRF).
    fn default() -> Self {
        Self {
            k: crate::reciprocal::DEFAULT_RRF_K,
            default_source_weight: 1.0,
            source_weights: BTreeMap::new(),
        }
    }
}

// ---- External recall-fusion configuration (RFC-0019) ---------------------
// Operator-facing, serializable config loaded from a `[recall_fusion]` profile
// section or `.engram/recall.json`. Validated on load via `to_reciprocal_config`,
// which builds the internal `ReciprocalFusionConfig` (weighted RRF) that
// `SqlUnifiedRecall` fuses with.

/// External recall-fusion configuration: RRF `k`, a default per-lane weight,
/// per-lane `source_weights` overrides, and an optional reranker. Lane source
/// tags are the weight keys (normalized vocabulary: `vector`, `lexical`,
/// `graph`, `associative_graph`, `community_summary`, `temporal`, `facts`,
/// `belief`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecallFusionConfig {
    /// RRF constant (`>= 1`). Lower ⇒ top ranks dominate more aggressively;
    /// default 60.
    #[serde(default = "default_rrf_k")]
    pub rrf_k: u32,
    /// Weight applied to any lane source not named in `source_weights`.
    /// Default 1.0 (pure RRF when no overrides are set).
    #[serde(default = "default_weight")]
    pub default_source_weight: f32,
    /// Per-lane-source weight overrides (keys = normalized lane tags).
    #[serde(default)]
    pub source_weights: BTreeMap<String, f32>,
    /// Optional reranker selection.
    #[serde(default)]
    pub rerank: Option<RerankConfig>,
}

impl RecallFusionConfig {
    /// Validates and builds the internal weighted-RRF config. Errors on `k == 0`
    /// or non-finite/negative weights.
    pub fn to_reciprocal_config(&self) -> CoreResult<ReciprocalFusionConfig> {
        ReciprocalFusionConfig::new(
            self.rrf_k,
            self.default_source_weight,
            self.source_weights.clone(),
        )
    }
}

impl Default for RecallFusionConfig {
    fn default() -> Self {
        Self {
            rrf_k: default_rrf_k(),
            default_source_weight: default_weight(),
            source_weights: BTreeMap::new(),
            rerank: None,
        }
    }
}

fn default_rrf_k() -> u32 {
    crate::reciprocal::DEFAULT_RRF_K
}

fn default_weight() -> f32 {
    1.0
}

/// Reranker selection + parameters. `strategy` reuses the domain
/// [`engram_domain::RerankStrategy`] enum; engram wires `None`/`Mmr`/
/// `CrossEncoder` today (the other variants deserialize but are not dispatched).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RerankConfig {
    /// Which reranker to apply.
    #[serde(default = "default_rerank_strategy")]
    pub strategy: RerankStrategy,
    /// MMR relevance/diversity trade-off in `[0, 1]` (1 ⇒ pure relevance,
    /// 0 ⇒ max diversity). Only consulted for [`RerankStrategy::Mmr`].
    #[serde(default = "default_mmr_lambda")]
    pub lambda: f32,
}

impl Default for RerankConfig {
    fn default() -> Self {
        Self {
            strategy: default_rerank_strategy(),
            lambda: default_mmr_lambda(),
        }
    }
}

fn default_rerank_strategy() -> RerankStrategy {
    RerankStrategy::None
}

fn default_mmr_lambda() -> f32 {
    0.5
}

#[cfg(test)]
mod recall_fusion_config_tests {
    use super::*;

    #[test]
    fn default_is_equal_weight_rrf() {
        let cfg = RecallFusionConfig::default();
        assert_eq!(cfg.rrf_k, crate::reciprocal::DEFAULT_RRF_K);
        assert_eq!(cfg.default_source_weight, 1.0);
        assert!(cfg.source_weights.is_empty());
        assert!(cfg.rerank.is_none());
        let recip = cfg.to_reciprocal_config().unwrap();
        assert_eq!(recip.source_weight("anything"), 1.0);
    }

    #[test]
    fn to_reciprocal_config_builds_with_weights() {
        let mut weights = BTreeMap::new();
        weights.insert("vector".to_string(), 0.7);
        weights.insert("lexical".to_string(), 0.3);
        let cfg = RecallFusionConfig {
            rrf_k: 42,
            default_source_weight: 1.0,
            source_weights: weights,
            rerank: None,
        };
        let recip = cfg.to_reciprocal_config().unwrap();
        assert_eq!(recip.k(), 42);
        assert_eq!(recip.source_weight("vector"), 0.7);
        assert_eq!(recip.source_weight("lexical"), 0.3);
        assert_eq!(recip.source_weight("facts"), 1.0); // default fallback
    }

    #[test]
    fn rejects_k_zero() {
        let cfg = RecallFusionConfig {
            rrf_k: 0,
            default_source_weight: 1.0,
            source_weights: BTreeMap::new(),
            rerank: None,
        };
        assert!(cfg.to_reciprocal_config().is_err());
    }

    #[test]
    fn rejects_negative_and_nan_weights() {
        let neg = RecallFusionConfig {
            rrf_k: 60,
            default_source_weight: -0.5,
            source_weights: BTreeMap::new(),
            rerank: None,
        };
        assert!(neg.to_reciprocal_config().is_err());
        let mut bad = BTreeMap::new();
        bad.insert("vector".to_string(), f32::NAN);
        let nan = RecallFusionConfig {
            rrf_k: 60,
            default_source_weight: 1.0,
            source_weights: bad,
            rerank: None,
        };
        assert!(nan.to_reciprocal_config().is_err());
    }

    #[test]
    fn serde_round_trips() {
        let json = r#"{"rrf_k":50,"default_source_weight":1.0,"source_weights":{"vector":0.7},"rerank":{"strategy":"mmr","lambda":0.4}}"#;
        let cfg: RecallFusionConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.rrf_k, 50);
        assert_eq!(
            cfg.to_reciprocal_config().unwrap().source_weight("vector"),
            0.7
        );
        let rerank = cfg.rerank.as_ref().unwrap();
        assert!(matches!(rerank.strategy, RerankStrategy::Mmr));
        assert_eq!(rerank.lambda, 0.4);
        let reser = serde_json::to_string(&cfg).unwrap();
        let cfg2: RecallFusionConfig = serde_json::from_str(&reser).unwrap();
        assert_eq!(cfg, cfg2);
    }

    #[test]
    fn rerank_strategy_snake_case() {
        let s: RerankStrategy = serde_json::from_str("\"cross_encoder\"").unwrap();
        assert!(matches!(s, RerankStrategy::CrossEncoder));
        let n: RerankStrategy = serde_json::from_str("\"none\"").unwrap();
        assert!(matches!(n, RerankStrategy::None));
    }

    #[test]
    fn serde_defaults_missing_fields() {
        let json = r#"{"source_weights":{"lexical":0.3}}"#;
        let cfg: RecallFusionConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.rrf_k, crate::reciprocal::DEFAULT_RRF_K); // defaulted
        assert_eq!(cfg.default_source_weight, 1.0); // defaulted
    }
}
