//! Source ingestion ports — readers, chunkers, and ingestion orchestration.

use async_trait::async_trait;
use engram_domain::*;
use engram_runtime::CoreResult;

/// Reads external sources without owning persistence.
///
/// Source readers translate filesystems, Git repositories, URLs, uploads, or
/// APIs into `SourceDocument` records and document content. They should report
/// adapter failures explicitly instead of returning partial reads as complete
/// ingestion.
#[async_trait]
pub trait SourceReader: Send + Sync {
    /// Lists or discovers documents available from a registered source.
    async fn read_source(&self, source: &KnowledgeSource) -> CoreResult<Vec<SourceDocument>>;

    /// Reads extracted textual content for one source document.
    async fn read_document(&self, document: &SourceDocument) -> CoreResult<String>;
}

/// Splits source document content into source-grounded chunks.
///
/// Chunkers preserve enough location and provenance information for later
/// retrieval explanations. Code-aware chunkers should emit symbol or file chunk
/// kinds instead of flattening everything into generic text.
pub trait Chunker: Send + Sync {
    /// Creates retrievable chunks from a document's extracted content.
    fn chunk_document(
        &self,
        source: &KnowledgeSource,
        document: &SourceDocument,
        content: &str,
    ) -> CoreResult<Vec<KnowledgeChunk>>;
}

/// Coordinates source reading, chunking, and knowledge persistence.
///
/// Ingestion services assemble source readers, chunkers, and repositories into
/// an idempotent source-to-knowledge pipeline. Dry runs should compute planned
/// writes without persisting sources, documents, chunks, entities, or graph
/// relationships.
#[async_trait]
pub trait IngestionService: Send + Sync {
    /// Ingests a registered source and returns chunks written or planned.
    async fn ingest(&self, request: IngestRequest) -> CoreResult<Vec<KnowledgeChunk>>;
}
