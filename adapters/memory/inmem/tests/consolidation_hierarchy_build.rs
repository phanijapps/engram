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
fn hierarchy_build_creates_base_nodes_and_audit_events_for_scoped_memories() {
    let memory_service = test_memory_service();
    let first = write_memory(&memory_service, "first hierarchy memory", "engram", None);
    let second = write_memory(&memory_service, "second hierarchy memory", "engram", None);
    let expired = write_memory(
        &memory_service,
        "expired hierarchy memory",
        "engram",
        Some(past_time()),
    );
    let other = write_memory(&memory_service, "other workspace memory", "other", None);

    put_existing_base_node(&memory_service, &second);

    let consolidation = consolidation_service(memory_service.clone());
    let run = block_on(consolidation.consolidate(consolidation_request("engram")))
        .expect("hierarchy build should complete");
    let hierarchy = task_result(&run, ConsolidationTaskKind::HierarchyBuild);
    let belief = task_result(&run, ConsolidationTaskKind::BeliefSynthesis);

    assert_eq!(run.status, ConsolidationRunStatus::Completed);
    assert_eq!(hierarchy.items_read, Some(3));
    assert_eq!(hierarchy.items_written, Some(1));
    assert_eq!(hierarchy.items_updated, Some(0));
    assert_eq!(hierarchy.items_skipped, Some(4));
    assert_eq!(belief.status, ConsolidationTaskStatus::Completed);
    assert_eq!(belief.items_written, Some(0));
    let stats = run.stats.expect("stats");
    assert_eq!(stats.hierarchy_nodes_created, Some(1));
    assert_eq!(stats.hierarchy_relations_created, Some(0));

    let first_path =
        block_on(memory_service.path_for(&[first.id.to_string()], &scope("engram"), Some(0)))
            .expect("path for first memory");
    assert_eq!(first_path.nodes.len(), 1);
    assert_eq!(
        first_path.nodes[0].source_target_id.as_deref(),
        Some(first.id.as_str())
    );
    assert_eq!(
        first_path.nodes[0].source_target_type,
        Some(RetrievalTargetType::Memory)
    );
    assert_eq!(first_path.nodes[0].kind, HierarchyNodeKind::Base);
    assert_eq!(first_path.nodes[0].layer, 0);
    assert_eq!(first_path.nodes[0].status, HierarchyNodeStatus::Active);
    assert_eq!(first_path.nodes[0].scope, scope("engram"));
    assert_eq!(first_path.nodes[0].policy, policy(None));

    let first_events = block_on(memory_service.list_events_for_memory(&first.id, &scope("engram")))
        .expect("events for first memory");
    let built = first_events
        .iter()
        .find(|event| event.kind == MemoryEventKind::HierarchyBuilt)
        .expect("hierarchy built event");
    assert_eq!(
        built.payload.get("reason").and_then(Scalar::as_str),
        Some("memory_base_hierarchy_build")
    );
    assert!(
        built
            .payload
            .get("hierarchyNodeId")
            .and_then(Scalar::as_str)
            .is_some()
    );

    let second_path =
        block_on(memory_service.path_for(&[second.id.to_string()], &scope("engram"), Some(0)))
            .expect("path for second memory");
    assert_eq!(second_path.nodes.len(), 1);

    let expired_path =
        block_on(memory_service.path_for(&[expired.id.to_string()], &scope("engram"), Some(0)))
            .expect("path for expired memory");
    assert!(expired_path.nodes.is_empty());

    let other_path =
        block_on(memory_service.path_for(&[other.id.to_string()], &scope("engram"), Some(0)))
            .expect("path for other workspace memory");
    assert!(other_path.nodes.is_empty());
}

#[test]
fn hierarchy_build_is_zero_update_when_all_scoped_memories_have_base_nodes() {
    let memory_service = test_memory_service();
    let first = write_memory(&memory_service, "first covered memory", "engram", None);
    let second = write_memory(&memory_service, "second covered memory", "engram", None);
    put_existing_base_node(&memory_service, &first);
    put_existing_base_node(&memory_service, &second);

    let consolidation = consolidation_service(memory_service.clone());
    let run = block_on(consolidation.consolidate(consolidation_request("engram")))
        .expect("hierarchy build should complete");
    let hierarchy = task_result(&run, ConsolidationTaskKind::HierarchyBuild);

    assert_eq!(run.status, ConsolidationRunStatus::Completed);
    assert_eq!(hierarchy.items_read, Some(2));
    assert_eq!(hierarchy.items_written, Some(0));
    assert_eq!(hierarchy.items_skipped, Some(4));
    assert_eq!(run.stats.expect("stats").hierarchy_nodes_created, Some(0));

    for id in [first.id, second.id] {
        let events = block_on(memory_service.list_events_for_memory(&id, &scope("engram")))
            .expect("events for memory");
        assert!(
            events
                .iter()
                .all(|event| event.kind != MemoryEventKind::HierarchyBuilt)
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
    expires_at: Option<Timestamp>,
) -> MemoryRecord {
    block_on(memory_service.write_memory(WriteMemoryRequest {
        kind: MemoryKind::Fact,
        content: MemoryContent {
            text: text.to_owned(),
            summary: Some(format!("summary: {text}")),
            entities: Vec::new(),
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

fn put_existing_base_node(memory_service: &InMemoryMemoryService, record: &MemoryRecord) {
    block_on(memory_service.put_node(HierarchyNode {
        id: Id::from(format!("existing-node-{}", record.id)),
        scope: record.scope.clone(),
        kind: HierarchyNodeKind::Base,
        layer: 0,
        name: format!("existing {}", record.id),
        summary: record.content.summary.clone(),
        parent_id: None,
        members: Vec::new(),
        source_target_type: Some(RetrievalTargetType::Memory),
        source_target_id: Some(record.id.to_string()),
        embedding_refs: Vec::new(),
        status: HierarchyNodeStatus::Active,
        policy: record.policy.clone(),
        provenance: provenance(),
        created_at: fixed_time(),
        updated_at: None,
        metadata: None,
    }))
    .expect("put existing base node");
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
        name: "Protected hierarchy fixture".to_owned(),
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
        source: "consolidation_hierarchy_build_test".to_owned(),
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
