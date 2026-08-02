//! Integration tests for `SqlUnifiedRecall` (engram-host-sdk brief, S4).
//!
//! These tests exercise the SQLite `UnifiedRecall` impl against in-memory stores
//! and stub retrieval lanes. They mirror the block_on driving style of
//! `tests/batch_ingest.rs` and `tests/provenance_query.rs` — no tokio.
//!
//! Cases:
//! 1. Multi-lane recall → a fused `ContextPayload` spanning lanes (items carry
//!    `fusion_trace` where RRF produces it).
//! 2. One lane fails → a `source_failures` entry while the other lanes' items
//!    still appear (degraded, not error).
//! 3. All lanes fail → `Ok` with empty items + one `source_failures` entry per
//!    lane (degraded success, not `Err`).

use std::sync::Arc;

use async_trait::async_trait;
use engram_belief::{BeliefQuery, BeliefReferenceQuery, BeliefRepository};
use engram_conformance::SqlUnifiedRecall;
use engram_domain::*;
use engram_integration::UnifiedRecall;
use engram_memory::{MemoryEventRepository, MemoryRepository, MemoryService};
use engram_retrieval::RetrievalIndex;
use engram_runtime::{CoreError, CoreResult};
use engram_store_sqlite::SqlBeliefStore;
use engram_store_sqlite::SqlMemoryService;
use futures::executor::block_on;

// ---------- helpers -------------------------------------------------------

fn scope() -> Scope {
    Scope {
        tenant: "tenant-recall".to_string(),
        subject: Some("subject-recall".to_string()),
        workspace: Some("workspace-recall".to_string()),
        session: None,
        environment: Some("test".to_string()),
    }
}

fn actor() -> Actor {
    Actor {
        id: Id::from("recall-agent"),
        kind: ActorKind::Agent,
        display_name: Some("Recall Test".to_string()),
        metadata: None,
    }
}

fn policy() -> Policy {
    Policy {
        visibility: Visibility::Workspace,
        retention: Retention::Durable,
        sensitivity: None,
        allowed_uses: vec![AllowedUse::Retrieval],
        expires_at: None,
        delete_mode: None,
    }
}

fn provenance() -> Provenance {
    Provenance {
        source: "recall-test".to_string(),
        actor: actor(),
        observed_at: chrono::Utc::now(),
        evidence: Vec::new(),
        derivations: Vec::new(),
        confidence: Some(1.0),
        method: Some("manual".to_string()),
    }
}

fn request(query: &str) -> RetrievalRequest {
    RetrievalRequest {
        query: query.to_string(),
        scope: scope(),
        requester: Requester {
            actor: actor(),
            roles: Vec::new(),
            permissions: vec!["memory.retrieve".to_string()],
            on_behalf_of: None,
        },
        modes: Vec::new(),
        filters: None,
        cues: Vec::new(),
        limit: Some(10),
        budget: None,
        include_explanations: None,
    }
}

/// Builds a RetrievalResult candidate for a stub retrieval lane.
fn lane_candidate(target_id: &str, source: &str) -> RetrievalResult {
    RetrievalResult {
        id: format!("lane-{target_id}"),
        target_type: RetrievalTargetType::Entity,
        target_id: target_id.to_string(),
        content: format!("lane-content-{target_id}"),
        score: RetrievalScore {
            total: 0.7,
            relevance: Some(0.7),
            confidence: None,
            recency: None,
            cue_match: None,
            hierarchical_fit: None,
            policy_fit: None,
        },
        provenance: provenance(),
        policy: policy(),
        explanation: None,
        fusion_trace: Some(FusionTrace {
            query_id: None,
            vector_index: None,
            embedding_time_ms: None,
            search_time_ms: None,
            source: source.to_string(),
            source_rank: Some(1),
            source_score: Some(0.7),
            score: None,
            rank: None,
            fusion_strategy: None,
            fusion_score: None,
            rerank_strategy: None,
            rerank_score: None,
            discard_reason: None,
            deduplicated_with: Vec::new(),
        }),
        metadata: None,
    }
}

