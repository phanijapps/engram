//! Conformance harness for capability verification.
//!
//! This crate provides deterministic fixtures that verify each capability family
//! before features are marked as supported. The EngramProvider calls these
//! fixtures during bootstrap to ensure capability reporting is accurate.

pub mod batch;
pub mod fastembed_provider;
pub mod fixtures;
pub mod harness;
pub mod migration_service;
pub mod provenance;
pub mod recall;
pub mod wiring;

pub use batch::SqlBatchIngest;
pub use harness::{ConformanceHarness, ConformanceResult, FixtureResult, FixtureStatus};
pub use migration_service::SqlMigrationService;
pub use provenance::SqlProvenanceQuery;
pub use recall::SqlUnifiedRecall;
pub use wiring::bootstrap_provider;

/// Creates a new conformance harness with all available fixtures.
///
/// The harness runs fixtures for each capability family and returns a
/// structured report that can be used to populate CapabilityReport.
pub fn new_harness() -> ConformanceHarness {
    ConformanceHarness::new()
}
