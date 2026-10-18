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
    /// Resolve a catalogue name and dimensions without loading model assets.
    pub fn resolve_config(config: &mut crate::EmbeddingProviderConfig) -> CoreResult<()> {
        let selected = engram_store_sqlite::FastEmbedModelSelection::resolve(
            &config.model,
            config.dimensions,
        )?;
        config.model = selected.model_id;
        config.dimensions = selected.dimensions;
        config
            .validate()
            .map_err(|reason| engram_runtime::CoreError::InvalidRequest { reason })?;
        if config.normalization.as_deref() == Some("none") {
            return Err(engram_runtime::CoreError::InvalidRequest {
                reason: "FastEmbed produces normalized vectors; normalization=none is unsupported"
                    .into(),
            });
        }
        Ok(())
    }

    pub fn from_config(config: &crate::EmbeddingProviderConfig) -> CoreResult<Self> {
        let selected = engram_store_sqlite::FastEmbedModelSelection::resolve(
            &config.model,
            config.dimensions,
        )?;
        let inner = engram_store_sqlite::FastEmbedBgeSmallQueryProvider::from_selection(
            selected,
            config.options.query_instruction.clone(),
            config.options.passage_instruction.clone(),
        )?;
        Ok(Self::new(
            std::sync::Arc::new(inner),
            config.embedding_space(),
        ))
    }

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
        crate::embedding_runtime::validate_output(
            self.inner.embed_query(query)?,
            self.space.dimensions,
            self.space.normalization.as_deref(),
        )
    }

    fn embed_passage(&self, text: &str) -> CoreResult<Vec<f32>> {
        crate::embedding_runtime::validate_output(
            self.inner.embed_passage(text)?,
            self.space.dimensions,
            self.space.normalization.as_deref(),
        )
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
        self.inner
            .embed_passage_batch(texts)?
            .into_iter()
            .map(|v| {
                if v.is_empty() {
                    Ok(v)
                } else {
                    crate::embedding_runtime::validate_output(
                        v,
                        self.space.dimensions,
                        self.space.normalization.as_deref(),
                    )
                }
            })
            .collect()
    }
}
