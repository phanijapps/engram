//! Postgres (pgvector) backend bootstrap — engine-specific zone (ADR-0022 exempt).
pub mod bootstrap;
pub use bootstrap::bootstrap_pgvector;
