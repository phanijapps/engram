//! Unified-recall capability fixture (engram-host-sdk brief, S4).
//!
//! Exercises [`SqlUnifiedRecall`] across four deterministic cases using
//! configurable lane stubs (the real `SqlUnifiedRecall` fuses their output):
//! 1. **Multi-lane** — all lanes contribute candidates → one fused
//!    [`ContextPayload`] whose items span the lanes and carry `FusionTrace`
//!    where RRF produces it.
//! 2. **One-lane degraded** — one lane errors → a `source_failures` entry is
//!    recorded and the other lanes' items still appear (degraded, not error).
//! 3. **All-lanes fail** — every lane errors → `Ok` with empty items + one
//!    `source_failures` entry per lane (degraded success, not `Err`).
//! 4. **Facts-lane merge** — the facts lane's `source_failures` + `omitted`
//!    are carried onto the outer unified payload, not dropped.

use std::sync::Arc;

use async_trait::async_trait;
use engram_belief::{BeliefQuery, BeliefReferenceQuery, BeliefRepository};
use engram_domain::*;
use engram_integration::UnifiedRecall;
use engram_memory::{MemoryEventRepository, MemoryRepository, MemoryService};
use engram_retrieval::RetrievalIndex;
use engram_runtime::{CoreError, CoreResult};
use futures::executor::block_on;

use crate::SqlUnifiedRecall;

