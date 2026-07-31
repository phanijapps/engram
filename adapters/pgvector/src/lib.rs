//! Postgres + pgvector adapter crate — engram's second storage backend.
//!
//! Consolidates every capability cell behind a shared Postgres connection,
//! mirroring `adapters/sqlite/`. Each cell implements the same engine-neutral
//! port traits (MemoryService, KnowledgeRepository, VectorIndex, etc.) over
//! Postgres tables + JSONB + pgvector.
//!
//! Feature-gated behind `pgvector` in `core/integration`; SQLite stays the
//! zero-dep default.

pub mod connection;
pub mod memory;
pub mod schema;

pub use connection::PgConnection;
pub use memory::{read_recent, write_memory, PgMemoryRow};
