//! Deterministic ingestion for source-grounded knowledge.
//!
//! This crate turns caller-provided document text into portable knowledge
//! records. It does not read files, call embedding providers, or retrieve
//! context; those responsibilities belong to adapter and retrieval crates.

mod chunker;
mod code_symbol;
mod filesystem;
mod git;
mod hash;
mod ingestor;
mod request;

pub use chunker::{ChunkCandidate, Chunker, PlainTextChunker, PlainTextChunkerOptions};
pub use code_symbol::CodeSymbolChunker;
pub use filesystem::{FilesystemSourceReader, FilesystemSourceReaderOptions};
pub use git::GitSourceReader;
pub use hash::content_hash;
pub use ingestor::{IngestedKnowledge, KnowledgeIngestor};
pub use request::{DocumentIngestRequest, DocumentMetadata};