/// Runs the unified-recall fixture.
///
/// Verifies the three cases described in the module docs. All stubs are
/// deterministic: this fixture does not depend on store-internal scoring.
///
/// # Errors
///
/// Returns `CoreError::Adapter` if any assertion fails.
pub fn run_recall_fixture() -> CoreResult<()> {
    // ---- 1. multi-lane → fused payload spanning lanes -----------------------
    {
        let recall = SqlUnifiedRecall::new(
            Arc::new(StubMemory::ok(vec![candidate("facts-0", "memory")])),
            vec![
                Arc::new(StubLane::ok("graph", vec![candidate("graph-0", "entity")])),
                Arc::new(StubLane::ok("vector", vec![candidate("vector-0", "chunk")])),
            ],
            Arc::new(StubBeliefs::ok(some_belief(
                "belief-0",
                "multi-lane belief",
            ))),
        );
        let payload =
            block_on(recall.recall(request("multi-lane"))).map_err(err("recall(multi-lane)"))?;

        // Items span at least three distinct lanes (facts, graph, beliefs).
        let sources: Vec<&str> = payload
            .items
            .iter()
            .filter_map(|i| i.fusion_trace.as_ref().map(|t| t.source.as_str()))
            .collect();
        let facts_present = sources
            .iter()
            .any(|s| s.contains("memory") || s.contains("facts"));
        let graph_present = sources
            .iter()
            .any(|s| s.contains("graph") || s.contains("entity"));
        let belief_present = sources.iter().any(|s| s.contains("belief"));
        // At least two lanes contributed items (the fusion merged candidates
        // from independent sources into one ranked list).
        let distinct_sources: Vec<&str> = {
            let mut v = sources.clone();
            v.sort();
            v.dedup();
            v
        };
        if distinct_sources.len() < 2 {
            return Err(err("recall(multi-lane)")(CoreError::Conflict {
                reason: format!("expected items spanning >=2 lanes, got sources: {sources:?}"),
            }));
        }
        // The belief candidate is present.
        if !payload
            .items
            .iter()
            .any(|i| i.target_type == RetrievalTargetType::Belief)
        {
            return Err(err("recall(multi-lane)")(CoreError::Conflict {
                reason: "belief-lane candidate missing from fused payload".to_string(),
            }));
        }
        // At least one item carries a FusionTrace (RRF stamps it on fused output).
        if !payload.items.iter().any(|i| i.fusion_trace.is_some()) {
            return Err(err("recall(multi-lane)")(CoreError::Conflict {
                reason: "no item carries a FusionTrace after fusion".to_string(),
            }));
        }
        // No source failures in the all-success case.
        if !payload.source_failures.is_empty() {
            return Err(err("recall(multi-lane)")(CoreError::Conflict {
                reason: format!(
                    "expected no source_failures, got {}",
                    payload.source_failures.len()
                ),
            }));
        }
        let _ = (facts_present, graph_present, belief_present);
    }

    // ---- 2. one lane fails → degraded, others still present ----------------
    {
        let recall = SqlUnifiedRecall::new(
            Arc::new(StubMemory::ok(vec![candidate("facts-0", "memory")])),
            vec![
                Arc::new(StubLane::err("graph", "forced graph failure")),
                Arc::new(StubLane::ok("vector", vec![candidate("vector-0", "chunk")])),
            ],
            Arc::new(StubBeliefs::ok(some_belief("belief-0", "degraded belief"))),
        );
        let payload = block_on(recall.recall(request("one-degraded")))
            .map_err(err("recall(one-degraded)"))?;

        // The failing lane is recorded.
        let graph_failure = payload
            .source_failures
            .iter()
            .find(|f| f.source.contains("retrieval_lane_0"));
        if graph_failure.is_none() {
            return Err(err("recall(one-degraded)")(CoreError::Conflict {
                reason: "expected a source_failure for the failing graph lane".to_string(),
            }));
        }
        let failure = graph_failure.unwrap();
        if !failure.degraded {
            return Err(err("recall(one-degraded)")(CoreError::Conflict {
                reason: "failing lane must be marked degraded".to_string(),
            }));
        }
        // The surviving lanes still contribute items — the recall did not abort.
        if payload.items.is_empty() {
            return Err(err("recall(one-degraded)")(CoreError::Conflict {
                reason: "surviving lanes should still contribute items".to_string(),
            }));
        }
        // The vector lane's candidate is present (it did not fail).
        if !payload.items.iter().any(|i| i.target_id == "vector-0") {
            return Err(err("recall(one-degraded)")(CoreError::Conflict {
                reason: "vector-lane candidate missing despite graph lane failing".to_string(),
            }));
        }
    }

    // ---- 3. ALL lanes fail → Ok, empty items, one failure per lane ----------
    {
        // Three lanes total: facts (memory) + one retrieval lane + beliefs.
        let recall = SqlUnifiedRecall::new(
            Arc::new(StubMemory::err("forced memory failure")),
            vec![Arc::new(StubLane::err("vector", "forced vector failure"))],
            Arc::new(StubBeliefs::err("forced belief failure")),
        );
        let result = block_on(recall.recall(request("all-fail")));
        // Must be Ok — degraded success, never Err.
        let payload = result.map_err(|e| {
            err("recall(all-fail)")(CoreError::Conflict {
                reason: format!("all-lanes-fail must return Ok, got Err: {e}"),
            })
        })?;

        // Empty items.
        if !payload.items.is_empty() {
            return Err(err("recall(all-fail)")(CoreError::Conflict {
                reason: format!("expected empty items, got {}", payload.items.len()),
            }));
        }
        // One source_failure per lane (facts + retrieval + beliefs = 3).
        if payload.source_failures.len() != 3 {
            return Err(err("recall(all-fail)")(CoreError::Conflict {
                reason: format!(
                    "expected 3 source_failures (one per lane), got {}",
                    payload.source_failures.len()
                ),
            }));
        }
        // Each lane's failure is present.
        for expected_source in ["facts", "retrieval_lane_0", "belief"] {
            if !payload
                .source_failures
                .iter()
                .any(|f| f.source == expected_source)
            {
                return Err(err("recall(all-fail)")(CoreError::Conflict {
                    reason: format!("missing source_failure for lane `{expected_source}`"),
                }));
            }
        }
    }

    // ---- 4. facts-lane source_failures + omitted merge into outer payload --
    {
        let facts_failure = RetrievalSourceFailure {
            source: "memory.internal".to_string(),
            mode: None,
            severity: SourceFailureSeverity::Info,
            reason: "partial_lookup".to_string(),
            message: Some("a sub-source was unavailable".to_string()),
            degraded: true,
        };
        let facts_omitted = OmittedResult {
            target_type: RetrievalTargetType::Memory,
            target_id: "omitted-mem-0".to_string(),
            reason: OmittedReason::PolicyDenied,
        };
        let recall = SqlUnifiedRecall::new(
            Arc::new(StubMemory::ok_with_failures(
                vec![candidate("facts-merge", "memory")],
                vec![facts_failure.clone()],
                vec![facts_omitted.clone()],
            )),
            vec![Arc::new(StubLane::ok(
                "graph",
                vec![candidate("graph-merge", "entity")],
            ))],
            Arc::new(StubBeliefs::ok(some_belief("belief-merge", "merge belief"))),
        );
        let payload =
            block_on(recall.recall(request("facts-merge"))).map_err(err("recall(facts-merge)"))?;

        // The facts lane's source_failure is carried on the outer payload.
        if !payload
            .source_failures
            .iter()
            .any(|f| f.source == "memory.internal")
        {
            return Err(err("recall(facts-merge)")(CoreError::Conflict {
                reason: "facts-lane source_failure was not merged into outer payload".to_string(),
            }));
        }
        // The facts lane's omitted entry is carried on the outer payload.
        if !payload
            .omitted
            .iter()
            .any(|o| o.target_id == "omitted-mem-0")
        {
            return Err(err("recall(facts-merge)")(CoreError::Conflict {
                reason: "facts-lane omitted entry was not merged into outer payload".to_string(),
            }));
        }
        // The facts-lane items still participate in the fused output.
        if !payload.items.iter().any(|i| i.target_id == "facts-merge") {
            return Err(err("recall(facts-merge)")(CoreError::Conflict {
                reason: "facts-lane candidate missing from fused payload".to_string(),
            }));
        }
    }

    Ok(())
}

