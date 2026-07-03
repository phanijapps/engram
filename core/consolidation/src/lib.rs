//! Consolidation ports and orchestration for the engram engine.
//!
//! Auditable consolidation cycles over memory and knowledge state: the
//! `ConsolidationService` port, the dry-run and gated mutating service
//! implementations, and the deterministic task planning and validation helpers.
//! Domain types live in `engram-domain`; this crate owns the port and the
//! orchestration. Concrete mutation algorithms stay behind the executor trait,
//! implemented by adapters.

mod consolidation;

use async_trait::async_trait;
use engram_domain::*;
use engram_eval::{EvaluationReport, EvaluationRunner};
use engram_runtime::{Clock, CoreError, CoreResult, IdGenerator};

pub use consolidation::{
    AllowAllConsolidationApplyGate, ConsolidationApplyGate, ConsolidationMutationExecutor,
    ConsolidationMutationOutcome, DryRunConsolidationService, GatedConsolidationService,
    plan_consolidation_operations,
};

/// Runs auditable consolidation cycles over memory and knowledge state.
///
/// Consolidation may synthesize memories, beliefs, contradictions, hierarchy
/// nodes, or taxonomy changes. Any durable mutation should be represented in a
/// `ConsolidationRun` with task-level outcomes and recoverable errors.
#[async_trait]
pub trait ConsolidationService: Send + Sync {
    /// Executes one consolidation cycle for the requested scope and strategy.
    async fn consolidate(&self, request: ConsolidationRequest) -> CoreResult<ConsolidationRun>;
}
