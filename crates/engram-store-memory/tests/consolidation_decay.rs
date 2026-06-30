use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use engram_core::{
    Clock, ConsolidationService, CoreResult, EvaluationCaseReport, EvaluationReport,
    EvaluationRunner, GatedConsolidationService, MemoryEventRepository, MemoryRepository,
    MemoryService,
};
use engram_domain::*;
use engram_store_memory::{
    AllowAllPolicyAuthorizer, InMemoryConsolidationExecutor, InMemoryMemoryService,
    SequentialIdGenerator,
};
use futures::executor::block_on;

#[test]
fn decay_expires_due_scoped_memories_and_records_events() {
    let memory_service = test_memory_service();
    let due = write_memory(
        &memory_service,
        "expired preference",
        "engram",
        Retention::Durable,
        Some(past_time()),
    );
    let future = write_memory(
        &memory_service,
        "future preference",
        "engram",
        Retention::Durable,
        Some(future_time()),
    );
    let legal_hold = write_memory(
        &memory_service,
        "legal hold preference",
        "engram",
        Retention::LegalHold,
        Some(past_time()),
    );
    let other = write_memory(
        &memory_service,
        "other workspace expired preference",
        "other",
        Retention::Durable,
        Some(past_time()),
    );

    let consolidation = consolidation_service(memory_service.clone());
    let run = block_on(consolidation.consolidate(consolidation_request("engram")))
        .expect("decay should complete");
    let decay = task_result(&run, ConsolidationTaskKind::Decay);
    let drift = task_result(&run, ConsolidationTaskKind::SemanticDriftDetection);

    assert_eq!(run.status, ConsolidationRunStatus::Completed);
    assert_eq!(decay.items_read, Some(3));
    assert_eq!(decay.items_updated, Some(1));
    assert_eq!(decay.items_written, Some(0));
    assert_eq!(decay.items_skipped, Some(2));
    assert_eq!(drift.status, ConsolidationTaskStatus::Completed);
    assert_eq!(drift.items_read, Some(3));
    assert_eq!(drift.items_written, Some(0));
    assert_eq!(run.stats.and_then(|stats| stats.records_decayed), Some(1));

    assert_status(&memory_service, &due.id, "engram", MemoryStatus::Expired);
    assert_status(&memory_service, &future.id, "engram", MemoryStatus::Active);
    assert_status(
        &memory_service,
        &legal_hold.id,
        "engram",
        MemoryStatus::Active,
    );
    assert_status(&memory_service, &other.id, "other", MemoryStatus::Active);

    let due_events = block_on(memory_service.list_events_for_memory(&due.id, &scope("engram")))
        .expect("events for expired memory");
    let expired = due_events
        .iter()
        .find(|event| event.kind == MemoryEventKind::Expired)
        .expect("expired event");
    assert_eq!(
        expired.payload.get("reason").and_then(Scalar::as_str),
        Some("policy_expiry_decay")
    );
    assert_eq!(
        expired
            .payload
            .get("policyExpiresAt")
            .and_then(Scalar::as_str),
        Some(past_time().to_rfc3339().as_str())
    );
}

#[test]
fn decay_without_due_records_is_a_zero_update_task() {
    let memory_service = test_memory_service();
    let future = write_memory(
        &memory_service,
        "future memory",
        "engram",
        Retention::Durable,
        Some(future_time()),
    );
    let no_expiry = write_memory(
        &memory_service,
        "durable memory",
        "engram",
        Retention::Durable,
        None,
    );

    let consolidation = consolidation_service(memory_service.clone());
    let run = block_on(consolidation.consolidate(consolidation_request("engram")))
        .expect("decay should complete");
    let decay = task_result(&run, ConsolidationTaskKind::Decay);

    assert_eq!(run.status, ConsolidationRunStatus::Completed);
    assert_eq!(decay.items_read, Some(2));
    assert_eq!(decay.items_updated, Some(0));
    assert_eq!(decay.items_skipped, Some(2));
    assert_eq!(run.stats.and_then(|stats| stats.records_decayed), Some(0));

    for id in [future.id, no_expiry.id] {
        assert_status(&memory_service, &id, "engram", MemoryStatus::Active);
        let events = block_on(memory_service.list_events_for_memory(&id, &scope("engram")))
            .expect("events for memory");
        assert!(
            events
                .iter()
                .all(|event| event.kind != MemoryEventKind::Expired)
        );
    }
}

fn task_result(run: &ConsolidationRun, task: ConsolidationTaskKind) -> &ConsolidationTaskResult {
    run.tasks
        .iter()
        .find(|result| result.task == task)
        .expect("task result")
}

fn assert_status(
    memory_service: &InMemoryMemoryService,
    id: &MemoryId,
    workspace: &str,
    expected: MemoryStatus,
) {
    let stored = block_on(memory_service.get_memory(id, &scope(workspace)))
        .expect("lookup memory")
        .expect("memory exists");
    assert_eq!(stored.status, expected);
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

fn write_memory(
    memory_service: &InMemoryMemoryService,
    text: &str,
    workspace: &str,
    retention: Retention,
    expires_at: Option<Timestamp>,
) -> MemoryRecord {
    block_on(memory_service.write_memory(WriteMemoryRequest {
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
        requester: requester(),
        provenance: provenance(),
        policy: policy(retention, expires_at),
        links: Vec::new(),
        idempotency_key: None,
    }))
    .expect("write memory")
    .record
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
        name: "Protected decay fixture".to_owned(),
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

fn future_time() -> Timestamp {
    Utc.with_ymd_and_hms(2026, 7, 1, 12, 0, 0)
        .single()
        .expect("valid future timestamp")
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
        source: "consolidation_decay_test".to_owned(),
        actor: requester().actor,
        observed_at: fixed_time(),
        evidence: Vec::new(),
        derivations: Vec::new(),
        confidence: Some(1.0),
        method: Some("manual".to_owned()),
    }
}

fn policy(retention: Retention, expires_at: Option<Timestamp>) -> Policy {
    Policy {
        visibility: Visibility::Workspace,
        retention,
        sensitivity: Some(Sensitivity::Low),
        allowed_uses: vec![AllowedUse::Retrieval],
        expires_at,
        delete_mode: Some(DeleteMode::Archive),
    }
}