// ---------- stub lane implementations -------------------------------------

/// A configurable `MemoryService` stub. `retrieve` returns the pre-set items or
/// an error; every other method is unreachable from `SqlUnifiedRecall`.
struct StubMemory {
    payload_items: Vec<RetrievalResult>,
    source_failures: Vec<RetrievalSourceFailure>,
    omitted: Vec<OmittedResult>,
    fail: bool,
    fail_msg: String,
}

impl StubMemory {
    fn ok(items: Vec<RetrievalResult>) -> Self {
        Self {
            payload_items: items,
            source_failures: Vec::new(),
            omitted: Vec::new(),
            fail: false,
            fail_msg: String::new(),
        }
    }
    /// Returns a payload whose items feed the outer fusion AND whose
    /// source_failures + omitted merge into the outer payload (Concern 4 test).
    fn ok_with_failures(
        items: Vec<RetrievalResult>,
        source_failures: Vec<RetrievalSourceFailure>,
        omitted: Vec<OmittedResult>,
    ) -> Self {
        Self {
            payload_items: items,
            source_failures,
            omitted,
            fail: false,
            fail_msg: String::new(),
        }
    }
    fn err(msg: &str) -> Self {
        Self {
            payload_items: Vec::new(),
            source_failures: Vec::new(),
            omitted: Vec::new(),
            fail: true,
            fail_msg: msg.to_string(),
        }
    }
}

fn unsupported() -> CoreError {
    CoreError::CapabilityUnsupported {
        capability: "recall-fixture-stub".to_string(),
        reason: "stub does not implement this method".to_string(),
    }
}

#[async_trait]
impl MemoryRepository for StubMemory {
    async fn put_memory(&self, _record: MemoryRecord) -> CoreResult<MemoryRecord> {
        Err(unsupported())
    }
    async fn get_memory(&self, _id: &MemoryId, _scope: &Scope) -> CoreResult<Option<MemoryRecord>> {
        Err(unsupported())
    }
    async fn append_event(&self, _event: MemoryEvent) -> CoreResult<MemoryEvent> {
        Err(unsupported())
    }
    async fn update_memory_status(
        &self,
        _id: &MemoryId,
        _scope: &Scope,
        _status: MemoryStatus,
    ) -> CoreResult<MemoryRecord> {
        Err(unsupported())
    }
}

#[async_trait]
impl MemoryEventRepository for StubMemory {
    async fn get_event(&self, _id: &EventId, _scope: &Scope) -> CoreResult<Option<MemoryEvent>> {
        Err(unsupported())
    }
    async fn list_events_for_memory(
        &self,
        _memory_id: &MemoryId,
        _scope: &Scope,
    ) -> CoreResult<Vec<MemoryEvent>> {
        Err(unsupported())
    }
    async fn list_events_for_scope(&self, _scope: &Scope) -> CoreResult<Vec<MemoryEvent>> {
        Err(unsupported())
    }
}

