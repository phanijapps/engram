//! SQL persistence adapter for Engram memory services.
//!
//! This crate keeps SQL-specific schema, serialization, and transaction details
//! behind the `engram-core` ports. The first implementation uses SQLite to make
//! durable adapter conformance runnable in CI without external services.

mod dependencies;
mod engine;
mod forget;
mod retrieval;
mod schema;
mod scope;
mod service;
mod transactional_write;
mod validation;
mod write;

pub use dependencies::{AllowAllPolicyAuthorizer, SequentialIdGenerator, SystemClock};
pub use engine::SqlMemoryService;
pub use service::SqlMemoryStore;
