//! Conformance harness for capability verification.
//!
//! This crate provides deterministic fixtures that verify each capability family
//! before features are marked as supported, plus a thin wiring delegate. The
//! SQLite port implementations (`Sql*`) and the bootstrap wiring have moved
//! into `core/integration` behind the `sqlite` feature; the re-exports below
//! keep `engram_conformance::Sql*` (used by the integration-test suite) and
//! `engram_conformance::bootstrap_provider` (used by `examples/rust-integration`
//! and `bindings/node`) source-compatible. New host code should call
//! [`engram_integration::EngramProvider::open`] directly.

pub mod fastembed_provider;
pub mod fixtures;
pub mod harness;
pub mod wiring;

pub use harness::{ConformanceHarness, ConformanceResult, FixtureResult, FixtureStatus};
pub use wiring::bootstrap_provider;

// Re-export the SQLite port impls from the core crate's `sqlite` module so the
// `engram_conformance::Sql*` paths used by the integration-test suite keep
// resolving. These are available only because this crate enables the `sqlite`
// feature on `engram-integration` (see Cargo.toml).
pub use engram_integration::sqlite::{
    SqlBatchIngest, SqlExportImport, SqlMigrationService, SqlObservability, SqlProvenanceQuery,
    SqlUnifiedRecall,
};

/// Creates a new conformance harness with all available fixtures.
///
/// The harness runs fixtures for each capability family and returns a
/// structured report that can be used to populate CapabilityReport.
pub fn new_harness() -> ConformanceHarness {
    ConformanceHarness::new()
}
