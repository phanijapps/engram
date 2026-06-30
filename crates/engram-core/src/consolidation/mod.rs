//! Deterministic consolidation orchestration.
//!
//! The first consolidation slice is dry-run only. It produces auditable
//! `ConsolidationRun` records from existing domain contracts without attaching
//! repositories, schedulers, model providers, or mutation tasks.

mod evaluation_gate;
mod mutating;
mod planner;
mod service;
mod validation;

pub use mutating::{
    ConsolidationMutationExecutor, ConsolidationMutationOutcome, GatedConsolidationService,
};
pub use service::DryRunConsolidationService;
