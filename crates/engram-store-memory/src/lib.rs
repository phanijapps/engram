//! In-memory adapter for Engram memory services.
//!
//! This crate owns process-local storage used by specification tests, examples,
//! and first vertical slices. It implements core ports without making
//! `engram-core` depend on concrete state, clocks, ID counters, or storage
//! details. Durable SQL, vector, and provider-backed adapters should live in
//! separate crates and satisfy the same core contracts.

mod belief;
mod dependencies;
mod forget;
mod hierarchy;
mod knowledge;
mod knowledge_retrieval;
mod retrieval;
mod scope;
mod service;
mod state;
mod validation;
mod write;

pub use dependencies::{AllowAllPolicyAuthorizer, SequentialIdGenerator, SystemClock};
pub use service::InMemoryMemoryService;
