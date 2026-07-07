//! FastEmbed provider implementing the EmbeddingProvider trait.
//!
//! This module wraps the existing `FastEmbedBgeSmallQueryProvider` from
//! `engram-store-vector` to implement the provider-neutral `EmbeddingProvider`
//! trait. It lives in the adapters crate (not `core/integration`) because it
//! depends on an adapter type; no functionality is duplicated — all embedding
//! generation delegates to the existing provider.
//!
//! Gated on the `engram-store-vector/fastembed-provider` feature, exposed here
//! as the `fastembed` feature.

#![cfg(feature = "fastembed")]

use engram_domain::EmbeddingSpace;
use engram_integration::embedding::EmbeddingProvider;
use engram_runtime::CoreResult;

/// Re-export the existing FastEmbed provider for use in the wrapper.
pub use engram_store_vector::FastEmbedBgeSmallQueryProvider;

impl EmbeddingProvider for FastEmbedBgeSmallQueryProvider {
    fn provider_id(&self) -> &str {
        "fastembed"
    }

    fn model_id(&self) -> &str {
        self.model_name()
    }

    fn dimensions(&self) -> u32 {
        384 // BGE-small-en-v1.5 produces 384-dimensional vectors
    }

    fn embedding_space(&self) -> EmbeddingSpace {
        EmbeddingSpace::new(
            self.provider_id(),
            self.model_id(),
            self.dimensions(),
            "query",
            None::<String>,
        )
    }

    fn embed_query(&self, query: &str) -> CoreResult<Vec<f32>> {
        // Delegate to the underlying provider. UFCS makes resolution explicit so
        // a rename cannot turn this into infinite recursion.
        FastEmbedBgeSmallQueryProvider::embed_query(self, query)
    }

    fn embed_passage(&self, text: &str) -> CoreResult<Vec<f32>> {
        FastEmbedBgeSmallQueryProvider::embed_passage(self, text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fastembed_metadata_is_correct() {
        let provider = FastEmbedBgeSmallQueryProvider::new().unwrap();
        assert_eq!(provider.provider_id(), "fastembed");
        assert_eq!(provider.dimensions(), 384);
    }

    #[test]
    fn fastembed_embedding_space_identity() {
        let provider = FastEmbedBgeSmallQueryProvider::new().unwrap();
        let space = provider.embedding_space();
        assert_eq!(space.provider, "fastembed");
        assert_eq!(space.dimensions, 384);
        assert_eq!(space.prompt_profile, "query");
    }
}
