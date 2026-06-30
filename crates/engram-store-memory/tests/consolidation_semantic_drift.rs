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
fn semantic_drift_detection_creates_temporal_review_records() {
    let memory_service = test_memory_service();
    let previous = put_memory(
        &memory_service,
        memory_record(
            "memory-previous",
            "Engram used SQLite",
            "engram",
            MemoryStatus::Active,
            None,
            Some(json!("sqlite")),
            first_time(),
        ),
    );
    let current = put_memory(
        &memory_service,
        memory_record(
            "memory-current",
            "Engram uses sqlite-vec",
            "engram",
            MemoryStatus::Active,
            None,
            Some(json!("sqlite-vec")),
            second_time(),
        ),
    );
    let same_object = put_memory(
        &memory_service,
        memory_record(
            "memory-same-object",
            "Engram still uses sqlite-vec",
            "engram",
            MemoryStatus::Active,
            None,
            Some(json!("sqlite-vec")),
            third_time(),
        ),
    );
    let expired = put_memory(
        &memory_service,
        memory_record(
            "memory-expired",
            "Expired Engram drift candidate",
            "engram",
            MemoryStatus::Active,
            Some(past_time()),
            Some(json!("postgres")),
            third_time(),
        ),
    );
    let other = put_memory(
        &memory_service,
        memory_record(
            "memory-other",
            "Other workspace Engram drift candidate",
            "other",
            MemoryStatus::Active,
            None,
            Some(json!("duckdb")),
            second_time(),
        ),
    );

    let consolidation = consolidation_service(memory_service.clone());
    let run = block_on(consolidation.consolidate(consolidation_request("engram")))
        .expect("semantic drift detection should complete");
    let drift = task_result(&run, ConsolidationTaskKind::SemanticDriftDetection);
    let decay = task_result(&run, ConsolidationTaskKind::Decay);

    assert_eq!(run.status, ConsolidationRunStatus::Completed);
    assert_eq!(drift.status, ConsolidationTaskStatus::Completed);
    assert_eq!(drift.items_read, Some(4));
    assert_eq!(drift.items_written, Some(2));
    assert_eq!(drift.items_updated, Some(0));
    assert_eq!(decay.items_updated, Some(1));
    assert_eq!(
        run.stats.and_then(|stats| stats.contradictions_detected),
        Some(2)
    );

    assert_drift_event(&memory_service, &previous.id, "engram", "previous");
    assert_drift_event(&memory_service, &current.id, "engram", "current");
    assert_drift_event(&memory_service, &same_object.id, "engram", "current");
    assert_no_drift_event(&memory_service, &expired.id, "engram");
    assert_no_drift_event(&memory_service, &other.id, "other");
}

#[test]
fn semantic_drift_detection_does_not_duplicate_open_records() {
    let memory_service = test_memory_service();
    let previous = put_memory(
        &memory_service,
        memory_record(
            "memory-previous",
            "Engram used SQLite",
            "engram",
            MemoryStatus::Active,
            None,
            Some(json!("sqlite")),
            first_time(),
        ),
    );
    let current = put_memory(
        &memory_service,
        memory_record(
            "memory-current",
            "Engram uses sqlite-vec",
            "engram",
            MemoryStatus::Active,
            None,
            Some(json!("sqlite-vec")),
            second_time(),
        ),
    );

    let consolidation = consolidation_service(memory_service.clone());
    let first = block_on(consolidation.consolidate(consolidation_request("engram")))
        .expect("first semantic drift run");
    assert_eq!(
        task_result(&first, ConsolidationTaskKind::SemanticDriftDetection).items_written,
        Some(1)
    );

    let second = block_on(consolidation.consolidate(consolidation_request("engram")))
        .expect("second semantic drift run");
    let drift = task_result(&second, ConsolidationTaskKind::SemanticDriftDetection);
    assert_eq!(drift.items_read, Some(2));
    assert_eq!(drift.items_written, Some(0));
    assert_eq!(
        second.stats.expect("stats").contradictions_detected,
        Some(0)
    );

    for id in [previous.id, current.id] {
        let events = block_on(memory_service.list_events_for_memory(&id, &scope("engram")))
            .expect("events for memory");
        assert_eq!(
            events
                .iter()
                .filter(|event| event.kind == MemoryEventKind::ContradictionDetected
                    && event.payload.get("reason").and_then(Scalar::as_str)
                        == Some("semantic_drift_detection"))
                .count(),
            1
        );
    }
}

