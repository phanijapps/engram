//! Ollama embedding provider implementing the provider-neutral
//! [`EmbeddingProvider`] trait.
//!
//! This module is gated behind the `ollama` feature so the integration crate
//! stays free of HTTP machinery by default. When enabled, the provider calls a
//! local Ollama daemon's `/api/embeddings` endpoint. The identity methods
//! (`provider_id`, `model_id`, `dimensions`, `embedding_space`) are always
//! available once constructed; only `embed_query`/`embed_passage`/`embed_batch`
//! perform network I/O.

#[cfg(feature = "ollama")]
use engram_domain::EmbeddingSpace;

#[cfg(feature = "ollama")]
use engram_runtime::{CoreError, CoreResult};

#[cfg(feature = "ollama")]
use crate::embedding::EmbeddingProvider;

/// Ollama-backed embedding provider.
///
/// Construct with [`OllamaEmbeddingProvider::new`], pointing at a reachable
/// Ollama daemon (default `http://localhost:11434`) and naming a model that
/// Ollama serves (e.g. `nomic-embed-text`). The declared `dimensions` must
/// match what the named model actually produces.
#[cfg(feature = "ollama")]
pub struct OllamaEmbeddingProvider {
    endpoint: String,
    model: String,
    dimensions: u32,
    prompt_profile: String,
    embedding_space: EmbeddingSpace,
}

#[cfg(feature = "ollama")]
impl OllamaEmbeddingProvider {
    /// Creates a new Ollama provider.
    ///
    /// `endpoint` should include the scheme and host (e.g.
    /// `http://localhost:11434`); the `/api/embeddings` path is appended
    /// internally.
    pub fn new(
        endpoint: impl Into<String>,
        model: impl Into<String>,
        dimensions: u32,
        prompt_profile: impl Into<String>,
    ) -> Self {
        let endpoint = endpoint.into();
        let model = model.into();
        let prompt_profile = prompt_profile.into();
        let embedding_space = EmbeddingSpace::new(
            "ollama",
            &model,
            dimensions,
            &prompt_profile,
            None::<String>,
        );
        Self {
            endpoint,
            model,
            dimensions,
            prompt_profile,
            embedding_space,
        }
    }
}

#[cfg(feature = "ollama")]
impl EmbeddingProvider for OllamaEmbeddingProvider {
    fn provider_id(&self) -> &str {
        "ollama"
    }

    fn model_id(&self) -> &str {
        &self.model
    }

    fn dimensions(&self) -> u32 {
        self.dimensions
    }

    fn embedding_space(&self) -> EmbeddingSpace {
        self.embedding_space.clone()
    }

    fn embed_query(&self, query: &str) -> CoreResult<Vec<f32>> {
        self.embed_one(query)
    }

    fn embed_passage(&self, text: &str) -> CoreResult<Vec<f32>> {
        self.embed_one(text)
    }
}

#[cfg(feature = "ollama")]
impl OllamaEmbeddingProvider {
    fn embed_one(&self, prompt: &str) -> CoreResult<Vec<f32>> {
        use std::time::Duration;
        if prompt.trim().is_empty() {
            return Err(CoreError::InvalidRequest {
                reason: "embed prompt cannot be empty".to_string(),
            });
        }
        let url = format!("{}/api/embeddings", self.endpoint.trim_end_matches('/'));
        let body = serde_json::json!({
            "model": self.model,
            "prompt": prompt,
        });
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|e| CoreError::Adapter {
                adapter: "ollama".to_string(),
                message: format!("http client build failed: {e}"),
            })?;
        let resp = client
            .post(&url)
            .json(&body)
            .send()
            .map_err(|e| CoreError::Adapter {
                adapter: "ollama".to_string(),
                message: format!("request failed: {e}"),
            })?;
        if !resp.status().is_success() {
            return Err(CoreError::Adapter {
                adapter: "ollama".to_string(),
                message: format!("non-success status: {}", resp.status()),
            });
        }
        let parsed: serde_json::Value = resp.json().map_err(|e| CoreError::Adapter {
            adapter: "ollama".to_string(),
            message: format!("invalid response body: {e}"),
        })?;
        let embedding = parsed
            .get("embedding")
            .and_then(|v| v.as_array())
            .ok_or_else(|| CoreError::Adapter {
                adapter: "ollama".to_string(),
                message: "response missing 'embedding' array".to_string(),
            })?;
        let vector = embedding
            .iter()
            .map(|v| {
                v.as_f64()
                    .map(|f| f as f32)
                    .ok_or_else(|| CoreError::Adapter {
                        adapter: "ollama".to_string(),
                        message: "embedding component is not a number".to_string(),
                    })
            })
            .collect::<CoreResult<Vec<f32>>>()?;
        if vector.len() as u32 != self.dimensions {
            return Err(CoreError::EmbeddingSpaceMismatch {
                expected: format!("dimensions={}", self.dimensions),
                actual: format!("dimensions={}", vector.len()),
            });
        }
        Ok(vector)
    }
}

#[cfg(all(test, feature = "ollama"))]
mod tests {
    use super::*;

    #[test]
    fn ollama_provider_identity_is_stable() {
        let p = OllamaEmbeddingProvider::new(
            "http://localhost:11434",
            "nomic-embed-text",
            768,
            "query",
        );
        assert_eq!(p.provider_id(), "ollama");
        assert_eq!(p.model_id(), "nomic-embed-text");
        assert_eq!(p.dimensions(), 768);
        assert_eq!(p.embedding_space().provider, "ollama");
    }

    #[test]
    fn ollama_rejects_empty_prompt() {
        let p = OllamaEmbeddingProvider::new("http://localhost:11434", "m", 8, "query");
        assert!(p.embed_query("").is_err());
    }
}
