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

/// The documented lane-source vocabulary — the set of `source_weights` keys
/// that actually match a lane's stamped `fusion_trace.source`. A key outside
/// this set is silently inert (the lane it targets does not exist), the same
/// defect class RFC-0019 fixed for the mixed `vector.semantic` / `unknown`
/// tags. `to_reciprocal_config` warns on unknown keys so a typo like
/// `"vectors"` (vs `"vector"`) surfaces instead of silently no-op'ing.
///
/// Kept as a plain slice (no `const Vec`) so it stays `'static` and clippy-
/// clean. Update in lockstep with the lane tags stamped by the adapters
/// (`adapters/sqlite/src/{knowledge,memory/vector}/retrieval.rs`,
/// `adapters/retrieval/{associative-graph,community-summary,tantivy-lexical}`,
/// `core/integration/src/sqlite/recall.rs`).
pub const KNOWN_LANE_TAGS: &[&str] = &[
    "vector",
    "lexical",
    "graph",
    "associative_graph",
    "community_summary",
    "temporal",
    "facts",
    "belief",
];

/// Returns the `source_weights` keys that are not in [`KNOWN_LANE_TAGS`].
/// Pure (no I/O) so it is unit-testable independent of stderr capture; the
/// load path emits the warning via `eprintln!` using this helper.
pub fn unknown_lane_keys(weights: &BTreeMap<String, f32>) -> Vec<&str> {
    weights
        .keys()
        .filter(|k| !KNOWN_LANE_TAGS.contains(&k.as_str()))
        .map(String::as_str)
        .collect()
}

/// Validates that `lambda ∈ [0, 1]`. MMR's relevance/diversity trade-off is
/// defined on this interval; a value outside it is an operator error (not a
/// graceful degrade), surfaced as a typed `InvalidRequest` for symmetry with
/// weight validation.
fn validate_lambda(lambda: f32) -> CoreResult<()> {
    if !lambda.is_finite() || lambda < 0.0 || lambda > 1.0 {
        return Err(CoreError::InvalidRequest {
            reason: format!("rerank.lambda must be in [0, 1], got {lambda}"),
        });
    }
    Ok(())
}

