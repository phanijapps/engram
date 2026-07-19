//! Configuration for deterministic retrieval fusion.
//!
//! Source weights let applications tune candidate contributions without making
//! fusion depend on a concrete index implementation or provider.

use std::collections::BTreeMap;

use engram_runtime::{CoreError, CoreResult};

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
