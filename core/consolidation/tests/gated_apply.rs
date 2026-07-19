use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use engram_consolidation::{
    ConsolidationApplyGate, ConsolidationMutationExecutor, ConsolidationMutationOutcome,
    ConsolidationService, GatedConsolidationService,
};
use engram_domain::{
    Actor, ActorKind, ConsolidationError, ConsolidationPlan, ConsolidationRequest,
    ConsolidationRunStatus, ConsolidationStats, ConsolidationStrategy, ConsolidationTaskKind,
    ConsolidationTaskResult, ConsolidationTaskStatus, EvaluationFixture, EvaluationSetup, Id,
    Requester, Scope, Timestamp,
};
use engram_eval::{EvaluationCaseReport, EvaluationReport, EvaluationRunner};
use engram_runtime::{Clock, CoreError, CoreResult, IdGenerator};
use futures::executor::block_on;

#[test]
fn policy_gate_denial_prevents_mutation_executor() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let service = gated_service(
        log.clone(),
        vec![Ok(passing_report())],
        Ok(mutation_outcome()),
        Arc::new(DenyingGate { log: log.clone() }),
    );

    let run = block_on(service.consolidate(mutating_request()))
        .expect("gate denial should return auditable run");

    assert_eq!(logged(&log), vec!["eval", "gate"]);
    assert_eq!(run.status, ConsolidationRunStatus::Failed);
    assert_eq!(run.tasks.len(), 1);
    assert_eq!(run.tasks[0].task, ConsolidationTaskKind::Evaluation);
    assert!(
        run.errors
            .iter()
            .any(|error| error.code == "policy_denied" && !error.recoverable)
    );
}

#[test]
fn allow_gate_passes_full_architecture_plan_to_executor() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let service = gated_service(
        log.clone(),
        vec![Ok(passing_report()), Ok(passing_report())],
        Ok(mutation_outcome()),
        Arc::new(RecordingGate { log: log.clone() }),
    );

    let run = block_on(service.consolidate(mutating_request()))
        .expect("gated consolidation should return run");

    assert_eq!(logged(&log), vec!["eval", "gate", "execute", "eval"]);
    assert_eq!(run.status, ConsolidationRunStatus::Completed);
    assert!(run.errors.is_empty());
}

#[derive(Debug)]
struct FixedClock(Timestamp);

impl Clock for FixedClock {
    fn now(&self) -> Timestamp {
        self.0
    }
}

#[derive(Debug)]
struct FixedIds;

impl IdGenerator for FixedIds {
    fn new_id(&self, entity_type: &'static str) -> Id {
        Id::from(format!("{entity_type}-fixed"))
    }
}

#[derive(Debug)]
struct ScriptedEvaluator {
    reports: Mutex<VecDeque<CoreResult<EvaluationReport>>>,
    log: Arc<Mutex<Vec<&'static str>>>,
}

#[async_trait]
impl EvaluationRunner for ScriptedEvaluator {
    async fn run_fixture(&self, _fixture: EvaluationFixture) -> CoreResult<EvaluationReport> {
        self.log.lock().expect("log lock").push("eval");
        self.reports
            .lock()
            .expect("reports lock")
            .pop_front()
            .expect("scripted evaluation report")
    }
}

#[derive(Debug)]
struct RecordingExecutor {
    outcome: Mutex<Option<CoreResult<ConsolidationMutationOutcome>>>,
    log: Arc<Mutex<Vec<&'static str>>>,
}

#[async_trait]
impl ConsolidationMutationExecutor for RecordingExecutor {
    async fn execute(
        &self,
        _request: &ConsolidationRequest,
        planned_tasks: &[ConsolidationTaskKind],
        _started_at: Timestamp,
    ) -> CoreResult<ConsolidationMutationOutcome> {
        assert!(planned_tasks.contains(&ConsolidationTaskKind::FactExtraction));
        assert!(planned_tasks.contains(&ConsolidationTaskKind::GraphEvolution));
        self.log.lock().expect("log lock").push("execute");
        self.outcome
            .lock()
            .expect("outcome lock")
            .take()
            .expect("scripted executor outcome")
    }
}

#[derive(Debug)]
struct DenyingGate {
    log: Arc<Mutex<Vec<&'static str>>>,
}

