use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use engram_core::{
    Clock, ConsolidationService, CoreResult, EvaluationCaseReport, EvaluationReport,
    EvaluationRunner, GatedConsolidationService, MemoryEventRepository, MemoryRepository,
};
use engram_domain::*;
use engram_store_memory::{
    AllowAllPolicyAuthorizer, InMemoryConsolidationExecutor, InMemoryMemoryService,
    SequentialIdGenerator,
};
use futures::executor::block_on;
use serde_json::json;

#[test]
fn contradiction_detection_creates_review_record_events_for_conflicting_assertions() {
    let memory_service = test_memory_service();
    let left = put_memory(
        &memory_service,
        memory_record(
            "memory-left",
            "engram uses sqlite",
            "engram",
            None,
            Some(json!("sqlite")),
        ),
    );
    let right = put_memory(
        &memory_service,
        memory_record(
            "memory-right",
            "engram uses postgres",
            "engram",
            None,
            Some(json!("postgres")),
        ),
    );
    let compatible = put_memory(
        &memory_service,
        memory_record(
            "memory-compatible",
            "engram uses sqlite again",
            "engram",
            None,
            Some(json!("sqlite")),
        ),
    );
    let expired = put_memory(
        &memory_service,
        memory_record(
            "memory-expired",
            "expired contradiction candidate",
            "engram",
            Some(past_time()),
            Some(json!("duckdb")),
        ),
    );
    let other = put_memory(
        &memory_service,
        memory_record(
            "memory-other",
            "other workspace contradiction candidate",
            "other",
            None,
            Some(json!("oracle")),
        ),
    );

    let consolidation = consolidation_service(memory_service.clone());
    let run = block_on(consolidation.consolidate(consolidation_request("engram")))
        .expect("contradiction detection should complete");
    let detection = task_result(&run, ConsolidationTaskKind::BeliefContradictionDetection);

    assert_eq!(run.status, ConsolidationRunStatus::Completed);
    assert_eq!(detection.items_read, Some(4));
    assert_eq!(detection.items_written, Some(2));
    assert_eq!(detection.items_updated, Some(0));
    assert_eq!(
        run.stats.and_then(|stats| stats.contradictions_detected),
        Some(2)
    );

    assert_contradiction_event(
        &memory_service,
        &left.id,
        "engram",
        "memory-left#assertion-0",
    );
    assert_contradiction_event(
        &memory_service,
        &right.id,
        "engram",
        "memory-right#assertion-0",
    );
    assert_contradiction_event(
        &memory_service,
        &compatible.id,
        "engram",
        "memory-compatible#assertion-0",
    );
    assert_no_contradiction_event(&memory_service, &expired.id, "engram");
    assert_no_contradiction_event(&memory_service, &other.id, "other");
}

#[test]
fn contradiction_detection_does_not_duplicate_open_contradictions() {
    let memory_service = test_memory_service();
    let left = put_memory(
        &memory_service,
        memory_record(
            "memory-left",
            "engram uses sqlite",
            "engram",
            None,
            Some(json!("sqlite")),
        ),
    );
    let right = put_memory(
        &memory_service,
        memory_record(
            "memory-right",
            "engram uses postgres",
            "engram",
            None,
            Some(json!("postgres")),
        ),
    );

    let consolidation = consolidation_service(memory_service.clone());
    let first = block_on(consolidation.consolidate(consolidation_request("engram")))
        .expect("first detection should complete");
    assert_eq!(
        task_result(&first, ConsolidationTaskKind::BeliefContradictionDetection).items_written,
        Some(1)
    );

    let second = block_on(consolidation.consolidate(consolidation_request("engram")))
        .expect("second detection should complete");
    let detection = task_result(&second, ConsolidationTaskKind::BeliefContradictionDetection);
    assert_eq!(detection.items_read, Some(2));
    assert_eq!(detection.items_written, Some(0));
    assert_eq!(
        second.stats.expect("stats").contradictions_detected,
        Some(0)
    );

    for id in [left.id, right.id] {
        let events = block_on(memory_service.list_events_for_memory(&id, &scope("engram")))
            .expect("events for memory");
        assert_eq!(
            events
                .iter()
                .filter(|event| event.kind == MemoryEventKind::ContradictionDetected)
                .count(),
            1
        );
    }
}

fn assert_contradiction_event(
    memory_service: &InMemoryMemoryService,
    memory_id: &MemoryId,
    workspace: &str,
    assertion_id: &str,
) {
    let events = block_on(memory_service.list_events_for_memory(memory_id, &scope(workspace)))
        .expect("events for memory");
    let event = events
        .iter()
        .find(|event| event.kind == MemoryEventKind::ContradictionDetected)
        .expect("contradiction event");
    assert_eq!(
        event.payload.get("reason").and_then(Scalar::as_str),
        Some("assertion_contradiction_detection")
    );
    assert_eq!(
        event.payload.get("assertionId").and_then(Scalar::as_str),
        Some(assertion_id)
    );
    assert!(
        event
            .payload
            .get("contradictionId")
            .and_then(Scalar::as_str)
            .is_some()
    );
}

fn assert_no_contradiction_event(
    memory_service: &InMemoryMemoryService,
    memory_id: &MemoryId,
    workspace: &str,
) {
    let events = block_on(memory_service.list_events_for_memory(memory_id, &scope(workspace)))
        .expect("events for memory");
    assert!(
        events
            .iter()
            .all(|event| event.kind != MemoryEventKind::ContradictionDetected)
    );
}