fn requester() -> Requester {
    Requester {
        actor: actor(),
        roles: Vec::new(),
        permissions: vec!["memory.write".to_string(), "memory.retrieve".to_string()],
        on_behalf_of: None,
    }
}

/// Seeds a memory record whose content text contains `query`, returning the
/// written record's ID. The facts lane (`memory.retrieve`) should return it.
fn seed_memory(memory: &Arc<SqlMemoryService>, query: &str, suffix: &str) -> Id {
    let written = block_on(memory.write_memory(WriteMemoryRequest {
        kind: MemoryKind::Observation,
        content: MemoryContent {
            text: format!("{query} fact {suffix}"),
            summary: None,
            entities: Vec::new(),
            language: None,
            format: None,
            structured: None,
            hash: None,
        },
        scope: scope(),
        requester: requester(),
        provenance: provenance(),
        policy: policy(),
        links: Vec::new(),
        idempotency_key: None,
    }))
    .expect("write memory");
    written.record.id.clone()
}

fn belief(id: &str, subject_key: &str, content: &str) -> Belief {
    Belief {
        id: Id::from(id),
        scope: scope(),
        subject: BeliefSubject {
            key: subject_key.to_string(),
            entity_ref: None,
            concept_ref: None,
            aliases: Vec::new(),
        },
        content: content.to_string(),
        status: BeliefStatus::Active,
        confidence: 0.9,
        sources: Vec::new(),
        valid_from: Some(chrono::Utc::now()),
        valid_until: None,
        superseded_by: None,
        stale: None,
        synthesizer: None,
        reasoning: None,
        embedding_refs: Vec::new(),
        policy: policy(),
        provenance: provenance(),
        created_at: chrono::Utc::now(),
        updated_at: None,
        metadata: None,
    }
}

// ---------- stub retrieval lane ------------------------------------------

struct StubLane {
    results: Vec<RetrievalResult>,
    fail: bool,
}

#[async_trait]
impl RetrievalIndex for StubLane {
    async fn retrieve_candidates(
        &self,
        _request: &RetrievalRequest,
    ) -> CoreResult<Vec<RetrievalResult>> {
        if self.fail {
            return Err(CoreError::Adapter {
                adapter: "stub-lane".to_string(),
                message: "forced lane failure".to_string(),
            });
        }
        Ok(self.results.clone())
    }
}

// ---------- stub failing memory + beliefs (all-lanes-fail case) ----------

struct FailingMemory;

fn unsupported() -> CoreError {
    CoreError::CapabilityUnsupported {
        capability: "recall-test-stub".to_string(),
        reason: "stub does not implement this method".to_string(),
    }
}

#[async_trait]
impl MemoryRepository for FailingMemory {
    async fn put_memory(&self, _r: MemoryRecord) -> CoreResult<MemoryRecord> {
        Err(unsupported())
    }
    async fn get_memory(&self, _i: &MemoryId, _s: &Scope) -> CoreResult<Option<MemoryRecord>> {
        Err(unsupported())
    }
    async fn append_event(&self, _e: MemoryEvent) -> CoreResult<MemoryEvent> {
        Err(unsupported())
    }
    async fn update_memory_status(
        &self,
        _i: &MemoryId,
        _s: &Scope,
        _st: MemoryStatus,
    ) -> CoreResult<MemoryRecord> {
        Err(unsupported())
    }
}

#[async_trait]
impl MemoryEventRepository for FailingMemory {
    async fn get_event(&self, _i: &EventId, _s: &Scope) -> CoreResult<Option<MemoryEvent>> {
        Err(unsupported())
    }
    async fn list_events_for_memory(
        &self,
        _i: &MemoryId,
        _s: &Scope,
    ) -> CoreResult<Vec<MemoryEvent>> {
        Err(unsupported())
    }
    async fn list_events_for_scope(&self, _s: &Scope) -> CoreResult<Vec<MemoryEvent>> {
        Err(unsupported())
    }
}

