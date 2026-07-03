//! Dry-run consolidation task planning.
//!
//! This module maps a validated consolidation request to deterministic task
//! reports. It owns strategy-to-trigger and strategy-to-task selection only; it
//! does not read repositories, call models, schedule work, or decide future
//! mutation policy.

use engram_domain::{
    ConsolidationError, ConsolidationOperationKind, ConsolidationPlan,
    ConsolidationPlannedOperation, ConsolidationRequest, ConsolidationStats, ConsolidationStrategy,
    ConsolidationTaskKind, ConsolidationTaskResult, ConsolidationTaskStatus, ConsolidationTrigger,
    Timestamp,
};

use crate::{CoreResult, consolidation::validation::validate_planning_request};

/// Chooses the audit trigger that best explains why the dry-run was requested.
///
/// The trigger is derived from request strategy rather than scheduler state so
/// dry-run reports remain deterministic. Future runtime schedulers should pass
/// explicit strategy information instead of making this function inspect clocks
/// or repositories.
pub(crate) fn trigger_for(request: &ConsolidationRequest) -> ConsolidationTrigger {
    match request.strategy {
        Some(ConsolidationStrategy::TimeWindow) => ConsolidationTrigger::Scheduled,
        Some(ConsolidationStrategy::EventCount) => ConsolidationTrigger::EventThreshold,
        Some(ConsolidationStrategy::RetrievalFailure) => ConsolidationTrigger::RetrievalFailure,
        Some(ConsolidationStrategy::Hybrid) => ConsolidationTrigger::ManualReview,
        Some(ConsolidationStrategy::Manual) | None => ConsolidationTrigger::OnDemand,
    }
}

/// Builds completed zero-mutation task results for a dry-run request.
///
/// The returned tasks are reports, not commands. They communicate which
/// consolidation areas would be considered by a strategy while guaranteeing no
/// model calls, repository reads, or writes happen in this planner.
pub(crate) fn plan_tasks(
    request: &ConsolidationRequest,
    started_at: Timestamp,
) -> Vec<ConsolidationTaskResult> {
    build_plan(request, started_at)
        .operations
        .into_iter()
        .map(|operation| completed_dry_run_task(operation, started_at))
        .collect()
}

/// Builds a deterministic operation plan for callers that need to inspect a
/// consolidation cycle before choosing dry-run rendering or mutating apply.
pub fn plan_consolidation_operations(
    request: &ConsolidationRequest,
    planned_at: Timestamp,
) -> CoreResult<ConsolidationPlan> {
    validate_planning_request(request)?;
    Ok(build_plan(request, planned_at))
}

/// Returns aggregate counters that prove the dry-run performed no mutations.
///
/// Counters are explicitly set to zero rather than omitted so callers and tests
/// can distinguish a successful no-op dry run from a service that failed to
/// collect stats.
pub(crate) fn empty_stats() -> ConsolidationStats {
    ConsolidationStats {
        memories_read: Some(0),
        memories_written: Some(0),
        beliefs_synthesized: Some(0),
        contradictions_detected: Some(0),
        hierarchy_nodes_created: Some(0),
        hierarchy_relations_created: Some(0),
        records_decayed: Some(0),
        records_pruned: Some(0),
        model_calls: Some(0),
    }
}

pub(crate) fn build_plan(
    request: &ConsolidationRequest,
    planned_at: Timestamp,
) -> ConsolidationPlan {
    ConsolidationPlan {
        scope: request.scope.clone(),
        requester: request.requester.clone(),
        strategy: request.strategy.clone(),
        dry_run: !matches!(request.dry_run, Some(false)),
        planned_at,
        operations: operation_specs(request)
            .into_iter()
            .enumerate()
            .map(|(index, spec)| planned_operation(index, spec))
            .collect(),
    }
}

