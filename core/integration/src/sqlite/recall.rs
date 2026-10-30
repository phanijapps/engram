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
//! the adapters directly), which is why it lives under
//! `core/integration/src/sqlite/` behind the `sqlite` feature.
//!
//! ADR-0022: this engine-specific module is intentionally exempt from the
//! engine-neutrality gate; the port it implements stays engine-neutral.

use std::sync::Arc;

use async_trait::async_trait;
use engram_belief::{BeliefQuery, BeliefRepository};
use engram_domain::{
    Belief, ContextPayload, FusionTrace, RetrievalRequest, RetrievalResult, RetrievalScore,
    RetrievalSourceFailure, RetrievalTargetType, SourceFailureSeverity,
};
use engram_memory::MemoryService;
use engram_retrieval::compose_context;
use engram_retrieval::{
    CandidateEvidence, CandidateEvidenceResolver, CodeEligibilityResolver, ReciprocalFusionConfig,
    ReciprocalRankFusion, RetrievalCompositionInput, RetrievalIndex, resolve_view,
};
use engram_runtime::{CoreError, CoreResult};
use futures::future::join_all;

use crate::UnifiedRecall;

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
/// construction; each `recall` runs the lanes independently. Use
/// [`SqlUnifiedRecall::with_reranker`] to inject an externally-configured
/// weighted-RRF fusion (RFC-0019) and/or a reranker.
pub struct SqlUnifiedRecall {
    memory: Arc<dyn MemoryService>,
    retrieval_lanes: Vec<Arc<dyn RetrievalIndex>>,
    beliefs: Arc<dyn BeliefRepository>,
    /// Weighted-RRF fusion config (RFC-0019). Built from the operator-facing
    /// [`engram_retrieval::RecallFusionConfig`] by the bootstrap; `default()`
    /// (equal-weight) when no config is supplied.
    fusion: ReciprocalFusionConfig,
    reranker: Option<Arc<dyn engram_retrieval::RetrievalReranker>>,
    /// Trust-aware scoring (F1): provenance-method boosts applied to
    /// facts-lane candidates (e.g. operator-approved memories outrank the
    /// same text written directly by an agent).
    trust: engram_retrieval::TrustWeights,
    /// Bound-code eligibility (task_013): when wired, the facts lane's
    /// code-derived memory items are classified against the published
    /// generation set BEFORE they enter fusion, and every recall resolves
    /// the view once and reuses it for the post-lane guard. `None` keeps
    /// the historical behavior (no binding system in scope).
    code_eligibility: Option<Arc<dyn CodeEligibilityResolver>>,
    candidate_evidence: Option<Arc<dyn CandidateEvidenceResolver>>,
}

impl SqlUnifiedRecall {
    /// Wraps the memory + retrieval-lane + beliefs handles to expose unified recall.
    ///
    /// Fuses with equal-weight RRF ([`ReciprocalFusionConfig::default`]); pass a
    /// configured fusion via [`Self::with_reranker`] to honor an external
    /// `[recall_fusion]` config (RFC-0019).
    pub fn new(
        memory: Arc<dyn MemoryService>,
        retrieval_lanes: Vec<Arc<dyn RetrievalIndex>>,
        beliefs: Arc<dyn BeliefRepository>,
    ) -> Self {
        Self::with_reranker(
            memory,
            retrieval_lanes,
            beliefs,
            ReciprocalFusionConfig::default(),
            None,
        )
    }

    /// Like [`new`] but with an externally-configured weighted-RRF `fusion`
    /// (RFC-0019) and an optional reranker wired into `compose_context` between
    /// fusion and budget. Passing [`ReciprocalFusionConfig::default`] reproduces
    /// [`new`]'s equal-weight behavior.
    pub fn with_reranker(
        memory: Arc<dyn MemoryService>,
        retrieval_lanes: Vec<Arc<dyn RetrievalIndex>>,
        beliefs: Arc<dyn BeliefRepository>,
        fusion: ReciprocalFusionConfig,
        reranker: Option<Arc<dyn engram_retrieval::RetrievalReranker>>,
    ) -> Self {
        Self {
            memory,
            retrieval_lanes,
            beliefs,
            fusion,
            reranker,
            trust: engram_retrieval::TrustWeights::default(),
            code_eligibility: None,
            candidate_evidence: None,
        }
    }

    /// Wires bound-code eligibility into the recall composition (task_013
    /// seam): the facts lane filters code-derived items by the published
    /// generation set before fusion, and a post-lane guard re-classifies every
    /// surviving candidate (defense in depth for lanes that cannot constrain
    /// internally) so ineligible code never reaches rerank input or
    /// `compose_context`. Non-code candidates are never removed (AC-006).
    /// Wires canonical evidence for degraded code-authority handling.
    pub fn with_candidate_evidence(
        mut self,
        candidate_evidence: Arc<dyn CandidateEvidenceResolver>,
    ) -> Self {
        self.candidate_evidence = Some(candidate_evidence);
        self
    }

    pub fn with_code_eligibility(
        mut self,
        code_eligibility: Arc<dyn CodeEligibilityResolver>,
    ) -> Self {
        self.code_eligibility = Some(code_eligibility);
        self
    }
}

