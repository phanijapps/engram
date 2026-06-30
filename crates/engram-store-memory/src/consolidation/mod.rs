//! Concrete in-memory consolidation tasks.
//!
//! Core owns consolidation gates and run orchestration. This module adapts
//! planned task kinds to focused in-memory algorithms without moving store
//! state, model providers, schedulers, or task policy into `engram-core`.

use std::sync::Arc;

use async_trait::async_trait;
use engram_core::{ConsolidationMutationExecutor, ConsolidationMutationOutcome};
use engram_domain::*;
use engram_runtime::CoreResult;

use crate::service::InMemoryMemoryService;

mod belief_synthesis;
mod common;
mod compaction;
mod contradiction_detection;
mod decay;
mod hierarchy_aggregate;
mod hierarchy_build;
mod semantic_drift;

/// Mutating consolidation executor for the in-memory adapter.
///
/// The executor delegates each supported task to a focused module and reports
/// skipped task results for planned task kinds this adapter does not implement
/// yet.
#[derive(Clone)]
pub struct InMemoryConsolidationExecutor {
    service: InMemoryMemoryService,
}

impl InMemoryConsolidationExecutor {
    /// Creates an executor that mutates the same state as the supplied service.
    ///
    /// Clone the `InMemoryMemoryService` passed to tests or examples so writes,
    /// retrieval, and consolidation operate over one process-local store.
    pub fn new(service: InMemoryMemoryService) -> Self {
        Self { service }
    }

    /// Creates a shared executor for `GatedConsolidationService` composition.
    ///
    /// The returned value is typed as `Arc<Self>` so callers can pass it through
    /// Rust's trait-object coercion to the core `ConsolidationMutationExecutor`
    /// port while still constructing it from the concrete in-memory service.
    pub fn shared(service: InMemoryMemoryService) -> Arc<Self> {
        Arc::new(Self::new(service))
    }
}

#[async_trait]
impl ConsolidationMutationExecutor for InMemoryConsolidationExecutor {
    async fn execute(
        &self,
        request: &ConsolidationRequest,
        planned_tasks: &[ConsolidationTaskKind],
        started_at: Timestamp,
    ) -> CoreResult<ConsolidationMutationOutcome> {
        let mut tasks = Vec::with_capacity(planned_tasks.len());
        let mut stats = common::empty_stats();

        for task in planned_tasks {
            match task {
                ConsolidationTaskKind::Compaction => {
                    let result = compaction::compact_duplicates(
                        &self.service,
                        request,
                        started_at,
                        &mut stats,
                    )?;
                    tasks.push(result);
                }
                ConsolidationTaskKind::Decay => {
                    let result =
                        decay::expire_due_memories(&self.service, request, started_at, &mut stats)?;
                    tasks.push(result);
                }
                ConsolidationTaskKind::HierarchyBuild => {
                    let base_result = hierarchy_build::build_base_nodes(
                        &self.service,
                        request,
                        started_at,
                        &mut stats,
                    )?;
                    let aggregate_result = hierarchy_aggregate::build_entity_aggregates(
                        &self.service,
                        request,
                        started_at,
                        &mut stats,
                    )?;
                    let result = common::merge_task_results(base_result, aggregate_result);
                    tasks.push(result);
                }
                ConsolidationTaskKind::BeliefSynthesis => {
                    let result = belief_synthesis::synthesize_assertion_beliefs(
                        &self.service,
                        request,
                        started_at,
                        &mut stats,
                    )?;
                    tasks.push(result);
                }
                ConsolidationTaskKind::BeliefContradictionDetection => {
                    let result = contradiction_detection::detect_assertion_contradictions(
                        &self.service,
                        request,
                        started_at,
                        &mut stats,
                    )?;
                    tasks.push(result);
                }
                ConsolidationTaskKind::SemanticDriftDetection => {
                    let result = semantic_drift::detect_assertion_drift(
                        &self.service,
                        request,
                        started_at,
                        &mut stats,
                    )?;
                    tasks.push(result);
                }
                unsupported => tasks.push(common::skipped_task(unsupported.clone(), started_at)),
            }
        }

        Ok(ConsolidationMutationOutcome::new(tasks, stats, Vec::new()))
    }
}
