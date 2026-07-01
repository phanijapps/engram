//! Reciprocal-rank fusion (RRF).
//!
//! Score-free fusion across heterogeneous retrievers. Each source contributes
//! `1/(k + rank)` per candidate, where `rank` is the candidate's position within
//! that source's own list; candidates appearing in more than one source sum their
//! contributions, so cross-retriever consensus boosts ranking. Because RRF
//! ignores raw scores it is robust when sources use incomparable scales (graph
//! traversal weights vs cosine similarity vs lexical BM25) — the multi-backend
//! case this crate supports (SQLite/Neo4j graph + sqlite-vec/pgvector vectors).
//!
//! Conventional `k = 60` (Cormack, Clarke, Buettcher, SIGIR 2009).

use std::{cmp::Ordering, collections::BTreeMap};

use engram_domain::{
    FusionStrategy, FusionTrace, RetrievalRequest, RetrievalResult, RetrievalScore,
};
use engram_runtime::CoreResult;

use crate::{RetrievalFusion, config::ReciprocalFusionConfig};

/// Conventional RRF constant (Cormack et al. 2009).
pub const DEFAULT_RRF_K: u32 = 60;

/// Reciprocal-rank fusion implementation of [`RetrievalFusion`].
///
/// Reranking strength is configurable via [`ReciprocalFusionConfig`] (RRF `k`
/// plus optional per-source weights); [`Default`] uses `k = 60` and equal
/// weights (pure RRF). Stateless after construction; safe to share across
/// retrieval flows. Rank is computed per source so interleaved or concatenated
/// lists fuse correctly.
#[derive(Debug, Clone)]
pub struct ReciprocalRankFusion {
    config: ReciprocalFusionConfig,
}

impl ReciprocalRankFusion {
    /// Creates a fuser from a reranking config (k + per-source weights).
    pub fn new(config: ReciprocalFusionConfig) -> Self {
        Self { config }
    }

    /// The configured RRF constant.
    pub fn k(&self) -> u32 {
        self.config.k()
    }

    /// The configured per-source weight (override or default).
    pub fn source_weight(&self, source: &str) -> f32 {
        self.config.source_weight(source)
    }
}

impl Default for ReciprocalRankFusion {
    fn default() -> Self {
        Self::new(ReciprocalFusionConfig::default())
    }
}

impl RetrievalFusion for ReciprocalRankFusion {
    fn fuse(
        &self,
        request: &RetrievalRequest,
        candidates: Vec<RetrievalResult>,
    ) -> CoreResult<Vec<RetrievalResult>> {
        if candidates.is_empty() {
            return Ok(Vec::new());
        }
        let k = self.config.k() as f32;
        let mut source_ranks: BTreeMap<String, u32> = BTreeMap::new();
        let mut groups: BTreeMap<CandidateKey, Group> = BTreeMap::new();

        for candidate in candidates {
            let source = candidate_source(&candidate);
            let rank = {
                let entry = source_ranks.entry(source.clone()).or_insert(0);
                *entry += 1;
                *entry
            };
            // Weighted RRF: the per-source weight scales the rank contribution.
            let contribution = self.config.source_weight(&source) / (k + rank as f32);
            let key = CandidateKey::from(&candidate);
            match groups.entry(key) {
                std::collections::btree_map::Entry::Occupied(mut occupied) => {
                    occupied
                        .get_mut()
                        .add(source, rank, contribution, candidate.id.clone());
                }
                std::collections::btree_map::Entry::Vacant(vacant) => {
                    vacant.insert(Group::new(candidate, source, rank, contribution));
                }
            }
        }

        let mut fused: Vec<RetrievalResult> = groups.into_values().map(Group::finalize).collect();
        fused.sort_by(compare_by_fused_score);
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

/// Accumulates RRF contributions for one `(target_type, target_id)`.
///
/// `best_rank` / `best_contribution` track the strongest single contribution
/// (the rank-1 source), not the first-arriving one, so the emitted
/// `FusionTrace` reports the most relevant contributor rather than an ordering
/// artifact.
struct Group {
    representative: RetrievalResult,
    best_rank: u32,
    best_contribution: f32,
    rrf_score: f32,
    sources: Vec<String>,
    dedup_ids: Vec<String>,
}

impl Group {
    fn new(representative: RetrievalResult, source: String, rank: u32, contribution: f32) -> Self {
        let rrf_score = contribution;
        Self {
            representative,
            best_rank: rank,
            best_contribution: contribution,
            rrf_score,
            sources: vec![source],
            dedup_ids: Vec::new(),
        }
    }

    fn add(&mut self, source: String, rank: u32, contribution: f32, id: String) {
        self.rrf_score += contribution;
        if contribution > self.best_contribution {
            self.best_contribution = contribution;
            self.best_rank = rank;
        }
        if !self.sources.contains(&source) {
            self.sources.push(source);
        }
        self.dedup_ids.push(id);
    }

    fn finalize(self) -> RetrievalResult {
        let mut result = self.representative;
        result.score = with_fused_score(result.score, self.rrf_score);
        result.fusion_trace = Some(FusionTrace {
            source: self.sources.join("+"),
            source_rank: Some(self.best_rank),
            source_score: Some(self.best_contribution),
            fusion_strategy: Some(FusionStrategy::ReciprocalRankFusion),
            fusion_score: Some(self.rrf_score),
            rerank_strategy: None,
            rerank_score: None,
            deduplicated_with: self.dedup_ids,
        });
        result
    }
}

fn with_fused_score(mut score: RetrievalScore, fused: f32) -> RetrievalScore {
    score.total = fused;
    score.relevance = Some(fused);
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

fn compare_by_fused_score(left: &RetrievalResult, right: &RetrievalResult) -> Ordering {
    right
        .score
        .total
        .total_cmp(&left.score.total)
        .then_with(|| left.target_id.cmp(&right.target_id))
        .then_with(|| left.id.cmp(&right.id))
}