#[async_trait]
impl UnifiedRecall for SqlUnifiedRecall {
    async fn recall(&self, request: RetrievalRequest) -> CoreResult<ContextPayload> {
        let now = chrono::Utc::now();
        let mut candidates: Vec<RetrievalResult> = Vec::new();
        let mut source_failures: Vec<RetrievalSourceFailure> = Vec::new();
        let mut omitted = Vec::new();

        // ---- Bound-code eligibility (task_013) --------------------------------
        // Resolve the published generation set ONCE per recall. The view
        // (a) filters the facts lane's code-derived memory items below,
        // (b) guards the fused candidate set after every lane — a lane that
        // could not constrain internally (or a stale warm entry) cannot slip
        // ineligible code into rerank input or compose_context. A resolver
        // failure degrades the recall with a source failure rather than
        // silently running unconstrained.
        // Corrective r2 (finding 6): an eligibility-resolution FAILURE fails
        // CLOSED. The authority-unavailable branch does not disable the
        // guard: code-capable candidates (chunk/entity targets — the types
        // that require the failed authority) are denied, while independently
        // authorized non-code results (facts-lane memories, beliefs) are
        // preserved and the failure is surfaced as a source failure.
        let mut authority_failed = false;
        let code_view = match &self.code_eligibility {
            Some(resolver) => {
                let requested = request
                    .filters
                    .as_ref()
                    .and_then(|filters| filters.code_index_keys.as_deref());
                match resolve_view(resolver.as_ref(), &request.scope, requested).await {
                    Ok(view) => Some(view),
                    Err(error) => {
                        source_failures.push(lane_failure("code_eligibility", &error));
                        authority_failed = true;
                        None
                    }
                }
            }
            None => None,
        };
        // ---- Facts lane: memory.retrieve (already-composed ContextPayload) ----
        // Its items feed the outer fusion; its source_failures + omitted merge
        // into the outer payload (compose_context keeps all candidates — no
        // double-budget).
        // Trust-aware facts lane (F1, review-hardened):
        // (a) The INNER retrieve runs WITHOUT the final output truncation
        //     (limit + budget stripped) — the memory service otherwise omits
        //     candidates before the trust boost can rescue them (review P1-2:
        //     an approved equal-score memory lost to a newer plain one at
        //     limit=1). The user's real budget is applied ONCE by the outer
        //     compose_context after fusion (the pre-existing no-double-budget
        //     design); scope/filter/policy checks are unchanged inside.
        // (b) Method boosts scale the source scores, then the lane reorders —
        //     RRF consumes WITHIN-SOURCE ENCOUNTER ORDER, not raw scores.
        // (c) The sort is STABLE with NO id tiebreak (review P1-3): equal
        //     boosted totals keep the memory service's deterministic order
        //     (newest-first), so the all-neutral case is byte-identical to
        //     pre-F1 behavior. Note: RRF's finalization overwrites `total`
        //     with the fused score and the trace keeps the UNBOOSTED
        //     source_score — the boost's durable effect is trust-adjusted
        //     source ORDERING, not a persisted score multiplier.
        let mut inner_request = request.clone();
        inner_request.limit = None;
        inner_request.budget = None;
        match self.memory.retrieve(inner_request).await {
            Ok(mut payload) => {
                for item in &mut payload.items {
                    let boost = self.trust.method_boost(item.provenance.method.as_deref());
                    if boost != 1.0 {
                        item.score.total *= boost;
                        if let Some(relevance) = item.score.relevance.as_mut() {
                            *relevance *= boost;
                        }
                    }
                }
                payload.items.sort_by(|a, b| {
                    b.score
                        .total
                        .partial_cmp(&a.score.total)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                // Facts-lane code-derived items were classified against the
                // published set here before A3; the post-lane guard below is
                // now the single eligibility decision point (it classifies
                // every candidate, facts-lane items included).
                candidates.extend(payload.items);
                source_failures.extend(payload.source_failures);
                omitted.extend(payload.omitted);
            }
            Err(e) => {
                source_failures.push(lane_failure(FACTS_LANE, &e));
            }
        }

        // ---- Graph/vector/lexical lanes: each RetrievalIndex lane -----------
        // Parallel fan-out: every lane's `retrieve_candidates` is polled
        // concurrently (their async I/O interleaves on this task) instead of
        // being awaited one at a time. This is a primary latency win — the
        // sequential loop paid the *sum* of all lane latencies, so a recall
        // backed by N lanes took ~N× the slowest lane. `join_all` returns
        // outcomes in input order, so candidate ordering and `source_failures`
        // tags are byte-for-byte stable with the previous sequential behavior
        // (each lane index is carried alongside its result). A lane `Err`
        // still degrades into `source_failures` rather than aborting the
        // recall; an all-lanes-failure still yields `Ok` with empty items.
        // Bind a shared reference once; each per-lane future copies this `&`
        // (references are `Copy`) rather than moving the owned `request`, which
        // is still needed downstream (beliefs lane + `compose_context`).
        let request_ref = &request;
        let lane_futures = self
            .retrieval_lanes
            .iter()
            .enumerate()
            .map(|(index, lane)| {
                // Clone the Arc (cheap refcount bump) so each future owns its lane
                // handle independently of `self`'s borrow.
                let lane = Arc::clone(lane);
                async move { (index, lane.retrieve_candidates(request_ref).await) }
            });
        for (index, outcome) in join_all(lane_futures).await {
            match outcome {
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

        // ---- Post-lane eligibility guard (defense in depth) ------------------
        // Every fused candidate is re-classified before rerank input and
        // compose_context: an ineligible code target that survived a lane —
        // e.g. via a stale warm index or a lane constructed without its own
        // constraint — is dropped here rather than entering the composed
        // context budget. With canonical evidence configured, a SUCCESSFUL
        // view additionally validates every candidate's canonical CODE
        // DEPENDENCIES against that view (corrective r4, canonical 6c): a
        // memory/belief whose provenance derives from stale code denies even
        // though its own id carries no lineage; positively authorized
        // independent ordinary records keep flowing, and genuinely Unknown
        // evidence fails closed. On AUTHORITY FAILURE the same evidence
        // decides (finding 6): only positively authorized ordinary records
        // survive.
        //
        // Work package A3 (r5): acceptance requires `complete` — a fully
        // validated dependency set; the aggregate-only trait default can
        // never pass. A code-dependent acceptance additionally requires a
        // NONEMPTY code set (never a vacuous `.all` over a default-empty
        // vector), and a MISSING evidence resolver is not derived-record
        // independence: without it the guard fails closed identically to
        // the authority-failure branch.
        // clippy roll 2026-10: paired checks preserve the explicit fail-closed branch.
        #[allow(clippy::unnecessary_unwrap)]
        if code_view.is_some() && self.candidate_evidence.is_some() {
            let evidence = self.candidate_evidence.as_ref().expect("checked");
            let view = code_view.as_ref().expect("checked");
            let mut allowed = Vec::with_capacity(candidates.len());
            for candidate in candidates {
                match evidence
                    .classify_with_dependencies(&candidate, &request)
                    .await
                {
                    Ok(classification) => {
                        let dependency_eligible = classification.complete
                            && !classification.code_dependencies.is_empty()
                            && classification
                                .code_dependencies
                                .iter()
                                .all(|dependency| view.allows(dependency.as_str(), true));
                        if classification.complete
                            && (classification.evidence == CandidateEvidence::Ordinary
                                || (classification.evidence == CandidateEvidence::CodeDependent
                                    && dependency_eligible))
                        {
                            allowed.push(candidate);
                        }
                    }
                    Err(error) => source_failures.push(lane_failure("candidate_evidence", &error)),
                }
            }
            candidates = allowed;
        } else if code_view.is_some() {
            // Missing evidence resolver ≠ derived-record independence (A3):
            // own-id lineage cannot prove a memory/belief free of stale code
            // dependencies, so every candidate fails closed while code
            // bindings exist and the denial is surfaced, not silent.
            source_failures.push(lane_failure(
                "candidate_evidence",
                &CoreError::Adapter {
                    adapter: "candidate-evidence".to_string(),
                    message: "evidence resolver not configured; candidates fail \
                             closed while code bindings exist"
                        .to_string(),
                },
            ));
            candidates.clear();
        } else if authority_failed {
            let Some(evidence) = &self.candidate_evidence else {
                candidates.clear();
                return compose_context(RetrievalCompositionInput {
                    request: &request,
                    fusion: &ReciprocalRankFusion::new(self.fusion.clone()),
                    reranker: self.reranker.as_deref(),
                    candidates,
                    omitted,
                    source_failures,
                    created_at: now,
                });
            };
            // Review r5 (finding 6): the degraded path uses the SAME
            // completeness contract as the successful-view path —
            // `classify_with_dependencies`, accepting ONLY complete,
            // positively ordinary evidence. An aggregate-only resolver
            // cannot pass here (its classification is explicitly
            // incomplete), and an incomplete Ordinary never flows.
            let mut allowed = Vec::with_capacity(candidates.len());
            for candidate in candidates {
                match evidence
                    .classify_with_dependencies(&candidate, &request)
                    .await
                {
                    Ok(classification) => {
                        if classification.complete
                            && classification.evidence == CandidateEvidence::Ordinary
                        {
                            allowed.push(candidate);
                        }
                    }
                    Err(error) => source_failures.push(lane_failure("candidate_evidence", &error)),
                }
            }
            candidates = allowed;
        }

        // ---- Fuse + compose via the existing RRF + compose_context -----------
        // The fusion carries the operator-facing weighted-RRF config (RFC-0019):
        // `ReciprocalFusionConfig::default()` (equal-weight) when no config is
        // supplied, or the configured `source_weights`/`k` otherwise.
        // compose_context disables request-level fusion truncation (keeps all
        // candidates) so there is no double-budget against the facts lane.
        compose_context(RetrievalCompositionInput {
            request: &request,
            fusion: &ReciprocalRankFusion::new(self.fusion.clone()),
            reranker: self.reranker.as_deref(),
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