#[async_trait]
impl MemoryService for FailingMemory {
    async fn write_memory(&self, _r: WriteMemoryRequest) -> CoreResult<WriteMemoryResponse> {
        Err(unsupported())
    }
    async fn retrieve(&self, _r: RetrievalRequest) -> CoreResult<ContextPayload> {
        Err(CoreError::Adapter {
            adapter: "failing-memory".to_string(),
            message: "forced facts-lane failure".to_string(),
        })
    }
    async fn forget(&self, _r: ForgetRequest) -> CoreResult<ForgetResult> {
        Err(unsupported())
    }
}

struct FailingBeliefs;

#[async_trait]
impl BeliefRepository for FailingBeliefs {
    async fn put_belief(&self, _b: Belief) -> CoreResult<Belief> {
        Err(unsupported())
    }
    async fn upsert_belief(&self, _b: Belief) -> CoreResult<Belief> {
        Err(unsupported())
    }
    async fn get_belief(&self, _q: BeliefQuery) -> CoreResult<Option<Belief>> {
        Err(CoreError::Adapter {
            adapter: "failing-beliefs".to_string(),
            message: "forced belief-lane failure".to_string(),
        })
    }
    async fn get_belief_by_id(&self, _i: &BeliefId, _s: &Scope) -> CoreResult<Option<Belief>> {
        Err(unsupported())
    }
    async fn mark_stale(&self, _i: &BeliefId, _s: &Scope, _a: Timestamp) -> CoreResult<Belief> {
        Err(unsupported())
    }
    async fn clear_stale(&self, _i: &BeliefId, _s: &Scope, _a: Timestamp) -> CoreResult<Belief> {
        Err(unsupported())
    }
    async fn supersede_belief(
        &self,
        _i: &BeliefId,
        _s: &Scope,
        _r: BeliefId,
        _a: Timestamp,
    ) -> CoreResult<Belief> {
        Err(unsupported())
    }
    async fn retract_belief(&self, _i: &BeliefId, _s: &Scope, _a: Timestamp) -> CoreResult<Belief> {
        Err(unsupported())
    }
    async fn list_stale(&self, _s: &Scope) -> CoreResult<Vec<Belief>> {
        Err(unsupported())
    }
    async fn beliefs_referencing_source(
        &self,
        _q: BeliefReferenceQuery,
    ) -> CoreResult<Vec<Belief>> {
        Err(unsupported())
    }
    async fn put_contradiction(&self, _c: Contradiction) -> CoreResult<Contradiction> {
        Err(unsupported())
    }
    async fn get_contradiction(
        &self,
        _i: &ContradictionId,
        _s: &Scope,
    ) -> CoreResult<Option<Contradiction>> {
        Err(unsupported())
    }
    async fn resolve_contradiction(
        &self,
        _i: &ContradictionId,
        _s: &Scope,
        _r: ContradictionResolution,
    ) -> CoreResult<Contradiction> {
        Err(unsupported())
    }
    async fn list_contradictions(&self, _scope: &Scope) -> CoreResult<Vec<Contradiction>> {
        Err(unsupported())
    }
}

// ---------- configurable memory stub (Concern 4: facts-lane merge) -------

/// A `MemoryService` stub whose `retrieve` returns a pre-configured
/// `ContextPayload` — used to verify the facts lane's `source_failures` +
/// `omitted` merge into the outer unified payload. All other methods are
/// unreachable from `SqlUnifiedRecall`.
struct StubMemory {
    payload: ContextPayload,
}

impl StubMemory {
    fn new(payload: ContextPayload) -> Self {
        Self { payload }
    }
}

#[async_trait]
impl MemoryRepository for StubMemory {
    async fn put_memory(&self, _r: MemoryRecord) -> CoreResult<MemoryRecord> {
        Err(unsupported())
    }
    async fn get_memory(&self, _i: &MemoryId, _s: &Scope) -> CoreResult<Option<MemoryRecord>> {
        Err(unsupported())
    }
    async fn append_event(&self, _e: MemoryEvent) -> CoreResult<MemoryEvent> {
        Err(unsupported())
    }
    async fn update_memory_status(
        &self,
        _i: &MemoryId,
        _s: &Scope,
        _st: MemoryStatus,
    ) -> CoreResult<MemoryRecord> {
        Err(unsupported())
    }
}

