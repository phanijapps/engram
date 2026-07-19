//! Composite consolidation executor — dispatches each planned task to the first
//! registered executor that handles it (doesn't Skip). Enables multiple
//! single-purpose executors (e.g. ReflectionExecutor for BeliefSynthesis,
//! DecayExecutor for Decay) to coexist in a single Hybrid consolidation run.
//!
//! Also provides the Ebbinghaus forgetting-curve helper (`R = e^(-t/S)`).

use std::sync::Arc;

use crate::{ConsolidationMutationExecutor, ConsolidationMutationOutcome};
use async_trait::async_trait;
use engram_domain::{
    ConsolidationError, ConsolidationRequest, ConsolidationStats, ConsolidationTaskKind,
    ConsolidationTaskResult, ConsolidationTaskStatus, Timestamp,
};
use engram_runtime::CoreResult;

/// Ebbinghaus forgetting-curve retention: `R = e^(-t/S)`.
///
/// `t` = elapsed time since creation (seconds); `S` = stability constant
/// (seconds). At `t = S`, retention is `1/e ≈ 0.368`. Returns 1.0 when `S <= 0`.
pub fn ebbinghaus_retention(elapsed_seconds: f64, stability_seconds: f64) -> f64 {
    if stability_seconds <= 0.0 {
        return 1.0;
    }
    (-elapsed_seconds / stability_seconds).exp()
}

/// A composite executor that delegates each planned task to the first registered
/// executor that produces a non-`Skipped` result for it. Each child executor
/// receives the full `planned_tasks` list; the composite merges outcomes by
/// picking the first non-`Skipped` result per task kind.
pub struct CompositeConsolidationExecutor {
    executors: Vec<Arc<dyn ConsolidationMutationExecutor>>,
}

impl CompositeConsolidationExecutor {
    /// Creates a composite from an ordered list of child executors.
    pub fn new(executors: Vec<Arc<dyn ConsolidationMutationExecutor>>) -> Self {
        Self { executors }
    }
}

#[async_trait]
impl ConsolidationMutationExecutor for CompositeConsolidationExecutor {
    async fn execute(
        &self,
        request: &ConsolidationRequest,
        planned_tasks: &[ConsolidationTaskKind],
        started_at: Timestamp,
    ) -> CoreResult<ConsolidationMutationOutcome> {
        let mut best: Vec<(ConsolidationTaskKind, ConsolidationTaskResult)> = Vec::new();
        let mut merged_stats = ConsolidationStats {
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
        let mut merged_errors: Vec<ConsolidationError> = Vec::new();

        for executor in &self.executors {
            let outcome = executor.execute(request, planned_tasks, started_at).await?;
            for tr in outcome.tasks {
                if tr.status != ConsolidationTaskStatus::Skipped
                    && !best.iter().any(|(k, _)| k == &tr.task)
                {
                    best.push((tr.task.clone(), tr));
                }
            }
            merge_stats(&mut merged_stats, &outcome.stats);
            merged_errors.extend(outcome.errors);
        }

        // Emit Skipped for any planned task no executor handled.
        let tasks: Vec<ConsolidationTaskResult> = planned_tasks
            .iter()
            .map(|kind| {
                best.iter()
                    .find(|(k, _)| k == kind)
                    .map(|(_, r)| r.clone())
                    .unwrap_or_else(|| ConsolidationTaskResult {
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
                    })
            })
            .collect();

        Ok(ConsolidationMutationOutcome::new(
            tasks,
            merged_stats,
            merged_errors,
        ))
    }
}

/// Sums two `ConsolidationStats` field-by-field (`Some(a) + Some(b) = Some(a+b)`).
fn merge_stats(into: &mut ConsolidationStats, from: &ConsolidationStats) {
    macro_rules! m {
        ($f:ident) => {
            match (into.$f, from.$f) {
                (Some(a), Some(b)) => into.$f = Some(a + b),
                (None, Some(b)) => into.$f = Some(b),
                _ => {}
            }
        };
    }
    m!(memories_read);
    m!(memories_written);
    m!(beliefs_synthesized);
    m!(contradictions_detected);
    m!(hierarchy_nodes_created);
    m!(hierarchy_relations_created);
    m!(records_decayed);
    m!(records_pruned);
    m!(model_calls);
}

#[cfg(test)]
mod tests {
    use super::*;
    use engram_domain::{Actor, ActorKind, Id, Requester, Scope};

    fn scope() -> Scope {
        Scope {
            tenant: "t".to_owned(),
            subject: None,
            workspace: None,
            session: None,
            environment: None,
        }
    }

