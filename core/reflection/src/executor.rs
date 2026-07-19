//! Reflection executor — the first production `ConsolidationMutationExecutor`.
//!
//! Dispatches `BeliefSynthesis` planned tasks to the [`BeliefSynthesizer`] +
//! persists each returned belief via [`BeliefSink`]. Every other task kind gets
//! one `Skipped` task result (never silently dropped). This executor is a
//! component — production wiring requires a composite-executor pattern (the
//! `Hybrid` strategy bundles 8 task kinds; a single-purpose executor alone
//! would skip 7).

use std::sync::Arc;

use async_trait::async_trait;
use engram_belief::BeliefSynthesizer;
use engram_consolidation::{ConsolidationMutationExecutor, ConsolidationMutationOutcome};
use engram_domain::{
    ConsolidationError, ConsolidationRequest, ConsolidationStats, ConsolidationTaskKind,
    ConsolidationTaskResult, ConsolidationTaskStatus, Timestamp,
};
use engram_runtime::CoreResult;

use crate::source::BeliefSink;

/// Reflection consolidation executor: runs the synthesizer + persists beliefs.
pub struct ReflectionExecutor {
    synthesizer: Arc<dyn BeliefSynthesizer>,
    sink: Arc<dyn BeliefSink>,
}

impl ReflectionExecutor {
    /// Creates a reflection executor with the given synthesizer + belief sink.
    pub fn new(synthesizer: Arc<dyn BeliefSynthesizer>, sink: Arc<dyn BeliefSink>) -> Self {
        Self { synthesizer, sink }
    }
}