#[async_trait]
impl MemoryEventRepository for StubMemory {
    async fn get_event(&self, _i: &EventId, _s: &Scope) -> CoreResult<Option<MemoryEvent>> {
        Err(unsupported())
    }
    async fn list_events_for_memory(
        &self,
        _i: &MemoryId,
        _s: &Scope,
    ) -> CoreResult<Vec<MemoryEvent>> {
        Err(unsupported())
    }
    async fn list_events_for_scope(&self, _s: &Scope) -> CoreResult<Vec<MemoryEvent>> {
        Err(unsupported())
    }
}

#[async_trait]
impl MemoryService for StubMemory {
    async fn write_memory(&self, _r: WriteMemoryRequest) -> CoreResult<WriteMemoryResponse> {
        Err(unsupported())
    }
    async fn retrieve(&self, _r: RetrievalRequest) -> CoreResult<ContextPayload> {
        Ok(self.payload.clone())
    }
    async fn forget(&self, _r: ForgetRequest) -> CoreResult<ForgetResult> {
        Err(unsupported())
    }
}

// ---------- tests ---------------------------------------------------------

#[test]
fn multi_lane_recall_fuses_candidates_from_multiple_lanes() {
    // Real in-memory memory + beliefs stores, seeded with data; three stub
    // retrieval lanes (graph, vector, lexical) contribute candidates. The fused
    // payload spans facts + graph + vector + lexical + beliefs.
    let memory = Arc::new(SqlMemoryService::open_in_memory().expect("memory open"));
    let beliefs_store = Arc::new(SqlBeliefStore::open_in_memory().expect("beliefs open"));

    let subject_key = "unified-service";

    // Seed a fact (memory) whose content contains the query — the facts lane
    // should return it.
    let fact_id = seed_memory(&memory, subject_key, "0");

    // Seed a belief whose subject key matches the query — the beliefs lane
    // returns it.
    block_on(beliefs_store.put_belief(belief("b-multi", subject_key, "the service is healthy")))
        .expect("put belief");

    let recall = SqlUnifiedRecall::new(
        memory,
        vec![
            Arc::new(StubLane {
                results: vec![lane_candidate("graph-multi", "graph")],
                fail: false,
            }),
            Arc::new(StubLane {
                results: vec![lane_candidate("vector-multi", "vector")],
                fail: false,
            }),
            Arc::new(StubLane {
                results: vec![lane_candidate("lexical-multi", "lexical")],
                fail: false,
            }),
        ],
        beliefs_store,
    );

    let payload = block_on(recall.recall(request(subject_key))).expect("recall");

    // The facts-lane candidate (real memory) is present.
    assert!(
        payload
            .items
            .iter()
            .any(|i| i.target_type == RetrievalTargetType::Memory
                && i.target_id == fact_id.to_string()),
        "facts-lane candidate (real memory) should be in the fused payload: items = {:?}",
        payload
            .items
            .iter()
            .map(|i| (i.target_type.clone(), i.target_id.clone()))
            .collect::<Vec<_>>()
    );
    // The belief-lane candidate is present.
    assert!(
        payload
            .items
            .iter()
            .any(|i| i.target_type == RetrievalTargetType::Belief),
        "belief-lane candidate should be in the fused payload"
    );
    // The graph + vector + lexical lane candidates are present.
    assert!(
        payload.items.iter().any(|i| i.target_id == "graph-multi"),
        "graph-lane candidate should be in the fused payload"
    );
    assert!(
        payload.items.iter().any(|i| i.target_id == "vector-multi"),
        "vector-lane candidate should be in the fused payload"
    );
    assert!(
        payload.items.iter().any(|i| i.target_id == "lexical-multi"),
        "lexical-lane candidate should be in the fused payload"
    );
    // No source failures in the all-success case.
    assert!(
        payload.source_failures.is_empty(),
        "no source_failures expected when all lanes succeed"
    );
    // At least one item carries a fusion_trace (RRF stamps it).
    assert!(
        payload.items.iter().any(|i| i.fusion_trace.is_some()),
        "at least one item should carry a FusionTrace after RRF fusion"
    );
}

