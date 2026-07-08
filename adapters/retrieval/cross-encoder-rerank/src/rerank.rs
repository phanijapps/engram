//! Cross-encoder reranking of fused retrieval candidates.

use std::sync::Arc;

use engram_domain::{FusionStrategy, FusionTrace, RerankStrategy, RetrievalResult};
use engram_runtime::CoreResult;

/// Scores one (query, candidate) pair by query-aware relevance.
///
/// Implementations may be deterministic fixtures, cached scorers, or
/// model-backed cross-encoders (feature-gated). Model lifecycle stays out of the
/// reranker adapter.
pub trait RerankScorer: Send + Sync {
    /// Returns the cross-encoder relevance score for `query` vs `candidate_text`.
    fn score(&self, query: &str, candidate_text: &str) -> CoreResult<f32>;
}

/// Reranks retrieval candidates by an injected cross-encoder scorer.
pub struct CrossEncoderReranker {
    scorer: Arc<dyn RerankScorer>,
}

impl CrossEncoderReranker {
    /// Creates a reranker over the given scorer.
    pub fn new(scorer: Arc<dyn RerankScorer>) -> Self {
        Self { scorer }
    }

    /// Reranks `candidates` best-first by the scorer's query-vs-content score,
    /// stamps each result's `FusionTrace` with the rerank strategy and score,
    /// and truncates to `limit` (if set). Provenance, policy, and target identity
    /// are preserved; ties keep input order (stable sort).
    pub fn rerank(
        &self,
        query: &str,
        candidates: Vec<RetrievalResult>,
        limit: Option<usize>,
    ) -> CoreResult<Vec<RetrievalResult>> {
        let mut scored: Vec<(f32, RetrievalResult)> = Vec::with_capacity(candidates.len());
        for mut result in candidates {
            let score = self.scorer.score(query, &result.content)?;
            stamp_rerank(&mut result, score);
            scored.push((score, result));
        }
        // Stable sort by score descending: equal scores keep their input order.
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        let mut out: Vec<RetrievalResult> = scored.into_iter().map(|(_, r)| r).collect();
        if let Some(limit) = limit {
            out.truncate(limit);
        }
        Ok(out)
    }
}

