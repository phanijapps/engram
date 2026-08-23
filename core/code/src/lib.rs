//! engram-code — deterministic code-indexing behavior crate (ADR-0028).
//!
//! Owns the pure code-extraction pipeline: tree-sitter parsing, symbol
//! chunking, and (with RFC-0020 Phase 2) receiver-qualified identity, typed
//! structural edges, cross-file resolution with the unresolved-reference
//! ledger, and framework pattern resolvers. The crate is pure — no filesystem
//! I/O, git, storage, async runtime, or LLM; `adapters/ingest` consumes it to
//! build the code graph.

pub mod edges;
pub mod identity;
pub mod parser;

pub use edges::{StructuralEdges, contains_pairs};
pub use identity::{Resolution, SymbolCandidate, SymbolIndex, bare_tail, qualified_name};
pub use parser::chunking::{ChunkCandidate, Chunker};
pub use parser::code_symbol::CodeSymbolChunker;
pub use parser::tree_sitter_chunker::TreeSitterChunker;
