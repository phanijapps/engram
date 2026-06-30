use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use engram_core::{
    Clock, ConsolidationService, CoreResult, EvaluationCaseReport, EvaluationReport,
    EvaluationRunner, GatedConsolidationService, HierarchyRepository, MemoryEventRepository,
    MemoryService,
};
use engram_domain::*;
use engram_store_memory::{
    AllowAllPolicyAuthorizer, InMemoryConsolidationExecutor, InMemoryMemoryService,
    SequentialIdGenerator,
};
use futures::executor::block_on;

#[test]
fn hierarchy_build_creates_entity_aggregate_for_scoped_base_nodes() {
    let memory_service = test_memory_service();
    let first = write_memory(
        &memory_service,
        "Engram uses sqlite-vec",
        "engram",
        None,
        Some(entity("entity-engram", "Engram")),
    );
    let second = write_memory(
        &memory_service,
        "Engram tests FastEmbed BGE small",
        "engram",
        None,
        Some(entity("entity-engram", "Engram")),
    );
    let singleton = write_memory(
        &memory_service,
        "Solo entity should not aggregate",
        "engram",
        None,
        Some(entity("entity-solo", "Solo")),
    );
    let entityless = write_memory(
        &memory_service,
        "No entity should not aggregate",
        "engram",
        None,
        None,
    );
    let expired = write_memory(
        &memory_service,
        "Expired Engram should not aggregate",
        "engram",
        Some(past_time()),
        Some(entity("entity-engram", "Engram")),
    );
    let other = write_memory(
        &memory_service,
        "Other workspace Engram should not aggregate",
        "other",
        None,
        Some(entity("entity-engram", "Engram")),
    );

    let consolidation = consolidation_service(memory_service.clone());
    let run = block_on(consolidation.consolidate(consolidation_request("engram")))
        .expect("hierarchy aggregate build should complete");
    let hierarchy = task_result(&run, ConsolidationTaskKind::HierarchyBuild);

    assert_eq!(run.status, ConsolidationRunStatus::Completed);
    assert_eq!(hierarchy.items_read, Some(5));
    assert_eq!(hierarchy.items_written, Some(5));
    assert_eq!(hierarchy.items_updated, Some(2));
    assert_eq!(hierarchy.items_skipped, Some(3));
    let stats = run.stats.expect("stats");
    assert_eq!(stats.hierarchy_nodes_created, Some(5));

    let path = block_on(memory_service.path_for(&[first.id.to_string()], &scope("engram"), None))
        .expect("path for first memory");
    assert_eq!(path.nodes.len(), 2);
    let aggregate = path
        .nodes
        .iter()
        .find(|node| node.kind == HierarchyNodeKind::Aggregate)
        .expect("aggregate node");
    assert_eq!(aggregate.layer, 1);
    assert_eq!(aggregate.name, "Entity: Engram");
    assert_eq!(
        aggregate.summary.as_deref(),
        Some(
            "Entity: Engram groups 2 memories: summary: Engram uses sqlite-vec; summary: Engram tests FastEmbed BGE small."
        )
    );
    assert_eq!(aggregate.scope, scope("engram"));
    assert_eq!(aggregate.members.len(), 2);
    assert!(
        aggregate
            .members
            .iter()
            .all(|member| member.member_type == HierarchyMemberType::HierarchyNode)
    );
    assert_eq!(
        aggregate
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("aggregateKey"))
            .and_then(Scalar::as_str),
        Some("entity:entity-engram")
    );

    let second_path =
        block_on(memory_service.path_for(&[second.id.to_string()], &scope("engram"), None))
            .expect("path for second memory");
    assert!(second_path.nodes.iter().any(|node| node.id == aggregate.id));

    for id in [&first.id, &second.id] {
        let events = block_on(memory_service.list_events_for_memory(id, &scope("engram")))
            .expect("events for memory");
        assert!(
            events.iter().any(|event| {
                event.kind == MemoryEventKind::HierarchyBuilt
                    && event.payload.get("reason").and_then(Scalar::as_str)
                        == Some("memory_entity_aggregate_hierarchy_build")
            }),
            "missing aggregate hierarchy event for {id}"
        );
    }

    for id in [&singleton.id, &entityless.id, &expired.id] {
        let path = block_on(memory_service.path_for(&[id.to_string()], &scope("engram"), None))
            .expect("path for skipped memory");
        assert!(
            path.nodes
                .iter()
                .all(|node| node.kind != HierarchyNodeKind::Aggregate)
        );
    }
    let other_path =
        block_on(memory_service.path_for(&[other.id.to_string()], &scope("engram"), None))
            .expect("other path");
    assert!(other_path.nodes.is_empty());
}

#[test]
fn hierarchy_aggregate_build_is_idempotent() {
    let memory_service = test_memory_service();
    let first = write_memory(
        &memory_service,
        "Engram memory one",
        "engram",
        None,
        Some(entity("entity-engram", "Engram")),
    );
    let second = write_memory(
        &memory_service,
        "Engram memory two",
        "engram",
        None,
        Some(entity("entity-engram", "Engram")),
    );

    let consolidation = consolidation_service(memory_service.clone());
    let first_run = block_on(consolidation.consolidate(consolidation_request("engram")))
        .expect("first aggregate run");
    assert_eq!(
        task_result(&first_run, ConsolidationTaskKind::HierarchyBuild).items_updated,
        Some(2)
    );

    let second_run = block_on(consolidation.consolidate(consolidation_request("engram")))
        .expect("second aggregate run");
    let hierarchy = task_result(&second_run, ConsolidationTaskKind::HierarchyBuild);
    assert_eq!(hierarchy.items_written, Some(0));
    assert_eq!(hierarchy.items_updated, Some(0));
    assert_eq!(
        second_run.stats.expect("stats").hierarchy_nodes_created,
        Some(0)
    );

    for id in [first.id, second.id] {
        let events = block_on(memory_service.list_events_for_memory(&id, &scope("engram")))
            .expect("events for memory");
        assert_eq!(
            events
                .iter()
                .filter(|event| event.kind == MemoryEventKind::HierarchyBuilt
                    && event.payload.get("reason").and_then(Scalar::as_str)
                        == Some("memory_entity_aggregate_hierarchy_build"))
                .count(),
            1
        );
    }
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

fn write_memory(
    memory_service: &InMemoryMemoryService,
    text: &str,
    workspace: &str,
    expires_at: Option<Timestamp>,
    entity: Option<EntityRef>,
) -> MemoryRecord {
    block_on(memory_service.write_memory(WriteMemoryRequest {
        kind: MemoryKind::Fact,
        content: MemoryContent {
            text: text.to_owned(),
            summary: Some(format!("summary: {text}")),
            entities: entity.into_iter().collect(),
            language: Some("en".to_owned()),
            format: Some(MemoryContentFormat::Text),
            structured: None,
            hash: None,
        },
        scope: scope(workspace),
        requester: requester(),
        provenance: provenance(),
        policy: policy(expires_at),
        links: Vec::new(),
        idempotency_key: None,
    }))
    .expect("write memory")
    .record
}

fn entity(id: &str, name: &str) -> EntityRef {
    EntityRef {
        id: Some(Id::from(id)),
        kind: Some("project".to_owned()),
        name: Some(name.to_owned()),
        aliases: Vec::new(),
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
        name: "Protected hierarchy aggregate fixture".to_owned(),
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
        source: "consolidation_hierarchy_aggregate_test".to_owned(),
        actor: requester().actor,
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
        allowed_uses: vec![AllowedUse::Retrieval],
        expires_at,
        delete_mode: Some(DeleteMode::Archive),
    }
}
