use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
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
fn compaction_archives_later_scoped_duplicates_and_records_events() {
    let memory_service = test_memory_service();
    let first = write_memory(
        &memory_service,
        "BGE small is the test embedding model",
        "engram",
    );
    let second = write_memory(
        &memory_service,
        "  bge small   is the test embedding model  ",
        "engram",
    );
    let unique = write_memory(&memory_service, "sqlite-vec backs vector tests", "engram");

    let consolidation = consolidation_service(memory_service.clone());
    let run = block_on(consolidation.consolidate(consolidation_request("engram")))
        .expect("compaction should complete");

    assert_eq!(run.status, ConsolidationRunStatus::Completed);
    assert_eq!(run.tasks[1].task, ConsolidationTaskKind::Compaction);
    assert_eq!(run.tasks[1].items_read, Some(3));
    assert_eq!(run.tasks[1].items_updated, Some(1));
    assert_eq!(run.tasks[1].items_written, Some(0));
    assert_eq!(run.tasks[1].items_skipped, Some(2));
    assert_eq!(run.tasks[2].task, ConsolidationTaskKind::MemorySynthesis);
    assert_eq!(run.tasks[2].status, ConsolidationTaskStatus::Skipped);
    assert_eq!(run.stats.and_then(|stats| stats.records_pruned), Some(1));

    let first_stored = block_on(memory_service.get_memory(&first.id, &scope("engram")))
        .expect("lookup first")
        .expect("first memory");
    let second_stored = block_on(memory_service.get_memory(&second.id, &scope("engram")))
        .expect("lookup second")
        .expect("second memory");
    let unique_stored = block_on(memory_service.get_memory(&unique.id, &scope("engram")))
        .expect("lookup unique")
        .expect("unique memory");

    assert_eq!(first_stored.status, MemoryStatus::Active);
    assert_eq!(second_stored.status, MemoryStatus::Archived);
    assert_eq!(unique_stored.status, MemoryStatus::Active);

    let second_events =
        block_on(memory_service.list_events_for_memory(&second.id, &scope("engram")))
            .expect("events for archived duplicate");
    let consolidated = second_events
        .iter()
        .find(|event| event.kind == MemoryEventKind::Consolidated)
        .expect("consolidated event");
    assert_eq!(
        consolidated
            .payload
            .get("preservedMemoryId")
            .and_then(Scalar::as_str),
        Some(first.id.as_str())
    );
    assert_eq!(
        consolidated.payload.get("reason").and_then(Scalar::as_str),
        Some("exact_duplicate_compaction")
    );
}

#[test]
fn compaction_does_not_mutate_other_workspace_duplicates() {
    let memory_service = test_memory_service();
    let scoped = write_memory(&memory_service, "same text", "engram");
    let other = write_memory(&memory_service, "same text", "other");

    let consolidation = consolidation_service(memory_service.clone());
    let run = block_on(consolidation.consolidate(consolidation_request("engram")))
        .expect("compaction should complete");

    assert_eq!(run.status, ConsolidationRunStatus::Completed);
    assert_eq!(run.tasks[1].items_updated, Some(0));

    let scoped_stored = block_on(memory_service.get_memory(&scoped.id, &scope("engram")))
        .expect("lookup scoped")
        .expect("scoped memory");
    let other_stored = block_on(memory_service.get_memory(&other.id, &scope("other")))
        .expect("lookup other")
        .expect("other memory");

    assert_eq!(scoped_stored.status, MemoryStatus::Active);
    assert_eq!(other_stored.status, MemoryStatus::Active);
}

#[test]
fn compaction_without_duplicates_is_a_zero_update_task() {
    let memory_service = test_memory_service();
    let first = write_memory(&memory_service, "alpha memory", "engram");
    let second = write_memory(&memory_service, "beta memory", "engram");

    let consolidation = consolidation_service(memory_service.clone());
    let run = block_on(consolidation.consolidate(consolidation_request("engram")))
        .expect("compaction should complete");

    assert_eq!(run.status, ConsolidationRunStatus::Completed);
    assert_eq!(run.tasks[1].items_read, Some(2));
    assert_eq!(run.tasks[1].items_updated, Some(0));
    assert_eq!(run.tasks[1].items_skipped, Some(2));
    assert_eq!(run.stats.and_then(|stats| stats.records_pruned), Some(0));

    for id in [first.id, second.id] {
        let stored = block_on(memory_service.get_memory(&id, &scope("engram")))
            .expect("lookup memory")
            .expect("memory remains");
        assert_eq!(stored.status, MemoryStatus::Active);
        let events = block_on(memory_service.list_events_for_memory(&id, &scope("engram")))
            .expect("events for memory");
        assert!(
            events
                .iter()
                .all(|event| event.kind != MemoryEventKind::Consolidated)
        );
    }
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
            reports: Mutex::new(VecDeque::from(vec![passing_report(), passing_report()])),
        }),
        evaluation_fixture(),
        InMemoryConsolidationExecutor::shared(memory_service),
    )
}

fn write_memory(
    memory_service: &InMemoryMemoryService,
    text: &str,
    workspace: &str,
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
        policy: policy(),
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
        strategy: Some(ConsolidationStrategy::EventCount),
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
        name: "Protected compaction fixture".to_owned(),
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
    DateTime::parse_from_rfc3339("2026-06-30T12:00:00Z")
        .expect("valid fixture timestamp")
        .with_timezone(&Utc)
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
        source: "consolidation_compaction_test".to_owned(),
        actor: requester().actor,
        observed_at: fixed_time(),
        evidence: Vec::new(),
        derivations: Vec::new(),
        confidence: Some(1.0),
        method: Some("manual".to_owned()),
    }
}

fn policy() -> Policy {
    Policy {
        visibility: Visibility::Workspace,
        retention: Retention::Durable,
        sensitivity: Some(Sensitivity::Low),
        allowed_uses: vec![AllowedUse::Retrieval],
        expires_at: None,
        delete_mode: Some(DeleteMode::Archive),
    }
}
