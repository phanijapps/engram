//! SQLite implementation of the [`UnifiedRecall`] port (engram-host-sdk brief, S4).
//!
//! [`SqlUnifiedRecall`] composes a [`MemoryService`] (facts lane), a set of
//! [`RetrievalIndex`] lanes (graph/vector/lexical), and a [`BeliefRepository`]
//! (beliefs lane), then fuses all candidates through the existing
//! [`ReciprocalRankFusion`] + [`compose_context`]. It reuses — never
//! reimplements — the fusion and composition machinery (ADR-0009).
//!
//! Per request, each lane is attempted independently:
//! - **Facts** — `memory.retrieve(request)` returns an already-composed
//!   [`ContextPayload`]; its `items` feed the outer fusion as candidates and its
//!   `source_failures` / `omitted` merge into the outer payload.
//!   `compose_context` keeps all candidates (truncation disabled), so there is no
//!   double-budget.
//! - **Graph/vector/lexical** — each `RetrievalIndex` lane's
//!   `retrieve_candidates` produces [`RetrievalResult`] candidates.
//! - **Beliefs** — `BeliefRepository::get_belief(BeliefQuery::live_subject(..))`
//!   returns **at most one** belief (0-or-1, not a list), wrapped as a single
//!   [`RetrievalResult`] candidate.
//!
//! A lane `Err` is recorded as a [`RetrievalSourceFailure`] and the recall
//! continues (degraded). **All lanes failing returns `Ok` with an empty items
//! list + one failure per lane** — degraded success, never `Err`.
//!
//! No schema change: the impl reuses the existing per-store reads and the
//! existing fusion/composition. It is engine-specific (it names `Sql*` and holds
//! the adapters directly), which is why it lives here rather than in the
//! engine-neutral port crate.
//!
//! ADR-0022: only this adapter crate may name `Sql*`; the port it implements
//! stays engine-neutral.

use std::sync::Arc;

use async_trait::async_trait;
use engram_belief::{BeliefQuery, BeliefRepository};
use engram_domain::{
    Belief, ContextPayload, FusionTrace, RetrievalRequest, RetrievalResult, RetrievalScore,
    RetrievalSourceFailure, RetrievalTargetType, SourceFailureSeverity,
};
use engram_integration::UnifiedRecall;
use engram_memory::MemoryService;
use engram_retrieval::compose_context;
use engram_retrieval::{ReciprocalRankFusion, RetrievalCompositionInput, RetrievalIndex};
use engram_runtime::{CoreError, CoreResult};

/// The source tag stamped on belief-lane candidates and failures.
const BELIEF_LANE: &str = "belief";
/// The source tag stamped on facts-lane failures (facts candidates carry their
/// own trace from `memory.retrieve`).
const FACTS_LANE: &str = "facts";

/// SQLite-backed [`UnifiedRecall`]: composes the v1 lanes and fuses via the
/// existing RRF + `compose_context`.
///
/// Construct with [`SqlUnifiedRecall::new`] from the memory handle, the
/// retrieval lanes, and the beliefs handle. The recall is stateless after
/// construction; each `recall` runs the lanes independently.
pub struct SqlUnifiedRecall {
    memory: Arc<dyn MemoryService>,
    retrieval_lanes: Vec<Arc<dyn RetrievalIndex>>,
    beliefs: Arc<dyn BeliefRepository>,
}

impl SqlUnifiedRecall {
    /// Wraps the memory + retrieval-lane + beliefs handles to expose unified recall.
    pub fn new(
        memory: Arc<dyn MemoryService>,
        retrieval_lanes: Vec<Arc<dyn RetrievalIndex>>,
        beliefs: Arc<dyn BeliefRepository>,
    ) -> Self {
        Self {
            memory,
            retrieval_lanes,
            beliefs,
        }
    }
}

