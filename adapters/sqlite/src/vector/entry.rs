//! Public vector adapter records.
//!
//! This module owns the adapter-local rows used by the sqlite-vec index. These
//! records reference domain targets but are not portable domain contracts and
//! should not leak store-specific fields into `engram-domain`.

use engram_domain::EmbeddingTargetType;

/// Vector row inserted into the SQLite vector index.
#[derive(Debug, Clone, PartialEq)]
pub struct VectorEntry {
    pub id: String,
    pub target_type: EmbeddingTargetType,
    pub target_id: String,
    pub model: String,
    pub dimensions: u32,
    pub content_hash: String,
    pub embedding: Vec<f32>,
}

/// Nearest-neighbor result returned by the SQLite vector index.
#[derive(Debug, Clone, PartialEq)]
pub struct VectorSearchResult {
    pub id: String,
    pub target_type: EmbeddingTargetType,
    pub target_id: String,
    pub model: String,
    pub dimensions: u32,
    pub content_hash: String,
    pub distance: f32,
}