    fn request() -> ConsolidationRequest {
        ConsolidationRequest {
            scope: scope(),
            requester: Requester {
                actor: Actor {
                    id: Id::from("test"),
                    kind: ActorKind::Agent,
                    display_name: None,
                    metadata: None,
                },
                roles: Vec::new(),
                permissions: Vec::new(),
                on_behalf_of: None,
            },
            since: None,
            until: None,
            strategy: None,
            dry_run: None,
        }
    }

    fn now() -> Timestamp {
        chrono::Utc::now()
    }

    /// A stub executor that handles exactly one task kind (Completed) + Skips others.
    struct SingleKindExecutor {
        kind: ConsolidationTaskKind,
    }

    #[async_trait]
    impl ConsolidationMutationExecutor for SingleKindExecutor {
        async fn execute(
            &self,
            _request: &ConsolidationRequest,
            planned_tasks: &[ConsolidationTaskKind],
            started_at: Timestamp,
        ) -> CoreResult<ConsolidationMutationOutcome> {
            let tasks: Vec<ConsolidationTaskResult> = planned_tasks
                .iter()
                .map(|k| {
                    if k == &self.kind {
                        ConsolidationTaskResult {
                            task: k.clone(),
                            status: ConsolidationTaskStatus::Completed,
                            started_at,
                            completed_at: Some(started_at),
                            items_read: Some(1),
                            items_written: Some(1),
                            items_updated: None,
                            items_skipped: None,
                            model_calls: None,
                            errors: Vec::new(),
                            output_refs: Vec::new(),
                        }
                    } else {
                        ConsolidationTaskResult {
                            task: k.clone(),
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
                        }
                    }
                })
                .collect();
            let stats = ConsolidationStats {
                memories_read: None,
                memories_written: None,
                beliefs_synthesized: if self.kind == ConsolidationTaskKind::BeliefSynthesis {
                    Some(1)
                } else {
                    None
                },
                contradictions_detected: None,
                hierarchy_nodes_created: None,
                hierarchy_relations_created: None,
                records_decayed: None,
                records_pruned: None,
                model_calls: None,
            };
            Ok(ConsolidationMutationOutcome::new(tasks, stats, Vec::new()))
        }
    }

    #[test]
    fn composite_dispatches_to_correct_executor() {
        let composite = CompositeConsolidationExecutor::new(vec![
            Arc::new(SingleKindExecutor {
                kind: ConsolidationTaskKind::BeliefSynthesis,
            }),
            Arc::new(SingleKindExecutor {
                kind: ConsolidationTaskKind::Decay,
            }),
        ]);
        let outcome = futures::executor::block_on(composite.execute(
            &request(),
            &[
                ConsolidationTaskKind::BeliefSynthesis,
                ConsolidationTaskKind::Decay,
            ],
            now(),
        ))
        .unwrap();

        assert_eq!(outcome.tasks.len(), 2);
        assert_eq!(outcome.tasks[0].status, ConsolidationTaskStatus::Completed);
        assert_eq!(
            outcome.tasks[0].task,
            ConsolidationTaskKind::BeliefSynthesis
        );
        assert_eq!(outcome.tasks[1].status, ConsolidationTaskStatus::Completed);
        assert_eq!(outcome.tasks[1].task, ConsolidationTaskKind::Decay);
        assert_eq!(outcome.stats.beliefs_synthesized, Some(1));
    }

    #[test]
    fn composite_emits_skipped_for_unhandled_tasks() {
        let composite = CompositeConsolidationExecutor::new(vec![Arc::new(SingleKindExecutor {
            kind: ConsolidationTaskKind::BeliefSynthesis,
        })]);
        let outcome = futures::executor::block_on(composite.execute(
            &request(),
            &[
                ConsolidationTaskKind::BeliefSynthesis,
                ConsolidationTaskKind::Compaction,
            ],
            now(),
        ))
        .unwrap();

        assert_eq!(outcome.tasks.len(), 2);
        assert_eq!(outcome.tasks[0].status, ConsolidationTaskStatus::Completed);
        assert_eq!(outcome.tasks[1].status, ConsolidationTaskStatus::Skipped);
    }

    #[test]
    fn ebbinghaus_curve_at_known_points() {
        let s = 3600.0; // 1 hour stability
        assert!((ebbinghaus_retention(0.0, s) - 1.0).abs() < 1e-9);
        assert!((ebbinghaus_retention(s, s) - std::f64::consts::E.recip()).abs() < 1e-3);
        assert!((ebbinghaus_retention(2.0 * s, s) - (-2.0f64).exp()).abs() < 1e-3);
        assert_eq!(ebbinghaus_retention(100.0, 0.0), 1.0); // S <= 0 → R = 1
    }
}
