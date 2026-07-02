use chrono::{DateTime, Utc};
use engram_consolidation::plan_consolidation_operations;
use engram_domain::{
    Actor, ActorKind, ConsolidationOperationKind, ConsolidationRequest, ConsolidationStrategy,
    ConsolidationTaskKind, Id, Requester, Scope, Timestamp,
};

#[test]
fn hybrid_plan_covers_architecture_candidate_operations() {
    let plan = plan_consolidation_operations(
        &request(Some(ConsolidationStrategy::Hybrid), Some(true)),
        fixed_time(),
    )
    .expect("plan consolidation");

    let kinds = plan
        .operations
        .iter()
        .map(|operation| operation.kind.clone())
        .collect::<Vec<_>>();
    assert_eq!(
        kinds,
        vec![
            ConsolidationOperationKind::Compaction,
            ConsolidationOperationKind::MemoryToFact,
            ConsolidationOperationKind::MemoryToBelief,
            ConsolidationOperationKind::ContradictionReview,
            ConsolidationOperationKind::HierarchyCandidate,
            ConsolidationOperationKind::TaxonomyCandidate,
            ConsolidationOperationKind::GraphCandidate,
            ConsolidationOperationKind::EvaluationGate,
        ]
    );
    assert!(
        plan.operations
            .iter()
            .filter(|operation| operation.mutates)
            .all(|operation| operation.requires_policy && operation.requires_evaluation)
    );
    assert_eq!(
        plan.operations.last().expect("evaluation operation").task,
        ConsolidationTaskKind::Evaluation
    );
}

#[test]
fn retrieval_failure_plan_targets_repair_candidates() {
    let plan = plan_consolidation_operations(
        &request(Some(ConsolidationStrategy::RetrievalFailure), Some(true)),
        fixed_time(),
    )
    .expect("plan consolidation");

    let tasks = plan
        .operations
        .iter()
        .map(|operation| operation.task.clone())
        .collect::<Vec<_>>();
    assert_eq!(
        tasks,
        vec![
            ConsolidationTaskKind::Evaluation,
            ConsolidationTaskKind::HierarchyBuild,
            ConsolidationTaskKind::GraphEvolution,
        ]
    );
}

#[test]
fn plan_validation_rejects_invalid_scope() {
    let error = plan_consolidation_operations(
        &request(Some(ConsolidationStrategy::Hybrid), Some(true)),
        fixed_time(),
    )
    .expect("valid request");
    assert_eq!(error.scope.tenant, "tenant-a");

    let invalid = plan_consolidation_operations(
        &ConsolidationRequest {
            scope: scope(" "),
            requester: requester("agent-a"),
            since: None,
            until: None,
            strategy: Some(ConsolidationStrategy::Hybrid),
            dry_run: Some(true),
        },
        fixed_time(),
    );
    assert!(invalid.is_err());
}

fn request(strategy: Option<ConsolidationStrategy>, dry_run: Option<bool>) -> ConsolidationRequest {
    ConsolidationRequest {
        scope: scope("tenant-a"),
        requester: requester("agent-a"),
        since: None,
        until: None,
        strategy,
        dry_run,
    }
}

fn fixed_time() -> Timestamp {
    DateTime::parse_from_rfc3339("2026-07-02T12:30:00Z")
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
