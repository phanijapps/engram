//! Trait bridge to the replaceable HTTP inference adapter.
use crate::{EmbeddingProvider, EmbeddingProviderConfig};
use engram_domain::EmbeddingSpace;
use engram_embedding_http::{HttpEmbeddingConfig, HttpEmbeddingProtocol, HttpEmbeddingProvider};
use engram_runtime::CoreResult;

pub(crate) struct ConfiguredHttpEmbedding {
    inner: HttpEmbeddingProvider,
    space: EmbeddingSpace,
}

impl ConfiguredHttpEmbedding {
    pub(crate) fn new(config: &EmbeddingProviderConfig) -> CoreResult<Self> {
        let protocol = if config.provider_type == "ollama" {
            HttpEmbeddingProtocol::Ollama
        } else {
            HttpEmbeddingProtocol::OpenAiCompatible
        };
        let endpoint = config
            .options
            .endpoint
            .clone()
            .or_else(|| {
                (config.provider_type == "ollama").then(|| "http://localhost:11434".to_string())
            })
            .ok_or_else(|| engram_runtime::CoreError::InvalidRequest {
                reason: "served embedding model requires an endpoint".into(),
            })?;
        let inner = HttpEmbeddingProvider::new(HttpEmbeddingConfig {
            protocol,
            model: config.model.clone(),
            endpoint: endpoint.clone(),
            dimensions: (config.dimensions != 0).then_some(config.dimensions),
            api_key_env: config.options.api_key_env.clone(),
            query_instruction: config.options.query_instruction.clone(),
            passage_instruction: config.options.passage_instruction.clone(),
            timeout_secs: config.options.request_timeout_secs,
            batch_size: config.options.batch_size,
        })?;
        let mut resolved = config.clone();
        resolved.dimensions = inner.dimensions();
        // An omitted Ollama endpoint and its explicit default share an identity.
        resolved.options.endpoint = Some(endpoint);
        Ok(Self {
            inner,
            space: resolved.embedding_space(),
        })
    }

    fn checked(&self, vector: Vec<f32>) -> CoreResult<Vec<f32>> {
        crate::embedding_runtime::validate_output(
            vector,
            self.space.dimensions,
            self.space.normalization.as_deref(),
        )
    }
}

impl EmbeddingProvider for ConfiguredHttpEmbedding {
    fn provider_id(&self) -> &str {
        &self.space.provider
    }
    fn model_id(&self) -> &str {
        &self.space.model
    }
    fn dimensions(&self) -> u32 {
        self.space.dimensions
    }
    fn embedding_space(&self) -> EmbeddingSpace {
        self.space.clone()
    }
    fn embed_query(&self, text: &str) -> CoreResult<Vec<f32>> {
        self.checked(self.inner.embed_query(text)?)
    }
    fn embed_passage(&self, text: &str) -> CoreResult<Vec<f32>> {
        self.checked(self.inner.embed_passage(text)?)
    }
    fn embed_batch(&self, texts: &[String]) -> CoreResult<Vec<Vec<f32>>> {
        self.inner
            .embed_batch(texts)?
            .into_iter()
            .map(|v| self.checked(v))
            .collect()
    }
}
