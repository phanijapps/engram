//! Deterministic ingestion for source-grounded knowledge.
//!
//! This crate turns caller-provided document text into portable knowledge
//! records. It does not read files, call embedding providers, or retrieve
//! context; those responsibilities belong to adapter and retrieval crates.

mod chunker;
mod code_symbol;
mod extractor;
mod filesystem;
mod git;
mod hash;
mod ingestor;
mod request;
mod scanner;
mod source_key;
mod tree_sitter_chunker;

pub use chunker::{ChunkCandidate, Chunker, PlainTextChunker, PlainTextChunkerOptions};
pub use code_symbol::CodeSymbolChunker;
pub use extractor::{ExtractedGraph, GraphExtractor};
pub use filesystem::{FilesystemSourceReader, FilesystemSourceReaderOptions};
pub use git::GitSourceReader;
pub use hash::content_hash;
pub use ingestor::{IngestedKnowledge, KnowledgeIngestor};
pub use request::{DocumentIngestRequest, DocumentMetadata};
pub use scanner::{
    FileKind, ScanOptions, ScanProgress, ScanSummary, classify_file, is_denylisted, is_secret_file,
    is_within_root, scan_repository,
};
pub use source_key::{SOURCE_PATH_KEY, STABLE_SOURCE_KEY, stable_source_key};
pub use tree_sitter_chunker::TreeSitterChunker;
