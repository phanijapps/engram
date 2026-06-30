use std::sync::Arc;

use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use engram_domain::*;
use engram_memory::{Clock, CoreError, CoreResult, MemoryService, PolicyAuthorizer};
use engram_retrieval::RetrievalIndex;
use engram_store_memory::{InMemoryMemoryService, SequentialIdGenerator};
use futures::executor::block_on;

#[derive(Debug)]
struct FixedClock(Timestamp);

impl Clock for FixedClock {
    fn now(&self) -> Timestamp {
        self.0
    }
}

#[derive(Debug)]
struct AllowAll;

impl PolicyAuthorizer for AllowAll {
    fn can_write(
        &self,
        _requester: &Requester,
        _scope: &Scope,
        _policy: &Policy,
    ) -> CoreResult<()> {
        Ok(())
    }

    fn can_retrieve(
        &self,
        _requester: &Requester,
        _scope: &Scope,
        _policy: &Policy,
    ) -> CoreResult<()> {
        Ok(())
    }

    fn can_forget(
        &self,
        _requester: &Requester,
        _scope: &Scope,
        _policy: &Policy,
    ) -> CoreResult<()> {
        Ok(())
    }
}

#[derive(Debug)]
struct StaticIndex {
    result: RetrievalResult,
}

#[async_trait]
impl RetrievalIndex for StaticIndex {
    async fn retrieve_candidates(
        &self,
        _request: &RetrievalRequest,
    ) -> CoreResult<Vec<RetrievalResult>> {
        Ok(vec![self.result.clone()])
    }
}

#[derive(Debug)]
struct FailingIndex;

#[async_trait]
impl RetrievalIndex for FailingIndex {
    async fn retrieve_candidates(
        &self,
        _request: &RetrievalRequest,
    ) -> CoreResult<Vec<RetrievalResult>> {
        Err(CoreError::Adapter {
            adapter: "test-vector".to_owned(),
            message: "vector unavailable".to_owned(),
        })
    }
}

fn fixed_time() -> Timestamp {
    Utc.with_ymd_and_hms(2026, 6, 30, 12, 0, 0)
        .single()
        .expect("fixed timestamp")
}

fn actor() -> Actor {
    Actor {
        id: Id::from("actor-agent-1"),
        kind: ActorKind::Agent,
        display_name: Some("Retrieval Index Agent".to_owned()),
        metadata: None,
    }
}

fn requester() -> Requester {
    Requester {
        actor: actor(),
        roles: vec!["maintainer".to_owned()],
        permissions: vec!["memory.retrieve".to_owned()],
        on_behalf_of: None,
    }
}

fn scope(tenant: &str) -> Scope {
    Scope {
        tenant: tenant.to_owned(),
        subject: None,
        workspace: Some("engram".to_owned()),
        session: None,
        environment: Some("test".to_owned()),
    }
}

fn policy() -> Policy {
    Policy {
        visibility: Visibility::Workspace,
        retention: Retention::Durable,
        sensitivity: Some(Sensitivity::Low),
        allowed_uses: vec![AllowedUse::Retrieval],
        expires_at: None,
        delete_mode: Some(DeleteMode::Tombstone),
    }
}

fn provenance() -> Provenance {
    Provenance {
        source: "retrieval_index_spec".to_owned(),
        actor: actor(),
        observed_at: fixed_time(),
        evidence: Vec::new(),
        derivations: Vec::new(),
        confidence: Some(1.0),
        method: Some("manual".to_owned()),
    }
}

fn service(indexes: Vec<Arc<dyn RetrievalIndex>>) -> InMemoryMemoryService {
    InMemoryMemoryService::with_retrieval_indexes(
        Arc::new(AllowAll),
        Arc::new(FixedClock(fixed_time())),
        Arc::new(SequentialIdGenerator::new()),
        indexes,
    )
}

fn write_request(text: &str) -> WriteMemoryRequest {
    WriteMemoryRequest {
        kind: MemoryKind::Fact,
        content: MemoryContent {
            text: text.to_owned(),
            summary: None,
            entities: Vec::new(),
            language: Some("en".to_owned()),
            format: Some(MemoryContentFormat::Text),
            structured: None,
            hash: None,
        },
        scope: scope("tenant-demo"),
        requester: requester(),
        provenance: provenance(),
        policy: policy(),
        links: Vec::new(),
        idempotency_key: None,
    }
}

