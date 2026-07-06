//! Structured data extraction stub for Excel/DB row extraction.
//!
//! This module is a placeholder for future structured data extraction (Excel
//! spreadsheets, database rows, CSV files). The stub implements `SourceExtractor`
//! but returns empty results until the full implementation is added.

use engram_domain::*;
use engram_knowledge::CoreResult;

use crate::chunker::{Chunker, PlainTextChunker, PlainTextChunkerOptions};
use crate::extractors::SourceExtractor;

/// Structured extractor stub for Excel/DB row data.
///
/// TODO: Implement full extraction for:
/// - Excel spreadsheets (.xlsx, .xls)
/// - Database rows (SQL export dumps)
/// - CSV files with structured data
/// - Other tabular formats
///
/// The implementation will extract entities from rows/cells and build
/// relationships based on foreign keys or table structure.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StructuredExtractor;

impl StructuredExtractor {
    /// Creates a new structured extractor.
    pub fn new() -> Self {
        Self
    }
}

impl SourceExtractor for StructuredExtractor {
    fn extract(
        &self,
        _document: &SourceDocument,
        _chunks: &[KnowledgeChunk],
        _scope: &Scope,
    ) -> CoreResult<(Vec<KnowledgeEntity>, Vec<KnowledgeRelationship>)> {
        // TODO: Implement structured data extraction
        // Parse Excel/DB rows and extract entities from cell data
        // Build relationships from foreign keys or table references
        Ok((Vec::new(), Vec::new()))
    }

    fn select_chunker(&self) -> CoreResult<Box<dyn Chunker>> {
        // TODO: Implement row-based or cell-based chunking for structured data
        // For now, use plain-text chunking as a placeholder
        Ok(Box::new(PlainTextChunker::new(
            PlainTextChunkerOptions::default(),
        )?))
    }
}
