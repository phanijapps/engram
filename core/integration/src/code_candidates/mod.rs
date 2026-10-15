//! Bounded current-code candidates; presentation and model scoring are later stages.
mod fusion;
mod graph;
mod lanes;
mod resolution;
mod retrieval;

use engram_domain::*;
pub use retrieval::collect;
use std::collections::BTreeMap;

pub struct CodeCandidate {
    pub segment: CodeEmbeddingSegment,
    pub parent: KnowledgeChunk,
    pub fusion_score: f32,
    pub fusion_traces: Vec<FusionTrace>,
}

pub struct CodeCandidateSet {
    pub candidates: Vec<CodeCandidate>,
    pub source_failures: Vec<RetrievalSourceFailure>,
    pub candidate_counts: BTreeMap<String, u32>,
    pub phase_ms: BTreeMap<String, f64>,
    pub query_representation_id: String,
    pub representation_id: String,
    pub source_revision: u64,
    pub reindex_required: bool,
}

pub(super) struct LaneHit {
    resolved: engram_knowledge::ResolvedCodeSegment,
    trace: FusionTrace,
}

fn trace(source: &str, rank: usize, score: f32) -> FusionTrace {
    FusionTrace {
        query_id: None,
        vector_index: None,
        embedding_time_ms: None,
        search_time_ms: None,
        source: source.into(),
        source_rank: Some(rank as u32),
        source_score: Some(score),
        score: None,
        rank: None,
        fusion_strategy: None,
        fusion_score: None,
        rerank_strategy: None,
        rerank_score: None,
        discard_reason: None,
        deduplicated_with: vec![],
    }
}

fn failure(lane: &str) -> RetrievalSourceFailure {
    RetrievalSourceFailure {
        source: lane.into(),
        mode: None,
        severity: SourceFailureSeverity::Warning,
        reason: "code_candidate_lane_unavailable".into(),
        message: Some("candidate source unavailable".into()),
        degraded: true,
    }
}
