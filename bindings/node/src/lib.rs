//! Node-API bridge for Engram memory operations.
//!
//! The binding is intentionally a JSON transport over Rust behavior. TypeScript
//! packages own ergonomics; this crate owns serialization round trips into the
//! Rust memory service.

#[cfg(feature = "sqlite")]
mod belief;
#[cfg(feature = "sqlite")]
mod codegraph;
mod consolidation;
mod embedding_config;
mod eval;
mod hierarchy;
#[cfg(feature = "sqlite")]
mod ingest;
mod integration;
#[cfg(feature = "sqlite")]
mod knowledge;
#[cfg(feature = "sqlite")]
mod memory;
mod provider;

#[cfg(feature = "sqlite")]
mod knowledge_chunks;
#[cfg(feature = "sqlite")]
mod knowledge_concepts;
#[cfg(feature = "sqlite")]
mod knowledge_documents;
#[cfg(feature = "sqlite")]
mod knowledge_entities;
#[cfg(feature = "sqlite")]
mod knowledge_fusion;
#[cfg(feature = "sqlite")]
mod knowledge_graph;
#[cfg(feature = "sqlite")]
mod knowledge_ontology;
#[cfg(feature = "sqlite")]
mod knowledge_relationships;
#[cfg(feature = "sqlite")]
mod knowledge_sources;

#[cfg(feature = "sqlite")]
mod cross_file;
#[cfg(feature = "sqlite")]
mod ingest_ops;
#[cfg(feature = "sqlite")]
mod ingest_state;
mod utils;

// Re-export public engines
#[cfg(feature = "sqlite")]
pub use belief::NativeBeliefEngine;
pub use consolidation::NativeConsolidationEngine;
pub use eval::NativeEvalEngine;
pub use hierarchy::NativeHierarchyEngine;
#[cfg(feature = "sqlite")]
pub use ingest::NativeIngestEngine;
#[cfg(feature = "sqlite")]
pub use knowledge::NativeKnowledgeEngine;
#[cfg(feature = "sqlite")]
pub use memory::NativeMemoryEngine;
pub use provider::NativeProvider;

// Re-export utility functions for internal use
pub use utils::{decode, encode, id_field, scope_field, to_napi_error};

// Re-export cross-file resolution for internal use
#[cfg(feature = "sqlite")]
pub use cross_file::resolve_cross_file_edges;

// Type alias for CoreError to avoid conflict
pub use engram_memory::CoreError;
