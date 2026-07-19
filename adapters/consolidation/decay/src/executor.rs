//! Decay executor — `ConsolidationMutationExecutor` handling `Decay`.

use std::sync::Arc;

use async_trait::async_trait;
use engram_consolidation::{ConsolidationMutationExecutor, ConsolidationMutationOutcome};
use engram_domain::{
    ConsolidationRequest, ConsolidationStats, ConsolidationTaskKind, ConsolidationTaskResult,
    ConsolidationTaskStatus, Timestamp,
};
use engram_runtime::CoreResult;

use crate::source::DecayMemorySource;

/// Decay consolidation executor: marks expired memories, skipping LegalHold.
pub struct DecayExecutor {
    source: Arc<dyn DecayMemorySource>,
}

impl DecayExecutor {
    pub fn new(source: Arc<dyn DecayMemorySource>) -> Self {
        Self { source }
    }
}

#[async_trait]
impl ConsolidationMutationExecutor for DecayExecutor {
    async fn execute(
        &self,
        request: &ConsolidationRequest,
        planned_tasks: &[ConsolidationTaskKind],
        started_at: Timestamp,
    ) -> CoreResult<ConsolidationMutationOutcome> {
        let mut task_results = Vec::new();
        let mut total_decayed = 0u64;

        for kind in planned_tasks {
            if kind == &ConsolidationTaskKind::Decay {
                let memories = self.source.memories(&request.scope).await?;
                let due: Vec<_> = memories.iter().filter(|m| m.is_due(started_at)).collect();
                for m in &due {
                    self.source.expire(&m.id, &request.scope).await?;
                }
                total_decayed = due.len() as u64;
                task_results.push(ConsolidationTaskResult {
                    task: ConsolidationTaskKind::Decay,
                    status: ConsolidationTaskStatus::Completed,
                    started_at,
                    completed_at: Some(started_at),
                    items_read: Some(memories.len() as u64),
                    items_written: Some(total_decayed),
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
            beliefs_synthesized: None,
            contradictions_detected: None,
            hierarchy_nodes_created: None,
            hierarchy_relations_created: None,
            records_decayed: if total_decayed > 0 {
                Some(total_decayed)
            } else {
                None
            },
            records_pruned: None,
            model_calls: None,
        };

        Ok(ConsolidationMutationOutcome::new(
            task_results,
            stats,
            Vec::new(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::{DecayCandidate, DecayMemorySource};
    use engram_domain::{
        Actor, ActorKind, AllowedUse, DeleteMode, Id, MemoryId, MemoryStatus, Policy, Requester,
        Retention, Scope, Sensitivity, Visibility,
    };
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
                    id: Id::from("decay-test"),
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

    fn policy_with_expiry(expires: Option<Timestamp>, retention: Retention) -> Policy {
        Policy {
            visibility: Visibility::Workspace,
            retention,
            sensitivity: Some(Sensitivity::Low),
            allowed_uses: vec![AllowedUse::Retrieval],
            expires_at: expires,
            delete_mode: Some(DeleteMode::Tombstone),
        }
    }

    struct StubSource {
        memories: Vec<DecayCandidate>,
        expired: Mutex<Vec<MemoryId>>,
    }

    #[async_trait]
    impl DecayMemorySource for StubSource {
        async fn memories(&self, _scope: &Scope) -> CoreResult<Vec<DecayCandidate>> {
            Ok(self.memories.clone())
        }
        async fn expire(&self, id: &MemoryId, _scope: &Scope) -> CoreResult<()> {
            self.expired.lock().unwrap().push(id.clone());
            Ok(())
        }
    }

    #[test]
    fn decays_expired_active_memories() {
        let past = chrono::Utc::now() - chrono::Duration::hours(1);
        let source = Arc::new(StubSource {
            memories: vec![
                DecayCandidate {
                    id: MemoryId::from(Id::from("expired-mem")),
                    status: MemoryStatus::Active,
                    policy: policy_with_expiry(Some(past), Retention::Durable),
                },
                DecayCandidate {
                    id: MemoryId::from(Id::from("fresh-mem")),
                    status: MemoryStatus::Active,
                    policy: policy_with_expiry(None, Retention::Durable),
                },
            ],
            expired: Mutex::new(Vec::new()),
        });
        let executor = DecayExecutor::new(source.clone());
        let outcome =
            block_on(executor.execute(&request(), &[ConsolidationTaskKind::Decay], now())).unwrap();

        assert_eq!(outcome.tasks.len(), 1);
        assert_eq!(outcome.tasks[0].status, ConsolidationTaskStatus::Completed);
        assert_eq!(outcome.tasks[0].items_written, Some(1));
        assert_eq!(outcome.stats.records_decayed, Some(1));
        let expired = source.expired.lock().unwrap();
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0], MemoryId::from(Id::from("expired-mem")));
    }

    #[test]
    fn skips_legal_hold() {
        let past = chrono::Utc::now() - chrono::Duration::hours(1);
        let source = Arc::new(StubSource {
            memories: vec![DecayCandidate {
                id: MemoryId::from(Id::from("legal-hold")),
                status: MemoryStatus::Active,
                policy: policy_with_expiry(Some(past), Retention::LegalHold),
            }],
            expired: Mutex::new(Vec::new()),
        });
        let executor = DecayExecutor::new(source.clone());
        let outcome =
            block_on(executor.execute(&request(), &[ConsolidationTaskKind::Decay], now())).unwrap();

        assert_eq!(outcome.stats.records_decayed, None);
        assert!(source.expired.lock().unwrap().is_empty());
    }

    #[test]
    fn skips_non_decay_tasks() {
        let source = Arc::new(StubSource {
            memories: vec![],
            expired: Mutex::new(Vec::new()),
        });
        let executor = DecayExecutor::new(source);
        let outcome = block_on(executor.execute(
            &request(),
            &[
                ConsolidationTaskKind::BeliefSynthesis,
                ConsolidationTaskKind::Decay,
            ],
            now(),
        ))
        .unwrap();

        assert_eq!(outcome.tasks.len(), 2);
        assert_eq!(outcome.tasks[0].status, ConsolidationTaskStatus::Skipped);
        assert_eq!(outcome.tasks[1].status, ConsolidationTaskStatus::Completed);
    }
}
