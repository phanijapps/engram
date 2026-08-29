//! Contradiction detection executor — the `BeliefContradictionDetection`
//! arm of the consolidation composite. Reads active beliefs for the scope,
//! groups by subject, detects conflicting stances, and persists
//! `Contradiction` records for agent review.
//!
//! Phase 2.3 of the improvement plan: "when new observations contradict
//! stored beliefs, flag for review" — this runs inside
//! `maintain op=consolidate` and surfaces the tensions.

use std::sync::Arc;

use async_trait::async_trait;
use engram_belief::{BeliefRepository, ContradictionDetector};
use engram_consolidation::{ConsolidationMutationExecutor, ConsolidationMutationOutcome};
use engram_domain::{
    ConsolidationError, ConsolidationRequest, ConsolidationStats, ConsolidationTaskKind,
    ConsolidationTaskResult, ConsolidationTaskStatus, Timestamp,
};
use engram_runtime::CoreResult;

/// Contradiction detection executor: detects + persists contradictions
/// among active beliefs for the scope.
pub struct ContradictionExecutor {
    detector: Arc<dyn ContradictionDetector>,
    repo: Arc<dyn BeliefRepository>,
}

impl ContradictionExecutor {
    pub fn new(detector: Arc<dyn ContradictionDetector>, repo: Arc<dyn BeliefRepository>) -> Self {
        Self { detector, repo }
    }
}

#[async_trait]
impl ConsolidationMutationExecutor for ContradictionExecutor {
    async fn execute(
        &self,
        request: &ConsolidationRequest,
        planned_tasks: &[ConsolidationTaskKind],
        started_at: Timestamp,
    ) -> CoreResult<ConsolidationMutationOutcome> {
        let mut task_results = Vec::new();
        let mut total_contradictions = 0u64;
        let mut errors = Vec::new();

        for kind in planned_tasks {
            if kind == &ConsolidationTaskKind::BeliefContradictionDetection {
                // Read active beliefs for the scope.
                let beliefs = self.repo.list_beliefs(&request.scope).await?;
                let count = beliefs.len() as u64;

                // Detect contradictions among them.
                let contradictions = self.detector.detect_contradictions(&beliefs).await?;
                let detected = contradictions.len() as u64;

                // Persist each contradiction for review.
                let errors_before = errors.len();
                for contradiction in contradictions {
                    if let Err(e) = self.repo.put_contradiction(contradiction).await {
                        errors.push(ConsolidationError {
                            task: Some(ConsolidationTaskKind::BeliefContradictionDetection),
                            code: "put_contradiction_failed".to_owned(),
                            message: e.to_string(),
                            target_type: None,
                            target_id: None,
                            recoverable: true,
                        });
                    }
                }

                let task_errors = errors.len() - errors_before;
                let items_written = detected - task_errors as u64;
                total_contradictions += items_written;

                task_results.push(ConsolidationTaskResult {
                    task: ConsolidationTaskKind::BeliefContradictionDetection,
                    status: if task_errors == 0 {
                        ConsolidationTaskStatus::Completed
                    } else {
                        ConsolidationTaskStatus::CompletedWithErrors
                    },
                    started_at,
                    completed_at: Some(started_at),
                    items_read: Some(count),
                    items_written: Some(items_written),
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

        let mut stats = ConsolidationStats {
            memories_read: None,
            memories_written: None,
            beliefs_synthesized: None,
            contradictions_detected: None,
            hierarchy_nodes_created: None,
            hierarchy_relations_created: None,
            records_decayed: None,
            records_pruned: None,
            model_calls: None,
        };
        if total_contradictions > 0 {
            stats.contradictions_detected = Some(total_contradictions);
        }

        Ok(ConsolidationMutationOutcome {
            tasks: task_results,
            stats,
            errors,
        })
    }
}