#[async_trait]
impl UnifiedRecall for SqlUnifiedRecall {
    async fn recall(&self, request: RetrievalRequest) -> CoreResult<ContextPayload> {
        let now = chrono::Utc::now();
        let mut candidates: Vec<RetrievalResult> = Vec::new();
        let mut source_failures: Vec<RetrievalSourceFailure> = Vec::new();
        let mut omitted = Vec::new();

        // ---- Facts lane: memory.retrieve (already-composed ContextPayload) ----
        // Its items feed the outer fusion; its source_failures + omitted merge
        // into the outer payload (compose_context keeps all candidates — no
        // double-budget).
        match self.memory.retrieve(request.clone()).await {
            Ok(payload) => {
                candidates.extend(payload.items);
                source_failures.extend(payload.source_failures);
                omitted.extend(payload.omitted);
            }
            Err(e) => {
                source_failures.push(lane_failure(FACTS_LANE, &e));
            }
        }

        // ---- Graph/vector/lexical lanes: each RetrievalIndex lane -----------
        for (index, lane) in self.retrieval_lanes.iter().enumerate() {
            match lane.retrieve_candidates(&request).await {
                Ok(results) => candidates.extend(results),
                Err(e) => {
                    source_failures.push(lane_failure(&format!("retrieval_lane_{index}"), &e));
                }
            }
        }

        // ---- Beliefs lane: get_belief is 0-or-1, wrapped as one candidate ----
        let belief_query =
            BeliefQuery::live_subject(request.scope.clone(), request.query.clone(), now);
        match self.beliefs.get_belief(belief_query).await {
            Ok(Some(belief)) => {
                candidates.push(belief_to_result(&belief));
            }
            Ok(None) => {} // no candidate — not a failure
            Err(e) => {
                source_failures.push(lane_failure(BELIEF_LANE, &e));
            }
        }

        // ---- Fuse + compose via the existing RRF + compose_context -----------
        // compose_context disables request-level fusion truncation (keeps all
        // candidates) so there is no double-budget against the facts lane.
        compose_context(RetrievalCompositionInput {
            request: &request,
            fusion: &ReciprocalRankFusion::default(),
            reranker: None,
            candidates,
            omitted,
            source_failures,
            created_at: now,
        })
    }
}

/// Wraps a [`Belief`] as a single [`RetrievalResult`] candidate (the beliefs lane
/// is 0-or-1, not a list). Reuses the existing `RetrievalResult` fields; does not
/// extend the type.
fn belief_to_result(belief: &Belief) -> RetrievalResult {
    let confidence = belief.confidence;
    RetrievalResult {
        id: belief.id.to_string(),
        target_type: RetrievalTargetType::Belief,
        target_id: belief.id.to_string(),
        content: belief.content.clone(),
        score: RetrievalScore {
            total: confidence,
            relevance: Some(confidence),
            confidence: Some(confidence),
            recency: None,
            cue_match: None,
            hierarchical_fit: None,
            policy_fit: None,
        },
        provenance: belief.provenance.clone(),
        policy: belief.policy.clone(),
        explanation: None,
        fusion_trace: Some(FusionTrace {
            query_id: None,
            vector_index: None,
            embedding_time_ms: None,
            search_time_ms: None,
            source: BELIEF_LANE.to_string(),
            source_rank: Some(1),
            source_score: Some(confidence),
            score: None,
            rank: None,
            fusion_strategy: None,
            fusion_score: None,
            rerank_strategy: None,
            rerank_score: None,
            discard_reason: None,
            deduplicated_with: Vec::new(),
        }),
        metadata: belief.metadata.clone(),
    }
}

/// Builds a degraded-mode [`RetrievalSourceFailure`] for a lane that errored.
fn lane_failure(source: &str, error: &CoreError) -> RetrievalSourceFailure {
    RetrievalSourceFailure {
        source: source.to_string(),
        mode: None,
        severity: SourceFailureSeverity::Warning,
        reason: "lane_error".to_string(),
        message: Some(format!("{error}")),
        degraded: true,
    }
}

#[cfg(test)]
mod tests {
    //! The SqlUnifiedRecall integration tests live in
    //! `adapters/integration/tests/recall.rs` so they can share the fixture
    //! helpers and the block_on driving style. This module is reserved for any
    //! future inline unit tests that do not require a store.
}
