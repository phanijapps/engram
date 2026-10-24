//! Outcome-ledger cell (F2): append-only telemetry, ports only, never a
//! retrieval lane. Owns exactly one table.

mod repository;
mod schema;

pub use repository::SqlOutcomeStore;
