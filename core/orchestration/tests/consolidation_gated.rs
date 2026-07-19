use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use engram_core::{
    Clock, ConsolidationMutationExecutor, ConsolidationMutationOutcome, ConsolidationService,
    CoreError, CoreResult, EvaluationCaseReport, EvaluationReport, EvaluationRunner,
    GatedConsolidationService, IdGenerator,
};
use engram_domain::{
    Actor, ActorKind, ConsolidationRequest, ConsolidationRunStatus, ConsolidationStats,
    ConsolidationStrategy, ConsolidationTaskKind, ConsolidationTaskResult, ConsolidationTaskStatus,
    EvaluationFixture, EvaluationSetup, Id, Requester, Scope, Timestamp,
};
use futures::executor::block_on;

#[test]
fn gated_consolidation_runs_pre_and_post_evaluation_around_executor() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let service = gated_service(
        log.clone(),
        vec![Ok(passing_report()), Ok(passing_report())],
        Ok(mutation_outcome()),
    );

    let run = block_on(service.consolidate(mutating_request()))
        .expect("gated consolidation should return run");

    assert_eq!(run.status, ConsolidationRunStatus::Completed);
    assert_eq!(logged(&log), vec!["eval", "execute", "eval"]);
    assert_eq!(run.tasks.len(), 3);
    assert_eq!(run.tasks[0].task, ConsolidationTaskKind::Evaluation);
    assert_eq!(run.tasks[1].task, ConsolidationTaskKind::Compaction);
    assert_eq!(run.tasks[2].task, ConsolidationTaskKind::Evaluation);
    assert!(run.errors.is_empty());
    assert_eq!(run.stats.and_then(|stats| stats.memories_written), Some(1));
}

#[test]
fn gated_consolidation_pre_evaluation_failure_prevents_executor() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let service = gated_service(
        log.clone(),
        vec![Ok(failing_report())],
        Ok(mutation_outcome()),
    );

    let run = block_on(service.consolidate(mutating_request()))
        .expect("pre-evaluation failure should return auditable run");

    assert_eq!(run.status, ConsolidationRunStatus::Failed);
    assert_eq!(logged(&log), vec!["eval"]);
    assert_eq!(run.tasks.len(), 1);
    assert_eq!(run.tasks[0].status, ConsolidationTaskStatus::Failed);
    assert!(
        run.errors
            .iter()
            .any(|error| error.code == "evaluation_failed")
    );
}

#[test]
fn gated_consolidation_post_evaluation_failure_reports_regression() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let service = gated_service(
        log.clone(),
        vec![Ok(passing_report()), Ok(failing_report())],
        Ok(mutation_outcome()),
    );

    let run = block_on(service.consolidate(mutating_request()))
        .expect("post-evaluation failure should return auditable run");

    assert_eq!(run.status, ConsolidationRunStatus::CompletedWithErrors);
    assert_eq!(logged(&log), vec!["eval", "execute", "eval"]);
    assert_eq!(
        run.tasks.last().expect("post task").status,
        ConsolidationTaskStatus::Failed
    );
    assert!(
        run.errors
            .iter()
            .any(|error| error.code == "evaluation_failed")
    );
}

#[test]
fn gated_consolidation_requires_explicit_mutating_mode() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let service = gated_service(log.clone(), Vec::new(), Ok(mutation_outcome()));

    let missing_flag = block_on(service.consolidate(ConsolidationRequest {
        dry_run: None,
        ..mutating_request()
    }));
    assert!(matches!(
        missing_flag,
        Err(CoreError::InvalidRequest { reason }) if reason.contains("dryRun=false")
    ));

    let dry_run_flag = block_on(service.consolidate(ConsolidationRequest {
        dry_run: Some(true),
        ..mutating_request()
    }));
    assert!(matches!(
        dry_run_flag,
        Err(CoreError::InvalidRequest { reason }) if reason.contains("dryRun=false")
    ));
    assert!(logged(&log).is_empty());
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
        assert!(!planned_tasks.is_empty());
        self.log.lock().expect("log lock").push("execute");
        self.outcome
            .lock()
            .expect("outcome lock")
            .take()
            .expect("scripted executor outcome")
    }
}

fn gated_service(
    log: Arc<Mutex<Vec<&'static str>>>,
    reports: Vec<CoreResult<EvaluationReport>>,
    outcome: CoreResult<ConsolidationMutationOutcome>,
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
}

fn mutating_request() -> ConsolidationRequest {
    ConsolidationRequest {
        scope: scope("tenant-a"),
        requester: requester("agent-a"),
        since: None,
        until: None,
        strategy: Some(ConsolidationStrategy::EventCount),
        dry_run: Some(false),
    }
}

fn mutation_outcome() -> ConsolidationMutationOutcome {
    ConsolidationMutationOutcome::new(
        vec![ConsolidationTaskResult {
            task: ConsolidationTaskKind::Compaction,
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
        }],
        ConsolidationStats {
            memories_read: Some(2),
            memories_written: Some(1),
            beliefs_synthesized: Some(0),
            contradictions_detected: Some(0),
            hierarchy_nodes_created: Some(0),
            hierarchy_relations_created: Some(0),
            records_decayed: Some(0),
            records_pruned: Some(0),
            model_calls: Some(0),
        },
        Vec::new(),
    )
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

fn failing_report() -> EvaluationReport {
    EvaluationReport {
        fixture_id: Id::from("eval-fixture"),
        cases: vec![EvaluationCaseReport {
            case_id: "protected-recall".to_owned(),
            passed: false,
            failures: vec!["missing required target memory:memory-1".to_owned()],
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
    DateTime::parse_from_rfc3339("2026-06-30T12:00:00Z")
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
