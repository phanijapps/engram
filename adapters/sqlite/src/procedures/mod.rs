//! Durable SQLite-backed procedure repository (RFC-0016 Layer 6).
//!
//! Procedures are replayable runbooks with success/failure accounting. This
//! adapter persists them as contract JSON with scope indexing, mirroring the
//! belief adapter. It stays distinct from knowledge, memory, and belief storage.

mod schema;
mod scope;
mod service;

pub use service::SqlProcedureStore;
