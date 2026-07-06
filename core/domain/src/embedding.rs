//! Embedding space identity types for the integration contract.
//!
//! This module defines the stable embedding space identity that goes beyond
//! vector dimensions to ensure compatibility between different embedding providers.
//! Two embeddings are compatible only if they share the same provider, model,
//! dimensions, prompt profile, and normalization settings.

use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};

/// Embedding space identity that defines vector compatibility.
///
/// Embeddings are compatible only if they share the same embedding space,
/// not just the same dimensions. A 384-dimensional FastEmbed vector and a
/// 384-dimensional OpenAI vector are NOT interchangeable because they have
/// different providers, models, and potentially different prompt profiles.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmbeddingSpace {
    /// Provider identity (e.g., "fastembed", "ollama", "openai").
    pub provider: String,

    /// Model identity within the provider (e.g., "BAAI/bge-small-en-v1.5").
    pub model: String,

    /// Vector dimensions (e.g., 384).
    pub dimensions: u32,

    /// Prompt profile used for embedding generation (e.g., "query", "passage").
    pub prompt_profile: String,

    /// Normalization applied to embeddings (e.g., "none", "l2", "cosine").
    pub normalization: Option<String>,
}

impl EmbeddingSpace {
    /// Creates a new embedding space with the given parameters.
    pub fn new(
        provider: impl Into<String>,
        model: impl Into<String>,
        dimensions: u32,
        prompt_profile: impl Into<String>,
        normalization: Option<impl Into<String>>,
    ) -> Self {
        Self {
            provider: provider.into(),
            model: model.into(),
            dimensions,
            prompt_profile: prompt_profile.into(),
            normalization: normalization.map(|n| n.into()),
        }
    }

    /// Returns a stable string identifier for this embedding space.
    ///
    /// The identifier format is: `{provider}/{model}/{dimensions}/{prompt_profile}/{normalization}`.
    pub fn identifier(&self) -> String {
        let norm = self.normalization.as_deref().unwrap_or("none");
        format!(
            "{}/{}/{}/{}/{}",
            self.provider, self.model, self.dimensions, self.prompt_profile, norm
        )
    }

    /// Checks if another embedding space is compatible with this one.
    ///
    /// Two embedding spaces are compatible if all fields match exactly.
    pub fn is_compatible(&self, other: &EmbeddingSpace) -> bool {
        self == other
    }
}

impl Hash for EmbeddingSpace {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.provider.hash(state);
        self.model.hash(state);
        self.dimensions.hash(state);
        self.prompt_profile.hash(state);
        self.normalization.hash(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedding_space_equality() {
        let space1 = EmbeddingSpace::new(
            "fastembed",
            "BAAI/bge-small-en-v1.5",
            384,
            "query",
            None::<String>,
        );
        let space2 = EmbeddingSpace::new(
            "fastembed",
            "BAAI/bge-small-en-v1.5",
            384,
            "query",
            None::<String>,
        );
        assert_eq!(space1, space2);
        assert!(space1.is_compatible(&space2));
    }

    #[test]
    fn test_embedding_space_inequality() {
        let space1 = EmbeddingSpace::new(
            "fastembed",
            "BAAI/bge-small-en-v1.5",
            384,
            "query",
            None::<String>,
        );
        let space2 = EmbeddingSpace::new(
            "fastembed",
            "BAAI/bge-small-en-v1.5",
            384,
            "passage",
            None::<String>,
        );
        assert_ne!(space1, space2);
        assert!(!space1.is_compatible(&space2));
    }

    #[test]
    fn test_embedding_space_provider_mismatch() {
        let space1 = EmbeddingSpace::new(
            "fastembed",
            "BAAI/bge-small-en-v1.5",
            384,
            "query",
            None::<String>,
        );
        let space2 =
            EmbeddingSpace::new("ollama", "nomic-embed-text", 768, "query", None::<String>);
        assert_ne!(space1, space2);
        assert!(!space1.is_compatible(&space2));
    }

    #[test]
    fn test_embedding_space_serialization() {
        let space = EmbeddingSpace::new(
            "fastembed",
            "BAAI/bge-small-en-v1.5",
            384,
            "query",
            None::<String>,
        );
        let json = serde_json::to_string(&space).unwrap();
        let deserialized: EmbeddingSpace = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, space);
    }

    #[test]
    fn test_embedding_space_identifier() {
        let space = EmbeddingSpace::new(
            "fastembed",
            "BAAI/bge-small-en-v1.5",
            384,
            "query",
            None::<String>,
        );
        let identifier = space.identifier();
        assert_eq!(
            identifier,
            "fastembed/BAAI/bge-small-en-v1.5/384/query/none"
        );
    }

    #[test]
    fn test_embedding_space_identifier_with_normalization() {
        let space = EmbeddingSpace::new(
            "openai",
            "text-embedding-3-small",
            1536,
            "query",
            Some("l2"),
        );
        let identifier = space.identifier();
        assert_eq!(identifier, "openai/text-embedding-3-small/1536/query/l2");
    }

    #[test]
    fn test_embedding_space_hash_consistency() {
        let space1 = EmbeddingSpace::new(
            "fastembed",
            "BAAI/bge-small-en-v1.5",
            384,
            "query",
            None::<String>,
        );
        let space2 = EmbeddingSpace::new(
            "fastembed",
            "BAAI/bge-small-en-v1.5",
            384,
            "query",
            None::<String>,
        );

        use std::collections::hash_map::DefaultHasher;
        let mut hasher = DefaultHasher::default();
        space1.hash(&mut hasher);
        let hash1 = hasher.finish();

        let mut hasher = DefaultHasher::default();
        space2.hash(&mut hasher);
        let hash2 = hasher.finish();

        assert_eq!(
            hash1, hash2,
            "Equal embedding spaces should hash identically"
        );
    }

    #[test]
    fn test_embedding_space_hash_different_for_different_spaces() {
        let space1 = EmbeddingSpace::new(
            "fastembed",
            "BAAI/bge-small-en-v1.5",
            384,
            "query",
            None::<String>,
        );
        let space2 = EmbeddingSpace::new(
            "fastembed",
            "BAAI/bge-small-en-v1.5",
            384,
            "passage",
            None::<String>,
        );

        use std::collections::hash_map::DefaultHasher;
        let mut hasher = DefaultHasher::default();
        space1.hash(&mut hasher);
        let hash1 = hasher.finish();

        let mut hasher = DefaultHasher::default();
        space2.hash(&mut hasher);
        let hash2 = hasher.finish();

        assert_ne!(
            hash1, hash2,
            "Different embedding spaces should hash differently"
        );
    }
}