fn operation_specs(request: &ConsolidationRequest) -> Vec<OperationSpec> {
    match request.strategy {
        Some(ConsolidationStrategy::TimeWindow) => {
            vec![
                OperationSpec::new(
                    ConsolidationOperationKind::SemanticDriftReview,
                    ConsolidationTaskKind::SemanticDriftDetection,
                    "review taxonomy and graph candidates for semantic drift",
                    true,
                ),
                OperationSpec::new(
                    ConsolidationOperationKind::DecayReview,
                    ConsolidationTaskKind::Decay,
                    "review expired or stale records for confidence decay",
                    true,
                ),
            ]
        }
        Some(ConsolidationStrategy::EventCount) => {
            vec![
                OperationSpec::new(
                    ConsolidationOperationKind::Compaction,
                    ConsolidationTaskKind::Compaction,
                    "compact high-volume episodic records into bounded summaries",
                    true,
                ),
                OperationSpec::new(
                    ConsolidationOperationKind::MemoryToFact,
                    ConsolidationTaskKind::FactExtraction,
                    "extract candidate semantic facts from accumulated episodic memory",
                    true,
                ),
            ]
        }
        Some(ConsolidationStrategy::RetrievalFailure) => {
            vec![
                OperationSpec::new(
                    ConsolidationOperationKind::EvaluationGate,
                    ConsolidationTaskKind::Evaluation,
                    "evaluate missed retrieval targets before proposing repair work",
                    false,
                ),
                OperationSpec::new(
                    ConsolidationOperationKind::HierarchyCandidate,
                    ConsolidationTaskKind::HierarchyBuild,
                    "propose hierarchy candidates that improve failed retrieval paths",
                    true,
                ),
                OperationSpec::new(
                    ConsolidationOperationKind::GraphCandidate,
                    ConsolidationTaskKind::GraphEvolution,
                    "propose graph edges that explain failed retrieval neighborhoods",
                    true,
                ),
            ]
        }
        Some(ConsolidationStrategy::Hybrid) => {
            vec![
                OperationSpec::new(
                    ConsolidationOperationKind::Compaction,
                    ConsolidationTaskKind::Compaction,
                    "compact episodic records before durable synthesis",
                    true,
                ),
                OperationSpec::new(
                    ConsolidationOperationKind::MemoryToFact,
                    ConsolidationTaskKind::FactExtraction,
                    "extract candidate semantic facts from episodic memory",
                    true,
                ),
                OperationSpec::new(
                    ConsolidationOperationKind::MemoryToBelief,
                    ConsolidationTaskKind::BeliefSynthesis,
                    "synthesize belief candidates from supported semantic facts",
                    true,
                ),
                OperationSpec::new(
                    ConsolidationOperationKind::ContradictionReview,
                    ConsolidationTaskKind::BeliefContradictionDetection,
                    "detect contradictions among new and existing belief candidates",
                    true,
                ),
                OperationSpec::new(
                    ConsolidationOperationKind::HierarchyCandidate,
                    ConsolidationTaskKind::HierarchyBuild,
                    "propose hierarchy build candidates from consolidated records",
                    true,
                ),
                OperationSpec::new(
                    ConsolidationOperationKind::TaxonomyCandidate,
                    ConsolidationTaskKind::TaxonomyEvolution,
                    "propose governed taxonomy changes with validation before merge",
                    true,
                ),
                OperationSpec::new(
                    ConsolidationOperationKind::GraphCandidate,
                    ConsolidationTaskKind::GraphEvolution,
                    "propose knowledge graph relationships from consolidated facts",
                    true,
                ),
                OperationSpec::new(
                    ConsolidationOperationKind::EvaluationGate,
                    ConsolidationTaskKind::Evaluation,
                    "evaluate recall, leakage, ranking, and policy before apply",
                    false,
                ),
            ]
        }
        Some(ConsolidationStrategy::Manual) | None => vec![OperationSpec::new(
            ConsolidationOperationKind::EvaluationGate,
            ConsolidationTaskKind::Evaluation,
            "evaluate the requested consolidation scope without scheduling mutations",
            false,
        )],
    }
}

fn planned_operation(index: usize, spec: OperationSpec) -> ConsolidationPlannedOperation {
    ConsolidationPlannedOperation {
        id: format!("operation-{:02}", index + 1),
        kind: spec.kind,
        task: spec.task,
        description: spec.description.to_owned(),
        mutates: spec.mutates,
        requires_policy: spec.mutates,
        requires_evaluation: true,
        input_refs: Vec::new(),
        output_refs: Vec::new(),
    }
}

fn completed_dry_run_task(
    operation: ConsolidationPlannedOperation,
    timestamp: Timestamp,
) -> ConsolidationTaskResult {
    ConsolidationTaskResult {
        task: operation.task,
        status: ConsolidationTaskStatus::Completed,
        started_at: timestamp,
        completed_at: Some(timestamp),
        items_read: Some(0),
        items_written: Some(0),
        items_updated: Some(0),
        items_skipped: Some(0),
        model_calls: Some(0),
        errors: Vec::<ConsolidationError>::new(),
        output_refs: Vec::new(),
    }
}

struct OperationSpec {
    kind: ConsolidationOperationKind,
    task: ConsolidationTaskKind,
    description: &'static str,
    mutates: bool,
}

impl OperationSpec {
    fn new(
        kind: ConsolidationOperationKind,
        task: ConsolidationTaskKind,
        description: &'static str,
        mutates: bool,
    ) -> Self {
        Self {
            kind,
            task,
            description,
            mutates,
        }
    }
}
