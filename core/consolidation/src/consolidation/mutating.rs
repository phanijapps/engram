//! Gated mutating consolidation orchestration.
//!
//! This module owns the safety envelope for durable consolidation work. It
//! composes evaluation gates and an injected mutation executor without owning
//! concrete stores, model providers, schedulers, or task algorithms.

use std::sync::Arc;

use async_trait::async_trait;
use engram_domain::{
    ConsolidationError, ConsolidationPlan, ConsolidationRequest, ConsolidationRun,
    ConsolidationRunStatus, ConsolidationStats, ConsolidationTaskKind, ConsolidationTaskResult,
    EvaluationFixture, Metadata, Scalar, Timestamp,
};
use serde_json::json;

use crate::{
    Clock, ConsolidationService, CoreError, CoreResult, EvaluationRunner, IdGenerator,
    consolidation::{
        evaluation_gate::{adapter_error, evaluation_errors, evaluation_task},
        planner::{build_plan, trigger_for},
        validation::validate_mutating_request,
    },
};

/// Audit payload returned by a mutating consolidation executor.
///
/// Concrete task implementations must report every durable effect through
/// `tasks`, `stats`, and `errors` instead of relying on adapter logs or hidden
/// side effects.
#[derive(Debug, Clone, PartialEq)]
pub struct ConsolidationMutationOutcome {
    pub tasks: Vec<ConsolidationTaskResult>,
    pub stats: ConsolidationStats,
    pub errors: Vec<ConsolidationError>,
}

impl ConsolidationMutationOutcome {
    /// Creates an executor outcome from task-level audit data.
    ///
    /// Callers should include one task result per attempted mutating task. Empty
    /// errors mean the executor believes durable work completed successfully.
    pub fn new(
        tasks: Vec<ConsolidationTaskResult>,
        stats: ConsolidationStats,
        errors: Vec<ConsolidationError>,
    ) -> Self {
        Self {
            tasks,
            stats,
            errors,
        }
    }
}

/// Executes the concrete mutating part of a consolidation cycle.
///
/// Core orchestration supplies the validated request, the planned task kinds,
/// and the run start timestamp. Implementations own repository/model work and
/// must return task-level audit records for anything they mutate.
#[async_trait]
pub trait ConsolidationMutationExecutor: Send + Sync {
    /// Executes planned consolidation tasks and reports durable outcomes.
    async fn execute(
        &self,
        request: &ConsolidationRequest,
        planned_tasks: &[ConsolidationTaskKind],
        started_at: Timestamp,
    ) -> CoreResult<ConsolidationMutationOutcome>;
}

/// Authorizes a planned mutating consolidation cycle before durable work runs.
///
/// Implementations should perform policy, tenancy, idempotency, and operational
/// gate checks that are independent of the concrete mutation executor. Returning
/// an error prevents executor work and produces an auditable failed run.
#[async_trait]
pub trait ConsolidationApplyGate: Send + Sync {
    /// Authorizes the planned operations for this request.
    async fn authorize(
        &self,
        request: &ConsolidationRequest,
        plan: &ConsolidationPlan,
    ) -> CoreResult<()>;
}

/// Apply gate that allows all validated plans.
///
/// This is the default so existing embedders can keep using the service while
/// stricter adapters inject policy-aware gates.
#[derive(Debug, Default)]
pub struct AllowAllConsolidationApplyGate;

#[async_trait]
impl ConsolidationApplyGate for AllowAllConsolidationApplyGate {
    async fn authorize(
        &self,
        _request: &ConsolidationRequest,
        _plan: &ConsolidationPlan,
    ) -> CoreResult<()> {
        Ok(())
    }
}

