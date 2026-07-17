//! Durable SQLite-backed hierarchy repository.
//!
//! Hierarchy nodes and relations are persisted as contract JSON with scope
//! indexing, mirroring `engram-store-belief-sqlite` and the knowledge SQLite
//! adapter. Path navigation replicates the in-memory adapter's traversal so the
//! durable backend behaves identically to the fixture.

mod schema;
mod scope;
mod service;

pub use service::SqlHierarchyStore;
