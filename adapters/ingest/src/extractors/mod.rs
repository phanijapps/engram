//! Source-type-specific extraction logic behind a unified trait.
//!
//! This module defines the `SourceExtractor` trait which encapsulates the
//! extraction logic that varies by source type (code symbols, prose concepts,
//! OpenAPI contracts, structured data). Each source type implements the trait
//! and provides its own entity extraction strategy while sharing a common
//! chunker selection interface.
//!
//! # Open/Closed Principle
//!
//! Adding a new source type (Excel, DB rows) requires implementing this trait
//! and registering the implementation in the dispatch — no edits to shared
//! scanner/extractor orchestration code are needed.

mod code;
mod contract;
mod docs;
mod structured;

pub use code::CodeExtractor;
pub use contract::ContractExtractor;
pub use docs::DocsExtractor;
pub use structured::StructuredExtractor;

use crate::chunker::Chunker;
use engram_domain::{
    Scope, SourceDocument,
    knowledge::{KnowledgeChunk, KnowledgeEntity, KnowledgeRelationship},
};
use engram_knowledge::CoreResult;

/// Extraction result containing entities and relationships produced from one document.
pub type ExtractedEntities = (Vec<KnowledgeEntity>, Vec<KnowledgeRelationship>);

/// Source-type-varying extraction logic.
///
/// Implementations extract entities and relationships from documents using
/// source-type-specific strategies (tree-sitter for code, NLP for prose,
/// OpenAPI parser for contracts, etc.). Each source type also selects its
/// preferred chunking strategy via `select_chunker`.
///
/// # OCP Compliance
///
/// - **Open for extension:** Add a new source type by implementing this trait.
/// - **Closed for modification:** No edits to scanner/extractor dispatch code needed.
pub trait SourceExtractor: Send + Sync {
    /// Extracts entities and relationships from a document and its chunks.
    ///
    /// Implementations may use different strategies:
    /// - Code: Parse tree-sitter anchors for symbols (functions, classes)
    /// - Prose: Extract concepts from chunk text
    /// - Contracts: Parse OpenAPI/GraphQL for API definitions
    /// - Structured: Parse Excel/DB rows for data entities
    ///
    /// # Arguments
    ///
    /// * `document` - The source document with metadata and classification
    /// * `chunks` - Pre-chunked sections of the document
    /// * `scope` - The scope to apply to extracted entities and relationships
    ///
    /// # Returns
    ///
    /// A tuple of (entities, relationships) extracted from the document.
    fn extract(
        &self,
        document: &SourceDocument,
        chunks: &[KnowledgeChunk],
        scope: &Scope,
    ) -> CoreResult<ExtractedEntities>;

    /// Selects the appropriate chunker for this source type.
    ///
    /// Different source types require different chunking strategies:
    /// - Code: Tree-sitter-aware chunker that preserves syntax structure
    /// - Prose: Plain-text line chunker
    /// - Contracts: YAML/JSON-aware chunker
    /// - Structured: Row-based or cell-based chunker
    ///
    /// # Returns
    ///
    /// A boxed chunker instance ready for use.
    fn select_chunker(&self) -> CoreResult<Box<dyn Chunker>>;
}
