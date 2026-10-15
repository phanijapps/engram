use super::CodeIntelligenceConfig;
use engram_domain::RerankStrategy;
use engram_runtime::{CoreError, CoreResult};

fn invalid(reason: &str) -> CoreError {
    CoreError::InvalidRequest {
        reason: reason.into(),
    }
}
impl CodeIntelligenceConfig {
    pub fn validate(&self) -> CoreResult<()> {
        if self.schema_version != 1 {
            return Err(invalid("code schema_version must be 1"));
        }
        let fusion = self.fusion.to_reciprocal_config()?;
        if self.fusion.rerank.is_none() {
            return Err(invalid(
                "effective code configuration requires an explicit rerank object",
            ));
        }
        let strategy = self
            .fusion
            .rerank
            .as_ref()
            .map_or(RerankStrategy::None, |r| r.strategy.clone());
        if !matches!(
            strategy,
            RerankStrategy::None | RerankStrategy::CrossEncoder
        ) {
            return Err(invalid(
                "code profiles support only none/cross_encoder reranking",
            ));
        }
        let i = &self.index;
        let r = &self.retrieval;
        let s = &self.local_scorer;
        let e = &self.evidence;
        if r.result_limit > r.candidates_per_lane
            || (strategy == RerankStrategy::CrossEncoder && r.result_limit > s.max_candidates)
            || (r.graph_hops > 0 && r.graph_neighbor_limit == 0)
            || !r
                .enabled_lanes
                .iter()
                .any(|lane| fusion.source_weight(lane.as_str()) > 0.0)
        {
            return Err(invalid(
                "code output exceeds candidate budget, expansion has no budget, or enabled lanes have no positive contribution",
            ));
        }
        if !(64..=8192).contains(&i.body_token_target)
            || i.context_token_budget > 1024
            || !(10..=1000).contains(&r.candidates_per_lane)
            || !(1..=100).contains(&r.result_limit)
            || r.graph_hops > 2
            || r.graph_neighbor_limit > 100
            || r.enabled_lanes.is_empty()
            || r.enabled_lanes
                .iter()
                .enumerate()
                .any(|(n, l)| r.enabled_lanes[..n].contains(l))
            || s.model_id.trim().is_empty()
            || !(16..=2048).contains(&s.query_token_limit)
            || !(128..=8192).contains(&s.pair_token_limit)
            || !(1..=200).contains(&s.max_candidates)
            || !(1..=64).contains(&s.batch_size)
            || !(256..=32768).contains(&e.token_budget)
            || e.max_supporting_units > 100
            || !(1..=100).contains(&e.max_per_file)
        {
            return Err(invalid(
                "invalid code-intelligence limits or duplicate/empty lanes",
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
                "code body/context/special tokens exceed embedding window",
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
                "code query/body/context/special tokens exceed scorer pair window",
            ));
        }
        Ok(())
    }
}