#[test]
fn one_lane_failure_degrades_without_aborting() {
    // One stub lane fails; the other lanes (including a succeeding stub lane +
    // beliefs) still contribute items.
    let memory = Arc::new(SqlMemoryService::open_in_memory().expect("memory open"));
    let beliefs_store = Arc::new(SqlBeliefStore::open_in_memory().expect("beliefs open"));
    let subject_key = "degraded-service";
    block_on(beliefs_store.put_belief(belief("b-deg", subject_key, "degraded belief")))
        .expect("put belief");

    let recall = SqlUnifiedRecall::new(
        memory,
        vec![
            Arc::new(StubLane {
                results: Vec::new(),
                fail: true, // this lane fails
            }),
            Arc::new(StubLane {
                results: vec![lane_candidate("vector-deg", "vector")],
                fail: false,
            }),
        ],
        beliefs_store,
    );

    let payload = block_on(recall.recall(request(subject_key))).expect("recall Ok");
    // The failing lane is recorded.
    assert!(
        payload
            .source_failures
            .iter()
            .any(|f| f.source.contains("retrieval_lane_0")),
        "failing lane must produce a source_failure entry"
    );
    let failure = payload
        .source_failures
        .iter()
        .find(|f| f.source.contains("retrieval_lane_0"))
        .unwrap();
    assert!(failure.degraded, "failing lane must be marked degraded");
    // The surviving lanes still contribute items.
    assert!(
        payload.items.iter().any(|i| i.target_id == "vector-deg"),
        "vector-lane candidate should survive despite the graph lane failing"
    );
    assert!(
        payload
            .items
            .iter()
            .any(|i| i.target_type == RetrievalTargetType::Belief),
        "belief-lane candidate should survive"
    );
}

#[test]
fn all_lanes_fail_returns_ok_with_empty_items_and_one_failure_per_lane() {
    // All three lanes (facts via FailingMemory, one retrieval lane, beliefs via
    // FailingBeliefs) return Err. The recall must return Ok with empty items and
    // one source_failure per lane.
    let recall = SqlUnifiedRecall::new(
        Arc::new(FailingMemory),
        vec![Arc::new(StubLane {
            results: Vec::new(),
            fail: true,
        })],
        Arc::new(FailingBeliefs),
    );

    let result = block_on(recall.recall(request("all-fail")));
    // Must be Ok — degraded success, never Err.
    let payload = result.expect("all-lanes-fail must return Ok, not Err");
    assert!(
        payload.items.is_empty(),
        "items should be empty when all lanes fail"
    );
    // One source_failure per lane: facts + retrieval_lane_0 + belief = 3.
    assert_eq!(
        payload.source_failures.len(),
        3,
        "expected one source_failure per lane (3), got {}",
        payload.source_failures.len()
    );
    for expected in ["facts", "retrieval_lane_0", "belief"] {
        assert!(
            payload.source_failures.iter().any(|f| f.source == expected),
            "missing source_failure for lane `{expected}`"
        );
    }
}

