//! Shared audit helpers for in-memory consolidation tasks.
//!
//! This module owns only task-result and aggregate-counter construction shared
//! by concrete task modules. It does not inspect store state, choose planned
//! tasks, mutate records, or interpret policy; those responsibilities stay in
//! the executor and task-specific modules.

use engram_domain::*;

/// Builds a zero-effect task result for a planned task this adapter skips.
///
/// Unsupported tasks are reported explicitly so callers can distinguish a
/// conservative in-memory adapter from a task that silently failed to run.
pub(super) fn skipped_task(
    task: ConsolidationTaskKind,
    timestamp: Timestamp,
) -> ConsolidationTaskResult {
    ConsolidationTaskResult {
        task,
        status: ConsolidationTaskStatus::Skipped,
        started_at: timestamp,
        completed_at: Some(timestamp),
        items_read: Some(0),
        items_written: Some(0),
        items_updated: Some(0),
        items_skipped: Some(0),
        model_calls: Some(0),
        errors: Vec::new(),
        output_refs: Vec::new(),
    }
}

/// Creates aggregate consolidation counters initialized to zero.
///
/// Returning explicit zeroes keeps mutating runs auditable: omitted counters are
/// reserved for unavailable measurements, not for successful no-op work.
pub(super) fn empty_stats() -> ConsolidationStats {
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

/// Adds a task-local count into an optional aggregate counter.
///
/// The aggregate starts at zero today, but this helper keeps future multi-task
/// runs from overwriting counts when more than one supported task executes.
pub(super) fn add_counter(counter: &mut Option<u64>, amount: u64) {
    *counter = Some(counter.unwrap_or(0) + amount);
}

/// Creates an evidence reference pointing at a memory record.
///
/// Consolidation task results use these references to name affected records
/// without embedding adapter-specific storage locations in the audit payload.
pub(super) fn memory_ref(id: MemoryId) -> EvidenceRef {
    EvidenceRef {
        target_type: EvidenceTargetType::Memory,
        target_id: Some(id.to_string()),
        uri: None,
        quote: None,
        location: None,
    }
}