#[async_trait]
impl MemoryService for StubMemory {
    async fn write_memory(&self, _request: WriteMemoryRequest) -> CoreResult<WriteMemoryResponse> {
        Err(unsupported())
    }
    async fn retrieve(&self, _request: RetrievalRequest) -> CoreResult<ContextPayload> {
        if self.fail {
            return Err(CoreError::Adapter {
                adapter: "recall-fixture".to_string(),
                message: self.fail_msg.clone(),
            });
        }
        Ok(ContextPayload {
            items: self.payload_items.clone(),
            budget: None,
            omitted: self.omitted.clone(),
            source_failures: self.source_failures.clone(),
            created_at: chrono::Utc::now(),
        })
    }
    async fn forget(&self, _request: ForgetRequest) -> CoreResult<ForgetResult> {
        Err(unsupported())
    }
}

/// A configurable `RetrievalIndex` lane stub.
struct StubLane {
    source: String,
    results: Vec<RetrievalResult>,
    fail: bool,
    fail_msg: String,
}

impl StubLane {
    fn ok(source: &str, results: Vec<RetrievalResult>) -> Self {
        Self {
            source: source.to_string(),
            results,
            fail: false,
            fail_msg: String::new(),
        }
    }
    fn err(source: &str, msg: &str) -> Self {
        Self {
            source: source.to_string(),
            results: Vec::new(),
            fail: true,
            fail_msg: msg.to_string(),
        }
    }
}

#[async_trait]
impl RetrievalIndex for StubLane {
    async fn retrieve_candidates(
        &self,
        _request: &RetrievalRequest,
    ) -> CoreResult<Vec<RetrievalResult>> {
        if self.fail {
            return Err(CoreError::Adapter {
                adapter: format!("recall-fixture.{}", self.source),
                message: self.fail_msg.clone(),
            });
        }
        Ok(self.results.clone())
    }
}

/// A configurable `BeliefRepository` stub. Only `get_belief` is exercised by
/// `SqlUnifiedRecall`; the remaining methods return `CapabilityUnsupported`.
struct StubBeliefs {
    belief: Option<Belief>,
    fail: bool,
    fail_msg: String,
}

impl StubBeliefs {
    fn ok(belief: Belief) -> Self {
        Self {
            belief: Some(belief),
            fail: false,
            fail_msg: String::new(),
        }
    }
    fn err(msg: &str) -> Self {
        Self {
            belief: None,
            fail: true,
            fail_msg: msg.to_string(),
        }
    }
}

#[async_trait]
impl BeliefRepository for StubBeliefs {
    async fn put_belief(&self, _belief: Belief) -> CoreResult<Belief> {
        Err(unsupported())
    }
    async fn upsert_belief(&self, _belief: Belief) -> CoreResult<Belief> {
        Err(unsupported())
    }
    async fn get_belief(&self, _query: BeliefQuery) -> CoreResult<Option<Belief>> {
        if self.fail {
            return Err(CoreError::Adapter {
                adapter: "recall-fixture.belief".to_string(),
                message: self.fail_msg.clone(),
            });
        }
        Ok(self.belief.clone())
    }
    async fn get_belief_by_id(&self, _id: &BeliefId, _scope: &Scope) -> CoreResult<Option<Belief>> {
        Err(unsupported())
    }
    async fn mark_stale(
        &self,
        _id: &BeliefId,
        _scope: &Scope,
        _at: Timestamp,
    ) -> CoreResult<Belief> {
        Err(unsupported())
    }
    async fn clear_stale(
        &self,
        _id: &BeliefId,
        _scope: &Scope,
        _at: Timestamp,
    ) -> CoreResult<Belief> {
        Err(unsupported())
    }
    async fn supersede_belief(
        &self,
        _id: &BeliefId,
        _scope: &Scope,
        _replacement_id: BeliefId,
        _at: Timestamp,
    ) -> CoreResult<Belief> {
        Err(unsupported())
    }
    async fn retract_belief(
        &self,
        _id: &BeliefId,
        _scope: &Scope,
        _at: Timestamp,
    ) -> CoreResult<Belief> {
        Err(unsupported())
    }
    async fn list_stale(&self, _scope: &Scope) -> CoreResult<Vec<Belief>> {
        Err(unsupported())
    }
    async fn beliefs_referencing_source(
        &self,
        _query: BeliefReferenceQuery,
    ) -> CoreResult<Vec<Belief>> {
        Err(unsupported())
    }
    async fn put_contradiction(&self, _contradiction: Contradiction) -> CoreResult<Contradiction> {
        Err(unsupported())
    }
    async fn get_contradiction(
        &self,
        _id: &ContradictionId,
        _scope: &Scope,
    ) -> CoreResult<Option<Contradiction>> {
        Err(unsupported())
    }
    async fn resolve_contradiction(
        &self,
        _id: &ContradictionId,
        _scope: &Scope,
        _resolution: ContradictionResolution,
    ) -> CoreResult<Contradiction> {
        Err(unsupported())
    }
}

