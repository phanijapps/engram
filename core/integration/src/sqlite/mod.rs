//! SQLite backend for the integration facade.
//!
//! Everything under this module is engine-specific (it names `Sql*`, holds the
//! SQLite adapter crates directly, and constructs file-backed stores) and is
//! gated behind the `sqlite` cargo feature. It is intentionally exempt from the
//! ADR-0022 engine-neutrality gate — the port traits it implements (in the
//! parent crate's `provenance`, `batch`, `recall`, `export_import`, and
//! `observability` modules) stay engine-neutral.
//!
//! [`bootstrap_sqlite`] is the sole entry point, reached by
//! [`EngramProvider::open`](crate::EngramProvider::open) when the `sqlite`
//! feature is enabled. Hosts select this backend declaratively via configuration
//! (no hand-written wiring) and reach every supported service through the
//! engine-neutral `Arc<dyn ...>` handles on the returned provider.

mod batch;
mod bootstrap;
mod conformance;
mod export_import;
mod migration_service;
mod observability;
mod provenance;
mod recall;
mod recall_lanes;

pub(crate) use bootstrap::bootstrap_sqlite;

// The `Sql*` port-impl structs are re-exported `pub` so the adapters-layer
// conformance crate (`engram-conformance`) and its integration tests can name
// them — `engram_conformance::SqlProvenanceQuery` etc. remain the stable
// test-facing path. They are `pub` only within the crate's public surface; the
// engine-neutral port traits they implement are the preferred handles.
pub use batch::SqlBatchIngest;
pub use export_import::SqlExportImport;
pub use migration_service::SqlMigrationService;
pub use observability::SqlObservability;
pub use provenance::SqlProvenanceQuery;
pub use recall::SqlUnifiedRecall;
pub use recall_lanes::{associative_recall_lane, community_summary_recall_lane};
