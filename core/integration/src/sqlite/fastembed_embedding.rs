//! EmbeddingProvider impl for FastEmbedBgeSmallQueryProvider.
//!
//! Lives in the engine-specific zone (core/integration/src/sqlite/) to avoid a
//! dependency cycle: engram-store-sqlite cannot depend on engram-integration,
//! but the impl needs the EmbeddingProvider trait from integration.

use engram_domain::EmbeddingSpace;
use engram_runtime::CoreResult;

use crate::EmbeddingProvider;

/// Adapter that wraps the FastEmbed query provider + carries the configured
/// embedding space so it matches what the VectorIndex was opened with.
#[cfg(feature = "fastembed")]
pub struct FastEmbedEmbeddingProvider {
    inner: std::sync::Arc<engram_store_sqlite::FastEmbedBgeSmallQueryProvider>,
    space: EmbeddingSpace,
}

#[cfg(feature = "fastembed")]
impl FastEmbedEmbeddingProvider {
    pub fn new(
        inner: std::sync::Arc<engram_store_sqlite::FastEmbedBgeSmallQueryProvider>,
        space: EmbeddingSpace,
    ) -> Self {
        Self { inner, space }
    }
}

#[cfg(feature = "fastembed")]
impl EmbeddingProvider for FastEmbedEmbeddingProvider {
    fn tokenize_untruncated(
        &self,
        text: &str,
        special_tokens: bool,
    ) -> CoreResult<engram_retrieval::TextTokenization> {
        if self.space.dimensions != self.inner.dimensions() {
            return Err(engram_runtime::CoreError::InvalidRequest {
                reason: "declared embedding dimensions differ from the actual local model".into(),
            });
        }
        self.inner.tokenize_untruncated(text, special_tokens)
    }
    fn embed_prepared_code_query(
        &self,
        query: &crate::code_query::PreparedCodeQuery,
    ) -> CoreResult<Vec<f32>> {
        // BGE uses the same encoder for passages and explicitly prepared queries.
        // The exact prepared text already includes its selected instruction.
        let actual = self.tokenize_untruncated(&query.text, true)?;
        if actual.tokenizer_id != query.tokenizer_id
            || actual.window != query.window
            || actual.total_tokens() != query.coverage.total_tokens as usize
            || actual.total_tokens() > actual.window as usize
        {
            return Err(engram_runtime::CoreError::InvalidRequest {
                reason: "prepared query tokenizer contract changed".into(),
            });
        }
        self.inner.embed_passage(&query.text)
    }

    fn is_local(&self) -> bool {
        true
    }

    fn provider_id(&self) -> &str {
        self.space.provider.as_str()
    }

    fn model_id(&self) -> &str {
        self.space.model.as_str()
    }

    fn dimensions(&self) -> u32 {
        self.space.dimensions
    }

    fn embedding_space(&self) -> EmbeddingSpace {
        self.space.clone()
    }

    fn embed_query(&self, query: &str) -> CoreResult<Vec<f32>> {
        self.inner.embed_query(query)
    }

    fn embed_passage(&self, text: &str) -> CoreResult<Vec<f32>> {
        self.inner.embed_passage(text)
    }

    /// Real batch embedding: one FastEmbed model call over all texts.
    ///
    /// Overrides the default per-item loop with FastEmbed's native batched
    /// `TextEmbedding::embed`, so ingest paths embed a whole chunk batch through
    /// the model once instead of N times. Slot-aligned with the per-item path
    /// (each non-empty result matches `embed_passage` within tolerance; empty
    /// trimmed texts yield an empty vector in their slot and are skipped by the
    /// ingest loop).
    fn embed_batch(&self, texts: &[String]) -> CoreResult<Vec<Vec<f32>>> {
        self.inner.embed_passage_batch(texts)
    }
}
