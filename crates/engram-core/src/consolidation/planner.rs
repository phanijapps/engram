//! Dry-run consolidation task planning.
//!
//! This module maps a validated consolidation request to deterministic task
//! reports. It owns strategy-to-trigger and strategy-to-task selection only; it
//! does not read repositories, call models, schedule work, or decide future
//! mutation policy.

use engram_domain::{
    ConsolidationError, ConsolidationRequest, ConsolidationStats, ConsolidationStrategy,
    ConsolidationTaskKind, ConsolidationTaskResult, ConsolidationTaskStatus, ConsolidationTrigger,
    Timestamp,
};

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
    task_kinds(request)
        .into_iter()
        .map(|task| completed_dry_run_task(task, started_at))
        .collect()
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

fn task_kinds(request: &ConsolidationRequest) -> Vec<ConsolidationTaskKind> {
    match request.strategy {
        Some(ConsolidationStrategy::TimeWindow) => {
            vec![
                ConsolidationTaskKind::SemanticDriftDetection,
                ConsolidationTaskKind::Decay,
            ]
        }
        Some(ConsolidationStrategy::EventCount) => {
            vec![
                ConsolidationTaskKind::Compaction,
                ConsolidationTaskKind::MemorySynthesis,
            ]
        }
        Some(ConsolidationStrategy::RetrievalFailure) => {
            vec![
                ConsolidationTaskKind::Evaluation,
                ConsolidationTaskKind::HierarchyBuild,
            ]
        }
        Some(ConsolidationStrategy::Hybrid) => {
            vec![
                ConsolidationTaskKind::Compaction,
                ConsolidationTaskKind::BeliefSynthesis,
                ConsolidationTaskKind::HierarchyBuild,
                ConsolidationTaskKind::Evaluation,
            ]
        }
        Some(ConsolidationStrategy::Manual) | None => vec![ConsolidationTaskKind::Evaluation],
    }
}

fn completed_dry_run_task(
    task: ConsolidationTaskKind,
    timestamp: Timestamp,
) -> ConsolidationTaskResult {
    ConsolidationTaskResult {
        task,
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
