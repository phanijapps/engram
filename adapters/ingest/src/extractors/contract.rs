//! Contract extraction wrapper around OpenAPI detection and parsing.
//!
//! This module implements `SourceExtractor` for API contracts by delegating
//! to the existing `contract.rs` detection and parsing logic. Contract extraction
//! is fundamentally different from code/docs extraction because it works on
//! raw document text rather than pre-chunked content.

use chrono::Utc;
use engram_domain::*;
use engram_knowledge::{CoreError, CoreResult};

use crate::chunker::Chunker;
use crate::contract::{build_api_entity, build_exposes_rel, detect_and_parse_openapi};
use crate::extractors::SourceExtractor;

/// Contract extractor that produces API entities from OpenAPI documents.
///
/// Contract extraction works on raw document text rather than chunks because
/// OpenAPI parsing requires the full YAML/JSON structure, not chunked text.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ContractExtractor;

impl ContractExtractor {
    /// Creates a new contract extractor.
    pub fn new() -> Self {
        Self
    }
}

impl SourceExtractor for ContractExtractor {
    fn extract(
        &self,
        document: &SourceDocument,
        chunks: &[KnowledgeChunk],
        scope: &Scope,
    ) -> CoreResult<(Vec<KnowledgeEntity>, Vec<KnowledgeRelationship>)> {
        // Contract extraction doesn't use chunks - it needs the full document text.
        // We'll reconstruct the text from chunks for now, but this is a limitation.
        // In the full refactoring, we'd pass the original text through the pipeline.
        let text = chunks
            .iter()
            .map(|c| c.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");

        // Detect file extension from document path
        let ext = document
            .path
            .as_ref()
            .and_then(|p| p.rsplit('.').next())
            .unwrap_or("");

        let now = Utc::now();

        // Parse OpenAPI operations - convert String error to CoreError
        let operations = detect_and_parse_openapi(&text, ext)
            .map_err(|e| CoreError::InvalidRequest { reason: e })?
            .unwrap_or_default();

        if operations.is_empty() {
            // Not an OpenAPI document, return empty results
            return Ok((Vec::new(), Vec::new()));
        }

        // We need stable_source_key to build entities, but it's not available here.
        // For now, we'll use a placeholder. This is a limitation of the current
        // trait design - contract extraction needs source-level metadata.
        let stable_source_key = "unknown";

        let mut entities = Vec::new();
        let mut relationships = Vec::new();

        for op in &operations {
            let entity = build_api_entity(scope, stable_source_key, op, &document.provenance, now);
            let rel = build_exposes_rel(scope, stable_source_key, op, &document.provenance, now);
            entities.push(entity);
            relationships.push(rel);
        }

        Ok((entities, relationships))
    }

    fn select_chunker(&self) -> CoreResult<Box<dyn Chunker>> {
        // Contract documents don't use chunking - they're parsed as YAML/JSON
        // Return a placeholder chunker that won't be used
        use crate::chunker::{PlainTextChunker, PlainTextChunkerOptions};
        Ok(Box::new(PlainTextChunker::new(
            PlainTextChunkerOptions::default(),
        )?))
    }
}