#[async_trait]
impl ConsolidationApplyGate for DenyingGate {
    async fn authorize(
        &self,
        _request: &ConsolidationRequest,
        plan: &ConsolidationPlan,
    ) -> CoreResult<()> {
        assert!(plan.operations.iter().any(|operation| operation.mutates));
        self.log.lock().expect("log lock").push("gate");
        Err(CoreError::PolicyDenied {
            reason: "consolidation apply denied by fixture".to_owned(),
        })
    }
}

#[derive(Debug)]
struct RecordingGate {
    log: Arc<Mutex<Vec<&'static str>>>,
}

#[async_trait]
impl ConsolidationApplyGate for RecordingGate {
    async fn authorize(
        &self,
        _request: &ConsolidationRequest,
        plan: &ConsolidationPlan,
    ) -> CoreResult<()> {
        assert_eq!(plan.operations.len(), 8);
        assert!(
            plan.operations
                .iter()
                .filter(|operation| operation.mutates)
                .all(|operation| operation.requires_policy)
        );
        self.log.lock().expect("log lock").push("gate");
        Ok(())
    }
}

fn gated_service(
    log: Arc<Mutex<Vec<&'static str>>>,
    reports: Vec<CoreResult<EvaluationReport>>,
    outcome: CoreResult<ConsolidationMutationOutcome>,
    gate: Arc<dyn ConsolidationApplyGate>,
) -> GatedConsolidationService {
    GatedConsolidationService::new(
        Arc::new(FixedClock(fixed_time())),
        Arc::new(FixedIds),
        Arc::new(ScriptedEvaluator {
            reports: Mutex::new(VecDeque::from(reports)),
            log: log.clone(),
        }),
        evaluation_fixture(),
        Arc::new(RecordingExecutor {
            outcome: Mutex::new(Some(outcome)),
            log,
        }),
    )
    .with_apply_gate(gate)
}

fn mutating_request() -> ConsolidationRequest {
    ConsolidationRequest {
        scope: scope("tenant-a"),
        requester: requester("agent-a"),
        since: None,
        until: None,
        strategy: Some(ConsolidationStrategy::Hybrid),
        dry_run: Some(false),
    }
}

fn mutation_outcome() -> ConsolidationMutationOutcome {
    ConsolidationMutationOutcome::new(
        vec![task_result(ConsolidationTaskKind::Compaction)],
        ConsolidationStats {
            memories_read: Some(2),
            memories_written: Some(1),
            beliefs_synthesized: Some(1),
            contradictions_detected: Some(0),
            hierarchy_nodes_created: Some(0),
            hierarchy_relations_created: Some(0),
            records_decayed: Some(0),
            records_pruned: Some(0),
            model_calls: Some(0),
        },
        Vec::<ConsolidationError>::new(),
    )
}

fn task_result(task: ConsolidationTaskKind) -> ConsolidationTaskResult {
    ConsolidationTaskResult {
        task,
        status: ConsolidationTaskStatus::Completed,
        started_at: fixed_time(),
        completed_at: Some(fixed_time()),
        items_read: Some(2),
        items_written: Some(1),
        items_updated: Some(0),
        items_skipped: Some(0),
        model_calls: Some(0),
        errors: Vec::new(),
        output_refs: Vec::new(),
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
        name: "Protected consolidation fixture".to_owned(),
        scope: scope("tenant-a"),
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

fn logged(log: &Arc<Mutex<Vec<&'static str>>>) -> Vec<&'static str> {
    log.lock().expect("log lock").clone()
}

fn fixed_time() -> Timestamp {
    DateTime::parse_from_rfc3339("2026-07-02T12:45:00Z")
        .expect("valid fixture timestamp")
        .with_timezone(&Utc)
}

fn requester(id: &str) -> Requester {
    Requester {
        actor: Actor {
            id: Id::from(id),
            kind: ActorKind::Agent,
            display_name: None,
            metadata: None,
        },
        roles: Vec::new(),
        permissions: Vec::new(),
        on_behalf_of: None,
    }
}

fn scope(tenant: &str) -> Scope {
    Scope {
        tenant: tenant.to_owned(),
        subject: None,
        workspace: None,
        session: None,
        environment: None,
    }
}
