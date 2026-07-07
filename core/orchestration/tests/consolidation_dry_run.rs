use std::sync::Arc;

use chrono::{DateTime, Utc};
use engram_core::{
    Clock, ConsolidationService, CoreError, DryRunConsolidationService, IdGenerator,
};
use engram_domain::{
    Actor, ActorKind, ConsolidationRequest, ConsolidationRunStatus, ConsolidationStrategy,
    ConsolidationTaskKind, ConsolidationTaskStatus, Id, Requester, Scope, Timestamp,
};
use futures::executor::block_on;

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

#[test]
fn dry_run_returns_completed_auditable_run() {
    let service = dry_run_service();

    let run = block_on(service.consolidate(ConsolidationRequest {
        scope: scope("tenant-a"),
        requester: requester("agent-a"),
        since: None,
        until: None,
        strategy: Some(ConsolidationStrategy::Hybrid),
        dry_run: Some(true),
    }))
    .expect("dry-run consolidation should succeed");

    assert_eq!(run.id.as_str(), "consolidation-run-fixed");
    assert_eq!(run.scope.tenant, "tenant-a");
    assert_eq!(run.requester.actor.id.as_str(), "agent-a");
    assert_eq!(run.status, ConsolidationRunStatus::Completed);
    assert_eq!(run.started_at, fixed_time());
    assert_eq!(run.completed_at, Some(fixed_time()));
    assert_eq!(run.tasks.len(), 8);
    assert!(
        run.tasks
            .iter()
            .any(|task| task.task == ConsolidationTaskKind::FactExtraction)
    );
    assert!(
        run.tasks
            .iter()
            .any(|task| task.task == ConsolidationTaskKind::BeliefContradictionDetection)
    );
    assert!(
        run.tasks
            .iter()
            .any(|task| task.task == ConsolidationTaskKind::TaxonomyEvolution)
    );
    assert!(
        run.tasks
            .iter()
            .any(|task| task.task == ConsolidationTaskKind::GraphEvolution)
    );
    assert!(
        run.tasks
            .iter()
            .all(|task| task.status == ConsolidationTaskStatus::Completed)
    );
    assert!(run.tasks.iter().all(|task| task.items_written == Some(0)));
    assert!(run.tasks.iter().all(|task| task.model_calls == Some(0)));
    assert!(run.errors.is_empty());

    let stats = run.stats.expect("dry-run should include zero stats");
    assert_eq!(stats.memories_written, Some(0));
    assert_eq!(stats.beliefs_synthesized, Some(0));
    assert_eq!(stats.hierarchy_nodes_created, Some(0));
    assert_eq!(stats.records_pruned, Some(0));
    assert_eq!(stats.model_calls, Some(0));
}

#[test]
fn dry_run_service_rejects_mutating_requests_before_planning() {
    let service = dry_run_service();

    let error = block_on(service.consolidate(ConsolidationRequest {
        scope: scope("tenant-a"),
        requester: requester("agent-a"),
        since: None,
        until: None,
        strategy: Some(ConsolidationStrategy::Manual),
        dry_run: Some(false),
    }))
    .expect_err("mutating consolidation must be rejected");

    assert!(matches!(error, CoreError::InvalidRequest { reason } if reason.contains("dry-run")));
}

#[test]
fn dry_run_service_validates_scope_requester_and_time_window() {
    let service = dry_run_service();

    let invalid_scope = block_on(service.consolidate(ConsolidationRequest {
        scope: scope(" "),
        requester: requester("agent-a"),
        since: None,
        until: None,
        strategy: None,
        dry_run: Some(true),
    }));
    assert!(matches!(
        invalid_scope,
        Err(CoreError::InvalidRequest { reason }) if reason.contains("scope.tenant")
    ));

    let invalid_requester = block_on(service.consolidate(ConsolidationRequest {
        scope: scope("tenant-a"),
        requester: requester(" "),
        since: None,
        until: None,
        strategy: None,
        dry_run: Some(true),
    }));
    assert!(matches!(
        invalid_requester,
        Err(CoreError::InvalidRequest { reason }) if reason.contains("requester.actor.id")
    ));

    let invalid_window = block_on(
        service.consolidate(ConsolidationRequest {
            scope: scope("tenant-a"),
            requester: requester("agent-a"),
            since: Some(fixed_time()),
            until: Some(
                DateTime::parse_from_rfc3339("2026-06-28T00:00:00Z")
                    .expect("valid fixture timestamp")
                    .with_timezone(&Utc),
            ),
            strategy: None,
            dry_run: Some(true),
        }),
    );
    assert!(matches!(
        invalid_window,
        Err(CoreError::InvalidRequest { reason }) if reason.contains("since")
    ));
}

fn dry_run_service() -> DryRunConsolidationService {
    DryRunConsolidationService::new(Arc::new(FixedClock(fixed_time())), Arc::new(FixedIds))
}

fn fixed_time() -> Timestamp {
    DateTime::parse_from_rfc3339("2026-06-29T12:00:00Z")
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
