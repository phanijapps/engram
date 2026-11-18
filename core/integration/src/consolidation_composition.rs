//! Shared bridges from governed memory and belief ports to existing consolidation executors.
//! Explicit previews delegate to the canonical planner; only dry_run=Some(false)
//! invokes the configured mutation executor.

use std::sync::Arc;

use async_trait::async_trait;
use engram_belief::BeliefRepository;
use engram_consolidation::{
    ConsolidationMutationExecutor, ConsolidationService, DryRunConsolidationService,
    plan_consolidation_operations,
};
use engram_decay::{DecayCandidate, DecayMemorySource};
use engram_domain::{
    Belief, ConsolidationRequest, ConsolidationRun, ConsolidationRunStatus, ConsolidationTrigger,
    Id, MemoryId, MemoryStatus, Scope,
};
use engram_memory::MemoryService;
use engram_reflection::{ActiveMemorySource, BeliefSink};
use engram_runtime::{Clock, CoreResult, IdGenerator};

/// Adapts the belief repository to `BeliefSink` (identical `put_belief` signature).
pub struct BeliefSinkAdapter(pub Arc<dyn BeliefRepository>);

#[async_trait]
impl BeliefSink for BeliefSinkAdapter {
    async fn put_belief(&self, belief: Belief) -> CoreResult<Belief> {
        self.0.put_belief(belief).await
    }
}

/// Adapts the governed memory service to `ActiveMemorySource`: reads scoped events →
/// resolves active memory records → returns TYPED EVIDENCE (identity,
/// policy, provenance — MCCL-audit F2 lineage).
pub struct ActiveMemorySourceAdapter(pub Arc<dyn MemoryService>);

#[async_trait]
impl ActiveMemorySource for ActiveMemorySourceAdapter {
    async fn active_memories(
        &self,
        scope: &Scope,
    ) -> CoreResult<Vec<engram_reflection::ActiveMemoryEvidence>> {
        let events = self.0.list_events_for_scope(scope).await?;
        let mut evidence = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        for event in &events {
            if let Some(mid) = &event.memory_id {
                let mid_str = mid.to_string();
                if seen.insert(mid_str)
                    && let Ok(Some(record)) = self.0.get_memory(mid, scope).await
                    && record.status == MemoryStatus::Active
                {
                    evidence.push(engram_reflection::ActiveMemoryEvidence {
                        memory_id: record.id.clone(),
                        text: record.content.text.clone(),
                        policy: record.policy.clone(),
                        observed_at: record.provenance.observed_at,
                    });
                }
            }
        }
        Ok(evidence)
    }
}

/// Adapts the governed memory service to `DecayMemorySource`: reads memories + expires them.
pub struct DecayMemorySourceAdapter(pub Arc<dyn MemoryService>);

#[async_trait]
impl DecayMemorySource for DecayMemorySourceAdapter {
    async fn memories(&self, scope: &Scope) -> CoreResult<Vec<DecayCandidate>> {
        let events = self.0.list_events_for_scope(scope).await?;
        let mut result = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        for event in &events {
            if let Some(mid) = &event.memory_id {
                let mid_str = mid.to_string();
                if seen.insert(mid_str)
                    && let Ok(Some(record)) = self.0.get_memory(mid, scope).await
                {
                    result.push(DecayCandidate {
                        id: mid.clone(),
                        status: record.status,
                        policy: record.policy.clone(),
                    });
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
/// them, and returns a `ConsolidationRun`.
///
/// Explicit-apply contract (planner/validation semantics): a request executes
/// ONLY when `dry_run == Some(false)`. Everything else — including `None` —
/// is a plan-only preview delegated to [`DryRunConsolidationService`]; the
/// mutating executor is never invoked. (Regression: this adapter previously
/// ignored the flag and executed unconditionally, so MCP `maintain
/// {"dry_run": true}` mutated the store while reporting a preview.)
pub struct ExecutorConsolidationService {
    planner: Arc<DryRunConsolidationService>,
    executor: Arc<dyn ConsolidationMutationExecutor>,
}

impl ExecutorConsolidationService {
    pub fn new(
        clock: Arc<dyn Clock>,
        ids: Arc<dyn IdGenerator>,
        executor: Arc<dyn ConsolidationMutationExecutor>,
    ) -> Self {
        Self {
            planner: Arc::new(DryRunConsolidationService::new(clock, ids)),
            executor,
        }
    }
}

#[async_trait]
impl ConsolidationService for ExecutorConsolidationService {
    async fn consolidate(&self, request: ConsolidationRequest) -> CoreResult<ConsolidationRun> {
        // Preview path: honor the explicit-apply contract — anything but
        // `Some(false)` plans without touching the executor.
        if !matches!(request.dry_run, Some(false)) {
            return self.planner.consolidate(request).await;
        }
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