fn retrieval_request(query: &str) -> RetrievalRequest {
    RetrievalRequest {
        query: query.to_owned(),
        scope: scope("tenant-demo"),
        requester: requester(),
        modes: vec![RetrievalMode::Keyword, RetrievalMode::Semantic],
        filters: None,
        cues: Vec::new(),
        limit: Some(10),
        budget: None,
        include_explanations: Some(true),
    }
}

fn external_result(id: &str, target_id: &str, score: f32) -> RetrievalResult {
    RetrievalResult {
        id: id.to_owned(),
        target_type: RetrievalTargetType::Chunk,
        target_id: target_id.to_owned(),
        content: "External semantic candidate about sqlitevec retrieval.".to_owned(),
        score: RetrievalScore {
            total: score,
            relevance: Some(score),
            recency: None,
            confidence: Some(1.0),
            cue_match: None,
            hierarchical_fit: None,
            policy_fit: Some(1.0),
        },
        provenance: provenance(),
        policy: policy(),
        explanation: Some(RetrievalExplanation {
            reason: "Matched by injected retrieval index.".to_owned(),
            matched_cues: Vec::new(),
            matched_terms: vec!["sqlitevec".to_owned()],
            path: Vec::new(),
            source_summary: Some("semantic index".to_owned()),
        }),
        fusion_trace: Some(FusionTrace {
            source: "vector.semantic".to_owned(),
            source_rank: Some(1),
            source_score: Some(score),
            fusion_strategy: Some(FusionStrategy::None),
            fusion_score: Some(score),
            rerank_strategy: Some(RerankStrategy::None),
            rerank_score: Some(score),
            deduplicated_with: Vec::new(),
        }),
        metadata: None,
    }
}

#[test]
fn injected_index_candidate_participates_in_context() {
    let service = service(vec![Arc::new(StaticIndex {
        result: external_result("external-1", "chunk-1", 0.95),
    })]);

    let context =
        block_on(service.retrieve(retrieval_request("sqlitevec retrieval"))).expect("retrieve");

    assert_eq!(context.items.len(), 1);
    assert_eq!(context.items[0].target_type, RetrievalTargetType::Chunk);
    assert_eq!(context.items[0].target_id, "chunk-1");
    assert_eq!(
        context
            .items
            .first()
            .and_then(|result| result.fusion_trace.as_ref())
            .map(|trace| trace.source.as_str()),
        Some("vector.semantic")
    );
    assert!(context.source_failures.is_empty());
}

#[test]
fn external_candidates_are_omitted_after_shared_truncation() {
    let service = service(vec![Arc::new(StaticIndex {
        result: external_result("external-low", "chunk-low", 0.10),
    })]);
    block_on(service.write_memory(write_request("sqlitevec retrieval local memory")))
        .expect("write local memory");
    let mut request = retrieval_request("sqlitevec retrieval");
    request.limit = Some(1);

    let context = block_on(service.retrieve(request)).expect("retrieve");

    assert_eq!(context.items.len(), 1);
    assert_eq!(context.items[0].target_type, RetrievalTargetType::Memory);
    assert_eq!(context.omitted.len(), 1);
    assert_eq!(context.omitted[0].target_type, RetrievalTargetType::Chunk);
    assert_eq!(context.omitted[0].target_id, "chunk-low");
    assert_eq!(context.omitted[0].reason, OmittedReason::BudgetExceeded);
}

#[test]
fn failing_index_reports_degraded_source_without_hiding_local_results() {
    let service = service(vec![Arc::new(FailingIndex)]);
    block_on(service.write_memory(write_request(
        "local keyword result survives vector failure",
    )))
    .expect("write local memory");

    let context = block_on(service.retrieve(retrieval_request("vector failure")))
        .expect("retrieve with degraded index");

    assert_eq!(context.items.len(), 1);
    assert_eq!(context.items[0].target_type, RetrievalTargetType::Memory);
    assert_eq!(context.source_failures.len(), 1);
    assert_eq!(context.source_failures[0].source, "retrieval_index.1");
    assert_eq!(
        context.source_failures[0].reason,
        "external_retrieval_index_failed"
    );
    assert_eq!(
        context.source_failures[0].severity,
        SourceFailureSeverity::Warning
    );
    assert!(context.source_failures[0].degraded);
    assert!(
        context.source_failures[0]
            .message
            .as_deref()
            .unwrap_or_default()
            .contains("vector unavailable")
    );
}
