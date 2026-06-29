//! Deterministic consolidation orchestration.
//!
//! The first consolidation slice is dry-run only. It produces auditable
//! `ConsolidationRun` records from existing domain contracts without attaching
//! repositories, schedulers, model providers, or mutation tasks.

mod planner;
mod service;
mod validation;

pub use service::DryRunConsolidationService;
