//! Dry-run consolidation service implementation.
//!
//! This module owns orchestration for the first sleep-cycle slice: validate a
//! request, choose deterministic task reports, and return an auditable run. It
//! deliberately has no repository, scheduler, model, or background runtime
//! dependency so it cannot hide durable mutations.

use std::sync::Arc;

use async_trait::async_trait;
use engram_domain::{
    ConsolidationRequest, ConsolidationRun, ConsolidationRunStatus, Metadata, Scalar,
};
use serde_json::json;

use crate::{
    Clock, ConsolidationService, CoreResult, IdGenerator,
    consolidation::{
        planner::{empty_stats, plan_tasks, trigger_for},
        validation::validate_request,
    },
};

/// Dry-run consolidation service for auditable sleep-cycle planning.
///
/// This implementation intentionally has no repository dependencies. It
/// validates a bounded request and returns the `ConsolidationRun` that a
/// consolidation cycle would report, while guaranteeing that no durable state
/// can be mutated through this first-slice service.
#[derive(Clone)]
pub struct DryRunConsolidationService {
    clock: Arc<dyn Clock>,
    ids: Arc<dyn IdGenerator>,
}

impl DryRunConsolidationService {
    /// Creates a dry-run consolidation service with deterministic dependencies.
    ///
    /// Tests should inject fixed clocks and ID generators so run reports are
    /// stable. Production callers can inject system implementations while still
    /// receiving dry-run-only behavior.
    pub fn new(clock: Arc<dyn Clock>, ids: Arc<dyn IdGenerator>) -> Self {
        Self { clock, ids }
    }
}

#[async_trait]
impl ConsolidationService for DryRunConsolidationService {
    async fn consolidate(&self, request: ConsolidationRequest) -> CoreResult<ConsolidationRun> {
        validate_request(&request)?;

        let started_at = self.clock.now();
        let trigger = trigger_for(&request);
        let tasks = plan_tasks(&request, started_at);

        Ok(ConsolidationRun {
            id: self.ids.new_id("consolidation-run"),
            scope: request.scope,
            requester: request.requester,
            trigger,
            status: ConsolidationRunStatus::Completed,
            started_at,
            completed_at: Some(started_at),
            tasks,
            stats: Some(empty_stats()),
            errors: Vec::new(),
            metadata: Some(dry_run_metadata()),
        })
    }
}

fn dry_run_metadata() -> Metadata {
    Metadata::from([("dryRun".to_owned(), Scalar::from(json!(true)))])
}