/// Mutating consolidation service protected by pre/post evaluation gates.
///
/// This service refuses ambiguous mutation requests, runs the protected fixture
/// before and after executor work, and returns an auditable `ConsolidationRun`
/// for successful, failed, or regression-detected cycles.
#[derive(Clone)]
pub struct GatedConsolidationService {
    clock: Arc<dyn Clock>,
    ids: Arc<dyn IdGenerator>,
    evaluator: Arc<dyn EvaluationRunner>,
    protected_fixture: EvaluationFixture,
    executor: Arc<dyn ConsolidationMutationExecutor>,
    apply_gate: Arc<dyn ConsolidationApplyGate>,
}

impl GatedConsolidationService {
    /// Creates a mutating consolidation service with explicit gates.
    ///
    /// The protected fixture is cloned for each pre- and post-run evaluation so
    /// the evaluator receives the same quality contract on both sides of the
    /// mutation executor.
    pub fn new(
        clock: Arc<dyn Clock>,
        ids: Arc<dyn IdGenerator>,
        evaluator: Arc<dyn EvaluationRunner>,
        protected_fixture: EvaluationFixture,
        executor: Arc<dyn ConsolidationMutationExecutor>,
    ) -> Self {
        Self {
            clock,
            ids,
            evaluator,
            protected_fixture,
            executor,
            apply_gate: Arc::new(AllowAllConsolidationApplyGate),
        }
    }

    /// Replaces the default allow-all apply gate with an explicit policy gate.
    pub fn with_apply_gate(mut self, apply_gate: Arc<dyn ConsolidationApplyGate>) -> Self {
        self.apply_gate = apply_gate;
        self
    }
}

impl GatedConsolidationService {
    fn planned_tasks(plan: &ConsolidationPlan) -> Vec<ConsolidationTaskKind> {
        plan.operations
            .iter()
            .map(|operation| operation.task.clone())
            .collect()
    }
}

#[async_trait]
impl ConsolidationService for GatedConsolidationService {
    async fn consolidate(&self, request: ConsolidationRequest) -> CoreResult<ConsolidationRun> {
        validate_mutating_request(&request)?;

        let started_at = self.clock.now();
        let plan = build_plan(&request, started_at);
        let planned_tasks = Self::planned_tasks(&plan);
        let pre_report = self
            .evaluator
            .run_fixture(self.protected_fixture.clone())
            .await;
        let pre_report = match pre_report {
            Ok(report) => report,
            Err(error) => {
                return Ok(self.failed_run(
                    request,
                    started_at,
                    vec![adapter_error(error.to_string())],
                    Vec::new(),
                    Some(empty_stats()),
                ));
            }
        };

        let pre_task = evaluation_task(&pre_report, started_at);
        let pre_errors = evaluation_errors(&pre_report);
        if !pre_errors.is_empty() {
            return Ok(self.failed_run(
                request,
                started_at,
                pre_errors,
                vec![pre_task],
                Some(empty_stats()),
            ));
        }

        if let Err(error) = self.apply_gate.authorize(&request, &plan).await {
            return Ok(self.failed_run(
                request,
                started_at,
                vec![apply_gate_error(error)],
                vec![pre_task],
                Some(empty_stats()),
            ));
        }

        let outcome = match self
            .executor
            .execute(&request, &planned_tasks, started_at)
            .await
        {
            Ok(outcome) => outcome,
            Err(error) => {
                return Ok(self.failed_run(
                    request,
                    started_at,
                    vec![executor_error(error)],
                    vec![pre_task],
                    Some(empty_stats()),
                ));
            }
        };

        let post_report = self
            .evaluator
            .run_fixture(self.protected_fixture.clone())
            .await;
        let (post_task, post_errors) = match post_report {
            Ok(report) => {
                let task = evaluation_task(&report, started_at);
                let errors = evaluation_errors(&report);
                (task, errors)
            }
            Err(error) => (
                failed_evaluation_task(started_at, adapter_error(error.to_string())),
                vec![adapter_error(error.to_string())],
            ),
        };

        let mut tasks = Vec::with_capacity(outcome.tasks.len() + 2);
        tasks.push(pre_task);
        tasks.extend(outcome.tasks);
        tasks.push(post_task);

        let mut errors = outcome.errors;
        errors.extend(post_errors);
        let status = if errors.is_empty() {
            ConsolidationRunStatus::Completed
        } else {
            ConsolidationRunStatus::CompletedWithErrors
        };
        let trigger = trigger_for(&request);

        Ok(ConsolidationRun {
            id: self.ids.new_id("consolidation-run"),
            scope: request.scope,
            requester: request.requester,
            trigger,
            status,
            started_at,
            completed_at: Some(self.clock.now()),
            tasks,
            stats: Some(outcome.stats),
            errors,
            metadata: Some(mutating_metadata()),
        })
    }
}

