//! Durable SQLite-backed belief + contradiction repository.
//!
//! Beliefs and contradictions are derived stance records owned by the
//! orchestration layer (`engram-core`). This adapter persists them as contract
//! JSON with scope indexing, mirroring `engram-store-knowledge-sqlite`. It stays
//! distinct from knowledge and memory storage: source-grounded evidence and
//! derived stance remain separate contract concepts.

mod detector;
mod rows;
mod schema;
mod scope;
mod service;

pub use service::SqlBeliefStore;
