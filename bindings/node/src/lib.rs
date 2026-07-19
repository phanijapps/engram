//! Node-API bridge for Engram memory operations.
//!
//! The binding is intentionally a JSON transport over Rust behavior. TypeScript
//! packages own ergonomics; this crate owns serialization round trips into the
//! Rust memory service.

mod belief;
mod codegraph;
mod consolidation;
mod eval;
mod hierarchy;
mod ingest;
mod integration;
mod knowledge;
mod memory;
mod provider;

mod knowledge_chunks;
mod knowledge_concepts;
mod knowledge_documents;
mod knowledge_entities;
mod knowledge_fusion;
mod knowledge_graph;
mod knowledge_ontology;
mod knowledge_relationships;
mod knowledge_sources;

mod cross_file;
mod ingest_ops;
mod ingest_state;
mod utils;

// Re-export public engines
pub use belief::NativeBeliefEngine;
pub use consolidation::NativeConsolidationEngine;
pub use eval::NativeEvalEngine;
pub use hierarchy::NativeHierarchyEngine;
pub use ingest::NativeIngestEngine;
pub use knowledge::NativeKnowledgeEngine;
pub use memory::NativeMemoryEngine;
pub use provider::NativeProvider;

// Re-export utility functions for internal use
pub use utils::{decode, encode, id_field, scope_field, to_napi_error};

// Re-export cross-file resolution for internal use
pub use cross_file::resolve_cross_file_edges;

// Type alias for CoreError to avoid conflict
pub use engram_memory::CoreError;
