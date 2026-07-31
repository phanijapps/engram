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
}