fn assert_drift_event(
    memory_service: &InMemoryMemoryService,
    memory_id: &MemoryId,
    workspace: &str,
    role: &str,
) {
    let events = block_on(memory_service.list_events_for_memory(memory_id, &scope(workspace)))
        .expect("events for memory");
    let event = events
        .iter()
        .find(|event| {
            event.kind == MemoryEventKind::ContradictionDetected
                && event.payload.get("reason").and_then(Scalar::as_str)
                    == Some("semantic_drift_detection")
        })
        .expect("semantic drift event");
    assert_eq!(
        event.payload.get("role").and_then(Scalar::as_str),
        Some(role)
    );
    assert!(
        event
            .payload
            .get("contradictionId")
            .and_then(Scalar::as_str)
            .is_some()
    );
}

fn assert_no_drift_event(
    memory_service: &InMemoryMemoryService,
    memory_id: &MemoryId,
    workspace: &str,
) {
    let events = block_on(memory_service.list_events_for_memory(memory_id, &scope(workspace)))
        .expect("events for memory");
    assert!(events.iter().all(|event| {
        event.payload.get("reason").and_then(Scalar::as_str) != Some("semantic_drift_detection")
    }));
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
    status: MemoryStatus,
    expires_at: Option<Timestamp>,
    object: Option<Scalar>,
    assertion_time: Timestamp,
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
        status,
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
                valid_from: Some(assertion_time),
                valid_until: None,
            })
            .into_iter()
            .collect(),
        created_at: assertion_time,
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
        strategy: Some(ConsolidationStrategy::TimeWindow),
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
        name: "Protected semantic drift fixture".to_owned(),
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

fn first_time() -> Timestamp {
    Utc.with_ymd_and_hms(2026, 6, 1, 12, 0, 0)
        .single()
        .expect("valid fixture timestamp")
}

fn second_time() -> Timestamp {
    Utc.with_ymd_and_hms(2026, 6, 15, 12, 0, 0)
        .single()
        .expect("valid fixture timestamp")
}

fn third_time() -> Timestamp {
    Utc.with_ymd_and_hms(2026, 6, 20, 12, 0, 0)
        .single()
        .expect("valid fixture timestamp")
}

fn past_time() -> Timestamp {
    Utc.with_ymd_and_hms(2026, 6, 1, 0, 0, 0)
        .single()
        .expect("valid fixture timestamp")
}

fn scope(workspace: &str) -> Scope {
    Scope {
        tenant: "tenant-demo".to_owned(),
        subject: None,
        workspace: Some(workspace.to_owned()),
        session: None,
        environment: Some("test".to_owned()),
    }
}

fn actor() -> Actor {
    Actor {
        id: Id::from("actor-agent-1"),
        kind: ActorKind::Agent,
        display_name: Some("Consolidation Agent".to_owned()),
        metadata: None,
    }
}

fn requester() -> Requester {
    Requester {
        actor: actor(),
        roles: vec!["maintainer".to_owned()],
        permissions: vec!["memory.consolidate".to_owned()],
        on_behalf_of: None,
    }
}

fn provenance() -> Provenance {
    Provenance {
        source: "semantic_drift_test".to_owned(),
        actor: actor(),
        observed_at: fixed_time(),
        evidence: Vec::new(),
        derivations: Vec::new(),
        confidence: Some(1.0),
        method: Some("manual".to_owned()),
    }
}

fn policy(expires_at: Option<Timestamp>) -> Policy {
    Policy {
        visibility: Visibility::Workspace,
        retention: Retention::Durable,
        sensitivity: Some(Sensitivity::Low),
        allowed_uses: vec![AllowedUse::Retrieval, AllowedUse::Consolidation],
        expires_at,
        delete_mode: Some(DeleteMode::Tombstone),
    }
}
