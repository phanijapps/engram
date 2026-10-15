//! engram-code — deterministic code-indexing behavior crate (ADR-0028).
//!
//! Owns the pure code-extraction pipeline: tree-sitter parsing, symbol
//! chunking, and (with RFC-0020 Phase 2) receiver-qualified identity, typed
//! structural edges, cross-file resolution with the unresolved-reference
//! ledger, and framework pattern resolvers. The crate is pure — no filesystem
//! I/O, git, storage, async runtime, or LLM; `adapters/ingest` consumes it to
//! build the code graph.

pub mod edges;
pub mod frameworks;
pub mod identity;
pub mod noise;
pub mod parser;
pub mod queries;
pub mod resolution;
mod segmentation;
pub use segmentation::{TokenSegmentPlan, segment_source_ranges, structural_boundaries};

pub use edges::{StructuralEdges, contains_pairs};
pub use frameworks::{FrameworkFacts, RouteFact, extract_frameworks};
pub use identity::{
    Resolution, SymbolCandidate, SymbolIndex, bare_tail, qualified_name, repository_entity_id,
};
pub use noise::is_noise_symbol;
pub use parser::chunking::{ChunkCandidate, Chunker};
pub use parser::code_symbol::CodeSymbolChunker;
pub use parser::source_coverage::{
    CoveredCodeCandidate, SOURCE_COVERAGE_FINGERPRINT, snapshot_ranges, source_parse_outcome,
    source_residual_ranges,
};
pub use parser::tree_sitter_chunker::TreeSitterChunker;
pub use queries::{ExploreNode, FileDependency, explore, file_dependencies};
pub use resolution::{receiver_type_hint, resolve_refs, split_dotted};