/// External recall-fusion configuration: RRF `k`, a default per-lane weight,
/// per-lane `source_weights` overrides, and an optional reranker. Lane source
/// tags are the weight keys (normalized vocabulary: [`KNOWN_LANE_TAGS`]).
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
    /// Validates and builds the internal weighted-RRF config. Errors on `k == 0`,
    /// non-finite/negative weights, or a `rerank.lambda` outside `[0, 1]`. Warns
    /// (stderr) on `source_weights` keys outside [`KNOWN_LANE_TAGS`] — a typo'd
    /// key (e.g. `"vectors"` vs `"vector"`) would otherwise be silently inert,
    /// the same defect class this RFC fixed for the lane tags themselves. The
    /// warning is non-fatal so future lane names do not break older configs.
    pub fn to_reciprocal_config(&self) -> CoreResult<ReciprocalFusionConfig> {
        // Lambda validation (Nit 6): MMR's relevance/diversity trade-off is
        // defined on [0, 1]; an out-of-range value is an operator error.
        if let Some(rerank) = &self.rerank
            && let Err(e) = validate_lambda(rerank.lambda)
        {
            return Err(e);
        }
        // Vocab check (Concern 5): warn on keys no lane will ever match. Warn
        // (not error) so a deployment running an older engram that adds a new
        // lane does not fail to boot on a config naming that lane.
        let unknown = unknown_lane_keys(&self.source_weights);
        if !unknown.is_empty() {
            eprintln!(
                "engram: warning: recall_fusion.source_weights keys {unknown:?} are not in the \
                 documented lane vocabulary {KNOWN_LANE_TAGS:?} — they will be inert (no lane \
                 stamps that source tag). Correct the key, or the weight has no effect.",
            );
        }
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

    // ---- Concern 5: source_weights vocab check ----------------------------

    #[test]
    fn unknown_lane_keys_flags_typo() {
        // `vectors` (plural) is the canonical typo — no lane stamps it, so a
        // weight on it would be silently inert without this check.
        let mut weights = BTreeMap::new();
        weights.insert("vector".to_string(), 0.7); // known
        weights.insert("vectors".to_string(), 0.2); // unknown (typo)
        weights.insert("semantic".to_string(), 0.1); // unknown
        let unknown = super::unknown_lane_keys(&weights);
        assert_eq!(unknown, vec!["semantic", "vectors"]); // BTreeMap-sorted
    }

    #[test]
    fn unknown_lane_keys_empty_when_all_known() {
        let mut weights = BTreeMap::new();
        for tag in super::KNOWN_LANE_TAGS {
            weights.insert((*tag).to_string(), 0.5);
        }
        let unknown = super::unknown_lane_keys(&weights);
        assert!(
            unknown.is_empty(),
            "all documented keys are known: {unknown:?}"
        );
    }

    #[test]
    fn unknown_lane_keys_empty_for_empty_map() {
        let weights = BTreeMap::new();
        assert!(super::unknown_lane_keys(&weights).is_empty());
    }

    #[test]
    fn to_reciprocal_config_accepts_known_vocab() {
        // A config using only documented keys builds without error; the vocab
        // check warns (stderr) only on unknown keys, not these.
        let mut weights = BTreeMap::new();
        weights.insert("vector".to_string(), 0.7);
        weights.insert("graph".to_string(), 0.3);
        let cfg = RecallFusionConfig {
            rrf_k: 60,
            default_source_weight: 1.0,
            source_weights: weights,
            rerank: None,
        };
        let recip = cfg.to_reciprocal_config().expect("known keys build");
        assert_eq!(recip.source_weight("vector"), 0.7);
        assert_eq!(recip.source_weight("graph"), 0.3);
    }

    #[test]
    fn to_reciprocal_config_warns_but_accepts_unknown_vocab() {
        // An unknown key still builds (warn, not error) so future lane names
        // do not break older configs — but the unknown_lane_keys helper lets
        // us assert the warning surface deterministically.
        let mut weights = BTreeMap::new();
        weights.insert("vectors".to_string(), 0.7); // typo
        let cfg = RecallFusionConfig {
            rrf_k: 60,
            default_source_weight: 1.0,
            source_weights: weights,
            rerank: None,
        };
        // The build succeeds (warn, not error) ...
        let recip = cfg
            .to_reciprocal_config()
            .expect("unknown key warns, not errors");
        // ... and the typo'd key carries through (it is *inert* in fusion, not
        // rejected at the map level — the lane it targets simply does not
        // exist).
        assert_eq!(recip.source_weight("vectors"), 0.7);
        // The deterministic surface: the typo is detectable via the helper.
        assert_eq!(
            super::unknown_lane_keys(&cfg.source_weights),
            vec!["vectors"]
        );
    }

    // ---- Nit 6: lambda validation -----------------------------------------

    #[test]
    fn to_reciprocal_config_rejects_lambda_below_zero() {
        let cfg = RecallFusionConfig {
            rrf_k: 60,
            default_source_weight: 1.0,
            source_weights: BTreeMap::new(),
            rerank: Some(RerankConfig {
                strategy: RerankStrategy::Mmr,
                lambda: -0.1,
            }),
        };
        let err = cfg.to_reciprocal_config().expect_err("lambda < 0 rejects");
        assert!(
            format!("{err}").contains("lambda"),
            "error names lambda: {err}"
        );
        assert!(
            format!("{err}").contains("[0, 1]"),
            "error states range: {err}"
        );
    }

    #[test]
    fn to_reciprocal_config_rejects_lambda_above_one() {
        let cfg = RecallFusionConfig {
            rrf_k: 60,
            default_source_weight: 1.0,
            source_weights: BTreeMap::new(),
            rerank: Some(RerankConfig {
                strategy: RerankStrategy::Mmr,
                lambda: 1.5,
            }),
        };
        assert!(cfg.to_reciprocal_config().is_err());
    }

    #[test]
    fn to_reciprocal_config_rejects_nan_lambda() {
        let cfg = RecallFusionConfig {
            rrf_k: 60,
            default_source_weight: 1.0,
            source_weights: BTreeMap::new(),
            rerank: Some(RerankConfig {
                strategy: RerankStrategy::Mmr,
                lambda: f32::NAN,
            }),
        };
        assert!(cfg.to_reciprocal_config().is_err());
    }

    #[test]
    fn to_reciprocal_config_accepts_boundary_lambdas() {
        for lambda in [0.0, 1.0] {
            let cfg = RecallFusionConfig {
                rrf_k: 60,
                default_source_weight: 1.0,
                source_weights: BTreeMap::new(),
                rerank: Some(RerankConfig {
                    strategy: RerankStrategy::Mmr,
                    lambda,
                }),
            };
            cfg.to_reciprocal_config()
                .unwrap_or_else(|e| panic!("boundary lambda {lambda} must accept: {e}"));
        }
    }

    #[test]
    fn to_reciprocal_config_lambda_validated_even_when_strategy_is_none() {
        // Lambda is validated whenever `rerank` is present, regardless of
        // strategy — symmetry with weight validation, and a `None` strategy
        // with a bogus lambda is still a malformed config.
        let cfg = RecallFusionConfig {
            rrf_k: 60,
            default_source_weight: 1.0,
            source_weights: BTreeMap::new(),
            rerank: Some(RerankConfig {
                strategy: RerankStrategy::None,
                lambda: 2.0,
            }),
        };
        assert!(cfg.to_reciprocal_config().is_err());
    }

    #[test]
    fn to_reciprocal_config_lambda_not_checked_when_rerank_absent() {
        // No `rerank` ⇒ no lambda to validate; builds fine.
        let cfg = RecallFusionConfig {
            rrf_k: 60,
            default_source_weight: 1.0,
            source_weights: BTreeMap::new(),
            rerank: None,
        };
        assert!(cfg.to_reciprocal_config().is_ok());
    }
}
