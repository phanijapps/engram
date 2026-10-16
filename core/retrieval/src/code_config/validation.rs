use super::CodeIntelligenceConfig;
use engram_domain::RerankStrategy;
use engram_runtime::{CoreError, CoreResult};

fn invalid(reason: &str) -> CoreError {
    CoreError::InvalidRequest {
        reason: reason.into(),
    }
}
impl CodeIntelligenceConfig {
    /// Allowlisted request limits may reduce the operator's bounds, never widen them.
    pub fn for_query(&self, query: &engram_domain::CodeEvidenceQuery) -> CoreResult<Self> {
        self.validate()?;
        query.validate().map_err(|e| invalid(&e.to_string()))?;
        let mut effective = self.clone();
        for (requested, bound, name) in [
            (
                query.result_limit,
                self.retrieval.result_limit,
                "result limit",
            ),
            (
                query.candidates_per_lane,
                self.retrieval.candidates_per_lane,
                "candidate depth",
            ),
            (
                query.evidence_token_budget,
                self.evidence.token_budget,
                "evidence tokens",
            ),
        ] {
            if requested.is_some_and(|v| v > bound) {
                return Err(invalid(&format!("query {name} exceeds operator bound")));
            }
        }
        if let Some(v) = query.result_limit {
            effective.retrieval.result_limit = v;
        }
        if let Some(v) = query.candidates_per_lane {
            effective.retrieval.candidates_per_lane = v;
        }
        if let Some(v) = query.evidence_token_budget {
            effective.evidence.token_budget = v;
        }
        effective.validate()?;
        Ok(effective)
    }

    pub fn validate(&self) -> CoreResult<()> {
        if self.schema_version != 1 {
            return Err(invalid("schema_version must be 1"));
        }
        let rerank = self
            .fusion
            .rerank
            .as_ref()
            .ok_or_else(|| invalid("fusion.rerank requires an explicit object"))?;
        if !matches!(
            rerank.strategy,
            RerankStrategy::None | RerankStrategy::CrossEncoder
        ) {
            return Err(invalid(
                "fusion.rerank.strategy supports only none/cross_encoder",
            ));
        }
        if self.fusion.rrf_k == 0 {
            return Err(invalid("fusion.rrf_k must be positive"));
        }
        if !self.fusion.default_source_weight.is_finite() || self.fusion.default_source_weight < 0.0
        {
            return Err(invalid(
                "fusion.default_source_weight must be finite and nonnegative",
            ));
        }
        for (key, weight) in &self.fusion.source_weights {
            if ![
                "vector",
                "lexical",
                "graph",
                "associative_graph",
                "community_summary",
            ]
            .contains(&key.as_str())
            {
                return Err(invalid(&format!(
                    "fusion.source_weights.{key} is an unknown code lane"
                )));
            }
            if !weight.is_finite() || *weight < 0.0 {
                return Err(invalid(&format!(
                    "fusion.source_weights.{key} must be finite and nonnegative"
                )));
            }
        }
        if !rerank.lambda.is_finite() || !(0.0..=1.0).contains(&rerank.lambda) {
            return Err(invalid("fusion.rerank.lambda must be finite and in [0,1]"));
        }
        let fusion = self.fusion.to_reciprocal_config()?;
        let i = &self.index;
        let r = &self.retrieval;
        let s = &self.local_scorer;
        let e = &self.evidence;
        for (key, value, minimum, maximum) in [
            ("index.body_token_target", i.body_token_target, 64, 8192),
            (
                "index.context_token_budget",
                i.context_token_budget,
                0,
                1024,
            ),
            (
                "retrieval.candidates_per_lane",
                r.candidates_per_lane,
                10,
                1000,
            ),
            ("retrieval.result_limit", r.result_limit, 1, 100),
            ("retrieval.graph_hops", r.graph_hops, 0, 2),
            (
                "retrieval.graph_neighbor_limit",
                r.graph_neighbor_limit,
                0,
                100,
            ),
            (
                "local_scorer.query_token_limit",
                s.query_token_limit,
                16,
                2048,
            ),
            (
                "local_scorer.pair_token_limit",
                s.pair_token_limit,
                128,
                8192,
            ),
            ("local_scorer.max_candidates", s.max_candidates, 1, 200),
            ("local_scorer.max_parents", s.max_parents, 1, 200),
            ("local_scorer.batch_size", s.batch_size, 1, 64),
            ("evidence.token_budget", e.token_budget, 256, 32768),
            (
                "evidence.max_supporting_units",
                e.max_supporting_units,
                0,
                100,
            ),
            ("evidence.max_per_file", e.max_per_file, 1, 100),
        ] {
            if !(minimum..=maximum).contains(&value) {
                return Err(invalid(&format!("{key} must be in [{minimum},{maximum}]")));
            }
        }
        if r.enabled_lanes.is_empty()
            || r.enabled_lanes
                .iter()
                .enumerate()
                .any(|(n, l)| r.enabled_lanes[..n].contains(l))
        {
            return Err(invalid(
                "retrieval.enabled_lanes must be nonempty and unique",
            ));
        }
        if s.model_id.trim().is_empty() {
            return Err(invalid("local_scorer.model_id must be nonempty"));
        }
        if r.result_limit > r.candidates_per_lane {
            return Err(invalid(
                "retrieval.result_limit exceeds retrieval.candidates_per_lane",
            ));
        }
        if rerank.strategy == RerankStrategy::CrossEncoder && r.result_limit > s.max_candidates {
            return Err(invalid(
                "retrieval.result_limit exceeds local_scorer.max_candidates",
            ));
        }
        if rerank.strategy == RerankStrategy::CrossEncoder && r.result_limit > s.max_parents {
            return Err(invalid(
                "retrieval.result_limit exceeds local_scorer.max_parents",
            ));
        }
        if r.graph_hops > 0 && r.graph_neighbor_limit == 0 {
            return Err(invalid(
                "retrieval.graph_hops requires positive retrieval.graph_neighbor_limit",
            ));
        }
        if !r
            .enabled_lanes
            .iter()
            .any(|lane| fusion.source_weight(lane.as_str()) > 0.0)
        {
            return Err(invalid(
                "retrieval.enabled_lanes require positive fusion.default_source_weight or fusion.source_weights contribution",
            ));
        }
        Ok(())
    }

    /// Windows and special-token costs come from admitted adapter tokenizers.
    pub fn validate_model_windows(
        &self,
        embedding_window: u32,
        embedding_special: u32,
        scorer_window: u32,
        scorer_special: u32,
    ) -> CoreResult<()> {
        self.validate()?;
        let body_and_context = self.index.body_token_target + self.index.context_token_budget;
        if body_and_context.saturating_add(embedding_special) > embedding_window {
            return Err(invalid(
                "index.body_token_target / index.context_token_budget exceed embedding window with special tokens",
            ));
        }
        if self
            .fusion
            .rerank
            .as_ref()
            .is_some_and(|r| r.strategy == RerankStrategy::CrossEncoder)
            && (self.local_scorer.pair_token_limit > scorer_window
                || body_and_context
                    .saturating_add(self.local_scorer.query_token_limit)
                    .saturating_add(scorer_special)
                    > self.local_scorer.pair_token_limit)
        {
            return Err(invalid(
                "index.body_token_target / index.context_token_budget / local_scorer.query_token_limit exceed local_scorer.pair_token_limit or scorer window",
            ));
        }
        Ok(())
    }
}
