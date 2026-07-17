//! Adapter impls bridging concrete SQLite stores to the narrow executor traits
//! (`BeliefSink`, `ActiveMemorySource`, `DecayMemorySource`).
//!
//! ADR-0022: engine-specific (names `Sql*`, holds the SQLite adapters). Lives
//! under `src/sqlite/` behind the `sqlite` feature, exempt from the neutrality
//! gate.

use std::sync::Arc;

use async_trait::async_trait;
use engram_belief::BeliefRepository;
use engram_consolidation::{
    ConsolidationMutationExecutor, ConsolidationService, plan_consolidation_operations,
};
use engram_decay::{DecayCandidate, DecayMemorySource};
use engram_domain::{
    Belief, ConsolidationRequest, ConsolidationRun, ConsolidationRunStatus, ConsolidationTrigger,
    Id, MemoryId, MemoryStatus, Scope,
};
use engram_memory::{MemoryEventRepository, MemoryRepository};
use engram_reflection::{ActiveMemorySource, BeliefSink};
use engram_runtime::CoreResult;
use engram_store_sqlite::SqlBeliefStore;
use engram_store_sqlite::SqlMemoryService;

/// Adapts `SqlBeliefStore` to `BeliefSink` (identical `put_belief` signature).
pub(crate) struct BeliefSinkAdapter(pub(crate) Arc<SqlBeliefStore>);

#[async_trait]
impl BeliefSink for BeliefSinkAdapter {
    async fn put_belief(&self, belief: Belief) -> CoreResult<Belief> {
        self.0.put_belief(belief).await
    }
}

/// Adapts `SqlMemoryService` to `ActiveMemorySource`: reads scoped events →
/// resolves active memory records → extracts text.
pub(crate) struct ActiveMemorySourceAdapter(pub(crate) Arc<SqlMemoryService>);

#[async_trait]
impl ActiveMemorySource for ActiveMemorySourceAdapter {
    async fn active_memory_texts(&self, scope: &Scope) -> CoreResult<Vec<String>> {
        let events = self.0.list_events_for_scope(scope).await?;
        let mut texts = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        for event in &events {
            if let Some(mid) = &event.memory_id {
                let mid_str = mid.to_string();
                if seen.insert(mid_str) {
                    if let Ok(Some(record)) = self.0.get_memory(mid, scope).await {
                        if record.status == MemoryStatus::Active {
                            texts.push(record.content.text.clone());
                        }
                    }
                }
            }
        }
        Ok(texts)
    }
}

/// Adapts `SqlMemoryService` to `DecayMemorySource`: reads memories + expires them.
pub(crate) struct DecayMemorySourceAdapter(pub(crate) Arc<SqlMemoryService>);

#[async_trait]
impl DecayMemorySource for DecayMemorySourceAdapter {
    async fn memories(&self, scope: &Scope) -> CoreResult<Vec<DecayCandidate>> {
        let events = self.0.list_events_for_scope(scope).await?;
        let mut result = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        for event in &events {
            if let Some(mid) = &event.memory_id {
                let mid_str = mid.to_string();
                if seen.insert(mid_str) {
                    if let Ok(Some(record)) = self.0.get_memory(mid, scope).await {
                        result.push(DecayCandidate {
                            id: mid.clone(),
                            status: record.status,
                            policy: record.policy.clone(),
                        });
                    }
                }
            }
        }
        Ok(result)
    }

    async fn expire(&self, id: &MemoryId, scope: &Scope) -> CoreResult<()> {
        self.0
            .update_memory_status(id, scope, MemoryStatus::Expired)
            .await?;
        Ok(())
    }
}

/// Wraps a composite executor as a `ConsolidationService`: plans tasks, executes
/// them, and returns a `ConsolidationRun`. No eval gates (ungated — the gated
/// `GatedConsolidationService` needs an evaluation fixture not yet shipped).
pub(crate) struct ExecutorConsolidationService {
    executor: Arc<dyn ConsolidationMutationExecutor>,
}

impl ExecutorConsolidationService {
    pub(crate) fn new(executor: Arc<dyn ConsolidationMutationExecutor>) -> Self {
        Self { executor }
    }
}

#[async_trait]
impl ConsolidationService for ExecutorConsolidationService {
    async fn consolidate(&self, request: ConsolidationRequest) -> CoreResult<ConsolidationRun> {
        let now = chrono::Utc::now();
        let plan = plan_consolidation_operations(&request, now)?;
        let planned: Vec<_> = plan.operations.iter().map(|o| o.task.clone()).collect();
        let outcome = self.executor.execute(&request, &planned, now).await?;
        let status = if outcome.errors.is_empty() {
            ConsolidationRunStatus::Completed
        } else {
            ConsolidationRunStatus::CompletedWithErrors
        };
        Ok(ConsolidationRun {
            id: Id::from(format!("consolidation-{}", now.timestamp())),
            scope: request.scope.clone(),
            requester: request.requester.clone(),
            trigger: ConsolidationTrigger::OnDemand,
            status,
            started_at: now,
            completed_at: Some(now),
            tasks: outcome.tasks,
            stats: Some(outcome.stats),
            errors: outcome.errors,
            metadata: None,
        })
    }
}