#[test]
fn facts_lane_source_failures_and_omitted_merge_into_outer_payload() {
    // The facts lane (memory.retrieve) returns a ContextPayload with non-empty
    // source_failures + omitted. SqlUnifiedRecall must carry those onto the
    // outer unified payload — not drop them.
    let memory_payload = ContextPayload {
        items: vec![{
            let mut r = lane_candidate("facts-merge-0", "memory");
            r.target_type = RetrievalTargetType::Memory;
            r
        }],
        budget: None,
        omitted: vec![OmittedResult {
            target_type: RetrievalTargetType::Memory,
            target_id: "omitted-mem-0".to_string(),
            reason: OmittedReason::PolicyDenied,
        }],
        source_failures: vec![RetrievalSourceFailure {
            source: "memory.internal".to_string(),
            mode: None,
            severity: SourceFailureSeverity::Info,
            reason: "partial_lookup".to_string(),
            message: Some("a sub-source was unavailable".to_string()),
            degraded: true,
        }],
        created_at: chrono::Utc::now(),
    };

    let recall = SqlUnifiedRecall::new(
        Arc::new(StubMemory::new(memory_payload)),
        vec![Arc::new(StubLane {
            results: vec![lane_candidate("graph-merge-0", "graph")],
            fail: false,
        })],
        Arc::new(SqlBeliefStore::open_in_memory().expect("beliefs open")),
    );

    let payload = block_on(recall.recall(request("facts-merge"))).expect("recall");

    // The facts-lane source_failure is carried on the outer payload.
    assert!(
        payload
            .source_failures
            .iter()
            .any(|f| f.source == "memory.internal"),
        "facts-lane source_failure was not merged into outer payload"
    );
    // The facts-lane omitted entry is carried on the outer payload.
    assert!(
        payload
            .omitted
            .iter()
            .any(|o| o.target_id == "omitted-mem-0"),
        "facts-lane omitted entry was not merged into outer payload"
    );
    // The facts-lane item still participates in the fused output.
    assert!(
        payload.items.iter().any(|i| i.target_id == "facts-merge-0"),
        "facts-lane candidate should be in the fused payload"
    );
}

// ---- RFC-0019 T1d: weighted fusion reorders recall ---------------------

#[test]
fn weighted_fusion_reorders_candidates_vs_equal_weight_default() {
    // A weighted `RecallFusionConfig` biases vector over lexical at equal
    // source_rank, reordering the fused output versus the equal-weight default.
    // Proves the lane source tags (T1b) + weighted config (T1a) +
    // `SqlUnifiedRecall` wiring (T1d) work end-to-end.
    use engram_retrieval::RecallFusionConfig;
    use std::collections::BTreeMap;

    // Two stub lanes, each contributing one candidate at source_rank 1. The
    // target_ids are chosen so that on an equal-weight tie RRF's secondary sort
    // (target_id ascending) places lexical first ("l" < "v"); the weighted
    // config (vector=0.7 > lexical=0.3) must flip that ordering.
    let vector_lane = || {
        Arc::new(StubLane {
            results: vec![lane_candidate("vector-weighted", "vector")],
            fail: false,
        })
    };
    let lexical_lane = || {
        Arc::new(StubLane {
            results: vec![lane_candidate("lexical-weighted", "lexical")],
            fail: false,
        })
    };

    let empty_payload = ContextPayload {
        items: Vec::new(),
        budget: None,
        omitted: Vec::new(),
        source_failures: Vec::new(),
        created_at: chrono::Utc::now(),
    };
    let beliefs = Arc::new(SqlBeliefStore::open_in_memory().expect("beliefs open"));

    // (1) Equal-weight default (new()): both candidates score 1.0/(60+1), tie,
    // and RRF breaks the tie by target_id ascending -> lexical first.
    let recall_default = SqlUnifiedRecall::new(
        Arc::new(StubMemory::new(empty_payload.clone())),
        vec![vector_lane(), lexical_lane()],
        beliefs.clone(),
    );
    let default_out =
        block_on(recall_default.recall(request("weighted-fusion"))).expect("recall default");
    let default_order: Vec<String> = default_out
        .items
        .iter()
        .map(|i| i.target_id.clone())
        .collect();
    assert_eq!(
        default_order.len(),
        2,
        "exactly two candidates (vector + lexical) expected: {default_order:?}"
    );
    assert_eq!(
        default_order[0], "lexical-weighted",
        "equal-weight default must tie-break lexical first (target_id ascending): {default_order:?}"
    );

    // (2) Weighted (vector=0.7, lexical=0.3) via with_reranker: vector's RRF
    // contribution 0.7/(60+1) > lexical's 0.3/(60+1), so vector ranks first.
    let mut weights = BTreeMap::new();
    weights.insert("vector".to_string(), 0.7_f32);
    weights.insert("lexical".to_string(), 0.3_f32);
    let weighted_fusion = RecallFusionConfig {
        rrf_k: 60,
        default_source_weight: 1.0,
        source_weights: weights,
        rerank: None,
    }
    .to_reciprocal_config()
    .expect("weighted config validates");

    let recall_weighted = SqlUnifiedRecall::with_reranker(
        Arc::new(StubMemory::new(empty_payload)),
        vec![vector_lane(), lexical_lane()],
        beliefs,
        weighted_fusion,
        None,
    );
    let weighted_out =
        block_on(recall_weighted.recall(request("weighted-fusion"))).expect("recall weighted");
    let weighted_order: Vec<String> = weighted_out
        .items
        .iter()
        .map(|i| i.target_id.clone())
        .collect();
    assert_eq!(
        weighted_order.len(),
        2,
        "exactly two candidates expected after weighting: {weighted_order:?}"
    );
    assert_eq!(
        weighted_order[0], "vector-weighted",
        "weighted config (vector=0.7) must rank vector first: {weighted_order:?}"
    );

    // The fusion_score actually reflects the weighting (vector > lexical), not
    // just the tie-break.
    let vector_score = weighted_out
        .items
        .iter()
        .find(|i| i.target_id == "vector-weighted")
        .and_then(|i| i.fusion_trace.as_ref())
        .and_then(|t| t.fusion_score)
        .expect("vector has a fusion_score");
    let lexical_score = weighted_out
        .items
        .iter()
        .find(|i| i.target_id == "lexical-weighted")
        .and_then(|i| i.fusion_trace.as_ref())
        .and_then(|t| t.fusion_score)
        .expect("lexical has a fusion_score");
    assert!(
        vector_score > lexical_score,
        "vector fusion_score {vector_score} must exceed lexical {lexical_score}"
    );
}

