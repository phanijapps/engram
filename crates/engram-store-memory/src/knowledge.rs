//! In-memory knowledge repository implementation.
//!
//! Knowledge records share the process-local adapter for tests, but stay in
//! separate maps from memories and events so source-grounded content does not
//! become agent memory by accident.

use async_trait::async_trait;
use engram_core::{CoreResult, KnowledgeRepository};
use engram_domain::*;

use crate::{scope::scope_allows, service::InMemoryMemoryService};

#[async_trait]
impl KnowledgeRepository for InMemoryMemoryService {
    async fn put_source(&self, source: KnowledgeSource) -> CoreResult<KnowledgeSource> {
        let mut state = self.lock_state()?;
        state
            .knowledge_sources
            .insert(source.id.to_string(), source.clone());
        Ok(source)
    }

    async fn put_document(&self, document: SourceDocument) -> CoreResult<SourceDocument> {
        let mut state = self.lock_state()?;
        state
            .source_documents
            .insert(document.id.to_string(), document.clone());
        Ok(document)
    }

    async fn put_chunk(&self, chunk: KnowledgeChunk) -> CoreResult<KnowledgeChunk> {
        let mut state = self.lock_state()?;
        state
            .knowledge_chunks
            .insert(chunk.id.to_string(), chunk.clone());
        Ok(chunk)
    }

    async fn get_chunk(&self, id: &ChunkId, scope: &Scope) -> CoreResult<Option<KnowledgeChunk>> {
        let state = self.lock_state()?;
        let Some(chunk) = state.knowledge_chunks.get(id.as_str()) else {
            return Ok(None);
        };
        let Some(document) = state.source_documents.get(chunk.document_id.as_str()) else {
            return Ok(None);
        };
        let Some(source) = state.knowledge_sources.get(document.source_id.as_str()) else {
            return Ok(None);
        };
        Ok(scope_allows(&source.scope, scope).then(|| chunk.clone()))
    }
}
