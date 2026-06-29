//! SQL persistence adapter for Engram memory services.
//!
//! This crate keeps SQL-specific schema, serialization, and transaction details
//! behind the `engram-core` ports. The first implementation uses SQLite to make
//! durable adapter conformance runnable in CI without external services.

mod schema;
mod scope;
mod service;

pub use service::SqlMemoryStore;