fn task_result(run: &ConsolidationRun, task: ConsolidationTaskKind) -> &ConsolidationTaskResult {
    run.tasks
        .iter()
        .find(|result| result.task == task)
        .expect("task result")
}

fn test_memory_service() -> InMemoryMemoryService {
    InMemoryMemoryService::with_dependencies(
        Arc::new(AllowAllPolicyAuthorizer),
        Arc::new(FixedClock(fixed_time())),
        Arc::new(SequentialIdGenerator::new()),
    )
}

fn consolidation_service(memory_service: InMemoryMemoryService) -> GatedConsolidationService {
    GatedConsolidationService::new(
        Arc::new(FixedClock(fixed_time())),
        Arc::new(SequentialIdGenerator::new()),
        Arc::new(ScriptedEvaluator {
            reports: Mutex::new(VecDeque::from(vec![
                passing_report(),
                passing_report(),
                passing_report(),
                passing_report(),
            ])),
        }),
        evaluation_fixture(),
        InMemoryConsolidationExecutor::shared(memory_service),
    )
}

fn put_memory(memory_service: &InMemoryMemoryService, record: MemoryRecord) -> MemoryRecord {
    block_on(memory_service.put_memory(record)).expect("put memory")
}

fn memory_record(
    id: &str,
    text: &str,
    workspace: &str,
    expires_at: Option<Timestamp>,
    object: Option<Scalar>,
) -> MemoryRecord {
    MemoryRecord {
        id: Id::from(id),
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
        scope: scope(workspace),
        provenance: provenance(),
        policy: policy(expires_at),
        status: MemoryStatus::Active,
        links: Vec::new(),
        assertions: object
            .map(|object| MemoryAssertion {
                subject: EntityRef {
                    id: Some(Id::from("entity-engram")),
                    kind: Some("project".to_owned()),
                    name: Some("Engram".to_owned()),
                    aliases: Vec::new(),
                },
                predicate: "uses_database".to_owned(),
                object,
                confidence: Some(0.8),
                valid_from: None,
                valid_until: None,
            })
            .into_iter()
            .collect(),
        created_at: fixed_time(),
        updated_at: None,
        metadata: None,
    }
}

fn consolidation_request(workspace: &str) -> ConsolidationRequest {
    ConsolidationRequest {
        scope: scope(workspace),
        requester: requester(),
        since: None,
        until: None,
        strategy: Some(ConsolidationStrategy::Hybrid),
        dry_run: Some(false),
    }
}

#[derive(Debug)]
struct FixedClock(Timestamp);

impl Clock for FixedClock {
    fn now(&self) -> Timestamp {
        self.0
    }
}

#[derive(Debug)]
struct ScriptedEvaluator {
    reports: Mutex<VecDeque<EvaluationReport>>,
}

#[async_trait]
impl EvaluationRunner for ScriptedEvaluator {
    async fn run_fixture(&self, _fixture: EvaluationFixture) -> CoreResult<EvaluationReport> {
        Ok(self
            .reports
            .lock()
            .expect("reports lock")
            .pop_front()
            .expect("scripted evaluation report"))
    }
}

fn passing_report() -> EvaluationReport {
    EvaluationReport {
        fixture_id: Id::from("eval-fixture"),
        cases: vec![EvaluationCaseReport {
            case_id: "protected-recall".to_owned(),
            passed: true,
            failures: Vec::new(),
        }],
    }
}

fn evaluation_fixture() -> EvaluationFixture {
    EvaluationFixture {
        id: Id::from("eval-fixture"),
        name: "Protected contradiction fixture".to_owned(),
        scope: scope("engram"),
        setup: EvaluationSetup {
            memories: Vec::new(),
            sources: Vec::new(),
            documents: Vec::new(),
            chunks: Vec::new(),
        },
        cases: Vec::new(),
        created_at: fixed_time(),
    }
}

fn fixed_time() -> Timestamp {
    Utc.with_ymd_and_hms(2026, 6, 30, 12, 0, 0)
        .single()
        .expect("valid fixture timestamp")
}

fn past_time() -> Timestamp {
    Utc.with_ymd_and_hms(2026, 6, 29, 12, 0, 0)
        .single()
        .expect("valid past timestamp")
}

fn scope(workspace: &str) -> Scope {
    Scope {
        tenant: "tenant-a".to_owned(),
        subject: None,
        workspace: Some(workspace.to_owned()),
        session: None,
        environment: Some("test".to_owned()),
    }
}

fn requester() -> Requester {
    Requester {
        actor: Actor {
            id: Id::from("agent-a"),
            kind: ActorKind::Agent,
            display_name: None,
            metadata: None,
        },
        roles: Vec::new(),
        permissions: Vec::new(),
        on_behalf_of: None,
    }
}

fn provenance() -> Provenance {
    Provenance {
        source: "consolidation_contradiction_detection_test".to_owned(),
        actor: requester().actor,
        observed_at: fixed_time(),
        evidence: Vec::new(),
        derivations: Vec::new(),
        confidence: Some(0.9),
        method: Some("manual".to_owned()),
    }
}

fn policy(expires_at: Option<Timestamp>) -> Policy {
    Policy {
        visibility: Visibility::Workspace,
        retention: Retention::Durable,
        sensitivity: Some(Sensitivity::Low),
        allowed_uses: vec![AllowedUse::Retrieval],
        expires_at,
        delete_mode: Some(DeleteMode::Archive),
    }
}