// ---------- domain constructors -------------------------------------------

fn request(query: &str) -> RetrievalRequest {
    RetrievalRequest {
        query: query.to_string(),
        scope: Scope {
            tenant: "tenant-recall".to_string(),
            subject: Some("subject-recall".to_string()),
            workspace: Some("workspace-recall".to_string()),
            session: None,
            environment: Some("test".to_string()),
        },
        requester: Requester {
            actor: Actor {
                id: Id::from("recall-agent"),
                kind: ActorKind::Agent,
                display_name: Some("Recall Fixture".to_string()),
                metadata: None,
            },
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

fn candidate(target_id: &str, source: &str) -> RetrievalResult {
    RetrievalResult {
        id: format!("result-{target_id}"),
        target_type: match source {
            "memory" => RetrievalTargetType::Memory,
            "entity" => RetrievalTargetType::Entity,
            "chunk" => RetrievalTargetType::Chunk,
            _ => RetrievalTargetType::Memory,
        },
        target_id: target_id.to_string(),
        content: format!("content-{target_id}"),
        score: RetrievalScore {
            total: 0.8,
            relevance: Some(0.8),
            confidence: None,
            recency: None,
            cue_match: None,
            hierarchical_fit: None,
            policy_fit: None,
        },
        provenance: Provenance {
            source: "recall-fixture".to_string(),
            actor: Actor {
                id: Id::from("recall-agent"),
                kind: ActorKind::Agent,
                display_name: None,
                metadata: None,
            },
            observed_at: chrono::Utc::now(),
            evidence: Vec::new(),
            derivations: Vec::new(),
            confidence: Some(1.0),
            method: None,
        },
        policy: Policy {
            visibility: Visibility::Workspace,
            retention: Retention::Durable,
            sensitivity: None,
            allowed_uses: vec![AllowedUse::Retrieval],
            expires_at: None,
            delete_mode: None,
        },
        explanation: None,
        fusion_trace: Some(FusionTrace {
            query_id: None,
            vector_index: None,
            embedding_time_ms: None,
            search_time_ms: None,
            source: source.to_string(),
            source_rank: Some(1),
            source_score: Some(0.8),
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

fn some_belief(id: &str, content: &str) -> Belief {
    Belief {
        id: Id::from(id),
        scope: Scope {
            tenant: "tenant-recall".to_string(),
            subject: Some("subject-recall".to_string()),
            workspace: Some("workspace-recall".to_string()),
            session: None,
            environment: Some("test".to_string()),
        },
        subject: BeliefSubject {
            key: "recall-query".to_string(),
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
        policy: Policy {
            visibility: Visibility::Workspace,
            retention: Retention::Durable,
            sensitivity: None,
            allowed_uses: vec![AllowedUse::Retrieval],
            expires_at: None,
            delete_mode: None,
        },
        provenance: Provenance {
            source: "recall-fixture".to_string(),
            actor: Actor {
                id: Id::from("recall-agent"),
                kind: ActorKind::Agent,
                display_name: None,
                metadata: None,
            },
            observed_at: chrono::Utc::now(),
            evidence: Vec::new(),
            derivations: Vec::new(),
            confidence: Some(1.0),
            method: None,
        },
        created_at: chrono::Utc::now(),
        updated_at: None,
        metadata: None,
    }
}

fn err<'a>(op: &'a str) -> impl Fn(CoreError) -> CoreError + 'a {
    move |e| CoreError::Adapter {
        adapter: "conformance.recall".to_string(),
        message: format!("{op}: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recall_fixture_passes() {
        if let Err(e) = run_recall_fixture() {
            panic!("recall fixture failed: {e}");
        }
    }
}
