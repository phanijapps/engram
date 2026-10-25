//! engram-code — deterministic code-indexing behavior crate (ADR-0028).
//!
//! Owns the pure code-extraction pipeline: tree-sitter parsing, symbol
//! chunking, and (with RFC-0020 Phase 2) receiver-qualified identity, typed
//! structural edges, cross-file resolution with the unresolved-reference
//! ledger, and framework pattern resolvers. The crate is pure — no filesystem
//! I/O, git, storage, async runtime, or LLM; `adapters/ingest` consumes it to
//! build the code graph.

pub mod edge_resolution;
pub mod edges;
pub mod frameworks;
pub mod identity;
pub mod noise;
pub mod parser;
pub mod queries;
pub mod resolution;

pub use edge_resolution::{
    AttemptOutcome, ResolutionBranch, attempt, branch_evidence, legacy_code_resolution, producer,
    sweep_evidence,
};
pub use edges::{StructuralEdges, contains_pairs};
pub use frameworks::{
    ConsumeFact, FrameworkFacts, RouteFact, extract_api_consumers, extract_frameworks,
};
pub use identity::{Resolution, SymbolCandidate, SymbolIndex, bare_tail, qualified_name};
pub use noise::{is_noise_declaration, is_noise_symbol};
pub use parser::chunking::{ChunkCandidate, Chunker};
pub use parser::code_symbol::CodeSymbolChunker;
pub use parser::tree_sitter_chunker::TreeSitterChunker;
pub use queries::{ExploreNode, FileDependency, explore, file_dependencies};
pub use resolution::{receiver_type_hint, resolve_refs, split_dotted};
