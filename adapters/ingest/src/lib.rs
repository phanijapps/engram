//! Deterministic ingestion for source-grounded knowledge.
//!
//! This crate turns caller-provided document text into portable knowledge
//! records. It does not read files, call embedding providers, or retrieve
//! context; those responsibilities belong to adapter and retrieval crates.

mod chunker;
mod classifier;
mod code_symbol;
mod contract;
mod contract_entities;
mod extractor;
mod filesystem;
mod git;
mod git_detect;
mod hash;
mod ingestor;
mod openapi_types;
mod reconcile;
mod request;
mod scanner;
mod source_key;
mod tree_sitter_chunker;
mod yaml_safety;

pub use chunker::{ChunkCandidate, Chunker, PlainTextChunker, PlainTextChunkerOptions};
pub use classifier::{FileKind, classify_file, is_denylisted, is_secret_file, is_within_root};
pub use code_symbol::CodeSymbolChunker;
pub use contract::{detect_and_parse_openapi, normalize_contract_key};
pub use contract_entities::ParsedOperation;
pub use extractor::{ExtractedGraph, GraphExtractor};
pub use filesystem::{FilesystemSourceReader, FilesystemSourceReaderOptions};
pub use git::GitSourceReader;
pub use git_detect::detect_git;
pub use hash::content_hash;
pub use ingestor::{IngestedKnowledge, KnowledgeIngestor};
pub use request::{DocumentIngestRequest, DocumentMetadata};
pub use scanner::{
    ScanOptions, ScanProgress, ScanSummary, detect_workspace, scan_repository, scan_workspace,
};
pub use source_key::{SOURCE_PATH_KEY, STABLE_SOURCE_KEY, stable_source_key};
pub use tree_sitter_chunker::TreeSitterChunker;
