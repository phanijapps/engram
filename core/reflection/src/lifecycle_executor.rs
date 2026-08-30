//! Memory lifecycle executor — the auto-archive arm of consolidation.
//!
//! Phase 3.2: memories that haven't been recalled in `stale_days` are
//! archived (status → Archived). This runs inside the consolidation
//! composite alongside reflection + decay. Archived memories are not
//! returned by default recall (they're visible with `include_archived: true`).
//!
//! The executor is honest: it only archives memories whose last retrieval
//! (or creation, if never retrieved) is older than the threshold. It
//! never touches Redacted/Forgotten records.

use std::sync::Arc;

use async_trait::async_trait;
use engram_consolidation::{ConsolidationMutationExecutor, ConsolidationMutationOutcome};
use engram_domain::{
    ConsolidationError, ConsolidationRequest, ConsolidationStats, ConsolidationTaskKind,
    ConsolidationTaskResult, ConsolidationTaskStatus, DeleteMode, ForgetRequest, ForgetTargetType,
    Timestamp,
};
use engram_memory::MemoryService;
use engram_runtime::CoreResult;

/// Days without retrieval before auto-archive. Configurable per deployment.
pub const DEFAULT_STALE_DAYS: i64 = 30;

/// Memory lifecycle executor: auto-archives memories not recalled in N days.
pub struct MemoryLifecycleExecutor {
    memory: Arc<dyn MemoryService>,
    stale_days: i64,
}

impl MemoryLifecycleExecutor {
    pub fn new(memory: Arc<dyn MemoryService>, stale_days: i64) -> Self {
        Self { memory, stale_days }
    }

    /// Default 30-day threshold.
    pub fn with_default_threshold(memory: Arc<dyn MemoryService>) -> Self {
        Self::new(memory, DEFAULT_STALE_DAYS)
    }
}

#[async_trait]
impl ConsolidationMutationExecutor for MemoryLifecycleExecutor {
    async fn execute(
        &self,
        request: &ConsolidationRequest,
        planned_tasks: &[ConsolidationTaskKind],
        started_at: Timestamp,
    ) -> CoreResult<ConsolidationMutationOutcome> {
        let mut task_results = Vec::new();
        let mut archived_count = 0u64;
        let mut errors = Vec::new();

        for kind in planned_tasks {
            if kind == &ConsolidationTaskKind::Decay {
                // List active memories for the scope
                let page = self
                    .memory
                    .list_memories_paged(&request.scope, None, 1000)
                    .await?;
                let cutoff = started_at - chrono::Duration::days(self.stale_days);
                let cutoff_ts = cutoff;

                let errors_before = errors.len();
                for record in &page.items {
                    // Only archive Active records past the cutoff
                    if record.status != engram_domain::MemoryStatus::Active {
                        continue;
                    }
                    // Check last retrieval (or creation if never retrieved)
                    let last_seen = record.updated_at.unwrap_or(record.created_at);
                    let last_seen_ts = last_seen;
                    if last_seen_ts > cutoff_ts {
                        continue; // recently used — keep
                    }

                    // Archive it
                    let forget = ForgetRequest {
                        target_type: ForgetTargetType::Memory,
                        target_id: record.id.to_string(),
                        scope: request.scope.clone(),
                        requester: request.requester.clone(),
                        mode: DeleteMode::Archive,
                        reason: Some(format!(
                            "auto-archive: no retrieval in {} days",
                            self.stale_days
                        )),
                    };
                    if let Err(e) = self.memory.forget(forget).await {
                        errors.push(ConsolidationError {
                            task: Some(ConsolidationTaskKind::Decay),
                            code: "auto_archive_failed".to_owned(),
                            message: e.to_string(),
                            target_type: None,
                            target_id: Some(record.id.to_string()),
                            recoverable: true,
                        });
                    } else {
                        archived_count += 1;
                    }
                }

                let task_errors = errors.len() - errors_before;
                let items_read = page.items.len() as u64;

                task_results.push(ConsolidationTaskResult {
                    task: ConsolidationTaskKind::Decay,
                    status: if task_errors == 0 {
                        ConsolidationTaskStatus::Completed
                    } else {
                        ConsolidationTaskStatus::CompletedWithErrors
                    },
                    started_at,
                    completed_at: Some(started_at),
                    items_read: Some(items_read),
                    items_written: Some(archived_count),
                    items_updated: None,
                    items_skipped: None,
                    model_calls: None,
                    errors: Vec::new(),
                    output_refs: Vec::new(),
                });
            } else {
                task_results.push(ConsolidationTaskResult {
                    task: kind.clone(),
                    status: ConsolidationTaskStatus::Skipped,
                    started_at,
                    completed_at: Some(started_at),
                    items_read: None,
                    items_written: None,
                    items_updated: None,
                    items_skipped: None,
                    model_calls: None,
                    errors: Vec::new(),
                    output_refs: Vec::new(),
                });
            }
        }

        let stats = ConsolidationStats {
            records_decayed: if archived_count > 0 {
                Some(archived_count)
            } else {
                None
            },
            memories_read: None,
            memories_written: None,
            beliefs_synthesized: None,
            contradictions_detected: None,
            hierarchy_nodes_created: None,
            hierarchy_relations_created: None,
            records_pruned: None,
            model_calls: None,
        };

        Ok(ConsolidationMutationOutcome {
            tasks: task_results,
            stats,
            errors,
        })
    }
}