#[test]
fn zero_weight_keeps_candidate_visible_but_scoreless() {
    // RFC-0019: a zero weight keeps a source visible (the candidate still
    // appears) while contributing nothing to its fused score — the documented
    // zero-weight semantics. A vector candidate at rank 1 with weight 0 fuses
    // to a 0 contribution; a lexical candidate with the default weight (1.0)
    // outranks it.
    use engram_retrieval::RecallFusionConfig;
    use std::collections::BTreeMap;

    let empty_payload = ContextPayload {
        items: Vec::new(),
        budget: None,
        omitted: Vec::new(),
        source_failures: Vec::new(),
        created_at: chrono::Utc::now(),
    };
    let beliefs = Arc::new(SqlBeliefStore::open_in_memory().expect("beliefs open"));

    let mut weights = BTreeMap::new();
    weights.insert("vector".to_string(), 0.0_f32); // vector silenced
    let fusion = RecallFusionConfig {
        rrf_k: 60,
        default_source_weight: 1.0,
        source_weights: weights,
        rerank: None,
    }
    .to_reciprocal_config()
    .expect("zero weight validates");

    let recall = SqlUnifiedRecall::with_reranker(
        Arc::new(StubMemory::new(empty_payload)),
        vec![
            Arc::new(StubLane {
                results: vec![lane_candidate("vector-zero", "vector")],
                fail: false,
            }),
            Arc::new(StubLane {
                results: vec![lane_candidate("lexical-zero", "lexical")],
                fail: false,
            }),
        ],
        beliefs,
        fusion,
        None,
    );
    let out = block_on(recall.recall(request("zero-weight"))).expect("recall");
    let order: Vec<String> = out.items.iter().map(|i| i.target_id.clone()).collect();
    // The vector candidate is still present (zero weight keeps it visible)...
    assert!(
        order.iter().any(|t| t == "vector-zero"),
        "zero-weight vector candidate must remain visible: {order:?}"
    );
    // ...but lexical (weight 1.0) outranks it.
    assert_eq!(
        order[0], "lexical-zero",
        "lexical (weight 1.0) must outrank vector (weight 0): {order:?}"
    );
}
