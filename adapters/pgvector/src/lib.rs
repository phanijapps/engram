//! Postgres + pgvector adapter crate — engram's second storage backend.
//!
//! Consolidates every capability cell behind a shared Postgres connection,
//! mirroring `adapters/sqlite/`. Each cell implements the same engine-neutral
//! port traits (KnowledgeRepository, VectorIndex, etc.) over Postgres tables +
//! JSONB + pgvector.
//!
//! Feature-gated behind `pgvector` in `core/integration`; SQLite stays the
//! zero-dep default.

pub mod cells;
pub mod connection;
pub mod knowledge;
pub mod memory;
pub mod schema;
pub mod vector;

pub use cells::{PgBeliefStore, PgHierarchyStore, PgMemoryService, PgProcedureStore};
pub use connection::PgConnection;
pub use knowledge::PgKnowledgeStore;
pub use memory::{read_recent, write_memory, PgMemoryRow};
pub use vector::PgVectorIndex;

/// Content-hash for vector dedup (SHA-256 of the f32 bytes, like the SQLite adapter).
use sha2::{Digest, Sha256};

pub(crate) fn content_hash(vector: &[f32]) -> String {
    let mut hasher = Sha256::new();
    for v in vector {
        hasher.update(v.to_le_bytes());
    }
    let hash = hasher.finalize();
    // Trim to match the SQLite adapter's format.
    format!("sha256:{}", hex::encode(hash))
}

// Minimal hex encoding (avoids adding a hex crate dep).
mod hex {
    pub fn encode(bytes: impl AsRef<[u8]>) -> String {
        bytes.as_ref().iter().map(|b| format!("{b:02x}")).collect()
    }
}