#[async_trait]
impl ConsolidationMutationExecutor for ReflectionExecutor {
    async fn execute(
        &self,
        request: &ConsolidationRequest,
        planned_tasks: &[ConsolidationTaskKind],
        started_at: Timestamp,
    ) -> CoreResult<ConsolidationMutationOutcome> {
        let mut task_results = Vec::new();
        let mut total_beliefs = 0u64;
        let mut errors = Vec::new();

        for kind in planned_tasks {
            if kind == &ConsolidationTaskKind::BeliefSynthesis {
                let beliefs = self.synthesizer.synthesize_beliefs(request).await?;
                let count = beliefs.len() as u64;
                let errors_before = errors.len();
                for belief in beliefs {
                    if let Err(e) = self.sink.put_belief(belief).await {
                        errors.push(ConsolidationError {
                            task: Some(ConsolidationTaskKind::BeliefSynthesis),
                            code: "put_belief_failed".to_owned(),
                            message: e.to_string(),
                            target_type: None,
                            target_id: None,
                            recoverable: true,
                        });
                    }
                }
                let task_errors = errors.len() - errors_before;
                let items_written = count - task_errors as u64;
                total_beliefs += items_written;
                task_results.push(ConsolidationTaskResult {
                    task: ConsolidationTaskKind::BeliefSynthesis,
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

        let stats = ConsolidationStats {
            memories_read: None,
            memories_written: None,
            beliefs_synthesized: if total_beliefs > 0 {
                Some(total_beliefs)
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

        Ok(ConsolidationMutationOutcome::new(
            task_results,
            stats,
            errors,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::belief_build::reflection_belief;
    use crate::source::BeliefSink;
    use engram_domain::{Actor, ActorKind, Belief, ConsolidationRequest, Id, Requester, Scope};
    use futures::executor::block_on;
    use std::sync::Mutex;

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
                    id: Id::from("reflection-executor-test"),
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

    struct StubSynthesizer {
        beliefs: Vec<Belief>,
    }

    #[async_trait]
    impl BeliefSynthesizer for StubSynthesizer {
        async fn synthesize_beliefs(
            &self,
            _request: &ConsolidationRequest,
        ) -> CoreResult<Vec<Belief>> {
            Ok(self.beliefs.clone())
        }
    }

    struct StubSink {
        put_count: Mutex<u64>,
    }

    #[async_trait]
    impl BeliefSink for StubSink {
        async fn put_belief(&self, belief: Belief) -> CoreResult<Belief> {
            *self.put_count.lock().unwrap() += 1;
            Ok(belief)
        }
    }

    #[test]
    fn belief_synthesis_task_persists_beliefs() {
        let beliefs = vec![reflection_belief(
            &["insight a".to_owned(), "insight b".to_owned()],
            &scope(),
            chrono::Utc::now(),
        )];
        let sink = Arc::new(StubSink {
            put_count: Mutex::new(0),
        });
        let executor = ReflectionExecutor::new(Arc::new(StubSynthesizer { beliefs }), sink.clone());
        let outcome = block_on(executor.execute(
            &request(),
            &[ConsolidationTaskKind::BeliefSynthesis],
            chrono::Utc::now(),
        ))
        .unwrap();

        assert_eq!(outcome.tasks.len(), 1);
        assert_eq!(outcome.tasks[0].status, ConsolidationTaskStatus::Completed);
        assert_eq!(
            outcome.tasks[0].items_written,
            Some(1),
            "one belief persisted"
        );
        assert_eq!(*sink.put_count.lock().unwrap(), 1);
        assert_eq!(outcome.stats.beliefs_synthesized, Some(1));
    }

    #[test]
    fn non_belief_synthesis_tasks_are_skipped() {
        let sink = Arc::new(StubSink {
            put_count: Mutex::new(0),
        });
        let executor = ReflectionExecutor::new(
            Arc::new(StubSynthesizer {
                beliefs: Vec::new(),
            }),
            sink.clone(),
        );
        let outcome = block_on(executor.execute(
            &request(),
            &[
                ConsolidationTaskKind::BeliefSynthesis,
                ConsolidationTaskKind::Compaction,
                ConsolidationTaskKind::Decay,
            ],
            chrono::Utc::now(),
        ))
        .unwrap();

        assert_eq!(outcome.tasks.len(), 3);
        assert_eq!(outcome.tasks[0].status, ConsolidationTaskStatus::Completed);
        assert_eq!(outcome.tasks[1].status, ConsolidationTaskStatus::Skipped);
        assert_eq!(outcome.tasks[2].status, ConsolidationTaskStatus::Skipped);
        assert_eq!(
            *sink.put_count.lock().unwrap(),
            0,
            "no beliefs for empty synth"
        );
    }

    #[test]
    fn all_skipped_when_no_belief_synthesis_planned() {
        let sink = Arc::new(StubSink {
            put_count: Mutex::new(0),
        });
        let executor = ReflectionExecutor::new(
            Arc::new(StubSynthesizer {
                beliefs: Vec::new(),
            }),
            sink,
        );
        let outcome = block_on(executor.execute(
            &request(),
            &[ConsolidationTaskKind::Compaction],
            chrono::Utc::now(),
        ))
        .unwrap();

        assert_eq!(outcome.tasks.len(), 1);
        assert_eq!(outcome.tasks[0].status, ConsolidationTaskStatus::Skipped);
        assert_eq!(outcome.stats.beliefs_synthesized, None);
    }

    struct FailingSink {
        put_count: Mutex<u64>,
    }

    #[async_trait]
    impl BeliefSink for FailingSink {
        async fn put_belief(&self, belief: Belief) -> CoreResult<Belief> {
            let mut count = self.put_count.lock().unwrap();
            *count += 1;
            if *count == 1 {
                Err(engram_runtime::CoreError::Adapter {
                    adapter: "failing-sink".to_owned(),
                    message: "forced failure".to_owned(),
                })
            } else {
                Ok(belief)
            }
        }
    }

    #[test]
    fn put_belief_failure_records_error_and_downgrades_status() {
        let beliefs = vec![
            reflection_belief(&["first".to_owned()], &scope(), chrono::Utc::now()),
            reflection_belief(&["second".to_owned()], &scope(), chrono::Utc::now()),
        ];
        let sink = Arc::new(FailingSink {
            put_count: Mutex::new(0),
        });
        let executor = ReflectionExecutor::new(Arc::new(StubSynthesizer { beliefs }), sink);
        let outcome = block_on(executor.execute(
            &request(),
            &[ConsolidationTaskKind::BeliefSynthesis],
            chrono::Utc::now(),
        ))
        .unwrap();

        assert!(!outcome.errors.is_empty(), "error recorded for failed put");
        assert_eq!(
            outcome.tasks[0].status,
            ConsolidationTaskStatus::CompletedWithErrors
        );
        assert_eq!(
            outcome.tasks[0].items_written,
            Some(1),
            "one of two beliefs persisted"
        );
        assert_eq!(
            outcome.stats.beliefs_synthesized,
            Some(1),
            "stat counts successes only"
        );
    }
}
