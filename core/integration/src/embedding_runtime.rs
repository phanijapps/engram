//! Provider selection before vector storage is constructed.
use crate::{EmbeddingProvider, EmbeddingProviderConfig};
use engram_runtime::{CoreError, CoreResult};
use std::sync::Arc;

pub struct PreparedEmbedding {
    pub config: EmbeddingProviderConfig,
    pub provider: Option<Arc<dyn EmbeddingProvider>>,
}

/// Resolve identity and native/observed dimensions before an index can be opened.
/// Disabled vectors never initialize models, read credentials or send HTTP.
pub fn prepare_embedding(
    config: &EmbeddingProviderConfig,
    enabled: bool,
) -> CoreResult<PreparedEmbedding> {
    let mut config = config.clone();
    config
        .validate()
        .map_err(|reason| CoreError::InvalidRequest { reason })?;
    if config.provider_type == "none" && config.dimensions == 0 {
        config.dimensions = EmbeddingProviderConfig::default().dimensions;
    }
    if !enabled || config.provider_type == "none" {
        return Ok(PreparedEmbedding {
            config,
            provider: None,
        });
    }
    let provider: Option<Arc<dyn EmbeddingProvider>> = match config.provider_type.as_str() {
        "fastembed" => {
            #[cfg(feature = "fastembed")]
            {
                crate::FastEmbedEmbeddingProvider::resolve_config(&mut config)?;
                match crate::FastEmbedEmbeddingProvider::from_config(&config) {
                    Ok(provider) => Some(Arc::new(provider)),
                    Err(error) => {
                        // Keep availability degradation, but never substitute a different model.
                        eprintln!(
                            "engram: configured local embedding model unavailable; inference disabled: {error}"
                        );
                        None
                    }
                }
            }
            #[cfg(not(feature = "fastembed"))]
            {
                if config.dimensions == 0 {
                    return Err(invalid(
                        "automatic local model dimensions require the fastembed feature",
                    ));
                }
                // A storage-only SDK build can still hold explicitly dimensioned vectors.
                None
            }
        }
        "ollama" | "openai-compatible" => {
            #[cfg(feature = "http-embeddings")]
            {
                if config.provider_type == "ollama" && config.options.endpoint.is_none() {
                    config.options.endpoint = Some("http://localhost:11434".into());
                }
                let provider = crate::http_embedding::ConfiguredHttpEmbedding::new(&config)?;
                config.dimensions = provider.dimensions();
                Some(Arc::new(provider))
            }
            #[cfg(not(feature = "http-embeddings"))]
            {
                return Err(invalid(
                    "served embedding providers require the http-embeddings feature",
                ));
            }
        }
        _ => {
            return Err(invalid(
                "unsupported embedding provider; expected fastembed, ollama, openai-compatible or none",
            ));
        }
    };
    Ok(PreparedEmbedding { config, provider })
}

fn invalid(reason: &str) -> CoreError {
    CoreError::InvalidRequest {
        reason: reason.into(),
    }
}

#[cfg(any(feature = "fastembed", feature = "http-embeddings"))]
pub(crate) fn validate_output(
    mut values: Vec<f32>,
    dimensions: u32,
    normalization: Option<&str>,
) -> CoreResult<Vec<f32>> {
    if values.len() != dimensions as usize
        || values.is_empty()
        || !values.iter().all(|v| v.is_finite())
    {
        return Err(invalid(
            "embedding output is non-finite, empty or has incorrect dimensions",
        ));
    }
    if normalization == Some("l2") {
        let norm = values
            .iter()
            .map(|v| f64::from(*v).powi(2))
            .sum::<f64>()
            .sqrt();
        if norm == 0.0 || !norm.is_finite() {
            return Err(invalid("embedding cannot be L2-normalized"));
        }
        for value in &mut values {
            *value = (f64::from(*value) / norm) as f32;
        }
    }
    Ok(values)
}