/// Stamps a result's `FusionTrace` with the cross-encoder rerank strategy/score,
/// creating a minimal trace when none was present.
fn stamp_rerank(result: &mut RetrievalResult, score: f32) {
    let mut trace = result.fusion_trace.take().unwrap_or_else(|| FusionTrace {
        query_id: None,
        vector_index: None,
        embedding_time_ms: None,
        search_time_ms: None,
        source: "rerank.cross_encoder".to_owned(),
        source_rank: None,
        source_score: None,
        score: None,
        rank: None,
        fusion_strategy: Some(FusionStrategy::None),
        fusion_score: None,
        rerank_strategy: Some(RerankStrategy::CrossEncoder),
        rerank_score: Some(score),
        discard_reason: None,
        deduplicated_with: Vec::new(),
    });
    trace.rerank_strategy = Some(RerankStrategy::CrossEncoder);
    trace.rerank_score = Some(score);
    result.fusion_trace = Some(trace);
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use engram_domain::{
        Actor, ActorKind, AllowedUse, DeleteMode, Id, Policy, Provenance, Retention,
        RetrievalScore, RetrievalTargetType, Sensitivity, Visibility,
    };
    use std::collections::HashSet;

    /// Deterministic scorer: number of query words that appear in the candidate.
    struct OverlapScorer;
    impl RerankScorer for OverlapScorer {
        fn score(&self, query: &str, candidate_text: &str) -> CoreResult<f32> {
            let q: HashSet<&str> = query.split_whitespace().collect();
            let c: HashSet<&str> = candidate_text.split_whitespace().collect();
            Ok(q.intersection(&c).count() as f32)
        }
    }

    #[test]
    fn rerank_orders_by_score_desc_preserving_identity() {
        let reranker = CrossEncoderReranker::new(Arc::new(OverlapScorer));
        let candidates = vec![
            candidate("c1", "blue sky"),
            candidate("c2", "red apple pie"),
            candidate("c3", "green tree"),
        ];

        let out = reranker.rerank("apple", candidates, None).unwrap();
        let ids: Vec<&str> = out.iter().map(|r| r.target_id.as_str()).collect();
        // Only c2 overlaps "apple" -> first. c1 and c3 tie at 0 -> input order.
        assert_eq!(ids, vec!["c2", "c1", "c3"]);
    }

    #[test]
    fn rerank_stamps_fusion_trace_and_preserves_provenance_policy() {
        let reranker = CrossEncoderReranker::new(Arc::new(OverlapScorer));
        let input = candidate("c2", "red apple pie");
        let out = reranker.rerank("apple", vec![input], None).unwrap();
        let result = &out[0];

        let trace = result.fusion_trace.as_ref().expect("fusion trace");
        assert_eq!(trace.rerank_strategy, Some(RerankStrategy::CrossEncoder));
        assert_eq!(trace.rerank_score, Some(1.0));
        // Identity + content + provenance/policy preserved.
        assert_eq!(result.target_id, "c2");
        assert_eq!(result.content, "red apple pie");
        assert_eq!(result.provenance.source, "rerank_test");
        assert_eq!(result.policy.visibility, Visibility::Workspace);
    }

    #[test]
    fn rerank_keeps_input_order_on_ties() {
        let reranker = CrossEncoderReranker::new(Arc::new(OverlapScorer));
        let candidates = vec![
            candidate("first", "alpha"),
            candidate("second", "beta"),
            candidate("third", "gamma"),
        ];
        let out = reranker.rerank("zzz", candidates, None).unwrap();
        let ids: Vec<&str> = out.iter().map(|r| r.target_id.as_str()).collect();
        // No overlaps -> all tie at 0 -> input order preserved.
        assert_eq!(ids, vec!["first", "second", "third"]);
    }

    #[test]
    fn rerank_truncates_to_limit_after_rerank() {
        let reranker = CrossEncoderReranker::new(Arc::new(OverlapScorer));
        let candidates = vec![
            candidate("low1", "x"),
            candidate("best", "apple apple"),
            candidate("low2", "y"),
        ];
        let out = reranker.rerank("apple", candidates, Some(1)).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].target_id, "best");
    }

    fn candidate(id: &str, content: &str) -> RetrievalResult {
        RetrievalResult {
            id: format!("result-{id}"),
            target_type: RetrievalTargetType::Chunk,
            target_id: id.to_owned(),
            content: content.to_owned(),
            score: RetrievalScore {
                total: 0.5,
                relevance: Some(0.5),
                recency: None,
                confidence: None,
                cue_match: None,
                hierarchical_fit: None,
                policy_fit: Some(1.0),
            },
            provenance: Provenance {
                source: "rerank_test".to_owned(),
                actor: Actor {
                    id: Id::from("actor-test"),
                    kind: ActorKind::Agent,
                    display_name: None,
                    metadata: None,
                },
                observed_at: Utc
                    .with_ymd_and_hms(2026, 7, 8, 12, 0, 0)
                    .single()
                    .expect("fixed timestamp"),
                evidence: Vec::new(),
                derivations: Vec::new(),
                confidence: Some(1.0),
                method: Some("test".to_owned()),
            },
            policy: Policy {
                visibility: Visibility::Workspace,
                retention: Retention::Durable,
                sensitivity: Some(Sensitivity::Low),
                allowed_uses: vec![AllowedUse::Retrieval],
                expires_at: None,
                delete_mode: Some(DeleteMode::Tombstone),
            },
            explanation: None,
            fusion_trace: None,
            metadata: None,
        }
    }
}