impl GatedConsolidationService {
    fn failed_run(
        &self,
        request: ConsolidationRequest,
        started_at: Timestamp,
        errors: Vec<ConsolidationError>,
        tasks: Vec<ConsolidationTaskResult>,
        stats: Option<ConsolidationStats>,
    ) -> ConsolidationRun {
        let trigger = trigger_for(&request);
        ConsolidationRun {
            id: self.ids.new_id("consolidation-run"),
            scope: request.scope,
            requester: request.requester,
            trigger,
            status: ConsolidationRunStatus::Failed,
            started_at,
            completed_at: Some(self.clock.now()),
            tasks,
            stats,
            errors,
            metadata: Some(mutating_metadata()),
        }
    }
}

fn failed_evaluation_task(
    timestamp: Timestamp,
    error: ConsolidationError,
) -> ConsolidationTaskResult {
    ConsolidationTaskResult {
        task: ConsolidationTaskKind::Evaluation,
        status: engram_domain::ConsolidationTaskStatus::Failed,
        started_at: timestamp,
        completed_at: Some(timestamp),
        items_read: Some(0),
        items_written: Some(0),
        items_updated: Some(0),
        items_skipped: Some(0),
        model_calls: Some(0),
        errors: vec![error],
        output_refs: Vec::new(),
    }
}

fn executor_error(error: CoreError) -> ConsolidationError {
    ConsolidationError {
        task: None,
        code: "mutation_executor_failed".to_owned(),
        message: error.to_string(),
        target_type: None,
        target_id: None,
        recoverable: true,
    }
}

fn apply_gate_error(error: CoreError) -> ConsolidationError {
    let (code, recoverable) = match error {
        CoreError::PolicyDenied { .. } => ("policy_denied", false),
        CoreError::Conflict { .. } => ("apply_gate_conflict", true),
        CoreError::InvalidRequest { .. } => ("apply_gate_invalid_request", false),
        CoreError::NotFound { .. } => ("apply_gate_missing_target", true),
        CoreError::Adapter { .. } => ("apply_gate_adapter_failed", true),
        CoreError::ProviderUnavailable { .. } => ("provider_unavailable", true),
        CoreError::MigrationPending { .. } => ("migration_pending", true),
        CoreError::DryRunMode { .. } => ("dry_run_mode", false),
        CoreError::CapabilityUnsupported { .. } => ("capability_unsupported", false),
        CoreError::EmbeddingSpaceMismatch { .. } => ("embedding_space_mismatch", true),
        CoreError::MigrationManifestStale { .. } => ("migration_manifest_stale", true),
        CoreError::MigrationFailed { .. } => ("migration_failed", false),
        CoreError::TransactionUnsupported { .. } => ("transaction_unsupported", false),
        CoreError::ValidationFailed { .. } => ("validation_failed", false),
        CoreError::BackendTransient { .. } => ("backend_transient", true),
        CoreError::BackendPermanent { .. } => ("backend_permanent", false),
    };
    ConsolidationError {
        task: None,
        code: code.to_owned(),
        message: error.to_string(),
        target_type: None,
        target_id: None,
        recoverable,
    }
}

fn empty_stats() -> ConsolidationStats {
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

fn mutating_metadata() -> Metadata {
    Metadata::from([("dryRun".to_owned(), Scalar::from(json!(false)))])
}
