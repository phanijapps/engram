//! Parsing-layer chunk contract (moved from `adapters/ingest/src/chunker.rs`;
//! the ADR-0028 `parse()`/`resolve()` API reshapes this when the ingest
//! cutover lands (T8) — text/markdown chunkers in the adapter keep
//! implementing this trait until then).

use engram_domain::{KnowledgeChunkKind, SourceLocation};
use engram_runtime::CoreResult;

/// Candidate chunk produced before domain IDs and provenance are attached.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkCandidate {
    pub kind: KnowledgeChunkKind,
    pub text: String,
    pub location: Option<SourceLocation>,
}

/// Splits source text into stable chunks without attaching domain identity.
///
/// Implementations should preserve enough source location detail for retrieval
/// explanations. Stable IDs, provenance, policy, and repository writes are
/// attached by the ingestor after chunk candidates are produced.
pub trait Chunker: Send + Sync {
    /// Returns candidate chunks with local source locations for one document.
    fn chunk(&self, text: &str) -> CoreResult<Vec<ChunkCandidate>>;
}
