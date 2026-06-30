//! Weighted-sum retrieval fusion.
//!
//! This module owns deterministic candidate merging and duplicate collapse. It
//! assumes candidates have already passed source-specific policy checks.

use std::{cmp::Ordering, collections::BTreeMap};

use engram_domain::{
    FusionStrategy, FusionTrace, RetrievalRequest, RetrievalResult, RetrievalScore,
};
use engram_runtime::CoreResult;

use crate::{RetrievalFusion, WeightedFusionConfig};

/// Deterministic weighted-sum implementation of `RetrievalFusion`.
///
/// Candidates are grouped by `(target_type, target_id)`, scored with source
/// weights, collapsed to the best representative, and sorted by fused score.
#[derive(Debug, Clone, Default)]
pub struct WeightedRetrievalFusion {
    config: WeightedFusionConfig,
}

impl WeightedRetrievalFusion {
    /// Creates a fusion collaborator with custom source weights.
    ///
    /// The collaborator is stateless after construction. Callers can share it
    /// across retrieval flows and pass already policy-filtered candidates into
    /// `RetrievalFusion::fuse`.
    pub fn new(config: WeightedFusionConfig) -> Self {
        Self { config }
    }
}

impl RetrievalFusion for WeightedRetrievalFusion {
    fn fuse(
        &self,
        request: &RetrievalRequest,
        candidates: Vec<RetrievalResult>,
    ) -> CoreResult<Vec<RetrievalResult>> {
        if candidates.is_empty() {
            return Ok(Vec::new());
        }

        let mut groups = BTreeMap::<CandidateKey, Vec<ScoredCandidate>>::new();
        for (index, candidate) in candidates.into_iter().enumerate() {
            let source = candidate_source(&candidate);
            let weight = self.config.source_weight(&source);
            let weighted_score = candidate.score.total * weight;
            groups
                .entry(CandidateKey::from(&candidate))
                .or_default()
                .push(ScoredCandidate {
                    candidate,
                    source,
                    source_rank: (index + 1) as u32,
                    weighted_score,
                });
        }

        let mut fused = groups
            .into_values()
            .map(fuse_group)
            .collect::<Vec<RetrievalResult>>();
        fused.sort_by(compare_results);

        if let Some(limit) = request.limit {
            fused.truncate(limit as usize);
        }
        Ok(fused)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct CandidateKey {
    target_type: String,
    target_id: String,
}

impl From<&RetrievalResult> for CandidateKey {
    fn from(value: &RetrievalResult) -> Self {
        Self {
            target_type: format!("{:?}", value.target_type),
            target_id: value.target_id.clone(),
        }
    }
}

#[derive(Debug)]
struct ScoredCandidate {
    candidate: RetrievalResult,
    source: String,
    source_rank: u32,
    weighted_score: f32,
}

fn fuse_group(mut group: Vec<ScoredCandidate>) -> RetrievalResult {
    group.sort_by(compare_scored_candidates);

    let fusion_score = group
        .iter()
        .map(|candidate| candidate.weighted_score)
        .sum::<f32>();
    let deduplicated_with = group
        .iter()
        .skip(1)
        .map(|candidate| candidate.candidate.id.clone())
        .collect::<Vec<_>>();
    let source_summary = group
        .iter()
        .map(|candidate| candidate.source.as_str())
        .collect::<Vec<_>>()
        .join("+");

    let winner = group.remove(0);
    let mut result = winner.candidate;
    result.score = fused_score(result.score, fusion_score);
    result.fusion_trace = Some(FusionTrace {
        source: source_summary,
        source_rank: Some(winner.source_rank),
        source_score: Some(winner.weighted_score),
        fusion_strategy: Some(FusionStrategy::WeightedSum),
        fusion_score: Some(fusion_score),
        rerank_strategy: None,
        rerank_score: None,
        deduplicated_with,
    });
    result
}

fn fused_score(mut score: RetrievalScore, fusion_score: f32) -> RetrievalScore {
    score.total = fusion_score;
    score.relevance = Some(fusion_score);
    score
}

fn candidate_source(candidate: &RetrievalResult) -> String {
    candidate
        .fusion_trace
        .as_ref()
        .map(|trace| trace.source.clone())
        .filter(|source| !source.trim().is_empty())
        .unwrap_or_else(|| "unknown".to_owned())
}

fn compare_scored_candidates(left: &ScoredCandidate, right: &ScoredCandidate) -> Ordering {
    right
        .weighted_score
        .total_cmp(&left.weighted_score)
        .then_with(|| left.source_rank.cmp(&right.source_rank))
        .then_with(|| left.candidate.id.cmp(&right.candidate.id))
}

fn compare_results(left: &RetrievalResult, right: &RetrievalResult) -> Ordering {
    right
        .score
        .total
        .total_cmp(&left.score.total)
        .then_with(|| left.target_id.cmp(&right.target_id))
        .then_with(|| left.id.cmp(&right.id))
}
